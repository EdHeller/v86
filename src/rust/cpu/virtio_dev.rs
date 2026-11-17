// What the virtio device models in the core share: a request's buffers (VirtQueueBufferChain of src/virtio.js), taking
// requests from a queue and replying (VirtQueue), on the transport's state (virtio_pci.rs); and the models
// themselves, chosen per device (MODEL): a queue notification goes to its model here, or to src/virtio.js.

use crate::cpu::global_pointers::memory_size;
use crate::cpu::memory::mem8;
use crate::cpu::virtio_pci::*;

const VIRTIO_F_RING_INDIRECT_DESC: u32 = 28;
const VIRTIO_F_RING_EVENT_IDX: u32 = 29;
const VIRTQ_DESC_F_WRITE: i32 = 2;
pub const ISR_QUEUE: i32 = 1;

pub const MODEL_JS: u8 = 0;
pub const MODEL_RNG: u8 = 1;
pub const MODEL_BLK: u8 = 2;
pub const MODEL_NET: u8 = 3;

/// the model of each device (MODEL_JS: virtio.js's)
pub static mut MODEL: [u8; DEVICES] = [MODEL_JS; DEVICES];

pub fn fields(d: usize) -> &'static mut [i32; FIELDS] {
    unsafe { &mut *(virtio_pci_fields(d as i32) as *mut [i32; FIELDS]) }
}

fn qf(d: usize, i: usize) -> &'static mut [i32] {
    let f = fields(d);
    &mut f[DEVICE_FIELDS + QUEUE_FIELDS * i..DEVICE_FIELDS + QUEUE_FIELDS * (i + 1)]
}

pub fn negotiated(d: usize, feature: u32) -> bool {
    fields(d)[F_DRV_FEAT + (feature >> 5) as usize] as u32 & 1 << (feature & 31) != 0
}

/// A request: its readable buffers then its writable ones (guest physical address, length), and where reading and
/// writing them have got to
pub struct Chain {
    pub head: i32,
    read: Vec<(u32, u32)>,
    write: Vec<(u32, u32)>,
    read_at: (usize, u32),
    write_at: (usize, u32),
    pub length_readable: u32,
    pub length_writable: u32,
    pub length_written: u32,
}

impl Chain {
    /// the next bytes of the readable buffers into `to`; how many
    pub unsafe fn read(&mut self, to: &mut [u8]) -> usize {
        let mut done = 0;
        while done < to.len() && self.read_at.0 < self.read.len() {
            let (addr, len) = self.read[self.read_at.0];
            let n = ((len - self.read_at.1) as usize).min(to.len() - done);
            let from = addr.wrapping_add(self.read_at.1);
            if from as u64 + n as u64 <= *memory_size as u64 {
                std::ptr::copy_nonoverlapping(mem8.add(from as usize), to.as_mut_ptr().add(done), n);
            }
            done += n;
            self.read_at.1 += n as u32;
            if self.read_at.1 == len {
                self.read_at = (self.read_at.0 + 1, 0);
            }
        }
        done
    }

    /// `from` to the next bytes of the writable buffers; how many
    pub unsafe fn write(&mut self, from: &[u8]) -> usize {
        let mut done = 0;
        while done < from.len() && self.write_at.0 < self.write.len() {
            let (addr, len) = self.write[self.write_at.0];
            let n = ((len - self.write_at.1) as usize).min(from.len() - done);
            let to = addr.wrapping_add(self.write_at.1);
            if to as u64 + n as u64 <= *memory_size as u64 {
                std::ptr::copy_nonoverlapping(from.as_ptr().add(done), mem8.add(to as usize), n);
                crate::jit::jit_dirty_cache(to, to + n as u32);
            }
            done += n;
            self.write_at.1 += n as u32;
            if self.write_at.1 == len {
                self.write_at = (self.write_at.0 + 1, 0);
            }
        }
        self.length_written += done as u32;
        done
    }

    /// `n` bytes of zeros to the writable buffers
    pub unsafe fn write_zeros(&mut self, n: usize) {
        let zeros = [0u8; 4096];
        let mut left = n;
        while left > 0 {
            let k = left.min(zeros.len());
            if self.write(&zeros[..k]) == 0 {
                break;
            }
            left -= k;
        }
    }
}

