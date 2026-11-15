// The VGA's planar memory and its host interface at 0xA0000-0xBFFFF: the read modes and latches, the write modes
// (rotate, set/reset, logical operation, bit mask), chain 4 and odd/even, the plane masks. Was part of src/vga.js,
// which keeps the register ports and the drawing: it sets the registers here through accessors, reads the planes
// through views, and after each slice of the main loop redraws what the writes here marked (text cells, a range
// of graphics addresses).

const VGA_BANK_SIZE: usize = 64 * 1024;
const HOST_SPACE_START: [u32; 4] = [0xA0000, 0xA0000, 0xB0000, 0xB8000];
const HOST_SPACE_SIZE: [u32; 4] = [0x20000, 0x10000, 0x8000, 0x8000];

// (the layout is read by src/vga.js: 32-bit fields, the dirty bitmap of text cells, then the planes)
#[repr(C)]
pub struct VgaLegacy {
    svga_enabled: i32,                    // 0
    miscellaneous_graphics_register: i32, // 1
    planar_mode: i32,                     // 2
    color_dont_care: i32,                 // 3
    color_compare: i32,                   // 4
    plane_read: i32,                      // 5
    graphical_mode: i32,                  // 6
    sequencer_memory_mode: i32,           // 7
    plane_write_bm: i32,                  // 8
    planar_bitmap: i32,                   // 9
    planar_setreset: i32,                 // 10
    planar_setreset_enable: i32,          // 11
    planar_rotate_reg: i32,               // 12
    latch_dword: i32,                     // 13
    /// the graphics addresses written since JavaScript last looked (min > max: none)
    dirty_min: i32, // 14
    dirty_max: i32, // 15
    /// text cells written since JavaScript last looked
    text_dirty: i32, // 16
    present: i32,    // 17
    /// one bit per text cell (two bytes of plane 0)
    text_dirty_bits: [u32; VGA_BANK_SIZE / 2 / 32], // 72
    memory: [u8; 4 * VGA_BANK_SIZE],                 // 72 + 4096
}

static mut VGA: VgaLegacy = VgaLegacy {
    svga_enabled: 0,
    miscellaneous_graphics_register: 0,
    planar_mode: 0,
    color_dont_care: 0,
    color_compare: 0,
    plane_read: 0,
    graphical_mode: 0,
    sequencer_memory_mode: 0,
    plane_write_bm: 0,
    planar_bitmap: 0,
    planar_setreset: 0,
    planar_setreset_enable: 0,
    planar_rotate_reg: 0,
    latch_dword: 0,
    dirty_min: i32::MAX,
    dirty_max: -1,
    text_dirty: 0,
    present: 0,
    text_dirty_bits: [0; VGA_BANK_SIZE / 2 / 32],
    memory: [0; 4 * VGA_BANK_SIZE],
};

#[allow(static_mut_refs)]
fn vga() -> &'static mut VgaLegacy { unsafe { &mut VGA } }

#[no_mangle]
pub fn get_vga_legacy_addr() -> u32 { &raw mut VGA as u32 }

#[no_mangle]
pub unsafe fn vga_legacy_init() {
    let v = vga();
    v.memory.fill(0);
    v.text_dirty_bits.fill(0);
    v.dirty_min = i32::MAX;
    v.dirty_max = -1;
    v.text_dirty = 0;
    v.present = 1;
}

fn feed(b: u32) -> u32 { b | b << 8 | b << 16 | b << 24 }

fn expand(b: u32) -> u32 {
    (if b & 1 != 0 { 0xFF } else { 0 })
        | (if b & 2 != 0 { 0xFF } else { 0 }) << 8
        | (if b & 4 != 0 { 0xFF } else { 0 }) << 16
        | (if b & 8 != 0 { 0xFF } else { 0 }) << 24
}

impl VgaLegacy {
    /// (past the end of a plane reads 0, as the undefined of a JavaScript typed array did)
    fn plane(&self, n: usize, addr: usize) -> u32 {
        if addr < VGA_BANK_SIZE { self.memory[n * VGA_BANK_SIZE + addr] as u32 } else { 0 }
    }

    /// the address within the selected memory space, or None outside it
    fn host_offset(&self, addr: u32) -> Option<usize> {
        let select = (self.miscellaneous_graphics_register >> 2 & 3) as usize;
        let offset = addr.wrapping_sub(HOST_SPACE_START[select]);
        if offset >= HOST_SPACE_SIZE[select] { None } else { Some(offset as usize) }
    }

