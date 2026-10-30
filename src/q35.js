// The Q35 chipset (machine_type: "q35"), after v64's src/q35.js and QEMU's
// q35/lpc_ich9: the MCH host bridge at 00:00.0 with the BIOS shadow (PAM)
// and the PCI Express configuration window (ECAM, PCIEXBAR), and the ICH9 LPC
// bridge at 00:1f.0 with the PM base (PMBASE, ACPI_CNTL), the PIRQ routing
// and the root complex register block. PCI interrupts: devices 0-24 rotate
// their pins over PIRQE-H, devices 25-31 go through the RCBA route registers
// (default INTA-D to PIRQA-D); PIRQn is IOAPIC input 16 + n, and the ISA IRQ
// in its route register at the PIC. Storage stays v86's PIIX IDE function.

import { LOG_PCI } from "./const.js";
import { h } from "./lib.js";
import { dbg_log } from "./log.js";

// For Types Only
import { CPU } from "./cpu.js";

const MCH_PCIEXBAR = 0x60;
const MCH_PAM0 = 0x90;
const MCH_SMRAM = 0x9D;
const MCH_ESMRAMC = 0x9E;

const LPC_PMBASE = 0x40;
const LPC_ACPI_CNTL = 0x44;
const LPC_ACPI_EN = 0x80;
const LPC_PIRQA_ROUT = 0x60;
const LPC_PIRQE_ROUT = 0x68;
const LPC_PIRQ_ROUT_DISABLED = 0x80;
const LPC_RCBA = 0xF0;

const RCBA_SIZE = 0x4000;
const RCBA_DIR = { 31: 0x3140, 29: 0x3144, 28: 0x3146, 27: 0x3148, 26: 0x314C, 25: 0x3150 };
const RCBA_DIR_DEFAULT = 0x3210;

/** ECAM: one 256 MiB window (PCIEXBAR length 0), buses 0 to 255 */
const ECAM_SIZE = 0x10000000;

export const Q35_MCH_PCI_ID = 0;
export const ICH9_LPC_PCI_ID = 0x1F << 3;

/**
 * @constructor
 * @param {CPU} cpu
 * @param {Object} pci
 */
export function Q35(cpu, pci)
{
    this.cpu = cpu;
    this.pci = pci;
    this.name = "q35";

    // 00:00.0 Host bridge: Intel Corporation 82G33/G31/P35/P31 Express DRAM Controller
    const mch_space = new Array(256).fill(0);
    mch_space.splice(0, 16, 0x86, 0x80, 0xC0, 0x29, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x00);
    // PAM0: "RAM present", so that SeaBIOS does not copy itself from the alias below 4 GiB
    mch_space[MCH_PAM0] = 0x10;
    mch_space[MCH_SMRAM] = 0x02;
    mch_space[MCH_ESMRAMC] = 0x38;
    this.mch = {
        pci_id: Q35_MCH_PCI_ID,
        pci_space: mch_space,
        pci_bars: [],
        name: "Q35 MCH",
        on_config_write: (offset, size) => this.mch_config_written(offset, size),
    };
    this.mch_space = pci.register_device(this.mch);
    this.mch_space8 = new Uint8Array(this.mch_space.buffer);

    // 00:1f.0 ISA bridge: Intel Corporation 82801IB (ICH9) LPC Interface Controller (rev 02)
    const lpc_space = new Array(256).fill(0);
    lpc_space.splice(0, 16, 0x86, 0x80, 0x18, 0x29, 0x07, 0x00, 0x00, 0x02, 0x02, 0x00, 0x01, 0x06, 0x00, 0x00, 0x80, 0x00);
    lpc_space[LPC_PMBASE] = 1;
    for(let i = 0; i < 4; i++)
    {
        lpc_space[LPC_PIRQA_ROUT + i] = LPC_PIRQ_ROUT_DISABLED;
        lpc_space[LPC_PIRQE_ROUT + i] = LPC_PIRQ_ROUT_DISABLED;
    }
    this.lpc = {
        pci_id: ICH9_LPC_PCI_ID,
        pci_space: lpc_space,
        pci_bars: [],
        name: "ICH9 LPC",
        on_config_write: (offset, size) => this.lpc_config_written(offset, size),
    };
    this.lpc_space = pci.register_device(this.lpc);
    this.lpc_space8 = new Uint8Array(this.lpc_space.buffer);

    this.rcba = new Uint8Array(RCBA_SIZE);
    for(const offset of Object.values(RCBA_DIR))
    {
        this.rcba[offset] = RCBA_DIR_DEFAULT & 0xFF;
        this.rcba[offset + 1] = RCBA_DIR_DEFAULT >> 8;
    }
    this.rcba_base = -1;
    this.ecam_base = -1;
    /** the ECAM window is mapped in 128 KiB blocks once and stays (its base moves rarely) */
    this.ecam_mapped = new Set();
    /** asserted INTx lines: pci_id -> pin */
    this.levels = new Map();
}

