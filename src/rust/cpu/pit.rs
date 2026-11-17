// Programmable Interval Timer (8254), and the timer pass of the main loop.
//
// The PIT was src/pit.js: the main loop called JavaScript on every pass (about every 100,000 instructions) to run
// it, and every read of its counters was a call out of the module. It lives here now; src/pit.js keeps the PC
// speaker's messages, the snapshot format and the ports reached from JavaScript (16-bit accesses, more cores).
// The devices that remain in JavaScript (RTC, ACPI timer, plugin timers) are called when they asked to be.

use crate::cpu::apic;
use crate::cpu::cpu::{device_lower_irq, device_raise_irq, js};

// In kHz
const OSCILLATOR_FREQ: f64 = 1193.1816666; // 1.193182 MHz

// (the layout is read by src/pit.js for snapshots)
#[repr(C)]
pub struct Pit {
    counter_start_time: [f64; 3],  // 0
    counter_start_value: [u16; 3], // 24
    counter_latch_value: [u16; 3], // 30
    counter_reload: [u16; 3],      // 36
    counter_next_low: [u8; 4],     // 42
    counter_enabled: [u8; 4],      // 46
    counter_mode: [u8; 4],         // 50
    counter_read_mode: [u8; 4],    // 54
    // 2 = latch low, 1 = latch high, 0 = no latch
    counter_latch: [u8; 4], // 58
}

static mut PIT: Pit = Pit {
    counter_start_time: [0.0; 3],
    counter_start_value: [0; 3],
    counter_latch_value: [0; 3],
    counter_reload: [0; 3],
    counter_next_low: [0; 4],
    counter_enabled: [0; 4],
    counter_mode: [0; 4],
    counter_read_mode: [0; 4],
    counter_latch: [0; 4],
};

#[allow(static_mut_refs)]
fn pit() -> &'static mut Pit { unsafe { &mut PIT } }

#[no_mangle]
pub fn get_pit_addr() -> u32 { &raw mut PIT as u32 }

impl Pit {
    fn did_rollover(&self, i: usize, now: f64) -> bool {
        let diff = now - self.counter_start_time[i];
        if diff < 0.0 {
            // should only happen after restore_state
            return true;
        }
        let diff_in_ticks = (diff * OSCILLATOR_FREQ).floor();
        (self.counter_start_value[i] as f64) < diff_in_ticks
    }

    fn get_counter_value(&self, i: usize, now: f64) -> i32 {
        if self.counter_enabled[i] == 0 {
            return 0;
        }
        let diff = now - self.counter_start_time[i];
        let diff_in_ticks = (diff * OSCILLATOR_FREQ).floor();
        let mut value = self.counter_start_value[i] as f64 - diff_in_ticks;
        let reload = self.counter_reload[i] as f64;
        if value >= reload {
            value %= reload;
        }
        else if value < 0.0 {
            value = value % reload + reload;
        }
        value as i32
    }

    unsafe fn timer(&mut self, now: f64, no_irq: bool) -> f64 {
        let mut time_to_next_interrupt = 100.0;

        // counter 0 produces interrupts
        if !no_irq {
            if self.counter_enabled[0] != 0 && self.did_rollover(0, now) {
                let diff = now - self.counter_start_time[0];
                let diff_in_ticks = (diff * OSCILLATOR_FREQ).floor();

                self.counter_start_value[0] = self.get_counter_value(0, now) as u16;
                self.counter_start_time[0] =
                    if diff < 0.0 { now } else { self.counter_start_time[0] + diff_in_ticks / OSCILLATOR_FREQ };

                // This isn't strictly correct, but it's necessary since browsers
                // may sleep longer than necessary to trigger the else branch below
                // and clear the irq
                device_lower_irq(0);
                device_raise_irq(0);

                if self.counter_mode[0] == 0 {
                    self.counter_enabled[0] = 0;
                }
            }
            else {
                device_lower_irq(0);
            }

            if self.counter_enabled[0] != 0 {
                let diff = now - self.counter_start_time[0];
                let diff_in_ticks = (diff * OSCILLATOR_FREQ).floor();
                let ticks_missing = self.counter_start_value[0] as f64 - diff_in_ticks; // XXX: to simplify
                time_to_next_interrupt = ticks_missing / OSCILLATOR_FREQ;
            }
        }

        time_to_next_interrupt
    }

    unsafe fn counter_read(&mut self, i: usize) -> i32 {
        let latch = self.counter_latch[i];

        if latch != 0 {
            self.counter_latch[i] -= 1;
            if latch == 2 {
                self.counter_latch_value[i] as i32 & 0xFF
            }
            else {
                self.counter_latch_value[i] as i32 >> 8
            }
        }
        else {
            let next_low = self.counter_next_low[i];
            if self.counter_mode[i] == 3 {
                self.counter_next_low[i] ^= 1;
            }
            let value = self.get_counter_value(i, js::microtick());
            if next_low != 0 {
                value & 0xFF
            }
            else {
                value >> 8 & 0xFF
            }
        }
    }

