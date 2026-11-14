import { dbg_log } from "./log.js";

// For Types Only
import { CPU } from "./cpu.js";
import { BusConnector } from "./bus.js";

// struct Ps2 in src/rust/cpu/ps2.rs: mouse_delta_x and _y (doubles), then these 32-bit fields
const F_ENABLE_MOUSE_STREAM = 0, F_USE_MOUSE = 1, F_HAVE_MOUSE = 2, F_MOUSE_CLICKS = 3, F_HAVE_KEYBOARD = 4,
    F_ENABLE_KEYBOARD_STREAM = 5, F_NEXT_IS_MOUSE_COMMAND = 6, F_NEXT_READ_SAMPLE = 7, F_NEXT_READ_LED = 8,
    F_NEXT_HANDLE_SCAN_CODE_SET = 9, F_NEXT_READ_RATE = 10, F_NEXT_READ_RESOLUTION = 11, F_LAST_PORT60_BYTE = 12,
    F_SAMPLE_RATE = 13, F_RESOLUTION = 14, F_SCALING2 = 15, F_COMMAND_REGISTER = 16, F_READ_OUTPUT_REGISTER = 17,
    F_READ_COMMAND_REGISTER = 18, F_CONTROLLER_OUTPUT_PORT = 19, F_READ_CONTROLLER_OUTPUT_PORT = 20, F_MOUSE_ID = 21,
    F_MOUSE_DETECT_STATE = 22, F_MOUSE_RESET_WORKAROUND = 23, F_MOUSE_ENABLE_EVENT = 29;

/**
 * The PS/2 controller, keyboard and mouse. The device is in the core (src/rust/cpu/ps2.rs), which answers ports
 * 0x60 and 0x64. Here: the bus messages, the reboot the controller can ask for, the timer for the rest of a
 * large mouse movement, the ports when they are reached through JavaScript, and the snapshot format.
 * @constructor
 * @param {CPU} cpu
 * @param {BusConnector} bus
 * @param {boolean=} hide_mouse_while_cli
 */
export function PS2(cpu, bus, hide_mouse_while_cli)
{
    /** TempleOS's debugger polls the status port with interrupts off and reads mouse bytes as keys */
    this.hide_mouse_while_cli = !!hide_mouse_while_cli;

    /** @const @type {CPU} */
    this.cpu = cpu;

    /** @const @type {BusConnector} */
    this.bus = bus;

    this.mouse_flush_scheduled = false;

    this.reset();

    this.bus.register("keyboard-code", function(code)
    {
        cpu.ps2_kbd_send_code(code);
    }, this);

    this.bus.register("mouse-click", function(data)
    {
        cpu.ps2_mouse_send_click(data[0], data[1], data[2]);
    }, this);

    this.bus.register("mouse-delta", function(data)
    {
        this.schedule_flush(cpu.ps2_mouse_send_delta(data[0], data[1]));
    }, this);

    this.bus.register("mouse-wheel", function(data)
    {
        cpu.ps2_mouse_wheel(data[0], data[1]);
    }, this);

    cpu.io.register_read(0x60, this, function() { return cpu.ps2_port_read(0x60); });
    cpu.io.register_read(0x64, this, function() { return cpu.ps2_port_read(0x64); });

    // (the core sends a write here when it is a command to the mouse: two of them turn the mouse on)
    cpu.io.register_write(0x60, this, function(write_byte)
    {
        cpu.ps2_port_write(0x60, write_byte);
        const fields = this.fields();
        if(fields[F_MOUSE_ENABLE_EVENT])
        {
            fields[F_MOUSE_ENABLE_EVENT] = 0;
            this.bus.send("mouse-enable", true);
        }
    });
    cpu.io.register_write(0x64, this, function(write_byte)
    {
        if(write_byte === 0xFE)
        {
            dbg_log("CPU reboot via PS2");
            this.cpu.reboot_internal();
        }
        else
        {
            cpu.ps2_port_write(0x64, write_byte);
        }
    });
}

PS2.prototype.reset = function()
{
    this.cpu.ps2_init(this.hide_mouse_while_cli);
};

/** @return {!Int32Array} the device's 32-bit fields in the core's memory */
PS2.prototype.fields = function()
{
    return new Int32Array(this.cpu.wasm_memory.buffer, this.cpu.get_ps2_addr() + 16, 31);
};