/** Whether a write at [offset, offset + size) touches [first, end) */
function touches(offset, size, first, end)
{
    return offset < end && offset + size > first;
}

Q35.prototype.mch_config_written = function(offset, size)
{
    if(touches(offset, size, MCH_PCIEXBAR, MCH_PCIEXBAR + 8))
    {
        this.update_ecam();
    }
    if(touches(offset, size, MCH_PAM0, MCH_PAM0 + 7))
    {
        this.pci.update_pam();
    }
};

/** PCIEXBAR: the ECAM window at its base while enabled (256 MiB only) */
Q35.prototype.update_ecam = function()
{
    const value = this.mch_space[MCH_PCIEXBAR >> 2];
    const base = value & 1 ? (value & 0xF0000000) >>> 0 : -1;
    if(base === this.ecam_base) return;
    dbg_log("Q35 ECAM " + (base === -1 ? "disabled" : "at " + h(base, 8)), LOG_PCI);
    this.ecam_base = base;
    // (the MCH decodes its window before any BAR: v86's own VGA keeps its frame buffer at 0xE0000000)
    this.cpu.wm.exports["set_vga_lfb_hidden"](base !== -1 && base <= 0xE0000000 && base + ECAM_SIZE > 0xE0000000);
    if(base === -1) return;
    const io = this.cpu.io;
    const pci = this.pci;
    const in_window = addr => (addr >>> 0) - this.ecam_base >>> 0;
    const read8 = addr => {
        const a = in_window(addr);
        if(a >= ECAM_SIZE) return 0xFF;
        return pci.config_read8(a >> 12 & 0xFFFF, a & 0xFFF);
    };
    const write8 = (addr, value) => {
        const a = in_window(addr);
        if(a >= ECAM_SIZE || (a & 0xFFF) >= 0x100) return;
        pci.pci_write8((a >> 12 & 0xFFFF) << 8 | a & 0xFF, value);
    };
    const read32 = addr => read8(addr) | read8(addr + 1) << 8 | read8(addr + 2) << 16 | read8(addr + 3) << 24;
    const write32 = (addr, value) => {
        const a = in_window(addr);
        if(a >= ECAM_SIZE || (a & 0xFFF) >= 0x100) return;
        pci.pci_write32((a >> 12 & 0xFFFF) << 8 | a & 0xFC, value);
    };
    io.mmap_register(base, ECAM_SIZE, read8, write8, read32, write32);
};

