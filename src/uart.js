// For Types Only
import { CPU } from "./cpu.js";
import { BusConnector } from "./bus.js";

/*
 * Serial ports. The device is in the core (src/rust/cpu/uart.rs), which answers its ports without a call to
 * JavaScript and collects the guest's output in a buffer. Here: the bus messages (the output bytes, taken from
 * that buffer after each slice of the main loop; input; the modem lines), the ports when they are reached
 * through JavaScript, and the snapshot format.
 */

// Modem control register
const UART_MCR_RTS = 0x2;
const UART_MCR_DTR = 0x1;

// Modem status register
const UART_MSR_DCD = 0x7; // Data Carrier Detect
const UART_MSR_RI = 0x6; // Ring Indicator
const UART_MSR_DSR = 0x5; // Data Set Ready
const UART_MSR_CTS = 0x4; // Clear To Send

// struct Uart in src/rust/cpu/uart.rs: eleven 32-bit registers, present, output_length, the output buffer
const UART_OUTPUT_LENGTH = 48;
const UART_OUTPUT = 52;

/**
 * @constructor
 * @param {CPU} cpu
 * @param {number} port
 * @param {BusConnector} bus
 */
export function UART(cpu, port, bus)
{
    /** @const @type {BusConnector} */
    this.bus = bus;

    /** @const @type {CPU} */
    this.cpu = cpu;

    this.com = { 0x3F8: 0, 0x2F8: 1, 0x3E8: 2, 0x2E8: 3 }[port] || 0;
    port = [0x3F8, 0x2F8, 0x3E8, 0x2E8][this.com];
    this.port = port;
    const com = this.com;

    cpu.uart_init(com);
    cpu.uarts.push(this);

    this.bus.register("serial" + com + "-input", function(data)
    {
        cpu.uart_receive(com, data);
    }, this);

    // Set individual modem status bits

    this.bus.register("serial" + com + "-carrier-detect-input", function(data)
    {
        cpu.uart_set_modem_status_bit(com, UART_MSR_DCD, data);
    }, this);

    this.bus.register("serial" + com + "-ring-indicator-input", function(data)
    {
        cpu.uart_set_modem_status_bit(com, UART_MSR_RI, data);
    }, this);

    this.bus.register("serial" + com + "-data-set-ready-input", function(data)
    {
        cpu.uart_set_modem_status_bit(com, UART_MSR_DSR, data);
    }, this);

    this.bus.register("serial" + com + "-clear-to-send-input", function(data)
    {
        cpu.uart_set_modem_status_bit(com, UART_MSR_CTS, data);
    }, this);

    var io = cpu.io;

    for(let reg = 0; reg < 8; reg++)
    {
        io.register_read(port | reg, this, function() { return cpu.uart_port_read(port | reg); });
    }
    for(const reg of [1, 2, 3, 5, 6, 7])
    {
        io.register_write(port | reg, this, function(out_byte) { cpu.uart_port_write(port | reg, out_byte); });
    }

    // (the core sends a data byte here only when its output buffer is full)
    io.register_write(port, this, function(out_byte)
    {
        this.drain();
        cpu.uart_port_write(port, out_byte);
    }, function(out_word)
    {
        this.drain();
        cpu.uart_port_write(port, out_word & 0xFF);
        cpu.uart_port_write(port, out_word >> 8);
    });

    // the modem control register: its lines are bus messages
    io.register_write(port | 4, this, function(out_byte)
    {
        const bits_changed = cpu.uart_port_read(port | 4) ^ out_byte;
        cpu.uart_port_write(port | 4, out_byte);
        if(bits_changed & UART_MCR_DTR)
        {
            this.bus.send("serial" + com + "-data-terminal-ready-output", !!(out_byte & UART_MCR_DTR));
        }
        if(bits_changed & UART_MCR_RTS)
        {
            this.bus.send("serial" + com + "-request-to-send-output", !!(out_byte & UART_MCR_RTS));
        }
    });
}

/** The guest's output since the last call, as serial<n>-output-byte messages */
UART.prototype.drain = function()
{
    let registers = this.registers;
    if(!registers || registers.length === 0)
    {
        // (the first call, or the memory has grown and its old buffer is detached)
        const buffer = this.cpu.wasm_memory.buffer, at = this.cpu.get_uart_addr(this.com);
        registers = this.registers = new Uint32Array(buffer, at, UART_OUTPUT >> 2);
        this.output = new Uint8Array(buffer, at + UART_OUTPUT, 4096);
    }
    const length = registers[UART_OUTPUT_LENGTH >> 2];
    if(length === 0)
    {
        return;
    }
    const bytes = this.output.slice(0, length);
    registers[UART_OUTPUT_LENGTH >> 2] = 0;
    const message = "serial" + this.com + "-output-byte";
    for(let i = 0; i < length; i++)
    {
        this.bus.send(message, bytes[i]);
    }
};

UART.prototype.get_state = function()
{
    this.drain();
    // ints, baud_rate, line_control, lsr, fifo_control, ier, iir, modem_control, modem_status, scratch_register, irq
    return Array.from(this.registers.subarray(0, 11));
};

UART.prototype.set_state = function(state)
{
    this.drain();
    for(let i = 0; i < 11; i++)
    {
        this.registers[i] = state[i];
    }
};
