// virtio-balloon (src/virtio_balloon.js): pages the driver gives up (queue 0) and takes back (1), memory statistics
// (2: to JavaScript), free page hinting (3: the hinted pages zeroed). The target and the counts are here (STATE:
// num_pages, actual, free_page_hint_cmd_id, bytes zeroed); virtio_balloon.js's Inflate, Deflate, Cleanup, GetStats
// and the configuration read them.

use crate::cpu::cpu::js;
use crate::cpu::virtio_dev::*;

pub const EVENT_STATS: i32 = 0;
pub const EVENT_FREE_DONE: i32 = 1;

const NUM_PAGES: usize = 0;
const ACTUAL: usize = 1;
const FP_CMD: usize = 2;
const ZEROED: usize = 3;

static mut STATE: [[i32; 4]; 16] = [[0; 4]; 16];

#[no_mangle]
pub unsafe fn virtio_balloon_attach(d: i32) {
    STATE[d as usize] = [0; 4];
    MODEL[d as usize] = MODEL_BALLOON;
}

/// num_pages, actual, free_page_hint_cmd_id, bytes zeroed (an Int32Array in virtio_balloon.js)
#[no_mangle]
pub unsafe fn virtio_balloon_state(d: i32) -> u32 { &raw mut STATE[d as usize] as u32 }

/// The configuration changed (the target, the free page hint command)
#[no_mangle]
pub unsafe fn virtio_balloon_config_changed(d: i32) { super::virtio_pci::virtio_pci_config_changed(d) }

/// Free page hinting starts
#[no_mangle]
pub unsafe fn virtio_balloon_cleanup(d: i32) {
    let s = &mut STATE[d as usize];
    s[FP_CMD] = 2;
    s[ZEROED] = 0;
    super::virtio_pci::virtio_pci_config_changed(d);
}

/// The statistics buffers back to the driver (it fills them again)
#[no_mangle]
pub unsafe fn virtio_balloon_get_stats(d: i32) {
    let d = d as usize;
    while has_request(d, 2) {
        let c = pop(d, 2);
        push(d, 2, &c);
    }
    flush(d, 2);
}

pub unsafe fn notify(d: usize, queue: usize) {
    let s = &mut STATE[d];
    match queue {
        0 | 1 => {
            while has_request(d, queue) {
                let mut c = pop(d, queue);
                let mut pages = vec![0u8; c.length_readable as usize];
                c.read(&mut pages);
                push(d, queue, &c);
                let n = (pages.len() / 4) as i32;
                s[ACTUAL] += if queue == 0 { n } else { -n };
            }
            flush(d, queue);
        },
        2 => {
            if has_request(d, queue) {
                let mut c = pop(d, queue);
                let mut stats = vec![0u8; c.length_readable as usize];
                c.read(&mut stats);
                // (the reply waits: GetStats flushes)
                push(d, queue, &c);
                js::virtio_model_event(d as i32, EVENT_STATS, stats.as_ptr() as u32, stats.len() as i32);
            }
        },
        3 => {
            while has_request(d, queue) {
                let mut c = pop(d, queue);
                if c.length_readable > 0 {
                    let mut command = vec![0u8; c.length_readable as usize];
                    c.read(&mut command);
                    let cmd = if command.len() >= 4 { u32::from_le_bytes(command[..4].try_into().unwrap()) } else { 1 };
                    if cmd == 0 {
                        let zeroed = s[ZEROED].to_le_bytes();
                        js::virtio_model_event(d as i32, EVENT_FREE_DONE, zeroed.as_ptr() as u32, 4);
                        if s[FP_CMD] > 1 {
                            s[FP_CMD] = 1;
                        }
                        super::virtio_pci::virtio_pci_config_changed(d as i32);
                    }
                }
                if c.length_writable > 0 {
                    s[ZEROED] = s[ZEROED].wrapping_add(c.zero_writable() as i32);
                }
                push(d, queue, &c);
            }
            flush(d, queue);
        },
        _ => {},
    }
}