    fn read(&mut self, addr: u32) -> i32 {
        let Some(mut addr) = self.host_offset(addr) else { return 0 };
        self.latch_dword =
            (self.plane(0, addr) | self.plane(1, addr) << 8 | self.plane(2, addr) << 16 | self.plane(3, addr) << 24) as i32;

        if self.planar_mode & 0x08 != 0 {
            // read mode 1
            let mut reading = 0xFFu32;
            for p in 0..4 {
                if self.color_dont_care & 1 << p != 0 {
                    let compare = if self.color_compare & 1 << p != 0 { 0xFF } else { 0x00 };
                    reading &= (self.plane(p, addr) ^ !compare) & 0xFF;
                }
            }
            reading as i32
        }
        else {
            // read mode 0
            let mut plane = self.plane_read as usize;
            if self.graphical_mode == 0 {
                // We store all text data linearly and font data in plane 2.
                plane &= 3;
            }
            else if self.sequencer_memory_mode & 0x8 != 0 {
                // Chain 4
                plane = addr & 3;
                addr &= !3;
            }
            else if self.planar_mode & 0x10 != 0 {
                // Odd/Even host read
                plane = addr & 1;
                addr &= !1;
            }
            *self.memory.get((plane << 16 | addr) & (4 * VGA_BANK_SIZE - 1)).unwrap_or(&0) as i32
        }
    }

    fn write(&mut self, addr: u32, value: u8) {
        let Some(addr) = self.host_offset(addr) else { return };
        if self.graphical_mode != 0 {
            self.write_graphical(addr, value as u32);
        }
        else if self.plane_write_bm & 3 == 0 {
            if self.plane_write_bm & 4 != 0 && addr < VGA_BANK_SIZE {
                // write to plane 2 (font-bitmap)
                self.memory[2 * VGA_BANK_SIZE + addr] = value;
            }
        }
        else {
            // text mode: the character and attribute bytes are in plane 0, one cell is two bytes (the window's second
            // 64 KiB reaches plane 1, as it did in JavaScript; no cell there is on the screen)
            self.memory[addr] = value;
            if addr < VGA_BANK_SIZE {
                let cell = addr >> 1;
                self.text_dirty_bits[cell >> 5] |= 1 << (cell & 31);
                self.text_dirty = 1;
            }
        }
    }

    fn rotate(&self, b: u32) -> u32 {
        let wrapped = b | b << 8;
        (wrapped >> (self.planar_rotate_reg & 7)) & 0xFF
    }

    fn logical(&self, data: u32, latch: u32) -> u32 {
        match self.planar_rotate_reg & 0x18 {
            0x08 => data & latch,
            0x10 => data | latch,
            0x18 => data ^ latch,
            _ => data,
        }
    }

    fn bitmask(&self, data: u32, mask: u32) -> u32 { mask & data | !mask & self.latch_dword as u32 }

    fn write_graphical(&mut self, mut addr: usize, value: u32) {
        let mut bitmask = feed(self.planar_bitmap as u32 & 0xFF);
        let setreset = expand(self.planar_setreset as u32);
        let setreset_enable = expand(self.planar_setreset_enable as u32);
        let latch = self.latch_dword as u32;

        // Write modes - see http://www.osdever.net/FreeVGA/vga/graphreg.htm#05
        let plane_dword = match self.planar_mode & 3 {
            0 => {
                let mut d = feed(self.rotate(value));
                d |= setreset_enable & setreset;
                d &= !setreset_enable | setreset;
                let d = self.logical(d, latch);
                self.bitmask(d, bitmask)
            },
            1 => latch,
            2 => {
                let d = self.logical(expand(value), latch);
                self.bitmask(d, bitmask)
            },
            _ => {
                bitmask &= feed(self.rotate(value));
                self.bitmask(setreset, bitmask)
            },
        };

        let mut plane_select = 0xF;
        match self.sequencer_memory_mode & 0xC {
            // Odd/Even (aka chain 2)
            0x0 => {
                plane_select = 0x5 << (addr & 1);
                addr &= !1;
            },
            // Chain 4
            0x8 | 0xC => {
                plane_select = 1 << (addr & 3);
                addr &= !3;
            },
            _ => {},
        }
        // Plane masks take precedence
        plane_select &= self.plane_write_bm;

        if addr < VGA_BANK_SIZE {
            for p in 0..4 {
                if plane_select & 1 << p != 0 {
                    self.memory[p * VGA_BANK_SIZE + addr] = (plane_dword >> (8 * p)) as u8;
                }
            }
        }
        self.dirty_min = self.dirty_min.min(addr as i32);
        self.dirty_max = self.dirty_max.max(addr as i32);
    }
}

/// Whether the core answers this access to the legacy window itself (not in VBE banked mode, which writes the
/// linear frame buffer through JavaScript's handler)
#[inline]
pub unsafe fn serves(addr: u32) -> bool {
    (0xA0000..0xC0000).contains(&addr) && vga().present != 0 && vga().svga_enabled == 0 && !super::parallel::WORKER
}

pub unsafe fn read8(addr: u32) -> i32 { vga().read(addr) }

pub unsafe fn write8(addr: u32, value: i32) { vga().write(addr, value as u8) }
