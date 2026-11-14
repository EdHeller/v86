import { LOG_DMA } from "./const.js";
import { h } from "./lib.js";
import { dbg_log } from "./log.js";

// For Types Only
import { CPU } from "./cpu.js";

/**
 * @constructor
 * @param {CPU} cpu
 */
export function DMA(cpu)
{
    /** @const @type {CPU} */
    this.cpu = cpu;

    // The registers are in the core (src/rust/cpu/dma.rs), which answers their ports. Here: the transfers,
    // the listeners for a channel being unmasked, the ports when they are reached through JavaScript.
    cpu.dma_init();
    this.unmask_listeners = [];

    // The registers in the core's memory (struct Dma in src/rust/cpu/dma.rs), as the arrays they were here
    for(const [name, type, offset] of [
        ["channel_addr", Uint16Array, 0], ["channel_addr_init", Uint16Array, 16],
        ["channel_count", Uint16Array, 32], ["channel_count_init", Uint16Array, 48],
        ["channel_page", Uint8Array, 64], ["channel_pagehi", Uint8Array, 72],
        ["channel_mask", Uint8Array, 80], ["channel_mode", Uint8Array, 88],
    ])
    {
        Object.defineProperty(this, name, {
            get: () => new type(cpu.wasm_memory.buffer, cpu.get_dma_addr() + offset, 8),
        });
    }

    var io = cpu.io;

    const ports = [
        0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0xC0, 0xC2, 0xC4, 0xC6, 0xC8, 0xCA, 0xCC, 0xCE,
        0x87, 0x83, 0x81, 0x82, 0x8F, 0x8B, 0x89, 0x8A, 0x487, 0x483, 0x481, 0x482, 0x48B, 0x489, 0x48A,
    ];
    for(const port of ports)
    {
        io.register_read(port, this, function() { return cpu.dma_port_read(port); });
        io.register_write(port, this, function(data_byte) { cpu.dma_port_write(port, data_byte); });
    }
    for(const port of [0x0F, 0xDE])
    {
        io.register_read(port, this, function() { return cpu.dma_port_read(port); });
    }
    for(const port of [0x0B, 0xD6, 0x0C, 0xD8])
    {
        io.register_write(port, this, function(data_byte) { cpu.dma_port_write(port, data_byte); });
    }

    // the mask registers: devices listen for a channel being unmasked
    for(const port of [0x0A, 0xD4, 0x0F, 0xDE])
    {
        io.register_write(port, this, function(data_byte)
        {
            const before = new Uint8Array(this.channel_mask);
            cpu.dma_port_write(port, data_byte);
            const after = this.channel_mask;
            for(let channel = 0; channel < 8; channel++)
            {
                if(before[channel] !== after[channel] && !after[channel])
                {
                    for(var i = 0; i < this.unmask_listeners.length; i++)
                    {
                        this.unmask_listeners[i].fn.call(this.unmask_listeners[i].this_value, channel);
                    }
                }
            }
        });
    }
}

DMA.prototype.get_state = function()
{
    return [
        new Uint8Array(this.channel_page),
        new Uint8Array(this.channel_pagehi),
        new Uint16Array(this.channel_addr),
        new Uint16Array(this.channel_addr_init),
        new Uint16Array(this.channel_count),
        new Uint16Array(this.channel_count_init),
        new Uint8Array(this.channel_mask),
        new Uint8Array(this.channel_mode),
        new Uint8Array(this.cpu.wasm_memory.buffer, this.cpu.get_dma_addr() + 96, 1)[0],
    ];
};

DMA.prototype.set_state = function(state)
{
    this.channel_page.set(state[0]);
    this.channel_pagehi.set(state[1]);
    this.channel_addr.set(state[2]);
    this.channel_addr_init.set(state[3]);
    this.channel_count.set(state[4]);
    this.channel_count_init.set(state[5]);
    this.channel_mask.set(state[6]);
    this.channel_mode.set(state[7]);
    new Uint8Array(this.cpu.wasm_memory.buffer, this.cpu.get_dma_addr() + 96, 1)[0] = state[8];
};

DMA.prototype.on_unmask = function(fn, this_value)
{
    this.unmask_listeners.push({
        fn: fn,
        this_value: this_value,
    });
};

DMA.prototype.do_read = function(buffer, start, len, channel, fn)
{
    var read_count = this.count_get_8bit(channel),
        addr = this.address_get_8bit(channel);

    dbg_log("DMA write channel " + channel, LOG_DMA);
    dbg_log("to " + h(addr) + " len " + h(read_count), LOG_DMA);

    if(len < read_count)
    {
        dbg_log("DMA should read more than provided: " + h(len) + " " + h(read_count), LOG_DMA);
    }

    if(start + read_count > buffer.byteLength)
    {
        dbg_log("DMA read outside of buffer", LOG_DMA);
        fn(true);
    }
    else
    {
        var cpu = this.cpu;
        this.channel_addr[channel] += read_count;

        buffer.get(start, read_count, function(data)
        {
            cpu.write_blob(data, addr);
            fn(false);
        });
    }
};

// write data, read memory
// start and len in bytes
DMA.prototype.do_write = function(buffer, start, len, channel, fn)
{
    var read_count = (this.channel_count[channel] + 1) & 0xFFFF,
        bytes_per_count = channel >= 5 ? 2 : 1,
        read_bytes = read_count * bytes_per_count,
        addr = this.address_get_8bit(channel),
        unfinished = false,
        want_more = false,
        autoinit = this.channel_mode[channel] & 0x10;

    dbg_log("DMA write channel " + channel, LOG_DMA);
    dbg_log("to " + h(addr) + " len " + h(read_bytes), LOG_DMA);

    if(len < read_bytes)
    {
        dbg_log("DMA should read more than provided", LOG_DMA);
        read_count = Math.floor(len / bytes_per_count);
        read_bytes = read_count * bytes_per_count;
        unfinished = true;
    }
    else if(len > read_bytes)
    {
        dbg_log("DMA attempted to read more than provided", LOG_DMA);
        want_more = true;
    }

    if(start + read_bytes > buffer.byteLength)
    {
        dbg_log("DMA write outside of buffer", LOG_DMA);
        fn(true);
    }
    else
    {
        this.channel_addr[channel] += read_count;
        this.channel_count[channel] -= read_count;
        // when complete, counter should underflow to 0xFFFF

        if(!unfinished && autoinit)
        {
            dbg_log("DMA autoinit", LOG_DMA);
            this.channel_addr[channel] = this.channel_addr_init[channel];
            this.channel_count[channel] = this.channel_count_init[channel];
        }

        buffer.set(start,
                this.cpu.mem8.subarray(addr, addr + read_bytes),
                () =>
                {
                    if(want_more && autoinit)
                    {
                        dbg_log("DMA continuing from start", LOG_DMA);
                        this.do_write(buffer, start + read_bytes, len - read_bytes, channel, fn);
                    }
                    else
                    {
                        fn(false);
                    }
                }
            );
    }
};

DMA.prototype.address_get_8bit = function(channel)
{
    var addr = this.channel_addr[channel];
    var page = this.channel_page[channel];

    // http://wiki.osdev.org/ISA_DMA#16_bit_issues
    if(channel >= 5)
    {
        addr = (addr << 1);
        page &= 0xFE;
    }

    addr |= page << 16;
    addr |= this.channel_pagehi[channel] << 24;

    return addr;
};

DMA.prototype.count_get_8bit = function(channel)
{
    var count = this.channel_count[channel] + 1;

    if(channel >= 5)
    {
        count *= 2;
    }

    return count;
};