/** The rest of a large mouse movement is sent when the guest has read the packets before it */
PS2.prototype.schedule_flush = function(movement_left)
{
    if(movement_left && !this.mouse_flush_scheduled)
    {
        this.mouse_flush_scheduled = true;
        setTimeout(() =>
        {
            this.mouse_flush_scheduled = false;
            this.schedule_flush(this.cpu.ps2_mouse_flush());
        }, 10);
    }
};

PS2.prototype.get_state = function()
{
    const f = this.fields();
    const deltas = new Float64Array(this.cpu.wasm_memory.buffer, this.cpu.get_ps2_addr(), 2);
    var state = [];

    state[0] = !!f[F_ENABLE_MOUSE_STREAM];
    state[1] = !!f[F_USE_MOUSE];
    state[2] = !!f[F_HAVE_MOUSE];
    state[3] = deltas[0];
    state[4] = deltas[1];
    state[5] = f[F_MOUSE_CLICKS];
    state[6] = !!f[F_HAVE_KEYBOARD];
    state[7] = !!f[F_ENABLE_KEYBOARD_STREAM];
    state[8] = !!f[F_NEXT_IS_MOUSE_COMMAND];
    state[9] = !!f[F_NEXT_READ_SAMPLE];
    state[10] = !!f[F_NEXT_READ_LED];
    state[11] = !!f[F_NEXT_HANDLE_SCAN_CODE_SET];
    state[12] = !!f[F_NEXT_READ_RATE];
    state[13] = !!f[F_NEXT_READ_RESOLUTION];
    state[15] = f[F_LAST_PORT60_BYTE];
    state[16] = f[F_SAMPLE_RATE];
    state[17] = f[F_RESOLUTION];
    state[18] = !!f[F_SCALING2];
    state[20] = f[F_COMMAND_REGISTER];
    state[21] = !!f[F_READ_OUTPUT_REGISTER];
    state[22] = !!f[F_READ_COMMAND_REGISTER];
    state[23] = f[F_CONTROLLER_OUTPUT_PORT];
    state[24] = !!f[F_READ_CONTROLLER_OUTPUT_PORT];
    state[25] = f[F_MOUSE_ID];
    state[26] = f[F_MOUSE_DETECT_STATE];
    state[27] = !!f[F_MOUSE_RESET_WORKAROUND];

    return state;
};

PS2.prototype.set_state = function(state)
{
    const f = this.fields();
    const deltas = new Float64Array(this.cpu.wasm_memory.buffer, this.cpu.get_ps2_addr(), 2);

    f[F_ENABLE_MOUSE_STREAM] = +state[0];
    f[F_USE_MOUSE] = +state[1];
    f[F_HAVE_MOUSE] = +state[2];
    deltas[0] = state[3];
    deltas[1] = state[4];
    f[F_MOUSE_CLICKS] = state[5];
    f[F_HAVE_KEYBOARD] = +state[6];
    f[F_ENABLE_KEYBOARD_STREAM] = +state[7];
    f[F_NEXT_IS_MOUSE_COMMAND] = +state[8];
    f[F_NEXT_READ_SAMPLE] = +state[9];
    f[F_NEXT_READ_LED] = +state[10];
    f[F_NEXT_HANDLE_SCAN_CODE_SET] = +state[11];
    f[F_NEXT_READ_RATE] = +state[12];
    f[F_NEXT_READ_RESOLUTION] = +state[13];
    f[F_LAST_PORT60_BYTE] = state[15];
    f[F_SAMPLE_RATE] = state[16];
    f[F_RESOLUTION] = state[17];
    f[F_SCALING2] = +state[18];
    f[F_COMMAND_REGISTER] = state[20];
    f[F_READ_OUTPUT_REGISTER] = +state[21];
    f[F_READ_COMMAND_REGISTER] = +state[22];
    f[F_CONTROLLER_OUTPUT_PORT] = state[23];
    f[F_READ_CONTROLLER_OUTPUT_PORT] = +state[24];
    f[F_MOUSE_ID] = state[25] || 0;
    f[F_MOUSE_DETECT_STATE] = state[26] || 0;
    f[F_MOUSE_RESET_WORKAROUND] = +(state[27] || false);

    this.cpu.ps2_state_restored();

    this.bus.send("mouse-enable", !!f[F_USE_MOUSE]);
};
