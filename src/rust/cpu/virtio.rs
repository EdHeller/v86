// Virtio split virtqueues: walking a request's descriptor chain (with indirect tables), the available ring, the used
// ring and the interrupt decision (with or without VIRTIO_F_RING_EVENT_IDX). Was VirtQueue and
// VirtQueueBufferChain in src/virtio.js, which keep each queue's addresses and indices and the device models, and
// call these once per operation instead of reading and writing the rings a field at a time.

use crate::cpu::memory::{read16, read32s, write16, write32};

const VIRTQ_DESC_ENTRYSIZE: u32 = 16;
const VIRTQ_AVAIL_ENTRYSIZE: u32 = 2;
const VIRTQ_USED_ENTRYSIZE: u32 = 8;
const VIRTQ_DESC_F_NEXT: i32 = 1;
const VIRTQ_DESC_F_WRITE: i32 = 2;
const VIRTQ_DESC_F_INDIRECT: i32 = 4;
const VIRTQ_AVAIL_F_NO_INTERRUPT: i32 = 1;

/// the descriptors of the last chain: addr_low, addr_high, len, flags for each
pub const CHAIN_MAX: usize = 4096;
static mut CHAIN: [i32; 4 * CHAIN_MAX] = [0; 4 * CHAIN_MAX];

#[no_mangle]
pub fn virtio_chain_addr() -> u32 { &raw mut CHAIN as u32 }

/// The descriptors of the chain at head_idx, the readable ones then the writable ones, into the chain buffer
/// (virtio_chain_addr); returns how many. As VirtQueueBufferChain: a readable descriptor after a writable one ends
/// the chain, so does a chain longer than the queue (or the indirect table)
#[no_mangle]
pub unsafe fn virtio_read_chain(desc_addr: u32, size: i32, head_idx: i32, indirect: bool) -> i32 {
    let mut table_address = desc_addr;
    let mut desc_idx = head_idx as u32;
    let mut chain_length = 0;
    let mut chain_max = size;
    let mut writable_region = false;
    let mut n = 0;
    loop {
        let at = table_address.wrapping_add(desc_idx.wrapping_mul(VIRTQ_DESC_ENTRYSIZE));
        let addr_low = read32s(at);
        let addr_high = read32s(at + 4);
        let len = read32s(at + 8);
        let flags = read16(at + 12);
        let next = read16(at + 14);

        if indirect && flags & VIRTQ_DESC_F_INDIRECT != 0 {
            table_address = addr_low as u32;
            desc_idx = 0;
            chain_length = 0;
            chain_max = (len as u32 / VIRTQ_DESC_ENTRYSIZE) as i32;
            continue;
        }
        if flags & VIRTQ_DESC_F_WRITE != 0 {
            writable_region = true;
        }
        else if writable_region {
            // readable after writable: not allowed, the chain ends
            break;
        }
        if n >= CHAIN_MAX {
            break;
        }
        CHAIN[4 * n] = addr_low;
        CHAIN[4 * n + 1] = addr_high;
        CHAIN[4 * n + 2] = len;
        CHAIN[4 * n + 3] = flags;
        n += 1;

        chain_length += 1;
        if chain_length > chain_max {
            break;
        }
        if flags & VIRTQ_DESC_F_NEXT != 0 {
            desc_idx = next as u32;
        }
        else {
            break;
        }
    }
    n as i32
}

/// The requests the driver made available and the device has not taken
#[no_mangle]
pub unsafe fn virtio_count_requests(avail_addr: u32, avail_last_idx: i32) -> i32 { (read16(avail_addr + 2) - avail_last_idx) & 0xFFFF }

#[no_mangle]
pub unsafe fn virtio_avail_idx(avail_addr: u32) -> i32 { read16(avail_addr + 2) }

/// The head of the available ring's entry i
#[no_mangle]
pub unsafe fn virtio_avail_entry(avail_addr: u32, i: i32, mask: i32) -> i32 {
    read16(avail_addr + 4 + VIRTQ_AVAIL_ENTRYSIZE * (i & mask) as u32)
}

/// A reply in the used ring, num_staged after the current index (not yet visible to the driver)
#[no_mangle]
pub unsafe fn virtio_push_reply(used_addr: u32, mask: i32, num_staged: i32, head_idx: i32, length_written: i32) {
    let i = (read16(used_addr + 2) + num_staged & mask) as u32;
    write32(used_addr + 4 + VIRTQ_USED_ENTRYSIZE * i, head_idx);
    write32(used_addr + 8 + VIRTQ_USED_ENTRYSIZE * i, length_written);
}

/// Makes the staged replies visible; returns whether the driver wants an interrupt for them
#[no_mangle]
pub unsafe fn virtio_flush_replies(used_addr: u32, avail_addr: u32, size: i32, num_staged: i32, event_idx: bool) -> bool {
    let old_idx = read16(used_addr + 2);
    let new_idx = old_idx + num_staged & 0xFFFF;
    write16(used_addr + 2, new_idx);
    if event_idx {
        // (the interrupt would be due when the indices of the replies reached or passed used_event; it is raised
        // always: a workaround of src/virtio.js for loads from the filesystem that hung with the emulator idle)
        let _ = (size, avail_addr);
        true
    }
    else {
        !read16(avail_addr) & VIRTQ_AVAIL_F_NO_INTERRUPT != 0
    }
}

/// Asks the driver to notify after num_skipped more requests (VIRTIO_F_RING_EVENT_IDX's avail_event)
#[no_mangle]
pub unsafe fn virtio_notify_me_after(used_addr: u32, avail_addr: u32, size: i32, num_skipped: i32) {
    let avail_event = read16(avail_addr + 2) + num_skipped & 0xFFFF;
    write16(used_addr + 4 + VIRTQ_USED_ENTRYSIZE * size as u32, avail_event);
}
