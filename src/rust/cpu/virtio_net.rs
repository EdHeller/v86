// virtio-net (src/virtio_net.js): frames the driver sends (queue 1: to JavaScript's network adapter, without their
// 12-byte header), frames JavaScript received (to queue 0, with the header), the control queue (2: the number of
// queue pairs, the MAC address). Its configuration (the MAC, status, MTU) is virtio_net.js's.

use crate::cpu::cpu::js;
use crate::cpu::virtio_dev::*;

const HEADER: usize = 12;
const CTRL_MQ: u16 = 4 << 8;
const CTRL_MAC_ADDR_SET: u16 = 1 << 8 | 1;

/// what js::virtio_model_event tells of virtio-net
pub const EVENT_SEND: i32 = 0;
pub const EVENT_MAC: i32 = 1;

#[no_mangle]
pub unsafe fn virtio_net_attach(d: i32) { MODEL[d as usize] = MODEL_NET; }

pub unsafe fn notify(d: usize, queue: usize) {
    match queue {
        1 => {
            while has_request(d, queue) {
                let mut c = pop(d, queue);
                let mut frame = vec![0u8; c.length_readable as usize];
                c.read(&mut frame);
                if frame.len() >= HEADER {
                    js::virtio_model_event(d as i32, EVENT_SEND, frame.as_ptr().add(HEADER) as u32, (frame.len() - HEADER) as i32);
                }
                push(d, queue, &c);
            }
            flush(d, queue);
        },
        2 => {
            while has_request(d, queue) {
                let mut c = pop(d, queue);
                let mut command = vec![0u8; c.length_readable as usize];
                c.read(&mut command);
                let class_command = if command.len() >= 2 { (command[0] as u16) << 8 | command[1] as u16 } else { 0 };
                let status = match class_command {
                    // (one pair of queues)
                    c if c & 0xFF00 == CTRL_MQ && c & 0xFF == 0 => 0u8,
                    CTRL_MAC_ADDR_SET if command.len() >= 8 => {
                        js::virtio_model_event(d as i32, EVENT_MAC, command.as_ptr().add(2) as u32, 6);
                        0
                    },
                    _ => 1,
                };
                c.write(&[status]);
                push(d, queue, &c);
                flush(d, queue);
                if status != 0 {
                    return;
                }
            }
        },
        // (the receive queue: buffers for later)
        _ => {},
    }
}

/// A frame JavaScript received (in virtio_scratch): to the receive queue with its header; false when the driver
/// has no buffer for it
#[no_mangle]
pub unsafe fn virtio_net_receive(d: i32, length: i32) -> bool {
    let d = d as usize;
    let mut with_header = vec![0u8; HEADER + length as usize];
    // (num_buffers, as virtio_net.js wrote it)
    with_header[10] = 0;
    with_header[11] = 1;
    std::ptr::copy_nonoverlapping(scratch(), with_header.as_mut_ptr().add(HEADER), length as usize);
    if !has_request(d, 0) {
        return false;
    }
    let mut c = pop(d, 0);
    c.write(&with_header);
    push(d, 0, &c);
    flush(d, 0);
    true
}
