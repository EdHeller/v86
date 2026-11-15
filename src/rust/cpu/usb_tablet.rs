// The USB HID tablet (src/usb/hid_tablet.js) with what every USB device does (src/usb/usb_device.js): an address,
// a configuration, endpoint 0's control transfers with the standard requests, halted endpoints; the tablet's
// interrupt IN endpoint, which NAKs until the pointer changed (or SET_IDLE's time passed). A full-speed device.
// The core's controllers (uhci.rs, xhci.rs) ask it directly, so that their polling of it every millisecond stays in
// the core; hid_tablet.js's CoreTablet is its JavaScript face (the pointer, the plugin's snapshots, transactions from
// the controllers JavaScript runs).

use crate::cpu::cpu::js;
use crate::cpu::global_pointers::memory_size;
use crate::cpu::memory::mem8;

pub const PID_SETUP: i32 = 0x2D;
pub const PID_IN: i32 = 0x69;
pub const PID_OUT: i32 = 0xE1;
const USB_NAK: i32 = -2;
const USB_STALL: i32 = -3;
const USB_BABBLE: i32 = -4;
const SPEED_FULL: i32 = 1;

const STAGE_IDLE: u8 = 0;
const STAGE_DATA_IN: u8 = 1;
const STAGE_DATA_OUT: u8 = 2;
const STAGE_STATUS_IN: u8 = 3;
const STAGE_STALLED: u8 = 4;

const DESCRIPTOR_DEVICE: u16 = 1;
const DESCRIPTOR_CONFIGURATION: u16 = 2;
const DESCRIPTOR_STRING: u16 = 3;
const DESCRIPTOR_HID: u16 = 0x21;
const DESCRIPTOR_REPORT: u16 = 0x22;

const REPORT_DESCRIPTOR: [u8; 74] = [
    0x05, 0x01, 0x09, 0x02, 0xA1, 0x01, 0x09, 0x01, 0xA1, 0x00, 0x05, 0x09, 0x19, 0x01, 0x29, 0x03, 0x15, 0x00, 0x25,
    0x01, 0x95, 0x03, 0x75, 0x01, 0x81, 0x02, 0x95, 0x01, 0x75, 0x05, 0x81, 0x01, 0x05, 0x01, 0x09, 0x30, 0x09, 0x31,
    0x15, 0x00, 0x26, 0xFF, 0x7F, 0x35, 0x00, 0x46, 0xFF, 0x7F, 0x75, 0x10, 0x95, 0x02, 0x81, 0x02, 0x05, 0x01, 0x09,
    0x38, 0x15, 0x81, 0x25, 0x7F, 0x35, 0x00, 0x45, 0x00, 0x75, 0x08, 0x95, 0x01, 0x81, 0x06, 0xC0, 0xC0,
];
const REPORT_SIZE: usize = 6;
pub const TABLET_MAX: i32 = 0x7FFF;
const HID_DESCRIPTOR: [u8; 9] = [9, 0x21, 0x11, 0x01, 0x00, 0x01, 0x22, REPORT_DESCRIPTOR.len() as u8, 0];
const DEVICE_DESCRIPTOR: [u8; 18] = [18, 1, 0x10, 0x01, 0, 0, 0, 8, 0x27, 0x06, 0x01, 0x00, 0x00, 0x00, 1, 2, 3, 1];
const CONFIGURATION: [u8; 34] = [
    9, 2, 34, 0, 1, 1, 0, 0x80, 50, // configuration
    9, 4, 0, 0, 1, 3, 0, 0, 0, // interface
    9, 0x21, 0x11, 0x01, 0x00, 0x01, 0x22, REPORT_DESCRIPTOR.len() as u8, 0, // HID
    7, 5, 0x81, 3, 8, 0, 10, // endpoint 1 IN, interrupt
];
const STRINGS: [&str; 3] = ["v86", "v86 USB Tablet", "1"];
const LANGUAGES: [u8; 4] = [4, 3, 0x09, 0x04];
const ENDPOINT_IN: u8 = 0x81;

const HID_GET_REPORT: u8 = 0x01;
const HID_GET_IDLE: u8 = 0x02;
const HID_GET_PROTOCOL: u8 = 0x03;
const HID_SET_REPORT: u8 = 0x09;
const HID_SET_IDLE: u8 = 0x0A;
const HID_SET_PROTOCOL: u8 = 0x0B;