Q35.prototype.lpc_config_written = function(offset, size)
{
    if(touches(offset, size, LPC_PMBASE, LPC_ACPI_CNTL + 1))
    {
        const pmbase = this.lpc_space[LPC_PMBASE >> 2] & 0xFF80;
        this.lpc_space[LPC_PMBASE >> 2] = pmbase | 1;
        const enabled = (this.lpc_space8[LPC_ACPI_CNTL] & LPC_ACPI_EN) !== 0;
        const acpi = this.cpu.devices.acpi;
        if(acpi) acpi.register_pm(enabled && pmbase ? pmbase : -1);
    }
    if(touches(offset, size, LPC_PIRQA_ROUT, LPC_PIRQA_ROUT + 4) || touches(offset, size, LPC_PIRQE_ROUT, LPC_PIRQE_ROUT + 4))
    {
        for(let i = 0; i < 4; i++)
        {
            this.lpc_space8[LPC_PIRQA_ROUT + i] &= 0x8F;
            this.lpc_space8[LPC_PIRQE_ROUT + i] &= 0x8F;
        }
        this.reroute();
    }
    if(touches(offset, size, LPC_RCBA, LPC_RCBA + 4))
    {
        this.update_rcba();
    }
};

Q35.prototype.update_rcba = function()
{
    const value = this.lpc_space[LPC_RCBA >> 2];
    this.lpc_space[LPC_RCBA >> 2] = value & ~0x3FFE;
    const base = value & 1 ? (value & ~0x3FFF) >>> 0 : -1;
    if(base === this.rcba_base) return;
    const io = this.cpu.io;
    if(this.rcba_base !== -1) io.mmap_unregister_range(this.rcba_base, RCBA_SIZE, this);
    this.rcba_base = base;
    if(base === -1) return;
    const offset = addr => (addr >>> 0) - this.rcba_base >>> 0;
    io.mmap_register_range(base, RCBA_SIZE, this,
        addr => this.rcba[offset(addr)],
        (addr, value) => this.rcba_write(offset(addr), value),
        addr => { const o = offset(addr); return this.rcba[o] | this.rcba[o + 1] << 8 | this.rcba[o + 2] << 16 | this.rcba[o + 3] << 24; },
        (addr, value) => { const o = offset(addr); for(let i = 0; i < 4; i++) this.rcba_write(o + i, value >>> (i << 3) & 0xFF); });
};

Q35.prototype.rcba_write = function(offset, value)
{
    this.rcba[offset] = value;
    if(offset >= 0x3140 && offset < 0x3154) this.reroute();
};

/**
 * Where pin (0 to 3) of a function on bus 0 goes
 * @return {{pic: number, gsi: number}}
 */
Q35.prototype.irq_route = function(pci_id, pin)
{
    const slot = pci_id >> 3;
    let pirq;
    if(slot <= 24)
    {
        pirq = 4 + (slot + pin) % 4;
    }
    else if(RCBA_DIR[slot] !== undefined)
    {
        const offset = RCBA_DIR[slot];
        pirq = (this.rcba[offset] | this.rcba[offset + 1] << 8) >> (pin << 2) & 7;
    }
    else
    {
        pirq = 4 + pin;
    }
    const rout = this.lpc_space8[pirq < 4 ? LPC_PIRQA_ROUT + pirq : LPC_PIRQE_ROUT + pirq - 4];
    return { pic: rout & LPC_PIRQ_ROUT_DISABLED ? -1 : rout & 0x0F, gsi: 16 + pirq };
};

/** @param {number} pci_id @param {number} pin @param {boolean} level */
Q35.prototype.set_intx = function(pci_id, pin, level)
{
    if(level) this.levels.set(pci_id, pin); else this.levels.delete(pci_id);
    const route = this.irq_route(pci_id, pin);
    this.cpu.device_set_pci_irq(route.pic, route.gsi, level);
};

/** The routing changed: asserted lines move to their new IRQs */
Q35.prototype.reroute = function()
{
    for(const [pci_id, pin] of this.levels)
    {
        const route = this.irq_route(pci_id, pin);
        this.cpu.device_set_pci_irq(route.pic, route.gsi, true);
    }
};

Q35.prototype.get_state = function()
{
    return [this.rcba];
};

Q35.prototype.set_state = function(state)
{
    this.rcba.set(state[0]);
    this.update_ecam();
    this.update_rcba();
};
