// RTC (real time clock) and CMOS: ports 0x70 and 0x71, IRQ 8. Was src/rtc.js, which keeps the snapshot format and
// cmos_read / cmos_write for the machine's setup.

use crate::cpu::apic;
use crate::cpu::cpu::{device_lower_irq, device_raise_irq, js};

const CMOS_RTC_SECONDS: u8 = 0x00;
const CMOS_RTC_SECONDS_ALARM: u8 = 0x01;
const CMOS_RTC_MINUTES: u8 = 0x02;
const CMOS_RTC_MINUTES_ALARM: u8 = 0x03;
const CMOS_RTC_HOURS: u8 = 0x04;
const CMOS_RTC_HOURS_ALARM: u8 = 0x05;
const CMOS_RTC_DAY_WEEK: u8 = 0x06;
const CMOS_RTC_DAY_MONTH: u8 = 0x07;
const CMOS_RTC_MONTH: u8 = 0x08;
const CMOS_RTC_YEAR: u8 = 0x09;
const CMOS_STATUS_A: u8 = 0x0a;
const CMOS_STATUS_B: u8 = 0x0b;
const CMOS_STATUS_C: u8 = 0x0c;
const CMOS_STATUS_D: u8 = 0x0d;
const CMOS_DIAG_STATUS: u8 = 0x0e;
const CMOS_CENTURY: u8 = 0x32;
const CMOS_CENTURY2: u8 = 0x37;

// (the layout is read by src/rtc.js for snapshots)
#[repr(C)]
pub struct Rtc {
    // used for cmos entries: milliseconds since 1970
    rtc_time: f64,    // 0
    last_update: f64, // 8
    // used for periodic interrupt
    next_interrupt: f64, // 16
    // next alarm interrupt
    next_interrupt_alarm: f64,    // 24
    periodic_interrupt_time: f64, // 32
    update_interrupt_time: f64,   // 40
    cmos_data: [u8; 128],         // 48
    cmos_index: u8,               // 176
    cmos_a: u8,                   // 177
    cmos_b: u8,                   // 178
    cmos_c: u8,                   // 179
    nmi_disabled: u8,             // 180
    periodic_interrupt: u8,       // 181
    update_interrupt: u8,         // 182
    cmos_diag_status: u8,         // 183
}

static mut RTC: Rtc = Rtc {
    rtc_time: 0.0,
    last_update: 0.0,
    next_interrupt: 0.0,
    next_interrupt_alarm: 0.0,
    // corresponds to default value for cmos_a
    periodic_interrupt_time: 1000.0 / 1024.0,
    update_interrupt_time: 0.0,
    cmos_data: [0; 128],
    cmos_index: 0,
    cmos_a: 0x26,
    cmos_b: 2,
    cmos_c: 0,
    nmi_disabled: 0,
    periodic_interrupt: 0,
    update_interrupt: 0,
    cmos_diag_status: 0,
};

/// Date.now() - performance.now(), given by JavaScript when the machine is made: the wall clock without a call
static mut DATE_OFFSET: f64 = 0.0;
static mut PRESENT: bool = false;

#[allow(static_mut_refs)]
fn rtc() -> &'static mut Rtc { unsafe { &mut RTC } }

#[no_mangle]
pub fn get_rtc_addr() -> u32 { &raw mut RTC as u32 }

/// `date_now` is JavaScript's Date.now(), `now` its clock (microtick) at the same moment
#[no_mangle]
pub unsafe fn rtc_init(date_now: f64, now: f64) {
    DATE_OFFSET = date_now - now;
    PRESENT = true;
    let r = rtc();
    r.rtc_time = date_now;
    r.last_update = date_now;
}

unsafe fn date_now() -> f64 { (js::microtick() + DATE_OFFSET).floor() }

