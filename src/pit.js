import { CPU } from "./cpu.js";

// In kHz
export const OSCILLATOR_FREQ = 1193.1816666; // 1.193182 MHz

/**
 * @constructor
 *
 * Programmable Interval Timer. The device is in the core (src/rust/cpu/pit.rs), which runs its timer in the main
 * loop and answers its ports without a call to JavaScript. Here: the PC speaker's messages, the ports when they
 * are reached through JavaScript (16-bit accesses, machines with more cores) and the snapshot format.
 */
export function PIT(cpu, bus)
{
    /** @const @type {CPU} */
    this.cpu = cpu;

    this.bus = bus;

    const read = port => cpu.pit_port_read(port);

    cpu.io.register_read(0x61, this, function() { return read(0x61); });
    cpu.io.register_write(0x61, this, function(data)
    {
        if(data & 1)
        {
            this.bus.send("pcspeaker-enable");
        }
        else
        {
            this.bus.send("pcspeaker-disable");
        }
    });

    cpu.io.register_read(0x40, this, function() { return read(0x40); });
    cpu.io.register_read(0x41, this, function() { return read(0x41); });
    cpu.io.register_read(0x42, this, function() { return read(0x42); });

    cpu.io.register_write(0x40, this, function(data) { cpu.pit_port_write(0x40, data); });
    cpu.io.register_write(0x41, this, function(data) { cpu.pit_port_write(0x41, data); });
    cpu.io.register_write(0x42, this, function(data) {
        cpu.pit_port_write(0x42, data);
        this.speaker_update();
    });

    cpu.io.register_write(0x43, this, function(reg_byte) {
        cpu.pit_port_write(0x43, reg_byte);
        // (counter 2's mode was set: not a latch command, not a read back)
        if((reg_byte >> 6 & 3) === 2 && (reg_byte >> 4 & 3) !== 0)
        {
            this.speaker_update();
        }
    });
}

// The device's memory in the core (struct Pit in src/rust/cpu/pit.rs)
PIT.prototype.fields = function()
{
    const buffer = this.cpu.wasm_memory.buffer, at = this.cpu.get_pit_addr();
    return {
        counter_start_time: new Float64Array(buffer, at, 3),
        counter_start_value: new Uint16Array(buffer, at + 24, 3),
        counter_latch_value: new Uint16Array(buffer, at + 30, 3),
        counter_reload: new Uint16Array(buffer, at + 36, 3),
        counter_next_low: new Uint8Array(buffer, at + 42, 4),
        counter_enabled: new Uint8Array(buffer, at + 46, 4),
        counter_mode: new Uint8Array(buffer, at + 50, 4),
        counter_read_mode: new Uint8Array(buffer, at + 54, 4),
        counter_latch: new Uint8Array(buffer, at + 58, 4),
    };
};

PIT.prototype.speaker_update = function()
{
    const f = this.fields();
    this.bus.send("pcspeaker-update", [f.counter_mode[2], f.counter_reload[2]]);
};

PIT.prototype.get_state = function()
{
    const f = this.fields();
    var state = [];

    // (copies: the views are on the core's memory)
    state[0] = new Uint8Array(f.counter_next_low);
    state[1] = new Uint8Array(f.counter_enabled);
    state[2] = new Uint8Array(f.counter_mode);
    state[3] = new Uint8Array(f.counter_read_mode);
    state[4] = new Uint8Array(f.counter_latch);
    state[5] = new Uint16Array(f.counter_latch_value);
    state[6] = new Uint16Array(f.counter_reload);
    state[7] = new Float64Array(f.counter_start_time);
    state[8] = new Uint16Array(f.counter_start_value);

    return state;
};

PIT.prototype.set_state = function(state)
{
    const f = this.fields();
    f.counter_next_low.set(state[0]);
    f.counter_enabled.set(state[1]);
    f.counter_mode.set(state[2]);
    f.counter_read_mode.set(state[3]);
    f.counter_latch.set(state[4]);
    f.counter_latch_value.set(state[5]);
    f.counter_reload.set(state[6]);
    f.counter_start_time.set(state[7]);
    f.counter_start_value.set(state[8]);
    this.cpu.timers_changed();
};

/**
 * @param {number} now
 * @param {boolean} no_irq
 * @return {number} milliseconds until the next interrupt
 */
PIT.prototype.timer = function(now, no_irq)
{
    return this.cpu.pit_timer(now, no_irq);
};
