// The virt machine's generic PCIe host bridge (docs/arm64.md, "PCIe"), as QEMU 9.2 virt's gpex with highmem-ecam:
// configuration through the ECAM at 0x40_1000_0000 (bus 0 only, function 0 of each slot), memory BARs decoded in
// the 32-bit window (0x1000_0000, 751 MiB) and the 64-bit one (0x80_0000_0000, 512 GiB) while the function's
// memory space is enabled, INTx: pin A of slot s is SPI 3 + s % 4, level-triggered, shared. 00:00.0 is QEMU's host
// bridge (1B36:0008). No I/O BARs (the I/O window reads as all ones), no MSI (the GIC has no ITS).
//
// A function is { config: Uint8Array(4096) (its power-on configuration), bars: [{ size, bits64, prefetch,
// read(offset, width), write(offset, width, value) }] by BAR number }.

import {
    VIRT_PCIE_ECAM_HIGH, VIRT_PCIE_ECAM_SIZE, VIRT_PCIE_MMIO, VIRT_PCIE_MMIO_HIGH, VIRT_PCIE_MMIO_HIGH_SIZE, VIRT_PCIE_MMIO_SIZE,
    VIRT_PCIE_PIO, VIRT_PCIE_PIO_SIZE,
} from "./platform.js";

const ALL_ONES = width => width === 8 ? 0xFFFFFFFFFFFFFFFFn : 2 ** (8 * width) - 1;

/**
 * A function's configuration header (type 0)
 * @param {{vendor: number, device: number, class_code: number, revision: (number|undefined),
 *          subsystem_vendor: (number|undefined), subsystem: (number|undefined), pin: (number|undefined)}} id
 * @return {!Uint8Array}
 */
export function pci_config_header(id)
{
    const config = new Uint8Array(4096);
    const view = new DataView(config.buffer);
    view.setUint16(0x00, id.vendor, true);
    view.setUint16(0x02, id.device, true);
    config[0x08] = id.revision || 0;
    config[0x09] = id.class_code & 0xFF;
    config[0x0A] = id.class_code >> 8 & 0xFF;
    config[0x0B] = id.class_code >> 16 & 0xFF;
    view.setUint16(0x2C, id.subsystem_vendor || 0, true);
    view.setUint16(0x2E, id.subsystem || 0, true);
    config[0x3D] = id.pin || 0;
    return config;
}

/**
 * @constructor
 * @param {Object} machine the A64Machine (its io and set_irq)
 */
export function PcieHost(machine)
{
    this.machine = machine;
    /** by slot: { config, initial, bars } */
    this.functions = new Array(32).fill(null);
    /** how many functions assert each INTx line (4 SPIs) */
    this.intx = [0, 0, 0, 0];
    /** by slot: whether the function asserts its line */
    this.asserted = new Array(32).fill(false);
    // (00:00.0: QEMU's "QEMU PCIe Host bridge")
    this.add(0, { config: pci_config_header({ vendor: 0x1B36, device: 0x0008, class_code: 0x060000 }), bars: [] });
    const io = machine.io;
    io.register_mmio(VIRT_PCIE_ECAM_HIGH, VIRT_PCIE_ECAM_SIZE, {
        name: "pcie-ecam",
        read: (offset, width) => this.config_read(offset, width),
        write: (offset, width, value) => { this.config_write(offset, width, value); },
    });
    const window = base => ({
        name: "pcie-window",
        read: (offset, width) => {
            const hit = this.decode(base + offset, width);
            return hit ? hit.bar.read(hit.offset, width) : ALL_ONES(width);
        },
        write: (offset, width, value) => {
            const hit = this.decode(base + offset, width);
            if(hit) hit.bar.write(hit.offset, width, value);
        },
    });
    io.register_mmio(VIRT_PCIE_MMIO, VIRT_PCIE_MMIO_SIZE, window(VIRT_PCIE_MMIO));
    const high = VIRT_PCIE_MMIO_HIGH[1] * 0x100000000 + VIRT_PCIE_MMIO_HIGH[0];
    io.register_mmio(VIRT_PCIE_MMIO_HIGH, VIRT_PCIE_MMIO_HIGH_SIZE, window(high));
    io.register_mmio(VIRT_PCIE_PIO, VIRT_PCIE_PIO_SIZE, { name: "pcie-io", read: (offset, width) => ALL_ONES(width), write: () => {} });
}

/**
 * Put a function in `slot`
 * @param {number} slot
 * @param {{config: !Uint8Array, bars: !Array}} fn
 */
PcieHost.prototype.add = function(slot, fn)
{
    if(this.functions[slot]) throw new Error("PCIe: slot " + slot + " is taken");
    // (the BARs' type bits in the power-on configuration)
    fn.bars.forEach((bar, i) => {
        if(!bar) return;
        const flags = (bar.bits64 ? 4 : 0) | (bar.prefetch ? 8 : 0);
        new DataView(fn.config.buffer).setUint32(0x10 + 4 * i, flags, true);
    });
    fn.initial = fn.config.slice();
    this.functions[slot] = fn;
};

