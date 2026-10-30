// Device plugins on this fork's CPU (the plugin side is src/device_machine.js,
// from v64): the handle a plugin works with. Plugins use quoted names only.
// Only the i440FX-style machine is supported: PCI I/O BARs, memory BARs fixed
// by the device (see pci.js), no MSI.

import { LOG_PCI } from "./const.js";
import { dbg_log } from "./log.js";
import { v86 } from "./main.js";

const IO_POOL_START = 0x9000;
const IO_POOL_END = 0xA000;

/** @const */
export const AUDIO_PLUGINS = { registry: "V86AudioDevices", option: "audio_device", dir: "audio_devices", tag: "audio_device" };
/** @const */
export const TPM_PLUGINS = { registry: "V86TpmDevices", option: "tpm", dir: "tpm", tag: "tpm" };
/** @const */
export const USB_PLUGINS = { registry: "V86UsbControllers", option: "usb", dir: "usb", tag: "usb", entry: "./usb/plugin.js" };

/** A free window of I/O ports, aligned to its size */
export function allocate_io_ports(cpu, size, label)
{
    const ports = cpu.io.ports;
    for(let base = IO_POOL_START; base + size <= IO_POOL_END; base += size)
    {
        let free = true;
        for(let port = base; port < base + size && free; port++)
        {
            free = !ports[port].device;
        }
        if(free) return base;
    }
    throw new Error(label + ": no free I/O ports");
}

/** The registered descriptor of a statically linked plugin */
export function registered_plugin(kind, name)
{
    const registry = globalThis[kind.registry];
    const descriptor = registry && registry[name];
    if(!descriptor || descriptor["name"] !== name || typeof descriptor["create"] !== "function")
    {
        throw new Error(kind.option + " \"" + name + "\": no such plugin");
    }
    return descriptor;
}

function byte_target(target)
{
    return {
        byteLength: target.length,
        set: (start, data, done) =>
        {
            target.set(data, start);
            done();
        },
    };
}

/**
 * What every plugin works with: ports, memory maps, guest memory, the clock,
 * timers, IRQs and the PCI function.
 */
export function device_handle(cpu, owner)
{
    const io = cpu.io;
    const pci = () => cpu.devices.pci;
    return {
        "name": owner.name,
        "wasm_memory": cpu.wasm_memory,
        "qemu_compatible": false,
        "machine_type": "i440fx",

        "register_read": (port, r8, r16, r32) => io.register_read(port, owner, r8, r16, r32),
        "register_write": (port, w8, w16, w32) => io.register_write(port, owner, w8, w16, w32),
        "register_write_consecutive": (port, w8_1, w8_2, w8_3, w8_4) =>
        {
            if(w8_3) io.register_write_consecutive(port, owner, w8_1, w8_2, w8_3, w8_4);
            else io.register_write_consecutive(port, owner, w8_1, w8_2);
        },
        "register_read_consecutive": (port, r8_1, r8_2, r8_3, r8_4) =>
        {
            if(r8_3) io.register_read_consecutive(port, owner, r8_1, r8_2, r8_3, r8_4);
            else io.register_read_consecutive(port, owner, r8_1, r8_2);
        },
        "mmap_register": (address, size, r8, w8, r32, w32) => io.mmap_register(address, size, r8, w8, r32, w32),
        "mmap_register_range": (address, size, r8, w8, r32, w32, w16) => io.mmap_register_range(address, size, owner, r8, w8, r32, w32, w16),
        "mmap_unregister_range": (address, size) => io.mmap_unregister_range(address, size, owner),
        "allocate_io": size => owner.allocate_io ? owner.allocate_io(size) : allocate_io_ports(cpu, size, owner.name),

        "register_pci": description => owner.register_pci(description),
        "pci_id": () => owner.pci_id,
        "raise_irq": () => pci().raise_irq(owner.pci_id),
        "lower_irq": () => pci().lower_irq(owner.pci_id),
        "memory_decoded": (base, size) => (pci().device_spaces[owner.pci_id][1] & 2) !== 0,
        "is_bus_master": () => (pci().device_spaces[owner.pci_id][1] & 4) !== 0,
        "msi_capability": () => [],
        "msi_config_written": () => {},
        "msi_enabled": () => false,
        "msi_notify": () => {},
        "pci_config": () => new Uint8Array(pci().device_spaces[owner.pci_id].buffer),

        "isa_raise_irq": irq => cpu.device_raise_irq(irq),
        "isa_lower_irq": irq => cpu.device_lower_irq(irq),
        "dma_on_unmask": listener => cpu.devices.dma.on_unmask(listener, owner),
        "dma_masked": channel => cpu.devices.dma.channel_mask[channel] !== 0,
        "dma_write": (channel, target, start, length, callback) =>
            cpu.devices.dma.do_write(byte_target(target), start, length, channel, callback),

        "read_physical": (address, length) => cpu.read_blob(address, length).slice(),
        "write_physical": (bytes, address) => cpu.write_blob(bytes, address),
        "read16_physical": address => cpu.mem8[address] | cpu.mem8[address + 1] << 8,
        "read32_physical": address => cpu.mem8[address] | cpu.mem8[address + 1] << 8 | cpu.mem8[address + 2] << 16 | cpu.mem8[address + 3] << 24,
        "write16_physical": (address, value) => cpu.write_blob([value, value >> 8], address),
        "write32_physical": (address, value) => cpu.write_blob([value, value >> 8, value >> 16, value >> 24], address),
        "validate_physical_range": (address, length) => address + length <= cpu.memory_size[0],

        "now": () => v86.microtick(),
        "add_timer": timer => cpu.add_device_timer(timer),
        "wake": () => cpu.stop_idling(),
        "block_backend": buffer => block_handle(buffer),
        "function": number => function_handle(cpu, owner, number),
    };
}

