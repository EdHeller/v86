// The data stage of the USB disks' Bulk-Only Transport (src/usb/mass_storage.js) in the core: what the image's reads
// gave goes to the host packet by packet from here, and the host's data for the image collects here, so that the
// core's controllers (uhci.rs, xhci.rs) need JavaScript once per piece of the image (64 KiB) and per command, not per
// packet. mass_storage.js keeps the rest (control transfers, CBW, SCSI, CSW, the block backend): it hands a data stage
// to the core (fields) and takes back what was done before it acts itself. A packet that would end the data stage or
// fill the piece being written is left to it.

const DISKS: usize = 8;
/// a read ahead while the host takes the piece before (mass_storage.js's CHUNK)
const CHUNK: u32 = 64 << 10;
const USB_NAK: i32 = -2;
const PID_IN: i32 = 0x69;
const PID_OUT: i32 = 0xE1;

/// what mass_storage.js and the core share, per disk (an Int32Array there)
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Fields {
    /// the port of the core's controller it is in, or -1
    port: i32,
    address: i32,
    /// the data stage here: 0 none, 1 to the host, 2 from it
    active: i32,
    /// bytes of the data stage the host still moves, and how many of them are the device's
    host_left: u32,
    device_left: u32,
    /// a read of the image under way; host I/O under way (OUT NAKs at 2); more of the image to read
    reading: i32,
    io: i32,
    read_more: i32,
    /// bytes the piece being written still takes (0: mass_storage.js takes the next packet)
    chunk_room: u32,
    /// bytes of the data stage done here since mass_storage.js last looked
    done: u32,
    /// the queue is low and more is to be read: mass_storage.js reads ahead after the slice
    need_read: i32,
}

const NO_FIELDS: Fields = Fields {
    port: -1,
    address: 0,
    active: 0,
    host_left: 0,
    device_left: 0,
    reading: 0,
    io: 0,
    read_more: 0,
    chunk_room: 0,
    done: 0,
    need_read: 0,
};

static mut FIELDS: [Fields; DISKS] = [NO_FIELDS; DISKS];
static mut USED: usize = 0;

#[derive(Default)]
struct Buffers {
    /// read and not taken yet, from head on
    queue: Vec<u8>,
    head: usize,
    /// from the host, not written yet
    out: Vec<u8>,
}

static mut BUFFERS: Vec<Buffers> = Vec::new();
/// what storage_take and storage_take_out hand JavaScript
static mut SCRATCH: Vec<u8> = Vec::new();

#[allow(static_mut_refs)]
unsafe fn buffers(d: usize) -> &'static mut Buffers {
    while BUFFERS.len() <= d {
        BUFFERS.push(Buffers::default());
    }
    &mut BUFFERS[d]
}

impl Buffers {
    fn queued(&self) -> usize { self.queue.len() - self.head }
    fn take_into(&mut self, to: *mut u8, n: usize) {
        unsafe { std::ptr::copy_nonoverlapping(self.queue.as_ptr().add(self.head), to, n) };
        self.head += n;
        if self.head == self.queue.len() {
            self.queue.clear();
            self.head = 0;
        }
    }
}

/// From a core controller, before it asks JavaScript: a disk's data stage packet when the disk is in one of the
/// ports of `port_mask` (at `address`, or any: -1) and the core has its data stage
#[inline]
#[allow(static_mut_refs)]
pub unsafe fn route(port_mask: u32, address: i32, pid: i32, endpoint: i32, data: *mut u8, length: usize) -> Option<i32> {
    for d in 0..USED {
        let f = &mut FIELDS[d];
        if f.active == 0 || f.port < 0 || port_mask & 1 << f.port == 0 || address >= 0 && address != f.address {
            continue;
        }
        let n = (length as u32).min(f.host_left);
        // (the stage's last packet: mass_storage.js's, with its CSW to follow)
        if n == f.host_left {
            return None;
        }
        let device = n.min(f.device_left);
        let b = buffers(d);
        if f.active == 1 && pid == PID_IN && endpoint == 1 {
            if device as usize > b.queued() {
                return if f.reading != 0 { Some(USB_NAK) } else { None };
            }
            b.take_into(data, device as usize);
            // (the device's bytes, then zeros up to the CBW's length)
            std::ptr::write_bytes(data.add(device as usize), 0, (n - device) as usize);
            f.host_left -= n;
            f.device_left -= device;
            f.done += n;
            if f.read_more != 0 && f.reading == 0 && (b.queued() as u32) < CHUNK {
                f.need_read = 1;
            }
            return Some(n as i32);
        }
        if f.active == 2 && pid == PID_OUT && endpoint == 2 {
            // (two pieces being written: the host waits)
            if f.io >= 2 {
                return Some(USB_NAK);
            }
            if device >= f.chunk_room && device != 0 {
                return None;
            }
            b.out.extend_from_slice(std::slice::from_raw_parts(data, device as usize));
            f.chunk_room -= device;
            f.host_left -= n;
            f.device_left -= device;
            f.done += n;
            return Some(length as i32);
        }
        return None;
    }
    None
}

// ---------------------------------------------------------------------------------------------------------------
// mass_storage.js's interface

/// a new disk: its index
#[no_mangle]
pub unsafe fn storage_new() -> i32 {
    if USED == DISKS {
        return -1;
    }
    FIELDS[USED] = NO_FIELDS;
    USED += 1;
    buffers(USED - 1);
    USED as i32 - 1
}

#[no_mangle]
pub unsafe fn storage_fields(d: i32) -> u32 { &raw mut FIELDS[d as usize] as u32 }

/// room for `n` more bytes at the queue's end: where to put them
#[no_mangle]
pub unsafe fn storage_push(d: i32, n: u32) -> u32 {
    let b = buffers(d as usize);
    if b.head > 0 && b.head * 2 > b.queue.len() {
        b.queue.drain(..b.head);
        b.head = 0;
    }
    let at = b.queue.len();
    b.queue.resize(at + n as usize, 0);
    b.queue.as_mut_ptr().add(at) as u32
}

#[no_mangle]
pub unsafe fn storage_queued(d: i32) -> u32 { buffers(d as usize).queued() as u32 }

/// the queue's first `n` bytes, taken (where they are now)
#[no_mangle]
#[allow(static_mut_refs)]
pub unsafe fn storage_take(d: i32, n: u32) -> u32 {
    let b = buffers(d as usize);
    let n = (n as usize).min(b.queued());
    SCRATCH.clear();
    SCRATCH.resize(n, 0);
    b.take_into(SCRATCH.as_mut_ptr(), n);
    SCRATCH.as_ptr() as u32
}

#[no_mangle]
pub unsafe fn storage_clear(d: i32) {
    let b = buffers(d as usize);
    b.queue.clear();
    b.head = 0;
    b.out.clear();
}

#[no_mangle]
pub unsafe fn storage_out_length(d: i32) -> u32 { buffers(d as usize).out.len() as u32 }

/// the host's data collected here, taken (where it is now: storage_out_length bytes)
#[no_mangle]
#[allow(static_mut_refs)]
pub unsafe fn storage_take_out(d: i32) -> u32 {
    let b = buffers(d as usize);
    SCRATCH.clear();
    SCRATCH.extend_from_slice(&b.out);
    b.out.clear();
    SCRATCH.as_ptr() as u32
}
