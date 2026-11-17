// DMA controllers (two 8237): the channels' address, count, page, mask and mode registers. Was src/dma.js, which
// keeps the transfers (they go through JavaScript's asynchronous disk and sound buffers), the listeners for a
// channel being unmasked and the snapshot format.


// (the layout is read by src/dma.js)
#[repr(C)]
pub struct Dma {
    channel_addr: [u16; 8],       // 0
    channel_addr_init: [u16; 8],  // 16
    channel_count: [u16; 8],      // 32
    channel_count_init: [u16; 8], // 48
    channel_page: [u8; 8],        // 64
    channel_pagehi: [u8; 8],      // 72
    channel_mask: [u8; 8],        // 80
    channel_mode: [u8; 8],        // 88
    lsb_msb_flipflop: u8,         // 96
    present: u8,                  // 97
}

static mut DMA: Dma = Dma {
    channel_addr: [0; 8],
    channel_addr_init: [0; 8],
    channel_count: [0; 8],
    channel_count_init: [0; 8],
    channel_page: [0; 8],
    channel_pagehi: [0; 8],
    channel_mask: [0; 8],
    channel_mode: [0; 8],
    lsb_msb_flipflop: 0,
    present: 0,
};

#[allow(static_mut_refs)]
fn dma() -> &'static mut Dma { unsafe { &mut DMA } }

#[no_mangle]
pub fn get_dma_addr() -> u32 { &raw mut DMA as u32 }

#[no_mangle]
pub unsafe fn dma_init() {
    let d = dma();
    d.channel_addr = [0; 8];
    d.channel_addr_init = [0; 8];
    d.channel_count = [0; 8];
    d.channel_count_init = [0; 8];
    d.channel_page = [0; 8];
    d.channel_pagehi = [0; 8];
    d.channel_mask = [0; 8];
    d.channel_mode = [0; 8];
    d.lsb_msb_flipflop = 0;
    d.present = 1;
}

/// What a port of the controllers is
enum Reg {
    Addr(usize),
    Count(usize),
    Page(usize),
    PageHi(usize),
    SingleMask(usize),
    MultiMask(usize),
    Mode(usize),
    FlipFlop,
}

fn decode(port: i32) -> Option<Reg> {
    Some(match port {
        0x00 | 0x02 | 0x04 | 0x06 => Reg::Addr((port >> 1) as usize),
        0x01 | 0x03 | 0x05 | 0x07 => Reg::Count((port >> 1) as usize),
        0xC0 | 0xC4 | 0xC8 | 0xCC => Reg::Addr(4 + ((port - 0xC0) >> 2) as usize),
        0xC2 | 0xC6 | 0xCA | 0xCE => Reg::Count(4 + ((port - 0xC2) >> 2) as usize),
        0x87 => Reg::Page(0),
        0x83 => Reg::Page(1),
        0x81 => Reg::Page(2),
        0x82 => Reg::Page(3),
        0x8F => Reg::Page(4),
        0x8B => Reg::Page(5),
        0x89 => Reg::Page(6),
        0x8A => Reg::Page(7),
        0x487 => Reg::PageHi(0),
        0x483 => Reg::PageHi(1),
        0x481 => Reg::PageHi(2),
        0x482 => Reg::PageHi(3),
        0x48B => Reg::PageHi(5),
        0x489 => Reg::PageHi(6),
        0x48A => Reg::PageHi(7),
        0x0A => Reg::SingleMask(0),
        0xD4 => Reg::SingleMask(4),
        0x0F => Reg::MultiMask(0),
        0xDE => Reg::MultiMask(4),
        0x0B => Reg::Mode(0),
        0xD6 => Reg::Mode(4),
        0x0C | 0xD8 => Reg::FlipFlop,
        _ => return None,
    })
}

impl Dma {
    fn flipflop_get(&mut self, old: u16, new_byte: u8, continuing: bool) -> u16 {
        if !continuing {
            self.lsb_msb_flipflop ^= 1;
        }
        if self.lsb_msb_flipflop != 0 {
            old & !0xFF | new_byte as u16
        }
        else {
            old & !0xFF00 | (new_byte as u16) << 8
        }
    }

    fn flipflop_read(&mut self, word: u16) -> i32 {
        self.lsb_msb_flipflop ^= 1;
        if self.lsb_msb_flipflop != 0 {
            (word & 0xFF) as i32
        }
        else {
            (word >> 8 & 0xFF) as i32
        }
    }
}

/// The controllers' ports, also for src/dma.js. Reads of ports that cannot be read return -1
#[no_mangle]
pub unsafe fn dma_port_read(port: i32) -> i32 {
    let d = dma();
    match decode(port) {
        Some(Reg::Addr(c)) => d.flipflop_read(d.channel_addr[c]),
        Some(Reg::Count(c)) => d.flipflop_read(d.channel_count[c]),
        Some(Reg::Page(c)) => d.channel_page[c] as i32,
        Some(Reg::PageHi(c)) => d.channel_pagehi[c] as i32,
        Some(Reg::MultiMask(o)) => {
            d.channel_mask[o] as i32
                | (d.channel_mask[o + 1] as i32) << 1
                | (d.channel_mask[o + 2] as i32) << 2
                | (d.channel_mask[o + 3] as i32) << 3
        },
        _ => -1,
    }
}