/** Another function of a plugin's PCI device (EHCI's companions) */
function function_handle(cpu, owner, number)
{
    const sub = {
        name: owner.name + "." + number,
        pci_id: owner.pci_id & ~7 | number,
        pci_device: null,
        register_pci: description =>
        {
            if(sub.pci_device) throw new Error(owner.name + ": PCI function " + number + " registered twice");
            sub.pci_device = plugin_pci_device(sub, description);
            sub.pci_device.reset = () => owner.reset();
            dbg_log(owner.name + " function " + number + " at PCI " + sub.pci_id.toString(16), LOG_PCI);
            return cpu.devices.pci.register_device(sub.pci_device);
        },
    };
    return device_handle(cpu, sub);
}

/** A disk image's block backend over a buffer of src/buffer.js */
export function block_handle(buffer)
{
    return {
        "byte_length": buffer.byteLength,
        "read": (start, length, callback) => buffer.get(start, length, callback),
        "write": (start, data, callback) => buffer.set(start, data, callback),
        "flush": callback => callback(),
        "reset": () => {},
        "drop": () => {},
    };
}

export function plugin_pci_device(owner, description)
{
    const device = {
        name: owner.name,
        pci_id: owner.pci_id,
        pci_space: description["pci_space"],
        pci_bars: description["pci_bars"].map(bar => bar && {
            size: bar["size"],
            on_move: bar["on_move"],
        }),
        pci_rom_size: description["pci_rom_size"],
        pci_rom_address: description["pci_rom_address"],
    };
    for(const hook of ["on_command_change", "on_config_restore", "on_upstream_change", "on_config_write"])
    {
        if(typeof description[hook] === "function") device[hook] = description[hook];
    }
    return device;
}

/**
 * @constructor
 * @param {Object} cpu
 * @param {Object} kind
 * @param {Object} descriptor
 * @param {number} pci_id
 * @param {Object} extra
 * @param {Object} options
 */
export function PluginDevice(cpu, kind, descriptor, pci_id, extra, options)
{
    this.cpu = cpu;
    this.kind = kind;
    this.name = descriptor["name"];
    this.pci_id = pci_id;
    this.pci_device = null;

    const handle = Object.assign(device_handle(cpu, this), extra);
    this.device = descriptor["create"](handle, options);
}

PluginDevice.prototype.register_pci = function(description)
{
    if(this.pci_device)
    {
        throw new Error(this.kind.option + " \"" + this.name + "\" registered its PCI function twice");
    }
    this.pci_device = plugin_pci_device(this, description);
    this.pci_device.reset = () => this.reset();
    dbg_log(this.kind.option + " \"" + this.name + "\" at PCI " + this.pci_id.toString(16), LOG_PCI);
    return this.cpu.devices.pci.register_device(this.pci_device);
};

PluginDevice.prototype.reset = function()
{
    const reset = this.device["reset"];
    if(typeof reset === "function") reset();
};

PluginDevice.prototype.get_state = function()
{
    return this.device["get_state"]();
};

PluginDevice.prototype.set_state = function(state)
{
    this.device["set_state"](state);
};