/// (year, month 1..12, day 1..31, hour, minute, second, weekday 0 = Sunday) of milliseconds since 1970, UTC
fn civil(ms: f64) -> (i64, i64, i64, i64, i64, i64, i64) {
    let secs = (ms / 1000.0).floor() as i64;
    let days = secs.div_euclid(86400);
    let rem = secs.rem_euclid(86400);
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + (m <= 2) as i64;
    (y, m, d, rem / 3600, rem / 60 % 60, rem % 60, (days + 4).rem_euclid(7))
}

/// days since 1970 of a date
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

impl Rtc {
    fn bcd_pack(mut n: i64) -> i32 {
        let (mut i, mut result) = (0, 0);
        while n != 0 {
            let digit = n % 10;
            result |= (digit as i32) << (4 * i);
            i += 1;
            n = (n - digit) / 10;
        }
        result
    }

    fn encode_time(&self, t: i64) -> i32 {
        if self.cmos_b & 4 != 0 {
            // binary mode
            t as i32
        }
        else {
            Rtc::bcd_pack(t)
        }
    }

    fn decode_time(&self, t: u8) -> i64 {
        if self.cmos_b & 4 != 0 {
            t as i64
        }
        else {
            (t & 0xF) as i64 + 10 * (t >> 4 & 0xF) as i64
        }
    }

    unsafe fn timer(&mut self) -> f64 {
        let time = date_now();
        self.rtc_time += time - self.last_update;
        self.last_update = time;

        if self.periodic_interrupt != 0 && self.next_interrupt < time {
            device_raise_irq(8);
            self.cmos_c |= 1 << 6 | 1 << 7;
            self.next_interrupt +=
                self.periodic_interrupt_time * ((time - self.next_interrupt) / self.periodic_interrupt_time).ceil();
        }
        else if self.next_interrupt_alarm != 0.0 && self.next_interrupt_alarm < time {
            device_raise_irq(8);
            self.cmos_c |= 1 << 5 | 1 << 7;
            self.next_interrupt_alarm = 0.0;
        }
        else if self.update_interrupt != 0 && self.update_interrupt_time < time {
            device_raise_irq(8);
            self.cmos_c |= 1 << 4 | 1 << 7;
            self.update_interrupt_time = time + 1000.0; // 1 second
        }

        let mut t: f64 = 100.0;
        if self.periodic_interrupt != 0 && self.next_interrupt != 0.0 {
            t = t.min((self.next_interrupt - time).max(0.0));
        }
        if self.next_interrupt_alarm != 0.0 {
            t = t.min((self.next_interrupt_alarm - time).max(0.0));
        }
        if self.update_interrupt != 0 {
            t = t.min((self.update_interrupt_time - time).max(0.0));
        }
        t
    }

    unsafe fn cmos_port_read(&mut self) -> i32 {
        let index = self.cmos_index;
        let (year, month, day, hour, minute, second, weekday) = civil(self.rtc_time);
        match index {
            CMOS_RTC_SECONDS => self.encode_time(second),
            CMOS_RTC_MINUTES => self.encode_time(minute),
            // TODO: 12 hour mode
            CMOS_RTC_HOURS => self.encode_time(hour),
            CMOS_RTC_DAY_WEEK => self.encode_time(weekday + 1),
            CMOS_RTC_DAY_MONTH => self.encode_time(day),
            CMOS_RTC_MONTH => self.encode_time(month),
            CMOS_RTC_YEAR => self.encode_time(year % 100),
            CMOS_STATUS_A => {
                if js::microtick() % 1000.0 >= 999.0 {
                    // Set update-in-progress for one millisecond every second (we
                    // may not have precision higher than that in browser
                    // environments)
                    return self.cmos_a as i32 | 0x80;
                }
                self.cmos_a as i32
            },
            CMOS_STATUS_B => self.cmos_b as i32,
            CMOS_STATUS_C => {
                // It is important to know that upon a IRQ 8, Status Register C
                // will contain a bitmask telling which interrupt happened.
                // What is important is that if register C is not read after an
                // IRQ 8, then the interrupt will not happen again.
                device_lower_irq(8);
                let c = self.cmos_c;
                self.cmos_c &= !0xF0;
                c as i32
            },
            CMOS_STATUS_D => 1 << 7, // CMOS battery charged
            CMOS_DIAG_STATUS => self.cmos_diag_status as i32,
            CMOS_CENTURY | CMOS_CENTURY2 => self.encode_time(year / 100),
            _ => self.cmos_data[index as usize & 127] as i32,
        }
    }

