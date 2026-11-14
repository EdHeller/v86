// ACPI: the PM I/O block (PM1 status, enable and control, the PM timer) and the GPE registers, IRQ 9. Was part of
// src/acpi.js, which keeps the PCI function, where the block is mapped, the hotplug registers and the snapshot
// format. http://www.uefi.org/sites/default/files/resources/ACPI_6_1.pdf

use crate::cpu::apic;
use crate::cpu::cpu::{device_lower_irq, device_raise_irq, js};

const PMTIMER_FREQ_SECONDS: f64 = 3579545.0;

// (the layout is read by src/acpi.js for snapshots)
#[repr(C)]
pub struct Acpi {
    timer_last_value: f64,         // 0
    timer_imprecision_offset: f64, // 8
    status: u32,                   // 16
    pm1_status: u32,               // 20
    pm1_enable: u32,               // 24
    last_timer: u32,               // 28
    /// where the PM block is mapped, -1 when it is not
    pm_base: i32, // 32
    gpe: [u8; 4], // 36
    present: u32, // 40
}

static mut ACPI: Acpi = Acpi {
    timer_last_value: 0.0,
    timer_imprecision_offset: 0.0,
    status: 1,
    pm1_status: 0,
    pm1_enable: 0,
    last_timer: 0,
    pm_base: -1,
    gpe: [0; 4],
    present: 0,
};

#[allow(static_mut_refs)]
fn acpi() -> &'static mut Acpi { unsafe { &mut ACPI } }

#[no_mangle]
pub fn get_acpi_addr() -> u32 { &raw mut ACPI as u32 }

#[no_mangle]
pub unsafe fn acpi_init(now: f64) {
    let a = acpi();
    *a = Acpi {
        timer_last_value: 0.0,
        timer_imprecision_offset: 0.0,
        status: 1,
        pm1_status: 0,
        pm1_enable: 0,
        last_timer: 0,
        pm_base: -1,
        gpe: [0; 4],
        present: 1,
    };
    a.last_timer = a.get_timer(now) as i64 as u32;
}

#[no_mangle]
pub unsafe fn acpi_set_pm_base(base: i32) { acpi().pm_base = base; }

impl Acpi {
    fn get_timer(&mut self, now: f64) -> f64 {
        let t = (now * (PMTIMER_FREQ_SECONDS / 1000.0) + 0.5).floor();

        // Due to the low precision of the host's time functions we increment the
        // returned timer value every time it is read
        if t == self.timer_last_value {
            // don't go past 1ms
            if self.timer_imprecision_offset < PMTIMER_FREQ_SECONDS / 1000.0 {
                self.timer_imprecision_offset += 1.0;
            }
        }
        else {
            let previous_timer = self.timer_last_value + self.timer_imprecision_offset;
            // don't go back in time
            if previous_timer <= t {
                self.timer_imprecision_offset = 0.0;
                self.timer_last_value = t;
            }
        }
        self.timer_last_value + self.timer_imprecision_offset
    }

    unsafe fn timer(&mut self, now: f64) -> f64 {
        let timer = self.get_timer(now) as i64 as u32;
        let highest_bit_changed = (timer ^ self.last_timer) & 1 << 23 != 0;

        if self.pm1_enable & 1 != 0 && highest_bit_changed {
            self.pm1_status |= 1;
            device_raise_irq(9);
        }
        else {
            device_lower_irq(9);
        }

        self.last_timer = timer;
        100.0 // TODO
    }
}

/// Which register of the device a port of this size is, if any: 0 PM1 status, 1 PM1 enable, 2 status, 3 the
/// timer, 4..7 the GPE bytes
#[inline]
unsafe fn register(port: i32, size: i32) -> Option<i32> {
    let a = acpi();
    if a.present == 0 || apic::CORE_COUNT != 1 {
        return None;
    }
    if (0xAFE0..=0xAFE3).contains(&port) {
        return if size == 8 { Some(4 + (port & 3)) } else { None };
    }
    if a.pm_base < 0 {
        return None;
    }
    match (port - a.pm_base, size) {
        (0, 16) => Some(0),
        (2, 16) => Some(1),
        (4, 8) | (4, 16) => Some(2),
        (8, 32) => Some(3),
        _ => None,
    }
}

#[inline]
pub unsafe fn serves(port: i32, size: i32) -> bool { register(port, size).is_some() }

/// The registers by port and access size (8, 16, 32), also for src/acpi.js
#[no_mangle]
pub unsafe fn acpi_port_read(port: i32, size: i32) -> i32 {
    let a = acpi();
    if (0xAFE0..=0xAFE3).contains(&port) {
        return a.gpe[(port & 3) as usize] as i32;
    }
    match (port - a.pm_base, size) {
        (0, _) => a.pm1_status as i32,
        (2, _) => a.pm1_enable as i32,
        (4, 8) => (a.status & 0xFF) as i32,
        (4, _) => a.status as i32,
        (8, _) => (a.get_timer(js::microtick()) as i64 & 0xFFFFFF) as i32,
        _ => -1,
    }
}

#[no_mangle]
pub unsafe fn acpi_port_write(port: i32, _size: i32, value: i32) {
    let a = acpi();
    if (0xAFE0..=0xAFE3).contains(&port) {
        a.gpe[(port & 3) as usize] = value as u8;
        return;
    }
    match port - a.pm_base {
        0 => a.pm1_status &= !(value as u32),
        2 => a.pm1_enable = value as u32,
        4 => a.status = value as u32,
        _ => {},
    }
}

#[no_mangle]
pub unsafe fn acpi_timer(now: f64) -> f64 { acpi().timer(now) }

#[inline]
pub unsafe fn present() -> bool { acpi().present != 0 }
