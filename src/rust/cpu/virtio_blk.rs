// virtio-blk (src/virtio_blk.js): the requests of its queue (read, write, flush, the device ID), their checks and
// their data; the image is JavaScript's (src/buffer.js, possibly asynchronous): js::virtio_blk_io asks for a read or
// a write of a request's bytes (its buffer here: virtio_blk_buffer) and virtio_blk_done completes it

use crate::cpu::cpu::js;
use crate::cpu::virtio_dev::*;
use std::collections::HashMap;

const T_IN: u32 = 0;
const T_OUT: u32 = 1;
const T_FLUSH: u32 = 4;
const T_GET_ID: u32 = 8;
const S_OK: u8 = 0;
const S_IOERR: u8 = 1;
const S_UNSUPP: u8 = 2;
const SECTOR: u64 = 512;
const ID_BYTES: usize = 20;

struct Pending {
    device: usize,
    queue: usize,
    chain: Chain,
    /// what a read brings, what a write takes
    data: Vec<u8>,
    writable: u32,
    write: bool,
}

#[derive(Default, Clone)]
struct Disk {
    sectors: u64,
    readonly: bool,
    id: [u8; ID_BYTES],
}

static mut DISKS: Vec<Disk> = Vec::new();
static mut PENDING: Option<HashMap<i32, Pending>> = None;
static mut NEXT_ID: i32 = 1;

#[allow(static_mut_refs)]
unsafe fn pending() -> &'static mut HashMap<i32, Pending> { PENDING.get_or_insert_with(HashMap::new) }

#[allow(static_mut_refs)]
unsafe fn disk(d: usize) -> &'static mut Disk {
    while DISKS.len() <= d {
        DISKS.push(Disk::default());
    }
    &mut DISKS[d]
}

/// The device's model is here: the image's size in sectors, whether it is read-only; its ID next (virtio_blk_id)
#[no_mangle]
pub unsafe fn virtio_blk_attach(d: i32, sectors: f64, readonly: bool) {
    let k = disk(d as usize);
    k.sectors = sectors as u64;
    k.readonly = readonly;
    MODEL[d as usize] = MODEL_BLK;
}

/// where the device's ID (20 bytes) goes
#[no_mangle]
pub unsafe fn virtio_blk_id(d: i32) -> u32 { disk(d as usize).id.as_mut_ptr() as u32 }

/// A request's bytes: what a read is to bring (filled by JavaScript), what a write takes
#[no_mangle]
pub unsafe fn virtio_blk_buffer(id: i32) -> u32 {
    match pending().get_mut(&id) {
        Some(p) => p.data.as_mut_ptr() as u32,
        None => 0,
    }
}

/// JavaScript's read or write of request `id` is done (status: 0 ok, else an I/O error)
#[no_mangle]
pub unsafe fn virtio_blk_done(id: i32, status: i32) {
    let Some(mut p) = pending().remove(&id) else { return };
    if status == 0 && !p.data.is_empty() && !p.write {
        // (a read: its bytes to the request's buffers)
        let data = std::mem::take(&mut p.data);
        p.chain.write(&data);
    }
    let (d, q) = (p.device, p.queue);
    complete(d, q, &mut p.chain, p.writable, if status == 0 { S_OK } else { S_IOERR });
}

/// The status of a request: its data padded to what the driver offered, the status byte, the reply
unsafe fn complete(d: usize, queue: usize, c: &mut Chain, writable: u32, status: u8) {
    if c.length_written < writable {
        c.write_zeros((writable - c.length_written) as usize);
    }
    c.write(&[status]);
    push(d, queue, c);
    flush(d, queue);
}

pub unsafe fn process(d: usize, queue: usize) {
    while has_request(d, queue) {
        let mut c = pop(d, queue);
        let mut header = [0u8; 16];
        c.read(&mut header);
        let ty = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
        let sector = u64::from_le_bytes(header[8..16].try_into().unwrap());
        let readable = c.length_readable as i64 - 16;
        let writable = c.length_writable as i64 - 1;
        let k = disk(d).clone();
        let in_range =
            |length: i64| length >= 0 && sector.saturating_mul(SECTOR).saturating_add(length as u64) <= k.sectors * SECTOR && length as u64 % SECTOR == 0;
        let w = writable.max(0) as u32;
        match ty {
            T_IN => {
                if writable < 0 || !in_range(writable) {
                    complete(d, queue, &mut c, w, S_IOERR);
                }
                else if writable == 0 {
                    complete(d, queue, &mut c, w, S_OK);
                }
                else {
                    start(d, queue, c, vec![0; writable as usize], w, false, sector * SECTOR);
                }
            },
            T_OUT => {
                if k.readonly || !in_range(readable) {
                    complete(d, queue, &mut c, w, S_IOERR);
                }
                else {
                    let mut data = vec![0; readable as usize];
                    c.read(&mut data);
                    start(d, queue, c, data, w, true, sector * SECTOR);
                }
            },
            T_FLUSH => complete(d, queue, &mut c, w, S_OK),
            T_GET_ID => {
                let n = (writable.max(0) as usize).min(ID_BYTES);
                c.write(&k.id[..n]);
                complete(d, queue, &mut c, w, S_OK);
            },
            _ => complete(d, queue, &mut c, w, S_UNSUPP),
        }
    }
}

/// A read or write for JavaScript (which may complete it right away, from inside this call)
unsafe fn start(d: usize, queue: usize, chain: Chain, data: Vec<u8>, writable: u32, write: bool, offset: u64) {
    let id = NEXT_ID;
    NEXT_ID = NEXT_ID.wrapping_add(1).max(1);
    let length = data.len() as i32;
    // (a write's data stays for JavaScript to take; a read's buffer is filled by it)
    pending().insert(id, Pending { device: d, queue, chain, data, writable, write });
    js::virtio_blk_io(d as i32, id, write, offset as f64, length);
}