#[derive(Clone, Copy, Default)]
struct Setup {
    ty: u8,
    request: u8,
    value: u16,
    index: u16,
    length: u16,
}

#[derive(Default)]
pub struct Tablet {
    present: bool,
    /// the port of the core's controller it is in, or -1
    port: i32,
    address: i32,
    configuration: u8,
    alternates: Vec<u8>,
    remote_wakeup: bool,
    halted: Vec<u8>,
    stage: u8,
    setup: Option<Setup>,
    control_data: Vec<u8>,
    control_offset: usize,
    out_data: Vec<u8>,
    pending_address: i32,
    x: i32,
    y: i32,
    buttons: u8,
    wheel: i32,
    changed: bool,
    last_report: f64,
    idle: u8,
    protocol: u8,
    in_use: bool,
}

static mut TABLET: Option<Tablet> = None;

#[allow(static_mut_refs)]
fn tablet() -> &'static mut Tablet { unsafe { TABLET.get_or_insert_with(Tablet::default) } }

type Answer = Result<Vec<u8>, ()>;

impl Tablet {
    fn reset(&mut self) {
        self.address = 0;
        self.configuration = 0;
        self.alternates = vec![0];
        self.remote_wakeup = false;
        self.halted.clear();
        self.stage = STAGE_IDLE;
        self.setup = None;
        self.control_data.clear();
        self.control_offset = 0;
        self.out_data.clear();
        self.pending_address = -1;
        self.idle = 0;
        self.protocol = 1;
        self.in_use = self.configuration != 0;
    }

    /// a transaction: IN's bytes into `data` (its length returned), SETUP's and OUT's from it (0: ACK); or NAK, STALL
    unsafe fn transaction(&mut self, pid: i32, endpoint: i32, data: *mut u8, length: usize) -> i32 {
        if endpoint == 0 {
            return self.control(pid, data, length);
        }
        let address = endpoint as u8 | if pid == PID_IN { 0x80 } else { 0 };
        if self.configuration == 0 || address != ENDPOINT_IN || pid == PID_SETUP || self.halted.contains(&address) {
            return USB_STALL;
        }
        self.data_in(data, length)
    }

    unsafe fn control(&mut self, pid: i32, data: *mut u8, length: usize) -> i32 {
        if pid == PID_SETUP {
            if length != 8 {
                return USB_STALL;
            }
            let b = std::slice::from_raw_parts(data, 8);
            let setup = Setup {
                ty: b[0],
                request: b[1],
                value: u16::from_le_bytes([b[2], b[3]]),
                index: u16::from_le_bytes([b[4], b[5]]),
                length: u16::from_le_bytes([b[6], b[7]]),
            };
            self.setup = Some(setup);
            self.pending_address = -1;
            if setup.ty & 0x80 != 0 {
                match self.request(&setup, &[]) {
                    Err(()) => self.stage = STAGE_STALLED,
                    Ok(mut bytes) => {
                        bytes.truncate(setup.length as usize);
                        self.control_data = bytes;
                        self.control_offset = 0;
                        self.stage = STAGE_DATA_IN;
                    },
                }
            }
            else if setup.length != 0 {
                self.out_data.clear();
                self.stage = STAGE_DATA_OUT;
            }
            else {
                self.stage = STAGE_STATUS_IN;
            }
            return 0;
        }
        if self.stage == STAGE_STALLED {
            return USB_STALL;
        }
        if pid == PID_IN {
            if self.stage == STAGE_DATA_IN {
                // (endpoint 0's packets: 8 bytes)
                let n = length.min(8).min(self.control_data.len() - self.control_offset);
                std::ptr::copy_nonoverlapping(self.control_data.as_ptr().add(self.control_offset), data, n);
                self.control_offset += n;
                return n as i32;
            }
            if self.stage == STAGE_STATUS_IN {
                let setup = self.setup.unwrap_or_default();
                let out = std::mem::take(&mut self.out_data);
                let result = self.request(&setup, &out);
                self.out_data = out;
                if result.is_err() {
                    self.stage = STAGE_STALLED;
                    return USB_STALL;
                }
                self.stage = STAGE_IDLE;
                if self.pending_address >= 0 {
                    self.address = self.pending_address;
                    self.pending_address = -1;
                }
                return 0;
            }
            return USB_STALL;
        }
        if pid == PID_OUT {
            if self.stage == STAGE_DATA_OUT {
                self.out_data.extend_from_slice(std::slice::from_raw_parts(data, length));
                if self.out_data.len() >= self.setup.map_or(0, |s| s.length as usize) {
                    self.stage = STAGE_STATUS_IN;
                }
                return 0;
            }
            if self.stage == STAGE_DATA_IN {
                self.stage = STAGE_IDLE;
                return 0;
            }
        }
        USB_STALL
    }

