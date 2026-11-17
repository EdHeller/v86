
// For Types Only
import { CPU } from "./cpu.js";
import { BusConnector } from "./bus.js";

// Sound Blaster 16. The card (DSP, mixer, DMA playback, MPU-401 reset, FM ports) is in the core:
// src/rust/cpu/sb16.rs, which answers its ports. Here: the bus (what the card tells the speaker, the samples the
// speaker asks for), DMA unmask notifications, the snapshot format.

// the fields of the core's state (struct State in src/rust/cpu/sb16.rs): name, byte offset
const FIELDS = [
    ["read_buffer_lastvalue", 8], ["command", 12], ["command_size", 16], ["mixer_current_address", 20],
    ["dummy_speaker_enabled", 24], ["test_register", 28], ["dsp_highspeed", 32], ["dsp_stereo", 36],
    ["dsp_16bit", 40], ["dsp_signed", 44], ["dma_sample_count", 48], ["dma_bytes_count", 52],
    ["dma_bytes_left", 56], ["dma_bytes_block", 60], ["dma_irq", 64], ["dma_channel", 68],
    ["dma_channel_8bit", 72], ["dma_channel_16bit", 76], ["dma_autoinit", 80], ["dma_waiting_transfer", 84],
    ["dma_paused", 88], ["bytes_per_sample", 92], ["e2_value", 96], ["e2_count", 100],
    ["mpu_read_buffer_lastvalue", 104], ["irq", 108],
];
const BYTE_ARRAYS = [["mixer_registers", 124, 256], ["asp_registers", 380, 256], ["irq_triggered", 636, 16], ["dma_buffer_uint8", 652, 65536]];

// what the core tells the bus (EV_* in src/rust/cpu/sb16.rs)
const EV_RATE = 0, EV_DAC_ENABLE = 1, EV_DAC_DISABLE = 2, EV_VOLUME = 3, EV_GAIN_LEFT = 4, EV_GAIN_RIGHT = 5,
    EV_TREBLE_LEFT = 6, EV_TREBLE_RIGHT = 7, EV_BASS_RIGHT = 8;

/**
 * @constructor
 * @param {CPU} cpu
 * @param {BusConnector} bus
 */
export function SB16(cpu, bus)
{
    /** @const @type {CPU} */
    this.cpu = cpu;

    /** @const @type {BusConnector} */
    this.bus = bus;

    const exports = cpu.wm.exports;
    this.exports = exports;
    const state = () => exports["sb16_state"]() >>> 0;

    for(const [name, offset] of FIELDS)
    {
        Object.defineProperty(this, name, {
            get: () => new Int32Array(cpu.wasm_memory.buffer, state() + offset, 1)[0],
            set: value => { new Int32Array(cpu.wasm_memory.buffer, state() + offset, 1)[0] = +value; },
        });
    }
    Object.defineProperty(this, "sampling_rate", {
        get: () => new Float64Array(cpu.wasm_memory.buffer, state(), 1)[0],
        set: value => { new Float64Array(cpu.wasm_memory.buffer, state(), 1)[0] = value; },
    });
    for(const [name, offset, length] of BYTE_ARRAYS)
    {
        Object.defineProperty(this, name, {
            get: () => new Uint8Array(cpu.wasm_memory.buffer, state() + offset, length),
            set: value => { new Uint8Array(cpu.wasm_memory.buffer, state() + offset, length).set(value); },
        });
    }

    bus.send("dac-tell-sampling-rate", 22050);
    cpu.devices.sb16 = this; // (the core's events during sb16_init find it)
    exports["sb16_init"]();

    // the ports, when reached through JavaScript (the core answers them directly)
    const io = cpu.io;
    for(const port of [0x220, 0x221, 0x222, 0x223, 0x224, 0x225, 0x226, 0x227, 0x228, 0x229, 0x22A, 0x22B,
                       0x22C, 0x22D, 0x22E, 0x22F, 0x388, 0x389, 0x330, 0x331])
    {
        io.register_read(port, this, () => exports["sb16_port_read"](port));
        io.register_write(port, this, value => exports["sb16_port_write"](port, value));
    }

    cpu.devices.dma.on_unmask(channel => exports["sb16_dma_unmask"](channel), this);

    bus.register("dac-request-data", function()
    {
        exports["sb16_dac_request"]();
        this.dac_send();
    }, this);
    bus.register("speaker-has-initialized", function()
    {
        exports["sb16_mixer_reset"]();
    }, this);
    bus.send("speaker-confirm-initialized");
}