#[no_mangle]
pub unsafe fn dma_port_write(port: i32, value: i32) {
    let d = dma();
    let data_byte = value as u8;
    match decode(port) {
        Some(Reg::Addr(c)) => {
            d.channel_addr[c] = d.flipflop_get(d.channel_addr[c], data_byte, false);
            d.channel_addr_init[c] = d.flipflop_get(d.channel_addr_init[c], data_byte, true);
        },
        Some(Reg::Count(c)) => {
            d.channel_count[c] = d.flipflop_get(d.channel_count[c], data_byte, false);
            d.channel_count_init[c] = d.flipflop_get(d.channel_count_init[c], data_byte, true);
        },
        Some(Reg::Page(c)) => d.channel_page[c] = data_byte,
        Some(Reg::PageHi(c)) => d.channel_pagehi[c] = data_byte,
        Some(Reg::SingleMask(o)) => d.channel_mask[(data_byte & 3) as usize + o] = (data_byte & 4 != 0) as u8,
        Some(Reg::MultiMask(o)) => {
            for i in 0..4 {
                // (as the JavaScript device stored it: the bit itself, not 0 or 1)
                d.channel_mask[o + i] = data_byte & 1 << i;
            }
        },
        Some(Reg::Mode(o)) => d.channel_mode[(data_byte & 3) as usize + o] = data_byte,
        Some(Reg::FlipFlop) => d.lsb_msb_flipflop = 0,
        None => {},
    }
}

/// Whether the core answers this port itself (not in a vCPU worker: the machine's instance has the devices). The mask registers are written through
/// JavaScript: devices there listen for a channel being unmasked
#[inline]
pub unsafe fn serves_read(port: i32) -> bool {
    (port < 0x10 || (0x81..=0x8F).contains(&port) || (0xC0..=0xDE).contains(&port) || (0x481..=0x48B).contains(&port))
        && dma().present != 0
        && !super::parallel::WORKER
        && matches!(decode(port), Some(Reg::Addr(_) | Reg::Count(_) | Reg::Page(_) | Reg::PageHi(_) | Reg::MultiMask(_)))
}
#[inline]
pub unsafe fn serves_write(port: i32) -> bool {
    (port < 0x10 || (0x81..=0x8F).contains(&port) || (0xC0..=0xDE).contains(&port) || (0x481..=0x48B).contains(&port))
        && dma().present != 0
        && !super::parallel::WORKER
        && matches!(decode(port), Some(Reg::Addr(_) | Reg::Count(_) | Reg::Page(_) | Reg::PageHi(_) | Reg::Mode(_) | Reg::FlipFlop))
}


/// A device reads `len` bytes from memory through `channel` (memory to device: dma.js's do_write) into `out` from
/// `start`: as many as the channel's count (or `len`, when the count is larger), the channel's address and count
/// stepped, reloaded on autoinit, and while the device wants more and the channel autoinitialises, again from its
/// start. Returns false when the transfer would go past `out`.
pub unsafe fn transfer_to_device(channel: usize, out: &mut [u8], start: usize, len: usize) -> bool {
    let d = dma();
    let bytes_per_count = if channel >= 5 { 2 } else { 1 };
    let mut read_count = (d.channel_count[channel] as usize + 1) & 0xFFFF;
    let mut read_bytes = read_count * bytes_per_count;
    let addr = address_8bit(channel);
    let mut unfinished = false;
    let mut want_more = false;
    let autoinit = d.channel_mode[channel] & 0x10 != 0;
    if len < read_bytes {
        read_count = len / bytes_per_count;
        read_bytes = read_count * bytes_per_count;
        unfinished = true;
    }
    else if len > read_bytes {
        want_more = true;
    }
    if start + read_bytes > out.len() {
        return false;
    }
    d.channel_addr[channel] = d.channel_addr[channel].wrapping_add(read_count as u16);
    d.channel_count[channel] = d.channel_count[channel].wrapping_sub(read_count as u16);
    if !unfinished && autoinit {
        d.channel_addr[channel] = d.channel_addr_init[channel];
        d.channel_count[channel] = d.channel_count_init[channel];
    }
    for i in 0..read_bytes {
        let a = addr as u64 + i as u64;
        out[start + i] = if a < *crate::cpu::global_pointers::memory_size as u64 { *crate::cpu::memory::mem8.add(a as usize) } else { 0 };
    }
    if want_more && autoinit {
        return transfer_to_device(channel, out, start + read_bytes, len - read_bytes);
    }
    true
}

/// The address of a channel's next byte (dma.js's address_get_8bit)
pub unsafe fn address_8bit(channel: usize) -> u32 {
    let d = dma();
    let mut addr = d.channel_addr[channel] as u32;
    let mut page = d.channel_page[channel] as u32;
    if channel >= 5 {
        addr <<= 1;
        page &= 0xFE;
    }
    addr | page << 16 | (d.channel_pagehi[channel] as u32) << 24
}

/// Whether a channel is masked
pub unsafe fn masked(channel: usize) -> bool { dma().channel_mask[channel] != 0 }
