// Display adapters (graphics_adapter option) as plugins, from v64: "bochs_vga"
// (v64's VGA core), "vmware_svga" (VMware SVGA II; 2D without a renderer, 3D
// only with a WebGPU renderer the page provides). Without the option the
// machine keeps its own VGA (src/vga.js). The plugin side is
// src/graphics_adapters/machine.js.

import { LOG_PCI, FLAG_VM } from "./const.js";
import { dbg_log } from "./log.js";
import { device_handle, allocate_io_ports, plugin_pci_device } from "./device_plugin.js";
import { create_adapter_virtio_device } from "./virtio_devices.js";

// For Types Only
import { CPU } from "./cpu.js";

/** @const */
export const GRAPHICS_PLUGINS = { registry: "V86GraphicsAdapters", option: "graphics_adapter", dir: "graphics_adapters", tag: "graphics_adapter" };

/* global __dirname */

/** The bundle's own URL, where a classic script in a page knows it */
const BUNDLE_URL = typeof document !== "undefined" && document.currentScript && document.currentScript.src || "";

/**
 * @param {string} path
 * @return {!Promise<*>}
 */
async function import_plugin(path)
{
    if(typeof require === "function" && !/^[a-z]+:/i.test(path))
    {
        return require(path);
    }
    return import(path);
}

/** The VGA's PCI slot */
const VGA_PCI_ID = 0x12 << 3;

/**
 * Load a display adapter plugin (from the source tree: its entry module),
 * unless the page already did
 * @param {string} name
 * @param {string|undefined} path graphics_adapter_path
 * @return {!Promise<!Object>}
 */
export async function load_graphics_adapter(name, path)
{
    const registry = () => globalThis[GRAPHICS_PLUGINS.registry];
    const file = "v86-" + name.replace(/_/g, "-") + ".js";
    if(!registry() || !registry()[name])
    {
        if(path)
        {
            await import_plugin(path);
        }
        else
        {
            // from the source tree its entry module, from the bundle the
            // plugin file beside it
            try
            {
                await import("./graphics_adapters/" + name + "/plugin.js");
            }
            catch(e)
            {
                await import_plugin(BUNDLE_URL ? new URL(file, BUNDLE_URL).href :
                    typeof __dirname === "string" ? __dirname + "/" + file : "build/" + file);
            }
        }
    }
    const descriptor = registry() && registry()[name];
    if(!descriptor || typeof descriptor["create"] !== "function")
    {
        throw new Error("graphics_adapter \"" + name + "\": no such plugin");
    }
    return descriptor;
}

/**
 * Make the PCI ROM header of a SeaBIOS-built VGA BIOS name the adapter's
 * device: SeaBIOS only runs a PCI ROM whose vendor and device match
 * @param {!Uint8Array} rom
 * @param {number} vendor
 * @param {number} device
 */
export function patch_vga_bios_ids(rom, vendor, device)
{
    if(rom.length < 0x1C || rom[0] !== 0x55 || rom[1] !== 0xAA) return;
    const pcir = rom[0x18] | rom[0x19] << 8;
    if(pcir + 8 > rom.length ||
        rom[pcir] !== 0x50 || rom[pcir + 1] !== 0x43 || rom[pcir + 2] !== 0x49 || rom[pcir + 3] !== 0x52)
    {
        return;
    }
    const size = Math.min(rom[2] * 512, rom.length);
    rom[pcir + 4] = vendor & 0xFF;
    rom[pcir + 5] = vendor >> 8;
    rom[pcir + 6] = device & 0xFF;
    rom[pcir + 7] = device >> 8;
    rom[6] = 0;
    let sum = 0;
    for(let i = 0; i < size; i++) sum += rom[i];
    rom[6] = -sum & 0xFF;
    dbg_log("VGA BIOS patched for PCI device " + vendor.toString(16) + ":" + device.toString(16), LOG_PCI);
}

/**
 * A layer's pixels as the screen adapter draws them (an ImageData where
 * there is one)
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
 * @param {CPU} cpu
 * @param {!Object} descriptor
 * @param {!Object} options { vram_size, renderer }
 */
export function GraphicsAdapter(cpu, descriptor, options)
{
    this.cpu = cpu;
    this.name = descriptor["name"];
    this.pci_id = VGA_PCI_ID;
    this.pci_device = null;

    this.device = descriptor["create"](this.create_handle(), {
        "vram_size": options.vram_size,
        "renderer": options.renderer || null,
    });
}

