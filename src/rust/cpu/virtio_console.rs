// virtio-console with multiport (src/virtio_console.js): four ports; what the driver writes to a port (to JavaScript),
// what JavaScript has typed for one, the control queues (ports added, named, opened, the console's window size).
// Queues: 0 and 1 port 0's receive and transmit, 2 and 3 the control queues (to the driver, from it), then a pair per
// further port. Its configuration (window size, ports) is virtio_console.js's.

use crate::cpu::cpu::js;
use crate::cpu::virtio_dev::*;
use crate::cpu::virtio_pci::{DEVICE_FIELDS, QUEUE_FIELDS, Q_AVAIL, Q_DESC, Q_USED};

const DEVICE_READY: u16 = 0;
const DEVICE_ADD: u16 = 1;
const PORT_READY: u16 = 3;
const CONSOLE_PORT: u16 = 4;
const RESIZE: u16 = 5;
const PORT_OPEN: u16 = 6;
const PORT_NAME: u16 = 7;

#[derive(Clone, Copy, Default)]
struct Console {
    ports: u32,
    cols: u16,
    rows: u16,
}

static mut CONSOLES: [Console; 16] = [Console { ports: 4, cols: 80, rows: 25 }; 16];

#[no_mangle]
pub unsafe fn virtio_console_attach(d: i32, ports: i32) {
    CONSOLES[d as usize].ports = ports as u32;
    MODEL[d as usize] = MODEL_CONSOLE;
}

/// The window size (the configuration's, for the next RESIZE)
#[no_mangle]
pub unsafe fn virtio_console_set_size(d: i32, cols: i32, rows: i32) {
    CONSOLES[d as usize].cols = cols as u16;
    CONSOLES[d as usize].rows = rows as u16;
}

unsafe fn send(d: usize, queue: usize, c: &mut Chain, data: &[u8]) {
    c.write(data);
    push(d, queue, c);
    flush(d, queue);
}

unsafe fn ack(d: usize, queue: usize, c: &mut Chain) { send(d, queue, c, &[]) }

unsafe fn control(d: usize, port: u32, event: u16, value: u16, extra: &[u8]) {
    if !has_request(d, 2) {
        return;
    }
    let mut c = pop(d, 2);
    let mut message = Vec::with_capacity(8 + extra.len());
    message.extend_from_slice(&port.to_le_bytes());
    message.extend_from_slice(&event.to_le_bytes());
    message.extend_from_slice(&value.to_le_bytes());
    message.extend_from_slice(extra);
    send(d, 2, &mut c, &message);
}

unsafe fn window_size(d: usize, port: u32, cols: u16, rows: u16) {
    let mut size = [0u8; 4];
    size[..2].copy_from_slice(&rows.to_le_bytes());
    size[2..].copy_from_slice(&cols.to_le_bytes());
    control(d, port, RESIZE, 0, &size);
}

pub unsafe fn notify(d: usize, queue: usize) {
    let console = CONSOLES[d];
    if queue == 1 || queue > 3 && queue & 1 == 1 {
        // a port's transmit queue: its bytes to JavaScript
        let port = if queue > 3 { (queue - 3) >> 1 } else { 0 };
        while has_request(d, queue) {
            let mut c = pop(d, queue);
            let mut bytes = vec![0u8; c.length_readable as usize];
            c.read(&mut bytes);
            js::virtio_model_event(d as i32, port as i32, bytes.as_ptr() as u32, bytes.len() as i32);
            ack(d, queue, &mut c);
        }
    }
    else if queue == 3 {
        while has_request(d, queue) {
            let mut c = pop(d, queue);
            let mut message = [0u8; 8];
            c.read(&mut message);
            let port = u32::from_le_bytes(message[..4].try_into().unwrap());
            let event = u16::from_le_bytes([message[4], message[5]]);
            ack(d, queue, &mut c);
            match event {
                DEVICE_READY => {
                    for i in 0..console.ports {
                        control(d, i, DEVICE_ADD, 0, &[]);
                    }
                },
                PORT_READY => {
                    // (acknowledged twice, as virtio_console.js does)
                    ack(d, queue, &mut c);
                    control(d, port, CONSOLE_PORT, 1, &[]);
                    let mut name = format!("virtio-{}", port).into_bytes();
                    name.push(0);
                    control(d, port, PORT_NAME, 1, &name);
                    control(d, port, PORT_OPEN, 1, &[]);
                },
                PORT_OPEN => {
                    ack(d, queue, &mut c);
                    if port == 0 {
                        window_size(d, port, console.cols, console.rows);
                    }
                },
                _ => return,
            }
        }
    }
}

/// Text JavaScript typed for a port (in virtio_scratch): to the port's receive queue when the driver has a buffer
#[no_mangle]
pub unsafe fn virtio_console_input(d: i32, port: i32, length: i32) {
    let d = d as usize;
    let queue = if port == 0 { 0 } else { port as usize * 2 + 2 };
    if !has_request(d, queue) {
        return;
    }
    let data = std::slice::from_raw_parts(scratch(), length as usize).to_vec();
    let mut c = pop(d, queue);
    send(d, queue, &mut c, &data);
}

/// The window size of a port changed (sent when the control queue has a buffer for it)
#[no_mangle]
pub unsafe fn virtio_console_resize(d: i32, port: i32, cols: i32, rows: i32) {
    let d = d as usize;
    if port == 0 {
        virtio_console_set_size(d as i32, cols, rows);
    }
    let q = &fields(d)[DEVICE_FIELDS + 2 * QUEUE_FIELDS..DEVICE_FIELDS + 3 * QUEUE_FIELDS];
    let configured = q[Q_DESC] != 0 && q[Q_AVAIL] != 0 && q[Q_USED] != 0;
    if configured && has_request(d, 2) {
        let c = CONSOLES[d];
        window_size(d, port as u32, if cols != 0 { cols as u16 } else { c.cols }, if rows != 0 { rows as u16 } else { c.rows });
    }
}
