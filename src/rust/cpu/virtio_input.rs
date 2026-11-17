// virtio-input (src/virtio_input.js): the events for the driver (queue 0, eight bytes each: type, code, value), its
// status queue (1: taken and returned), and the host's input turned into events: keyboard scancodes (set 1 with E0
// and Pause's E1), the pointer's absolute position, its buttons and wheel. The configuration (names, bitmaps, axes)
// and multitouch (which sends ready events: virtio_input_send) are virtio_input.js's.

use crate::cpu::virtio_dev::*;

const EV_SYN: u16 = 0;
const EV_KEY: u16 = 1;
const EV_REL: u16 = 2;
const EV_ABS: u16 = 3;
const REL_WHEEL: u16 = 8;
const ABS_X: u16 = 0;
const ABS_Y: u16 = 1;
const BTN_LEFT: u16 = 0x110;
const BTN_RIGHT: u16 = 0x111;
const BTN_MIDDLE: u16 = 0x112;
const KEY_PAUSE: u16 = 119;
const ABS_MAX: i32 = 0x7FFF;
const PENDING_LIMIT: usize = 256;
const STATUS_DRIVER_OK: i32 = 4;

const EXTENDED_KEYS: [(u8, u16); 28] = [
    (0x10, 165), (0x19, 163), (0x1C, 96), (0x1D, 97), (0x20, 113), (0x22, 164), (0x24, 166), (0x2E, 114), (0x30, 115),
    (0x35, 98), (0x37, 99), (0x38, 100), (0x47, 102), (0x48, 103), (0x49, 104), (0x4B, 105), (0x4D, 106), (0x4F, 107),
    (0x50, 108), (0x51, 109), (0x52, 110), (0x53, 111), (0x5B, 125), (0x5C, 126), (0x5D, 127), (0x5E, 116), (0x5F, 142),
    (0x63, 143),
];

fn plain_key(code: u8) -> u16 { if code >= 0x01 && code <= 0x58 && code != 0x54 && code != 0x55 { code as u16 } else { 0 } }

#[derive(Default)]
struct Input {
    pending: Vec<(u16, u16, i32)>,
    prefix: u8,
    pause_bytes: u8,
    buttons: [bool; 3],
    last_x: i32,
    last_y: i32,
}

static mut INPUTS: Vec<Input> = Vec::new();

#[allow(static_mut_refs)]
unsafe fn input(d: usize) -> &'static mut Input {
    while INPUTS.len() <= d {
        INPUTS.push(Input { last_x: -1, last_y: -1, ..Input::default() });
    }
    &mut INPUTS[d]
}

#[no_mangle]
pub unsafe fn virtio_input_attach(d: i32) {
    *input(d as usize) = Input { last_x: -1, last_y: -1, ..Input::default() };
    MODEL[d as usize] = MODEL_INPUT;
}

/// The events still to deliver go (the device was reset)
#[no_mangle]
pub unsafe fn virtio_input_reset(d: i32) { input(d as usize).pending.clear(); }

unsafe fn send(d: usize, events: &[(u16, u16, i32)]) {
    let i = input(d);
    i.pending.extend_from_slice(events);
    i.pending.push((EV_SYN, 0, 0));
    // (too many: the oldest reports go)
    while i.pending.len() > PENDING_LIMIT {
        let end = i.pending.iter().position(|e| e.0 == EV_SYN).unwrap_or(i.pending.len() - 1);
        i.pending.drain(..=end);
    }
    flush_events(d);
}

/// The pending events into the buffers the driver offers (once it is ready)
pub unsafe fn flush_events(d: usize) {
    let i = input(d);
    let f = fields(d);
    let q = &f[crate::cpu::virtio_pci::DEVICE_FIELDS..];
    let configured = q[crate::cpu::virtio_pci::Q_DESC] != 0 && q[crate::cpu::virtio_pci::Q_AVAIL] != 0 && q[crate::cpu::virtio_pci::Q_USED] != 0;
    if i.pending.is_empty() || !configured || f[crate::cpu::virtio_pci::F_STATUS] & STATUS_DRIVER_OK == 0 {
        return;
    }
    let mut sent = 0;
    while sent < i.pending.len() && has_request(d, 0) {
        let mut c = pop(d, 0);
        let (ty, code, value) = i.pending[sent];
        sent += 1;
        let mut event = [0u8; 8];
        event[..2].copy_from_slice(&ty.to_le_bytes());
        event[2..4].copy_from_slice(&code.to_le_bytes());
        event[4..].copy_from_slice(&value.to_le_bytes());
        c.write(&event);
        push(d, 0, &c);
    }
    i.pending.drain(..sent);
    if sent != 0 {
        flush(d, 0);
    }
}