/** A machine reset: every function's power-on configuration, the lines low */
PcieHost.prototype.reset = function()
{
    for(const fn of this.functions)
    {
        if(fn) fn.config.set(fn.initial);
    }
    for(let slot = 0; slot < 32; slot++) this.set_intx(slot, false);
};

/** INTA of `slot`, as its function drives it */
PcieHost.prototype.set_intx = function(slot, level)
{
    if(this.asserted[slot] === !!level) return;
    this.asserted[slot] = !!level;
    const line = slot % 4;
    const before = this.intx[line] > 0;
    this.intx[line] += level ? 1 : -1;
    const after = this.intx[line] > 0;
    if(before !== after) this.machine.set_irq(32 + 3 + line, after);
};

/** The function at an ECAM offset (bus 0, function 0) and the register */
PcieHost.prototype.target = function(offset)
{
    if(offset >= 1 << 20 || offset >> 12 & 7) return null;
    const fn = this.functions[offset >> 15 & 31];
    return fn ? { fn, reg: offset & 0xFFF } : null;
};

PcieHost.prototype.config_read = function(offset, width)
{
    const t = this.target(offset);
    if(!t) return ALL_ONES(width);
    let value = 0n;
    for(let i = width - 1; i >= 0; i--) value = value << 8n | BigInt(t.fn.config[t.reg + i] || 0);
    return width === 8 ? value : Number(value);
};

PcieHost.prototype.config_write = function(offset, width, value)
{
    const t = this.target(offset);
    if(!t) return;
    let v = BigInt.asUintN(64, BigInt(value));
    for(let i = 0; i < width; i++, v >>= 8n) this.write_byte(t.fn, t.reg + i, Number(v & 0xFFn));
};

/** A byte of configuration written: the command register, the BARs, the interrupt line; the rest is read-only */
PcieHost.prototype.write_byte = function(fn, reg, byte)
{
    const config = fn.config;
    if(reg === 0x04) config[reg] = byte & 0x46;                         // I/O, memory, bus master; parity error response
    else if(reg === 0x05) config[reg] = byte & 0x05;                    // SERR#, interrupt disable
    else if(reg === 0x0C || reg === 0x0D || reg === 0x3C) config[reg] = byte;
    else if(reg >= 0x10 && reg < 0x28)
    {
        const index = reg - 0x10 >> 2;
        const lower = fn.bars[index] ? index : fn.bars[index - 1] && fn.bars[index - 1].bits64 ? index - 1 : -1;
        if(lower < 0) return;
        const bar = fn.bars[lower];
        const at = 0x10 + 4 * lower;
        const view = new DataView(config.buffer);
        // (the address bits the size leaves writable, the type bits fixed)
        const size_mask = BigInt(bar.size) - 1n;
        let address = BigInt(view.getUint32(at, true)) | (bar.bits64 ? BigInt(view.getUint32(at + 4, true)) << 32n : 0n);
        const shift = BigInt(8 * (reg - at));
        address = address & ~(0xFFn << shift) | BigInt(byte) << shift;
        const flags = BigInt((bar.bits64 ? 4 : 0) | (bar.prefetch ? 8 : 0));
        address = address & ~size_mask & (bar.bits64 ? 0xFFFFFFFFFFFFFFFFn : 0xFFFFFFFFn) | flags;
        view.setUint32(at, Number(address & 0xFFFFFFFFn), true);
        if(bar.bits64) view.setUint32(at + 4, Number(address >> 32n), true);
    }
};

/** The BAR of an enabled function that holds [address, address + width) */
PcieHost.prototype.decode = function(address, width)
{
    for(const fn of this.functions)
    {
        if(!fn || !(fn.config[4] & 2)) continue;
        const view = new DataView(fn.config.buffer);
        for(let i = 0; i < fn.bars.length; i++)
        {
            const bar = fn.bars[i];
            if(!bar) continue;
            const base = (view.getUint32(0x10 + 4 * i, true) & ~0xF >>> 0) + (bar.bits64 ? view.getUint32(0x14 + 4 * i, true) * 0x100000000 : 0);
            if(base && address >= base && address + width <= base + bar.size) return { bar, offset: address - base };
        }
    }
    return null;
};

/**
 * virtio over PCI (virtio 1.2 section 4.1) for a device of src/arm/virtio_mmio.js: a non-transitional function as
 * QEMU's virtio-*-pci without MSI-X (1AF4:1040 + id, subsystem 1AF4:1100), its structures in BAR 4 (64 bits,
 * prefetchable, 16 KiB): common configuration at 0, ISR at 0x1000, the device's configuration at 0x2000,
 * notifications at 0x3000 (4 bytes a queue). The registers map onto the device's virtio-mmio ones.
 * @param {PcieHost} host
 * @param {number} slot
 * @param {function(Object): Object} create makes the device, given the machine its interrupt goes through
 * @param {number} class_code
 * @return {Object} the device
 */
