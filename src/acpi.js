// http://www.uefi.org/sites/default/files/resources/ACPI_6_1.pdf

import { v86 } from "./main.js";
import { LOG_ACPI } from "../src/const.js";
import { h } from "./lib.js";
import { dbg_log } from "./log.js";

// For Types Only
import { CPU } from "./cpu.js";

/**
 * @constructor
 * @param {CPU} cpu
 */
export function ACPI(cpu)
{
    /** @type {CPU} */
    this.cpu = cpu;

    var io = cpu.io;

    var acpi = {
        pci_id: 0x07 << 3,
        pci_space: [
            0x86, 0x80, 0x13, 0x71, 0x07, 0x00, 0x80, 0x02, 0x08, 0x00, 0x80, 0x06, 0x00, 0x00, 0x80, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x09, 0x01, 0x00, 0x00,
        ],
        pci_bars: [],
        name: "acpi",
    };

    this.acpi_pci = acpi;

    // the registers and the timer are in the core (src/rust/cpu/acpi.rs)
    cpu.acpi_init(v86.microtick());

    // the PM I/O block: at 0xB000 (the tables SeaBIOS builds for v86); with
    // the UEFI firmware where it puts PMBA, as PIIX4's at 00:01.3
    this.pm_base = -1;
    if(cpu.machine_type === "q35")
    {
        // ICH9: the LPC decodes the PM block at PMBASE (src/q35.js); no PCI function
        acpi = null;
    }
    else if(cpu.firmware)
    {
        acpi.pci_id = 0x01 << 3 | 3;
        acpi.on_config_write = () => this.update_pm_base();
    }
    else
    {
        this.register_pm(0xB000);
    }

    // 00:07.0 Bridge: Intel Corporation 82371AB/EB/MB PIIX4 ACPI (rev 08)
    this.pci_space = acpi ? cpu.devices.pci.register_device(acpi) : null;

    // PCI hotplug registers of the firmware's DSDT (the slot devices' _STA/_EJ0 and the GPE 1 handler read them):
    // no slot is being inserted (PCIU) or removed (PCID), ejecting does nothing. Unhandled reads return all ones,
    // which made Linux's acpiphp "remove" every device (the IDE controller disappeared and nothing could boot).
    io.register_read(0xAE00, this, undefined, undefined, function() { return 0; });
    io.register_read(0xAE04, this, undefined, undefined, function() { return 0; });
    io.register_read(0xAE0C, this, undefined, undefined, function() { return 0; });
    io.register_write(0xAE08, this, undefined, undefined, function(value) { dbg_log("ACPI pci eject: " + h(value >>> 0), LOG_ACPI); });
    io.register_write(0xAE0C, this, undefined, undefined, function(value) {});

    // ACPI, gpe
    for(let port = 0xAFE0; port <= 0xAFE3; port++)
    {
        io.register_read(port, this, function() { return cpu.acpi_port_read(port, 8); });
        io.register_write(port, this, function(value) { cpu.acpi_port_write(port, 8, value); });
    }
}

/**
 * The PM registers (PM1 status, enable, control, the timer) at `base`
 * @param {number} base
 */
ACPI.prototype.register_pm = function(base)
{
    const io = this.cpu.io, cpu = this.cpu;
    if(this.pm_base !== -1)
    {
        for(let port = this.pm_base; port < this.pm_base + 0x10; port++)
        {
            io.ports[port] = io.create_empty_entry();
            io.sync_port(port);
        }
    }
    this.pm_base = base;
    cpu.acpi_set_pm_base(base);
    if(base === -1) return;

    // (the core answers these itself; here for accesses that come through JavaScript)
    io.register_read(base, this, undefined, function() { return cpu.acpi_port_read(base, 16); });
    io.register_write(base, this, undefined, function(value) { cpu.acpi_port_write(base, 16, value); });

    io.register_read(base + 2, this, undefined, function() { return cpu.acpi_port_read(base + 2, 16); });
    io.register_write(base + 2, this, undefined, function(value) { cpu.acpi_port_write(base + 2, 16, value); });

    // ACPI status
    io.register_read(base + 4, this,
        function() { return cpu.acpi_port_read(base + 4, 8); },
        function() { return cpu.acpi_port_read(base + 4, 16); });
    io.register_write(base + 4, this, undefined, function(value) { cpu.acpi_port_write(base + 4, 16, value); });

    // ACPI, pmtimer
    io.register_read(base + 8, this, undefined, undefined, function() { return cpu.acpi_port_read(base + 8, 32); });
};

/** PIIX4: the PM block where PMBA (config 0x40) says, while PMREGMISC (0x80) enables it */
ACPI.prototype.update_pm_base = function()
{
    const space = this.pci_space;
    space[0x40 >> 2] = space[0x40 >> 2] & 0xFFC0 | 1;
    const base = space[0x80 >> 2] & 1 ? space[0x40 >> 2] & 0xFFC0 : -1;
    if(base !== this.pm_base) this.register_pm(base);
};

ACPI.prototype.timer = function(now)
{
    return this.cpu.acpi_timer(now);
};

// struct Acpi in src/rust/cpu/acpi.rs: status, pm1_status, pm1_enable at 16, the GPE bytes at 36
ACPI.prototype.fields = function()
{
    const buffer = this.cpu.wasm_memory.buffer, at = this.cpu.get_acpi_addr();
    return { registers: new Uint32Array(buffer, at + 16, 3), gpe: new Uint8Array(buffer, at + 36, 4) };
};

ACPI.prototype.get_state = function()
{
    const f = this.fields();
    var state = [];
    state[0] = f.registers[0];
    state[1] = f.registers[1];
    state[2] = f.registers[2];
    state[3] = new Uint8Array(f.gpe);
    return state;
};

ACPI.prototype.set_state = function(state)
{
    const f = this.fields();
    f.registers[0] = state[0];
    f.registers[1] = state[1];
    f.registers[2] = state[2];
    f.gpe.set(state[3]);
};