/** Something from the core for the bus */
SB16.prototype.core_event = function(kind, a, b, c)
{
    switch(kind)
    {
        case EV_RATE: this.bus.send("dac-tell-sampling-rate", a); break;
        case EV_DAC_ENABLE: this.bus.send("dac-enable"); break;
        case EV_DAC_DISABLE: this.bus.send("dac-disable"); break;
        case EV_VOLUME: this.bus.send("mixer-volume", [a, b, c]); break;
        case EV_GAIN_LEFT: this.bus.send("mixer-gain-left", a); break;
        case EV_GAIN_RIGHT: this.bus.send("mixer-gain-right", a); break;
        case EV_TREBLE_LEFT: this.bus.send("mixer-treble-left", a); break;
        case EV_TREBLE_RIGHT: this.bus.send("mixer-treble-right", a); break;
        case EV_BASS_RIGHT: this.bus.send("mixer-bass-right", a); break;
    }
};

/** The converted samples, to the speaker */
SB16.prototype.dac_send = function()
{
    const n = this.exports["sb16_dac_take"]();
    if(!n)
    {
        return;
    }
    const at = this.exports["sb16_dac_samples"]() >>> 0;
    const all = new Float32Array(this.cpu.wasm_memory.buffer, at, 2 * n);
    const out0 = all.slice(0, n);
    const out1 = all.slice(n, 2 * n);
    this.bus.send("dac-send-data", [out0, out1], [out0.buffer, out1.buffer]);
};

SB16.prototype.get_state = function()
{
    var state = [];
    state[2] = this.read_buffer_lastvalue;
    state[3] = this.command;
    state[4] = this.command_size;
    state[5] = this.mixer_current_address;
    state[6] = this.mixer_registers.slice();
    state[7] = !!this.dummy_speaker_enabled;
    state[8] = this.test_register;
    state[9] = !!this.dsp_highspeed;
    state[10] = !!this.dsp_stereo;
    state[11] = !!this.dsp_16bit;
    state[12] = !!this.dsp_signed;
    state[15] = this.dma_sample_count;
    state[16] = this.dma_bytes_count;
    state[17] = this.dma_bytes_left;
    state[18] = this.dma_bytes_block;
    state[19] = this.dma_irq;
    state[20] = this.dma_channel;
    state[21] = this.dma_channel_8bit;
    state[22] = this.dma_channel_16bit;
    state[23] = !!this.dma_autoinit;
    state[24] = this.dma_buffer_uint8.slice();
    state[25] = !!this.dma_waiting_transfer;
    state[26] = !!this.dma_paused;
    state[27] = this.sampling_rate;
    state[28] = this.bytes_per_sample;
    state[29] = this.e2_value;
    state[30] = this.e2_count;
    state[31] = this.asp_registers.slice();
    state[33] = this.mpu_read_buffer_lastvalue;
    state[34] = this.irq;
    state[35] = this.irq_triggered.slice();
    return state;
};

SB16.prototype.set_state = function(state)
{
    this.read_buffer_lastvalue = state[2];
    this.command = state[3];
    this.command_size = state[4];
    this.mixer_current_address = state[5];
    this.mixer_registers = state[6];
    this.exports["sb16_mixer_full_update"]();
    this.dummy_speaker_enabled = state[7];
    this.test_register = state[8];
    this.dsp_highspeed = state[9];
    this.dsp_stereo = state[10];
    this.dsp_16bit = state[11];
    this.dsp_signed = state[12];
    this.dma_sample_count = state[15];
    this.dma_bytes_count = state[16];
    this.dma_bytes_left = state[17];
    this.dma_bytes_block = state[18];
    this.dma_irq = state[19];
    this.dma_channel = state[20];
    this.dma_channel_8bit = state[21];
    this.dma_channel_16bit = state[22];
    this.dma_autoinit = state[23];
    this.dma_buffer_uint8 = state[24];
    this.dma_waiting_transfer = state[25];
    this.dma_paused = state[26];
    this.sampling_rate = state[27];
    this.bytes_per_sample = state[28];
    this.e2_value = state[29];
    this.e2_count = state[30];
    this.asp_registers = state[31];
    // (state[33]: saved as mpu_read_buffer_last_value before, which was never set)
    this.mpu_read_buffer_lastvalue = state[33] | 0;
    this.irq = state[34];
    this.irq_triggered = state[35];

    this.bus.send("dac-tell-sampling-rate", this.sampling_rate);
    this.bus.send(this.dma_paused ? "dac-disable" : "dac-enable");
};