    fn request(&mut self, setup: &Setup, _data: &[u8]) -> Answer {
        if setup.ty & 0x60 != 0 {
            return self.class_request(setup);
        }
        let recipient = setup.ty & 0x1F;
        match setup.request {
            // GET_STATUS
            0x00 => Ok(match recipient {
                0 => vec![if self.remote_wakeup { 2 } else { 0 }, 0],
                2 => vec![self.halted.contains(&(setup.index as u8 & 0x8F)) as u8, 0],
                _ => vec![0, 0],
            }),
            // CLEAR_FEATURE, SET_FEATURE
            0x01 | 0x03 => {
                let set = setup.request == 0x03;
                if recipient == 0 && setup.value == 1 {
                    self.remote_wakeup = set;
                    return Ok(vec![]);
                }
                if recipient == 2 && setup.value == 0 {
                    let endpoint = setup.index as u8 & 0x8F;
                    if endpoint & 0xF != 0 && endpoint != ENDPOINT_IN {
                        return Err(());
                    }
                    if set && endpoint & 0xF != 0 {
                        if !self.halted.contains(&endpoint) {
                            self.halted.push(endpoint);
                        }
                    }
                    else {
                        self.halted.retain(|&e| e != endpoint);
                    }
                    return Ok(vec![]);
                }
                Err(())
            },
            // SET_ADDRESS
            0x05 => {
                if setup.value > 127 {
                    return Err(());
                }
                self.pending_address = setup.value as i32;
                Ok(vec![])
            },
            // GET_DESCRIPTOR
            0x06 => {
                let (ty, index) = (setup.value >> 8, setup.value & 0xFF);
                let bytes = if recipient == 1 { self.interface_descriptor(ty, setup.index) } else { self.descriptor(ty, index) };
                bytes.ok_or(())
            },
            // GET_CONFIGURATION
            0x08 => Ok(vec![self.configuration]),
            // SET_CONFIGURATION
            0x09 => {
                let value = setup.value as u8;
                if value > 1 {
                    return Err(());
                }
                self.configuration = value;
                self.alternates = vec![0];
                self.halted.clear();
                // (the guest's driver takes the pointer from a fresh report)
                self.changed = true;
                self.in_use = value != 0;
                Ok(vec![])
            },
            // GET_INTERFACE
            0x0A => {
                if self.configuration == 0 {
                    return Err(());
                }
                Ok(vec![*self.alternates.get(setup.index as usize).unwrap_or(&0)])
            },
            // SET_INTERFACE
            0x0B => {
                if self.configuration == 0 || setup.value != 0 {
                    return Err(());
                }
                let i = setup.index as usize;
                if i < 256 {
                    if self.alternates.len() <= i {
                        self.alternates.resize(i + 1, 0);
                    }
                    self.alternates[i] = 0;
                }
                Ok(vec![])
            },
            // (SET_SEL, SET_ISOCH_DELAY: SuperSpeed only)
            _ => Err(()),
        }
    }

    fn descriptor(&self, ty: u16, index: u16) -> Option<Vec<u8>> {
        match ty {
            DESCRIPTOR_DEVICE => Some(DEVICE_DESCRIPTOR.to_vec()),
            DESCRIPTOR_CONFIGURATION => if index == 0 { Some(CONFIGURATION.to_vec()) } else { None },
            DESCRIPTOR_STRING => {
                if index == 0 {
                    return Some(LANGUAGES.to_vec());
                }
                let text = STRINGS.get(index as usize - 1)?;
                let mut bytes = vec![2 + 2 * text.len() as u8, DESCRIPTOR_STRING as u8];
                for c in text.encode_utf16() {
                    bytes.extend_from_slice(&c.to_le_bytes());
                }
                Some(bytes)
            },
            // (a full-speed only device: no qualifier, no other speed; no BOS)
            _ => None,
        }
    }

    fn interface_descriptor(&self, ty: u16, interface: u16) -> Option<Vec<u8>> {
        if interface != 0 {
            return None;
        }
        match ty {
            DESCRIPTOR_HID => Some(HID_DESCRIPTOR.to_vec()),
            DESCRIPTOR_REPORT => Some(REPORT_DESCRIPTOR.to_vec()),
            _ => None,
        }
    }