pub unsafe fn notify(d: usize, queue: usize) {
    if queue == 0 {
        flush_events(d);
    }
    else {
        let mut taken = 0;
        while has_request(d, 1) {
            let c = pop(d, 1);
            push(d, 1, &c);
            taken += 1;
        }
        if taken != 0 {
            flush(d, 1);
        }
    }
}

/// A scancode of the keyboard
#[no_mangle]
pub unsafe fn virtio_input_scancode(d: i32, code: i32) {
    let d = d as usize;
    let code = code as u8;
    let i = input(d);
    if i.pause_bytes != 0 {
        i.pause_bytes -= 1;
        if i.pause_bytes == 0 {
            send(d, &[(EV_KEY, KEY_PAUSE, if code & 0x80 != 0 { 0 } else { 1 })]);
        }
        return;
    }
    if code == 0xE1 {
        i.pause_bytes = 2;
        return;
    }
    if code == 0xE0 {
        i.prefix = 0xE0;
        return;
    }
    let extended = i.prefix == 0xE0;
    i.prefix = 0;
    let make = code & 0x7F;
    let key = if extended { EXTENDED_KEYS.iter().find(|e| e.0 == make).map_or(0, |e| e.1) } else { plain_key(make) };
    if key != 0 {
        send(d, &[(EV_KEY, key, if code & 0x80 != 0 { 0 } else { 1 })]);
    }
}

/// The pointer at (x, y) of a screen of width × height
#[no_mangle]
pub unsafe fn virtio_input_absolute(d: i32, x: f64, y: f64, width: f64, height: f64) {
    let d = d as usize;
    let scale = |v: f64, size: f64| ((v / size * ABS_MAX as f64).round() as i32).clamp(0, ABS_MAX);
    let (x, y) = (scale(x, width), scale(y, height));
    let i = input(d);
    let mut events = Vec::new();
    if x != i.last_x {
        events.push((EV_ABS, ABS_X, x));
    }
    if y != i.last_y {
        events.push((EV_ABS, ABS_Y, y));
    }
    i.last_x = x;
    i.last_y = y;
    if !events.is_empty() {
        send(d, &events);
    }
}

/// The buttons (left, middle, right)
#[no_mangle]
pub unsafe fn virtio_input_click(d: i32, left: bool, middle: bool, right: bool) {
    let d = d as usize;
    let codes = [BTN_LEFT, BTN_MIDDLE, BTN_RIGHT];
    let state = [left, middle, right];
    let i = input(d);
    let mut events = Vec::new();
    for k in 0..3 {
        if state[k] != i.buttons[k] {
            events.push((EV_KEY, codes[k], state[k] as i32));
        }
        i.buttons[k] = state[k];
    }
    if !events.is_empty() {
        send(d, &events);
    }
}

#[no_mangle]
pub unsafe fn virtio_input_wheel(d: i32, delta: i32) {
    if delta != 0 {
        send(d as usize, &[(EV_REL, REL_WHEEL, delta)]);
    }
}

/// Events made elsewhere (multitouch): `count` (type, code, value) triples of i32 in virtio_scratch
#[no_mangle]
pub unsafe fn virtio_input_send(d: i32, count: i32) {
    let words = std::slice::from_raw_parts(scratch() as *const i32, 3 * count as usize);
    let events: Vec<(u16, u16, i32)> = words.chunks(3).map(|e| (e[0] as u16, e[1] as u16, e[2])).collect();
    send(d as usize, &events);
}

/// The pending events, for a snapshot: how many (their triples in virtio_scratch)
#[no_mangle]
pub unsafe fn virtio_input_save(d: i32) -> i32 {
    let i = input(d as usize);
    let at = virtio_scratch(12 * i.pending.len() as u32) as *mut i32;
    for (k, e) in i.pending.iter().enumerate() {
        *at.add(3 * k) = e.0 as i32;
        *at.add(3 * k + 1) = e.1 as i32;
        *at.add(3 * k + 2) = e.2;
    }
    i.pending.len() as i32
}

/// The pending events of a snapshot (`count` triples in virtio_scratch)
#[no_mangle]
pub unsafe fn virtio_input_load(d: i32, count: i32) {
    let words = std::slice::from_raw_parts(scratch() as *const i32, 3 * count as usize);
    input(d as usize).pending = words.chunks(3).map(|e| (e[0] as u16, e[1] as u16, e[2])).collect();
}

/// The pending events to the driver now (DRIVER_OK)
#[no_mangle]
pub unsafe fn virtio_input_flush(d: i32) { flush_events(d as usize) }