GraphicsAdapter.prototype.create_handle = function()
{
    const cpu = this.cpu;
    const hub = cpu.devices.display;
    const x = cpu.wm.exports;
    return Object.assign(device_handle(cpu, this), {
        "qemu_compatible": false,
        "has_vga": true,

        "mmio_ram_allocate": size => x["mmio_ram_allocate"](size),
        "mmio_ram_backing": region => x["mmio_ram_backing"](region) >>> 0,
        "mmio_ram_map": (region, address) => x["mmio_ram_map"](region, address),
        "mmio_ram_read8": (region, offset) => x["mmio_ram_read8"](region, offset),
        "mmio_ram_write8": (region, offset, value) => x["mmio_ram_write8"](region, offset, value),
        "mmio_ram_mark_dirty": region => x["mmio_ram_mark_dirty"](region),
        "mmio_ram_take_dirty": region => x["mmio_ram_take_dirty"](region) >>> 0,
        "mmio_ram_unmap": region => x["mmio_ram_unmap"](region),
        "mmio_ram_allocate_pixels": (region, pixels) => x["mmio_ram_allocate_pixels"](region, pixels) >>> 0,
        "mmio_ram_fill_pixels": (region, bpp, offset) => x["mmio_ram_fill_pixels"](region, bpp, offset),
        "dirty_min_offset": () => cpu.svga_dirty_bitmap_min_offset[0],
        "dirty_max_offset": () => cpu.svga_dirty_bitmap_max_offset[0],

        // a virtio function in the adapter's slot (virtio-gpu as virtio-vga; the
        // descriptor as in v64's virtio_devices, see create_adapter_virtio_device)
        "create_virtio": descriptor => this.create_virtio(descriptor),
        "on_host_display_size": handler =>
            cpu.bus.register("display-host-size", data => handler(data[0], data[1], data[2] || 0), this),
        "in_vm86": () => !!(cpu.flags[0] & FLAG_VM),

        "display": {
            "set_mode": graphical => hub.set_mode(graphical),
            "set_size_text": (cols, rows) => hub.set_size_text(cols, rows),
            "set_size_graphical": (width, height, buffer_width, buffer_height, bpp) =>
                hub.set_size_graphical(width, height, buffer_width, buffer_height, bpp),
            "put_char": (row, col, chr, flags, bg_color, fg_color) =>
                hub.put_char(row, col, chr, flags, bg_color, fg_color),
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
            }))),
            "time_since_vblank": now => hub.time_since_vblank(now),
            "add_source": source => hub.add_source(new PluginDisplaySource(source)),
            "add_timer": timer => hub.add_timer(timer),
        },
    });
};

GraphicsAdapter.prototype.allocate_io = function(size)
{
    return allocate_io_ports(this.cpu, size, "graphics_adapter \"" + this.name + "\"");
};

GraphicsAdapter.prototype.register_pci = function(description)
{
    if(this.pci_device)
    {
        throw new Error("graphics_adapter \"" + this.name + "\" registered its PCI function twice");
    }
    this.pci_device = plugin_pci_device(this, description);
    return this.cpu.devices.pci.register_device(this.pci_device);
};

GraphicsAdapter.prototype.create_virtio = function(descriptor)
{
    if(this.pci_device)
    {
        throw new Error("graphics_adapter \"" + this.name + "\" registered its PCI function twice");
    }
    const device = create_adapter_virtio_device(this.cpu, descriptor, this.pci_id);
    this.pci_device = device.virtio;
    return device.handle;
};

GraphicsAdapter.prototype.vga = function()
{
    return this.device["vga"];
};

GraphicsAdapter.prototype.reset = function()
{
    const reset = this.device["reset"];
    if(typeof reset === "function") reset();
};

GraphicsAdapter.prototype.get_state = function()
{
    return [GRAPHICS_PLUGINS.tag, 1, this.name, this.device["get_state"]()];
};

GraphicsAdapter.prototype.set_state = function(state)
{
    if(!Array.isArray(state) || state[0] !== GRAPHICS_PLUGINS.tag || state[2] !== this.name)
    {
        throw new Error("The snapshot is from a machine with another graphics_adapter");
    }
    this.device["set_state"](state[3]);
};