    unsafe fn cmos_port_write(&mut self, data_byte: u8) {
        match self.cmos_index {
            CMOS_STATUS_A => {
                self.cmos_a = data_byte & 0x7F;
                // (as JavaScript's 32768 >> (a & 0xF) - 1: the shift count is taken modulo 32)
                let shift = ((self.cmos_a & 0xF) as i32 - 1) & 31;
                self.periodic_interrupt_time = 1000.0 / (32768 >> shift) as f64;
            },
            CMOS_STATUS_B => {
                self.cmos_b = data_byte;
                if self.cmos_b & 0x80 != 0 {
                    // remove update interrupt flag
                    self.cmos_b &= 0xEF;
                }
                if self.cmos_b & 0x40 != 0 {
                    self.next_interrupt = date_now();
                }
                if self.cmos_b & 0x20 != 0 {
                    let now = date_now();
                    let (year, month, day, ..) = civil(now);
                    let seconds = self.decode_time(self.cmos_data[CMOS_RTC_SECONDS_ALARM as usize]);
                    let minutes = self.decode_time(self.cmos_data[CMOS_RTC_MINUTES_ALARM as usize]);
                    let hours = self.decode_time(self.cmos_data[CMOS_RTC_HOURS_ALARM as usize]);
                    let alarm = days_from_civil(year, month, day) * 86400 + hours * 3600 + minutes * 60 + seconds;
                    self.next_interrupt_alarm = alarm as f64 * 1000.0;
                }
                if self.cmos_b & 0x10 != 0 {
                    self.update_interrupt_time = date_now();
                }
            },
            CMOS_DIAG_STATUS => self.cmos_diag_status = data_byte,
            CMOS_RTC_SECONDS_ALARM | CMOS_RTC_MINUTES_ALARM | CMOS_RTC_HOURS_ALARM => {
                self.cmos_data[self.cmos_index as usize] = data_byte;
            },
            _ => {},
        }

        self.update_interrupt = (self.cmos_b & 0x10 == 0x10 && self.cmos_a & 0xF > 0) as u8;
        self.periodic_interrupt = (self.cmos_b & 0x40 == 0x40 && self.cmos_a & 0xF > 0) as u8;
    }
}

/// Ports 0x70 and 0x71, also for src/rtc.js
#[no_mangle]
pub unsafe fn rtc_port_read(port: i32) -> i32 {
    match port {
        0x71 => rtc().cmos_port_read(),
        _ => 0xFF,
    }
}

#[no_mangle]
pub unsafe fn rtc_port_write(port: i32, value: i32) {
    match port {
        0x70 => {
            rtc().cmos_index = value as u8 & 0x7F;
            rtc().nmi_disabled = (value >> 7 & 1) as u8;
        },
        0x71 => rtc().cmos_port_write(value as u8),
        _ => {},
    }
}

#[no_mangle]
pub unsafe fn rtc_timer() -> f64 { rtc().timer() }

/// Whether the core answers this port itself (a one-core machine with an RTC)
#[inline]
pub unsafe fn serves_read(port: i32) -> bool { port == 0x71 && PRESENT && apic::CORE_COUNT == 1 }
#[inline]
pub unsafe fn serves_write(port: i32) -> bool { (port == 0x70 || port == 0x71) && PRESENT && apic::CORE_COUNT == 1 }
#[inline]
pub unsafe fn present() -> bool { PRESENT }