export function add_virtio_pci(host, slot, create, class_code)
{
    const machine = Object.create(host.machine);
    machine.set_irq = (intid, level) => host.set_intx(slot, level);
    const device = create(machine);
    const config = pci_config_header({
        vendor: 0x1AF4, device: 0x1040 + device.device_id, class_code, revision: 1, subsystem_vendor: 0x1AF4, subsystem: 0x1100, pin: 1,
    });
    config[0x06] = 0x10;                                                // the capabilities list
    config[0x34] = 0x40;
    // (vendor capabilities: type, BAR 4, offset, length; the notify one with its multiplier)
    const caps = [[1, 0x0000], [3, 0x1000], [4, 0x2000], [2, 0x3000]];
    caps.forEach(([type, offset], i) => {
        const at = 0x40 + 0x14 * i;
        const view = new DataView(config.buffer);
        config[at] = 0x09;
        config[at + 1] = i + 1 < caps.length ? at + 0x14 : 0;
        config[at + 2] = type === 2 ? 20 : 16;
        config[at + 3] = type;
        config[at + 4] = 4;
        view.setUint32(at + 8, offset, true);
        view.setUint32(at + 12, 0x1000, true);
        if(type === 2) view.setUint32(at + 16, 4, true);
    });
    const common_read = (offset, width) => {
        const q = device.queues[device.queue_sel];
        switch(offset)
        {
            case 0x00: return device.device_features_sel;
            case 0x04: return device.read(0x010, 4);
            case 0x08: return device.driver_features_sel;
            case 0x0C: return Number(BigInt.asUintN(32, device.driver_features >> BigInt(32 * device.driver_features_sel)));
            case 0x10: return 0xFFFF;
            case 0x12: return device.queue_count;
            case 0x14: return width === 2 ? device.status | device.config_generation << 8 : device.status;
            case 0x15: return device.config_generation;
            case 0x16: return device.queue_sel;
            case 0x18: return q ? q.num : 0;
            case 0x1A: return 0xFFFF;
            case 0x1C: return q && q.ready ? 1 : 0;
            case 0x1E: return device.queue_sel;
            case 0x20: return q ? q.desc % 0x100000000 : 0;
            case 0x24: return q ? Math.floor(q.desc / 0x100000000) : 0;
            case 0x28: return q ? q.avail % 0x100000000 : 0;
            case 0x2C: return q ? Math.floor(q.avail / 0x100000000) : 0;
            case 0x30: return q ? q.used % 0x100000000 : 0;
            case 0x34: return q ? Math.floor(q.used / 0x100000000) : 0;
            default: return 0;
        }
    };
    const common_write = (offset, width, value) => {
        value = Number(BigInt.asUintN(32, BigInt(value)));
        const map = { 0x00: 0x014, 0x08: 0x024, 0x0C: 0x020, 0x14: 0x070, 0x16: 0x030, 0x18: 0x038, 0x1C: 0x044,
            0x20: 0x080, 0x24: 0x084, 0x28: 0x090, 0x2C: 0x094, 0x30: 0x0A0, 0x34: 0x0A4 };
        if(offset === 0x14) value &= 0xFF;
        if(map[offset] !== undefined) device.write(map[offset], 4, value);
    };
    const split = (fn, offset, width, value) => {
        // (a 64-bit access as two 32-bit ones)
        if(width !== 8) return fn(offset, width, value);
        if(value === undefined) return BigInt(fn(offset, 4) >>> 0) | BigInt(fn(offset + 4, 4) >>> 0) << 32n;
        const v = BigInt.asUintN(64, BigInt(value));
        fn(offset, 4, v & 0xFFFFFFFFn);
        fn(offset + 4, 4, v >> 32n);
    };
    const bar4 = {
        size: 0x4000, bits64: true, prefetch: true,
        read: (offset, width) => {
            if(offset < 0x1000) return split(common_read, offset, width);
            if(offset < 0x2000)
            {
                if(offset !== 0x1000) return 0;
                // (ISR: reading it clears it and lowers the line)
                const isr = device.interrupt_status;
                if(isr) device.write(0x064, 4, isr);
                return isr;
            }
            if(offset < 0x3000)
            {
                const v = device.read(0x100 + offset - 0x2000, Math.min(width, 4));
                return width === 8 ? BigInt(v >>> 0) | BigInt(device.read(0x104 + offset - 0x2000, 4) >>> 0) << 32n : v;
            }
            return width === 8 ? 0n : 0;
        },
        write: (offset, width, value) => {
            if(offset < 0x1000) split(common_write, offset, width, value);
            else if(offset >= 0x2000 && offset < 0x3000) device.write(0x100 + offset - 0x2000, Math.min(width, 4), Number(BigInt.asUintN(32, BigInt(value))));
            else if(offset >= 0x3000) device.write(0x050, 4, offset - 0x3000 >> 2);
        },
    };
    host.add(slot, { config, bars: [null, null, null, null, bar4] });
    return device;
}