    fn report(&mut self) -> [u8; REPORT_SIZE] {
        let wheel = self.wheel.clamp(-127, 127);
        self.wheel -= wheel;
        [self.buttons, self.x as u8, (self.x >> 8) as u8, self.y as u8, (self.y >> 8) as u8, wheel as u8]
    }

    fn class_request(&mut self, setup: &Setup) -> Answer {
        if setup.ty & 0x1F != 1 || setup.index & 0xFF != 0 {
            return Err(());
        }
        match setup.request {
            HID_GET_REPORT => if setup.value >> 8 == 1 { Ok(self.report().to_vec()) } else { Err(()) },
            HID_SET_REPORT => Err(()),
            HID_GET_IDLE => Ok(vec![self.idle]),
            HID_SET_IDLE => {
                self.idle = (setup.value >> 8) as u8;
                Ok(vec![])
            },
            HID_GET_PROTOCOL => Ok(vec![self.protocol]),
            HID_SET_PROTOCOL => {
                self.protocol = setup.value as u8 & 1;
                Ok(vec![])
            },
            _ => Err(()),
        }
    }

    unsafe fn data_in(&mut self, data: *mut u8, length: usize) -> i32 {
        let now = js::microtick();
        if !self.changed && !(self.idle != 0 && now - self.last_report >= self.idle as f64 * 4.0) {
            return USB_NAK;
        }
        let report = self.report();
        self.changed = self.wheel != 0;
        self.last_report = now;
        let n = length.min(REPORT_SIZE);
        std::ptr::copy_nonoverlapping(report.as_ptr(), data, n);
        n as i32
    }
}

/// From a core controller, before it asks JavaScript: the tablet's answer when it is in one of the ports of
/// `port_mask` (at `address`, or any: -1), its data at `data` (this module's memory). The tablet's port's bit in
/// `reset_pending` (a reset the controller marked for JavaScript to do) is done here first.
#[inline]
pub unsafe fn route(port_mask: u32, address: i32, pid: i32, endpoint: i32, data: *mut u8, length: usize, reset_pending: &mut u32) -> Option<i32> {
    #[allow(static_mut_refs)]
    let Some(t) = TABLET.as_mut()
    else {
        return None;
    };
    if !t.present || t.port < 0 || port_mask & 1 << t.port == 0 {
        return None;
    }
    if *reset_pending & 1 << t.port != 0 {
        *reset_pending &= !(1 << t.port);
        t.reset();
    }
    if address >= 0 && address != t.address {
        return None;
    }
    let result = t.transaction(pid, endpoint, data, length);
    // (as the JavaScript bridges: an ACKed SETUP or OUT took all its bytes)
    Some(if pid != PID_IN && result == 0 { length as i32 } else { result })
}

/// a guest address as a pointer for route, or None outside memory
pub unsafe fn guest(address: u32, length: u32) -> Option<*mut u8> {
    if address as u64 + length as u64 > *memory_size as u64 {
        return None;
    }
    Some(mem8.add(address as usize))
}

// ---------------------------------------------------------------------------------------------------------------
// hid_tablet.js's interface

static mut SCRATCH: [u8; 1024] = [0; 1024];

#[no_mangle]
pub unsafe fn tablet_init() {
    let t = tablet();
    t.present = true;
    t.port = -1;
    t.x = TABLET_MAX >> 1;
    t.y = TABLET_MAX >> 1;
    t.reset();
}

/// the port of the core's controller it is in (-1: none, or a controller JavaScript runs)
#[no_mangle]
pub unsafe fn tablet_set_port(port: i32) { tablet().port = port; }

#[no_mangle]
pub unsafe fn tablet_reset() { tablet().reset(); }

/// a pointer to 1 KiB for transaction's data
#[no_mangle]
pub unsafe fn tablet_scratch() -> u32 { &raw mut SCRATCH as u32 }

/// a transaction from a controller JavaScript runs, its data in tablet_scratch
#[no_mangle]
pub unsafe fn tablet_transaction(pid: i32, endpoint: i32, length: i32) -> i32 {
    let n = (length.max(0) as usize).min(1024);
    let result = tablet().transaction(pid, endpoint, &raw mut SCRATCH as *mut u8, n);
    if result > n as i32 { USB_BABBLE } else { result }
}