    unsafe fn counter_write(&mut self, i: usize, value: i32) {
        let value = value as u16 & 0xFF;
        if self.counter_next_low[i] != 0 {
            self.counter_reload[i] = self.counter_reload[i] & !0xFF | value;
        }
        else {
            self.counter_reload[i] = self.counter_reload[i] & 0xFF | value << 8;
        }

        if self.counter_read_mode[i] != 3 || self.counter_next_low[i] == 0 {
            if self.counter_reload[i] == 0 {
                self.counter_reload[i] = 0xFFFF;
            }

            // depends on the mode, should actually
            // happen on the first tick
            self.counter_start_value[i] = self.counter_reload[i];
            self.counter_enabled[i] = 1;
            self.counter_start_time[i] = js::microtick();
        }

        if self.counter_read_mode[i] == 3 {
            self.counter_next_low[i] ^= 1;
        }
    }

    unsafe fn port43_write(&mut self, reg_byte: i32) {
        let mut mode = (reg_byte >> 1 & 7) as u8;
        let i = (reg_byte >> 6 & 3) as usize;
        let read_mode = (reg_byte >> 4 & 3) as u8;

        if i == 3 {
            // Unimplemented read back
            return;
        }

        if read_mode == 0 {
            // latch
            self.counter_latch[i] = 2;
            let value = self.get_counter_value(i, js::microtick());
            self.counter_latch_value[i] = if value != 0 { (value - 1) as u16 } else { 0 };
            return;
        }

        if mode >= 6 {
            // 6 and 7 are aliased to 2 and 3
            mode &= !4;
        }

        // lsb (1), msb (2), or first lsb then msb
        self.counter_next_low[i] = if read_mode == 2 { 0 } else { 1 };

        if i == 0 {
            device_lower_irq(0);
        }

        self.counter_mode[i] = mode;
        self.counter_read_mode[i] = read_mode;
    }

    unsafe fn port61_read(&self) -> i32 {
        let now = js::microtick();
        let ref_toggle = (now * (1000.0 * 1000.0 / 15000.0)) as i64 as i32 & 1;
        let counter2_out = self.did_rollover(2, now) as i32;
        ref_toggle << 4 | counter2_out << 5
    }
}

/// The ports of the PIT (0x40 to 0x43, and 0x61 for reading), also for src/pit.js
#[no_mangle]
pub unsafe fn pit_port_read(port: i32) -> i32 {
    match port {
        0x40 | 0x41 | 0x42 => pit().counter_read((port - 0x40) as usize),
        0x61 => pit().port61_read(),
        _ => 0xFF,
    }
}

#[no_mangle]
pub unsafe fn pit_port_write(port: i32, value: i32) {
    match port {
        0x40 | 0x41 | 0x42 => pit().counter_write((port - 0x40) as usize, value),
        0x43 => pit().port43_write(value),
        _ => {},
    }
}

/// Whether the core answers this port itself: the machine's instance, not a vCPU worker. What the PC speaker hears (port
/// 0x61 written, counter 2 programmed) goes through JavaScript, which calls pit_port_write
#[inline]
pub unsafe fn serves_read(port: i32) -> bool {
    (port & !3 == 0x40 && port != 0x43 || port == 0x61) && !super::parallel::WORKER
}
#[inline]
pub unsafe fn serves_write(port: i32, value: i32) -> bool {
    (port == 0x40 || port == 0x41 || port == 0x43 && value >> 6 & 3 != 2) && !super::parallel::WORKER
}

#[no_mangle]
pub unsafe fn pit_timer(now: f64, no_irq: bool) -> f64 { pit().timer(now, no_irq) }

// The time at which the timers in JavaScript want to run again. Reset by everything that can change them: port
// I/O and memory-mapped I/O handled in JavaScript, a restored state (timers_changed, also for JavaScript)
pub static mut JS_TIMERS_DUE: f64 = 0.0;
// (the longest they are left alone: the ACPI timer's interrupt and events from outside the guest have no deadline)
const JS_TIMERS_MAX: f64 = 20.0;

#[no_mangle]
pub unsafe fn timers_changed() { JS_TIMERS_DUE = 0.0; }

/// The timer pass of the main loop: the PIT and the APIC timers here, the rest in JavaScript when due.
/// Returns the milliseconds until the next timer needs to run
pub unsafe fn run_hardware_timers(acpi_enabled: bool, now: f64) -> f64 {
    if super::parallel::WORKER {
        // (a vCPU worker has no devices: the machine's instance runs the timers)
        return js::run_hardware_timers(acpi_enabled, now);
    }
    let mut t = pit().timer(now, false);
    if super::rtc::present() {
        t = t.min(super::rtc::rtc_timer());
    }
    t = t.min(super::uhci::timer(now));
    t = t.min(super::virtio_snd::timer(now));
    t = t.min(super::xhci::timer(now));
    t = t.min(super::hda::timer(now));
    t = t.min(super::floppy::fdc_run_transfer());
    if acpi_enabled {
        t = t.min(apic::apic_timer(now));
        if super::acpi::present() {
            t = t.min(super::acpi::acpi_timer(now));
        }
    }
    // (with cores in workers JavaScript answers them in its pass: every time)
    if now >= JS_TIMERS_DUE || now < JS_TIMERS_DUE - JS_TIMERS_MAX || super::parallel::MACHINE {
        let t_js = js::run_hardware_timers(acpi_enabled, now);
        JS_TIMERS_DUE = now + t_js.min(JS_TIMERS_MAX);
    }
    t.min(JS_TIMERS_DUE - now)
}
