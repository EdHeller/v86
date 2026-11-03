// A display adapter plugin (graphics_adapter: "virtio_gpu", src/graphics_adapters/virtio_gpu) on the virt machine:
// the plugin's view of the machine (the quoted handle src/graphics_adapters/machine.js reads; x86's is
// src/graphics_adapter.js), with its virtio function on a virtio-mmio slot (VirtioMmioPlugin) and its pictures on a
// display hub, which the page's screen asks for frames. No VGA, no I/O ports, no PCI (virtio-pci: plan task 4), no
// device memory yet (host visible memory: the 3D levels with hostmem).

import { DisplayHub, DisplaySink } from "../display.js";
import { VirtioMmioPlugin } from "./virtio_mmio.js";

/**
 * A layer's pixels as the screen adapter draws them (an ImageData where there is one)
 * @param {!Object} pixels quoted { data, width, height }
 */
function image_data(pixels)
{
    const data = pixels["data"];
    const bytes = new Uint8ClampedArray(data.buffer, data.byteOffset, data.byteLength);
    return typeof ImageData === "undefined" ? { data: bytes, width: pixels["width"], height: pixels["height"] } :
        new ImageData(bytes, pixels["width"], pixels["height"]);
}

/**
 * @constructor
 * @param {!Object} source quoted { vblank_period, on_vblank, render, invalidate }
 */
function PluginDisplaySource(source)
{
    this.source = source;
}
PluginDisplaySource.prototype.vblank_period = function() { return this.source["vblank_period"](); };
PluginDisplaySource.prototype.on_vblank = function() { this.source["on_vblank"](); };
PluginDisplaySource.prototype.render = function() { this.source["render"](); };
PluginDisplaySource.prototype.invalidate = function() { this.source["invalidate"](); };

/**
 * @constructor
 * @param {!Object} machine the A64Machine
 * @param {{descriptor: !Object, screen: (Object|undefined), renderer: (Object|undefined), test: (Object|undefined)}} options
 *     descriptor: the plugin's (load_graphics_adapter); screen: the page's ScreenAdapter (a DisplaySink)
 * @param {function(function(number, number): !Object): void} add_virtio takes the slot's device from a function of
 *     (slot, intid)
 */
export function A64Graphics(machine, options, add_virtio)
{
    this.machine = machine;
    this.name = options.descriptor["name"];
    this.hub = new DisplayHub(machine.bus, options.screen || new DisplaySink());
    /** @type {VirtioMmioPlugin} */
    this.transport = null;
    this.add_virtio = add_virtio;
    this.device = options.descriptor["create"](this.create_handle(), {
        "renderer": options.renderer || null,
        "test": options.test,
    });
}

A64Graphics.prototype.create_handle = function()
{
    const machine = this.machine;
    const hub = this.hub;
    const no = what => () => { throw new Error("graphics_adapter \"" + this.name + "\" on the virt machine: no " + what); };
    return {
        "name": this.name,
        "wasm_memory": machine.wasm_memory,
        "qemu_compatible": false,
        "machine_type": "virt",
        "has_vga": false,

        "register_read": no("I/O ports"),
        "register_write": no("I/O ports"),
        "register_write_consecutive": no("I/O ports"),
        "register_read_consecutive": no("I/O ports"),
        "allocate_io": no("I/O ports"),
        "register_pci": no("PCI functions"),
        "mmap_register": no("memory maps"),
        "mmap_register_range": no("memory maps"),
        "mmap_unregister_range": no("memory maps"),
        // (device memory: the 3D levels' host visible memory)
        "mmio_ram_allocate": () => -1,

        "read_physical": (address, length) => { const view = machine.ram(address, length); if(!view) throw new RangeError("not RAM"); return view.slice(); },
        "write_physical": (bytes, address) => machine.write_blob_physical(bytes, address),
        "validate_physical_range": (address, length) => !!machine.ram(address, length),
        "now": () => machine.clock.now(),
        "add_timer": timer => hub.add_timer(timer),
        "wake": () => {},

        "create_virtio": descriptor =>
        {
            if(this.transport) throw new Error("graphics_adapter \"" + this.name + "\" made its virtio function twice");
            this.add_virtio((slot, intid) => (this.transport = new VirtioMmioPlugin(machine, intid, descriptor)));
            return this.transport.handle;
        },
        "on_host_display_size": handler => machine.bus.register("display-host-size", data => handler(data[0], data[1], data[2] || 0), this),
        "in_vm86": () => false,

        "display": {
            "set_mode": graphical => hub.set_mode(graphical),
            "set_size_text": (cols, rows) => hub.set_size_text(cols, rows),
            "set_size_graphical": (width, height, buffer_width, buffer_height, bpp) =>
                hub.set_size_graphical(width, height, buffer_width, buffer_height, bpp),
            "put_char": (row, col, chr, flags, bg_color, fg_color) => hub.put_char(row, col, chr, flags, bg_color, fg_color),
            "update_cursor": (row, col) => hub.update_cursor(row, col),
            "update_cursor_scanline": (start, end, enabled) => hub.update_cursor_scanline(start, end, enabled),
            "set_font_bitmap": (height, width_9px, width_dbl, copy_8th_col, bitmap, bitmap_changed) =>
                hub.set_font_bitmap(height, width_9px, width_dbl, copy_8th_col, bitmap, bitmap_changed),
            "set_font_page": (page_a, page_b) => hub.set_font_page(page_a, page_b),
            "clear_screen": () => hub.clear_screen(),
            "clear_text_state": () => hub.clear_text_state(),
            "update_buffer": layers => hub.update_buffer(layers.map(layer => ({
                image_data: image_data(layer["pixels"]),
                screen_x: layer["screen_x"],
                screen_y: layer["screen_y"],
                buffer_x: layer["buffer_x"],
                buffer_y: layer["buffer_y"],
                buffer_width: layer["buffer_width"],
                buffer_height: layer["buffer_height"],
                frame: layer["frame"],
            }))),
            "time_since_vblank": now => hub.time_since_vblank(now),
            "add_source": source => hub.add_source(new PluginDisplaySource(source)),
            "add_timer": timer => hub.add_timer(timer),
        },
    };
};

/** @return {number} milliseconds until the display's timers need to run again */
A64Graphics.prototype.timer = function()
{
    return this.hub.timer(this.machine.clock.now());
};

/** The page's screen wants a picture */
A64Graphics.prototype.request_frame = function(full)
{
    this.hub.request_frame(full);
};

A64Graphics.prototype.reset = function()
{
    const reset = this.device["reset"];
    if(typeof reset === "function") reset();
};
