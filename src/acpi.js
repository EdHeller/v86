// http://www.uefi.org/sites/default/files/resources/ACPI_6_1.pdf

import { v86 } from "./main.js";
import { LOG_ACPI } from "../src/const.js";
import { h } from "./lib.js";
import { dbg_log, dbg_assert } from "./log.js";

// For Types Only
import { CPU } from "./cpu.js";

const PMTIMER_FREQ_SECONDS = 3579545;

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

    this.timer_last_value = 0;
    this.timer_imprecision_offset = 0;

    this.status = 1;
    this.pm1_status = 0;
    this.pm1_enable = 0;
    this.last_timer = this.get_timer(v86.microtick());

    this.gpe = new Uint8Array(4);

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
    io.register_read(0xAFE0, this, function()
    {
        dbg_log("Read gpe#0", LOG_ACPI);
        return this.gpe[0];
    });
    io.register_read(0xAFE1, this, function()
    {
        dbg_log("Read gpe#1", LOG_ACPI);
        return this.gpe[1];
    });
    io.register_read(0xAFE2, this, function()
    {
        dbg_log("Read gpe#2", LOG_ACPI);
        return this.gpe[2];
    });
    io.register_read(0xAFE3, this, function()
    {
        dbg_log("Read gpe#3", LOG_ACPI);
        return this.gpe[3];
    });

    io.register_write(0xAFE0, this, function(value)
    {
        dbg_log("Write gpe#0: " + h(value), LOG_ACPI);
        this.gpe[0] = value;
    });
    io.register_write(0xAFE1, this, function(value)
    {
        dbg_log("Write gpe#1: " + h(value), LOG_ACPI);
        this.gpe[1] = value;
    });
    io.register_write(0xAFE2, this, function(value)
    {
        dbg_log("Write gpe#2: " + h(value), LOG_ACPI);
        this.gpe[2] = value;
    });
    io.register_write(0xAFE3, this, function(value)
    {
        dbg_log("Write gpe#3: " + h(value), LOG_ACPI);
        this.gpe[3] = value;
    });
}

/**
 * The PM registers (PM1 status, enable, control, the timer) at `base`
 * @param {number} base
 */
ACPI.prototype.register_pm = function(base)
{
    const io = this.cpu.io;
    if(this.pm_base !== -1)
    {
        for(let port = this.pm_base; port < this.pm_base + 0x10; port++)
        {
            io.ports[port] = io.create_empty_entry();
        }
    }
    this.pm_base = base;
    if(base === -1) return;

    io.register_read(base, this, undefined, function()
    {
        dbg_log("ACPI pm1_status read", LOG_ACPI);
        return this.pm1_status;
    });
    io.register_write(base, this, undefined, function(value)
    {
        dbg_log("ACPI pm1_status write: " + h(value, 4), LOG_ACPI);
        this.pm1_status &= ~value;
    });

    io.register_read(base + 2, this, undefined, function()
    {
        dbg_log("ACPI pm1_enable read", LOG_ACPI);
        return this.pm1_enable;
    });
    io.register_write(base + 2, this, undefined, function(value)
    {
        dbg_log("ACPI pm1_enable write: " + h(value), LOG_ACPI);
        this.pm1_enable = value;
    });

    // ACPI status
    io.register_read(base + 4, this, function()
    {
        dbg_log("ACPI status read8", LOG_ACPI);
        return this.status & 0xFF;
    }, function()
    {
        dbg_log("ACPI status read", LOG_ACPI);
        return this.status;
    });
    io.register_write(base + 4, this, undefined, function(value)
    {
        dbg_log("ACPI status write: " + h(value), LOG_ACPI);
        this.status = value;
    });

    // ACPI, pmtimer
    io.register_read(base + 8, this, undefined, undefined, function()
    {
        var value = this.get_timer(v86.microtick()) & 0xFFFFFF;
        //dbg_log("pmtimer read: " + h(value >>> 0), LOG_ACPI);
        return value;
    });

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
    var timer = this.get_timer(now);
    var highest_bit_changed = ((timer ^ this.last_timer) & (1 << 23)) !== 0;

    if((this.pm1_enable & 1) && highest_bit_changed)
    {
        dbg_log("ACPI raise irq", LOG_ACPI);
        this.pm1_status |= 1;
        this.cpu.device_raise_irq(9);
    }
    else
    {
        this.cpu.device_lower_irq(9);
    }

    this.last_timer = timer;
    return 100; // TODO
};

ACPI.prototype.get_timer = function(now)
{
    const t = Math.round(now * (PMTIMER_FREQ_SECONDS / 1000));

    // Due to the low precision of JavaScript's time functions we increment the
    // returned timer value every time it is read

    if(t === this.timer_last_value)
    {
        // don't go past 1ms

        if(this.timer_imprecision_offset < PMTIMER_FREQ_SECONDS / 1000)
        {
            this.timer_imprecision_offset++;
        }
    }
    else
    {
        dbg_assert(t > this.timer_last_value);

        const previous_timer = this.timer_last_value + this.timer_imprecision_offset;

        // don't go back in time

        if(previous_timer <= t)
        {
            this.timer_imprecision_offset = 0;
            this.timer_last_value = t;
        }
        else
        {
            dbg_log("Warning: Overshot pmtimer, waiting;" +
                    " current=" + t +
                    " last=" + this.timer_last_value +
                    " offset=" + this.timer_imprecision_offset, LOG_ACPI);
        }
    }

    return this.timer_last_value + this.timer_imprecision_offset;
};

ACPI.prototype.get_state = function()
{
    var state = [];
    state[0] = this.status;
    state[1] = this.pm1_status;
    state[2] = this.pm1_enable;
    state[3] = this.gpe;
    return state;
};

ACPI.prototype.set_state = function(state)
{
    this.status = state[0];
    this.pm1_status = state[1];
    this.pm1_enable = state[2];
    this.gpe = state[3];
};