/// Whether queue `i` has requests the device has not taken
pub unsafe fn has_request(d: usize, i: usize) -> bool {
    let q = qf(d, i);
    if q[Q_FIX_WRAPPING] != 0 {
        q[Q_FIX_WRAPPING] = 0;
        let mask = q[Q_SIZE] - 1;
        q[Q_AVAIL_LAST] = (super::virtio::virtio_avail_idx(q[Q_AVAIL] as u32) & !mask) + (q[Q_AVAIL_LAST] & mask);
    }
    super::virtio::virtio_count_requests(q[Q_AVAIL] as u32, q[Q_AVAIL_LAST]) != 0
}

/// The next request of queue `i`
pub unsafe fn pop(d: usize, i: usize) -> Chain {
    let q = qf(d, i);
    let mask = q[Q_SIZE] - 1;
    let head = super::virtio::virtio_avail_entry(q[Q_AVAIL] as u32, q[Q_AVAIL_LAST], mask);
    q[Q_AVAIL_LAST] = (q[Q_AVAIL_LAST] + 1) & 0xFFFF;
    let n = super::virtio::virtio_read_chain(q[Q_DESC] as u32, q[Q_SIZE], head, negotiated(d, VIRTIO_F_RING_INDIRECT_DESC));
    let descs = std::slice::from_raw_parts(super::virtio::virtio_chain_addr() as *const i32, 4 * n as usize);
    let mut c = Chain {
        head,
        read: Vec::new(),
        write: Vec::new(),
        read_at: (0, 0),
        write_at: (0, 0),
        length_readable: 0,
        length_writable: 0,
        length_written: 0,
    };
    for k in 0..n as usize {
        let (addr, len, flags) = (descs[4 * k] as u32, descs[4 * k + 2] as u32, descs[4 * k + 3]);
        if flags & VIRTQ_DESC_F_WRITE != 0 {
            c.write.push((addr, len));
            c.length_writable += len;
        }
        else {
            c.read.push((addr, len));
            c.length_readable += len;
        }
    }
    c
}

/// The reply of a request (seen by the driver after flush)
pub unsafe fn push(d: usize, i: usize, c: &Chain) {
    let q = qf(d, i);
    super::virtio::virtio_push_reply(q[Q_USED] as u32, q[Q_SIZE] - 1, q[Q_STAGED], c.head, c.length_written as i32);
    q[Q_STAGED] += 1;
}

/// The replies to the driver, and the interrupt when it wants one
pub unsafe fn flush(d: usize, i: usize) {
    let q = qf(d, i);
    if q[Q_STAGED] == 0 {
        return;
    }
    let interrupt = super::virtio::virtio_flush_replies(q[Q_USED] as u32, q[Q_AVAIL] as u32, q[Q_SIZE], q[Q_STAGED],
        negotiated(d, VIRTIO_F_RING_EVENT_IDX));
    q[Q_STAGED] = 0;
    if interrupt {
        virtio_pci_raise_irq(d as i32, ISR_QUEUE);
    }
}

/// A queue notification: true when a model here took it
pub unsafe fn notify(d: usize, value: i32) -> bool {
    match MODEL[d] {
        MODEL_RNG => {
            super::virtio_rng::fill(d, (value & 0xFFFF) as usize);
            true
        },
        MODEL_BLK => {
            super::virtio_blk::process(d, (value & 0xFFFF) as usize);
            true
        },
        MODEL_NET => {
            super::virtio_net::notify(d, (value & 0xFFFF) as usize);
            true
        },
        _ => false,
    }
}

/// Bytes JavaScript hands a model (a received frame, typed text, ...): virtio_scratch(length) says where to put them
static mut SCRATCH: Vec<u8> = Vec::new();

#[no_mangle]
#[allow(static_mut_refs)]
pub unsafe fn virtio_scratch(length: u32) -> u32 {
    if SCRATCH.len() < length as usize {
        SCRATCH.resize(length as usize, 0);
    }
    SCRATCH.as_mut_ptr() as u32
}

#[allow(static_mut_refs)]
pub unsafe fn scratch() -> *const u8 { SCRATCH.as_ptr() }