#[no_mangle]
pub unsafe fn tablet_address() -> i32 { tablet().address }
#[no_mangle]
pub unsafe fn tablet_in_use() -> bool { tablet().in_use }
#[no_mangle]
pub unsafe fn tablet_speed() -> i32 { SPEED_FULL }

#[no_mangle]
pub unsafe fn tablet_move(x: i32, y: i32) {
    let t = tablet();
    if x == t.x && y == t.y {
        return;
    }
    t.x = x;
    t.y = y;
    t.changed = true;
}

#[no_mangle]
pub unsafe fn tablet_press(buttons: i32) {
    let t = tablet();
    if buttons as u8 == t.buttons {
        return;
    }
    t.buttons = buttons as u8;
    t.changed = true;
}

#[no_mangle]
pub unsafe fn tablet_scroll(delta: i32) {
    if delta == 0 {
        return;
    }
    let t = tablet();
    t.wheel += delta;
    t.changed = true;
}

// snapshots: numbers (hid_tablet.js builds its arrays from them, and back)
static mut STATE: Vec<f64> = Vec::new();

#[no_mangle]
#[allow(static_mut_refs)]
pub unsafe fn tablet_state_addr() -> u32 { STATE.as_ptr() as u32 }
#[no_mangle]
#[allow(static_mut_refs)]
pub unsafe fn tablet_state_len() -> u32 { STATE.len() as u32 }
#[no_mangle]
#[allow(static_mut_refs)]
pub unsafe fn tablet_state_resize(n: u32) -> u32 {
    STATE.clear();
    STATE.resize(n as usize, 0.0);
    STATE.as_ptr() as u32
}

/// [address, configuration, n, alternates.., remote wakeup, n, halted.., stage, setup? (1, its 5 fields | 0), n,
/// control data.., control offset, n, out data.., pending address, x, y, buttons, wheel, changed, idle, protocol]
#[no_mangle]
#[allow(static_mut_refs)]
pub unsafe fn tablet_save() {
    let t = tablet();
    let s = &mut STATE;
    s.clear();
    s.extend([t.address as f64, t.configuration as f64, t.alternates.len() as f64]);
    s.extend(t.alternates.iter().map(|&a| a as f64));
    s.extend([t.remote_wakeup as u8 as f64, t.halted.len() as f64]);
    s.extend(t.halted.iter().map(|&a| a as f64));
    s.push(t.stage as f64);
    match t.setup {
        Some(p) => s.extend([1.0, p.ty as f64, p.request as f64, p.value as f64, p.index as f64, p.length as f64]),
        None => s.push(0.0),
    }
    s.push(t.control_data.len() as f64);
    s.extend(t.control_data.iter().map(|&a| a as f64));
    s.extend([t.control_offset as f64, t.out_data.len() as f64]);
    s.extend(t.out_data.iter().map(|&a| a as f64));
    s.extend([t.pending_address as f64, t.x as f64, t.y as f64, t.buttons as f64, t.wheel as f64, t.changed as u8 as f64, t.idle as f64, t.protocol as f64]);
}

#[no_mangle]
#[allow(static_mut_refs)]
pub unsafe fn tablet_load() {
    let t = tablet();
    let s = &STATE;
    let mut i = 0;
    let mut next = || {
        let v = s.get(i).copied().unwrap_or(0.0);
        i += 1;
        v
    };
    t.address = next() as i32;
    t.configuration = next() as u8;
    let n = next() as usize;
    t.alternates = (0..n).map(|_| next() as u8).collect();
    t.remote_wakeup = next() != 0.0;
    let n = next() as usize;
    t.halted = (0..n).map(|_| next() as u8).collect();
    t.stage = next() as u8;
    t.setup = if next() != 0.0 {
        Some(Setup { ty: next() as u8, request: next() as u8, value: next() as u16, index: next() as u16, length: next() as u16 })
    }
    else {
        None
    };
    let n = next() as usize;
    t.control_data = (0..n).map(|_| next() as u8).collect();
    t.control_offset = (next() as usize).min(t.control_data.len());
    let n = next() as usize;
    t.out_data = (0..n).map(|_| next() as u8).collect();
    t.pending_address = next() as i32;
    t.x = next() as i32;
    t.y = next() as i32;
    t.buttons = next() as u8;
    t.wheel = next() as i32;
    t.changed = next() != 0.0;
    t.idle = next() as u8;
    t.protocol = next() as u8;
    t.last_report = js::microtick();
    t.in_use = t.configuration != 0;
}
