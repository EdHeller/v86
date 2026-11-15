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

// ---------------------------------------------------------------------------------------------------------------
// Drawing, once per frame (src/vga.js's screen_fill_buffer): the planes to one palette index per pixel (replot),
// the indices through the attribute controller's and the DAC's palettes to RGBA (redraw), and the 256 colour VBE
// modes' frame buffer through the DAC's palette. Was vga_replot / vga_redraw in src/vga.js, which passes the
// registers they depend on and copies its palettes here before a redraw.

const VGA_PIXEL_BUFFER_SIZE: usize = 8 * VGA_BANK_SIZE;
static mut PIXEL_BUFFER: [u8; VGA_PIXEL_BUFFER_SIZE] = [0; VGA_PIXEL_BUFFER_SIZE];
/// the DAC's 256 colours (0xRRGGBB) and the attribute controller's 16 entries, copied in by JavaScript
static mut PALETTE: [i32; 256] = [0; 256];
static mut DAC_MAP: [u8; 16] = [0; 16];

#[no_mangle]
pub fn vga_pixel_buffer_addr() -> u32 { &raw mut PIXEL_BUFFER as u32 }
#[no_mangle]
pub fn vga_palette_addr() -> u32 { &raw mut PALETTE as u32 }
#[no_mangle]
pub fn vga_dac_map_addr() -> u32 { &raw mut DAC_MAP as u32 }

/// The palette indices of pixels start..=end from the planes (start rounded down and end up to 16 pixels by the
/// caller's rule: see src/vga.js)
#[no_mangle]
#[allow(static_mut_refs)]
pub unsafe fn vga_replot(
    start: i32,
    end: i32,
    addr_shift: i32,
    addr_substitution: i32,
    shift_mode: i32,
    pel_width: bool,
    virtual_width: i32,
    start_address: i32,
) {
    let v = vga();
    let pixels = &mut PIXEL_BUFFER;
    let width = virtual_width.max(1);
    let mut pixel_addr = start;
    while pixel_addr <= end {
        let mut addr = (pixel_addr as u32 >> addr_shift) as i32;
        if addr_substitution != 0 {
            let mut row = pixel_addr / width;
            let col = pixel_addr - width * row;
            match addr_substitution {
                // Alternating rows using bit 13, assumes max scan line = 1
                1 => {
                    addr = (row & 1) << 13;
                    row >>= 1;
                },
                // Alternating rows using bit 14, assumes max scan line = 3
                2 => {
                    addr = (row & 1) << 14;
                    row >>= 1;
                },
                // Cycling through rows using bit 13 and 14, assumes max scan line = 3
                _ => {
                    addr = (row & 3) << 13;
                    row >>= 2;
                },
            }
            addr |= ((row * width + col) as u32 >> addr_shift) as i32 + start_address;
        }

        let a = addr as usize;
        let (byte0, byte1, byte2, byte3) = if addr >= 0 {
            (v.plane(0, a), v.plane(1, a), v.plane(2, a), v.plane(3, a))
        }
        else {
            (0, 0, 0, 0)
        };

        let mut shift_loads = [0u32; 8];
        match shift_mode {
            // Planar Shift Mode, see http://www.osdever.net/FreeVGA/vga/vgaseq.htm
            0x00 => {
                for i in 0..8 {
                    let bit = 7 - i;
                    shift_loads[i] = byte0 >> bit & 1 | (byte1 << 1) >> bit & 2 | (byte2 << 2) >> bit & 4 | (byte3 << 3) >> bit & 8;
                }
            },
            // Packed Shift Mode, aka Interleaved Shift Mode: video modes 4h and 5h
            0x20 => {
                shift_loads[0] = (byte0 >> 6 & 0x3) | (byte2 >> 4 & 0xC);
                shift_loads[1] = (byte0 >> 4 & 0x3) | (byte2 >> 2 & 0xC);
                shift_loads[2] = (byte0 >> 2 & 0x3) | (byte2 & 0xC);
                shift_loads[3] = (byte0 & 0x3) | (byte2 << 2 & 0xC);
                shift_loads[4] = (byte1 >> 6 & 0x3) | (byte3 >> 4 & 0xC);
                shift_loads[5] = (byte1 >> 4 & 0x3) | (byte3 >> 2 & 0xC);
                shift_loads[6] = (byte1 >> 2 & 0x3) | (byte3 & 0xC);
                shift_loads[7] = (byte1 & 0x3) | (byte3 << 2 & 0xC);
            },
            // 256-Color Shift Mode: video mode 13h and unchained 256 colour
            _ => {
                shift_loads = [byte0 >> 4 & 0xF, byte0 & 0xF, byte1 >> 4 & 0xF, byte1 & 0xF, byte2 >> 4 & 0xF, byte2 & 0xF, byte3 >> 4 & 0xF, byte3 & 0xF];
            },
        }

        if pel_width {
            // Assemble from two sets of 4 bits.
            for j in 0..4 {
                if let Some(p) = pixels.get_mut(pixel_addr as usize) {
                    *p = (shift_loads[2 * j] << 4 | shift_loads[2 * j + 1]) as u8;
                }
                pixel_addr += 1;
            }
        }
        else {
            for i in 0..8 {
                if let Some(p) = pixels.get_mut(pixel_addr as usize) {
                    *p = shift_loads[i] as u8;
                }
                pixel_addr += 1;
            }
        }
    }
}

fn rgba(color: i32) -> i32 { color & 0xFF00 | color << 16 | color >> 16 & 0xFF | 0xFF000000u32 as i32 }

/// Pixels start..=end of the palette indices to RGBA at dest (the screen's buffer in this module's memory)
#[no_mangle]
#[allow(static_mut_refs)]
pub unsafe fn vga_redraw(start: i32, end: i32, dest: u32, mask: i32, colorset: i32, eight_bit: bool, color_plane_enable: i32) {
    let out = dest as *mut i32;
    let end = end.min(VGA_PIXEL_BUFFER_SIZE as i32 - 1);
    let mut pixel_addr = start.max(0);
    while pixel_addr <= end {
        let index = PIXEL_BUFFER[pixel_addr as usize] as i32;
        let color256 = if eight_bit {
            (index & mask) | colorset
        }
        else {
            (DAC_MAP[(index & color_plane_enable & 0xF) as usize] as i32 & mask) | colorset
        };
        *out.add(pixel_addr as usize) = rgba(PALETTE[(color256 & 0xFF) as usize]);
        pixel_addr += 1;
    }
}

/// A 256 colour VBE mode: count bytes of the frame buffer at svga + base through the DAC's palette to dest
#[no_mangle]
#[allow(static_mut_refs)]
pub unsafe fn vga_svga8_fill(dest: u32, svga: u32, base: u32, count: u32) {
    let out = dest as *mut i32;
    let src = (svga + base) as *const u8;
    for i in 0..count as usize {
        *out.add(i) = rgba(PALETTE[*src.add(i) as usize]);
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The register ports (0x3C0-0x3CF, 0x3B4/0x3D4, 0x3B5/0x3D5, 0x3BA/0x3DA): the attribute controller, the
// sequencer, the graphics controller, the DAC and its palette, the CRT controller, the status register. The core
// answers every access whose effect is on these registers alone, or a redraw, a replot or the cursor, which it
// marks in `events` for src/vga.js's sync_from_core. Accesses that change the screen's size, layout, mode or font
// (the attribute mode, panning, the memory map, the timing and start address registers, a font plane closed) go
// through JavaScript, which works on the same registers through accessors.

pub const EVENT_REDRAW: i32 = 1;
pub const EVENT_REPLOT: i32 = 2;
pub const EVENT_CURSOR: i32 = 4;
pub const EVENT_CURSOR_SCANLINE: i32 = 8;

// (the layout is read by src/vga.js: 34 32-bit fields, then the other CRT controller registers at 136)
#[repr(C)]
pub struct VgaRegs {
    attribute_controller_index: i32,    // 0
    palette_source: i32,                // 1
    attribute_mode: i32,                // 2
    color_plane_enable: i32,            // 3
    horizontal_panning: i32,            // 4
    color_select: i32,                  // 5
    miscellaneous_output_register: i32, // 6
    sequencer_index: i32,               // 7
    clocking_mode: i32,                 // 8
    character_map_select: i32,          // 9
    dac_mask: i32,                      // 10
    dac_color_index_read: i32,          // 11
    dac_color_index_write: i32,         // 12
    dac_state: i32,                     // 13
    dispi_enable_value: i32,            // 14
    graphics_index: i32,                // 15
    index_crtc: i32,                    // 16
    cursor_scanline_start: i32,         // 17
    cursor_scanline_end: i32,           // 18
    cursor_address: i32,                // 19
    horizontal_display_enable_end: i32, // 20
    horizontal_blank_start: i32,        // 21
    vertical_display_enable_end: i32,   // 22
    vertical_blank_start: i32,          // 23
    line_compare: i32,                  // 24
    preset_row_scan: i32,               // 25
    max_scan_line: i32,                 // 26
    start_address: i32,                 // 27
    offset_register: i32,               // 28
    underline_location_register: i32,   // 29
    crtc_mode: i32,                     // 30
    port_3da_value: i32,                // 31
    events: i32,                        // 32
    present: i32,                       // 33
    crtc: [u8; 0x19],                   // 136
}

static mut REGS: VgaRegs = VgaRegs {
    attribute_controller_index: -1,
    palette_source: 0,
    attribute_mode: 0,
    color_plane_enable: 0,
    horizontal_panning: 0,
    color_select: 0,
    miscellaneous_output_register: 0,
    sequencer_index: 0,
    clocking_mode: 0,
    character_map_select: 0,
    dac_mask: 0,
    dac_color_index_read: 0,
    dac_color_index_write: 0,
    dac_state: 0,
    dispi_enable_value: 0,
    graphics_index: 0,
    index_crtc: 0,
    cursor_scanline_start: 0,
    cursor_scanline_end: 0,
    cursor_address: 0,
    horizontal_display_enable_end: 0,
    horizontal_blank_start: 0,
    vertical_display_enable_end: 0,
    vertical_blank_start: 0,
    line_compare: 0,
    preset_row_scan: 0,
    max_scan_line: 0,
    start_address: 0,
    offset_register: 0,
    underline_location_register: 0,
    crtc_mode: 0,
    port_3da_value: 0,
    events: 0,
    present: 0,
    crtc: [0; 0x19],
};

#[allow(static_mut_refs)]
fn regs() -> &'static mut VgaRegs { unsafe { &mut REGS } }

#[no_mangle]
pub fn get_vga_regs_addr() -> u32 { &raw mut REGS as u32 }

#[no_mangle]
pub unsafe fn vga_regs_init() { regs().present = 1; }

/// Whether the core can take a write of `value` to the data register of the given block at register `index`
/// without JavaScript (see above). block: 0 attribute (data phase), 1 sequencer, 2 graphics, 3 CRT controller
unsafe fn data_write_servable(block: u8, index: i32, value: i32) -> bool {
    let r = regs();
    let v = vga();
    match block {
        0 => index < 0x10 || index == 0x12 || index == 0x14 || index > 0x14 || index == 0x11,
        1 => match index {
            // (closing the font plane in text mode uploads the font)
            0x02 => !(v.graphical_mode == 0 && v.plane_write_bm & 4 != 0 && value & 4 == 0),
            0x04 => true,
            0x01 | 0x03 => false,
            _ => true,
        },
        2 => index != 6,
        _ => match index {
            0xA | 0xB | 0xE | 0xF => true,
            0x1 | 0x2 | 0x7 | 0x8 | 0x9 | 0xC | 0xD | 0x12 | 0x13 | 0x14 | 0x15 | 0x17 | 0x18 => false,
            _ => {
                let _ = r;
                true
            },
        },
    }
}

/// Whether the core answers this access to a register port itself
#[inline]
pub unsafe fn ports_serve(port: i32, size: i32, value: i32, write: bool) -> bool {
    if !(0x3B4..=0x3DA).contains(&port) || regs().present == 0 || super::parallel::WORKER {
        return false;
    }
    let r = regs();
    if size == 32 {
        return false;
    }
    match (port, write, size) {
        (0x3C0, true, 8) => {
            if r.attribute_controller_index == -1 {
                // the index phase: a change of the palette source changes the layers
                (r.palette_source & 0x20) == (value & 0x20)
            }
            else {
                data_write_servable(0, r.attribute_controller_index, value)
            }
        },
        (0x3C0, false, _) => true,
        (0x3C1, false, 8) => true,
        (0x3C2, true, 8) => true,
        (0x3C4 | 0x3CE, true, 8) => true,
        (0x3C4, true, 16) => data_write_servable(1, value & 0xFF, value >> 8 & 0xFF),
        (0x3CE, true, 16) => data_write_servable(2, value & 0xFF, value >> 8 & 0xFF),
        (0x3C5, true, 8) => data_write_servable(1, r.sequencer_index, value),
        (0x3CF, true, 8) => data_write_servable(2, r.graphics_index, value),
        (0x3C4 | 0x3C5 | 0x3CE | 0x3CF, false, 8) => true,
        (0x3C6..=0x3C9, _, 8) => true,
        (0x3C8, true, 16) => true,
        (0x3CC, false, 8) => true,
        (0x3B4 | 0x3D4, true, 8) => true,
        (0x3B4 | 0x3D4, true, 16) => data_write_servable(3, value & 0xFF, value >> 8 & 0xFF),
        (0x3B4 | 0x3D4, false, 8) => true,
        (0x3B5 | 0x3D5, true, _) => data_write_servable(3, r.index_crtc, value & 0xFF),
        (0x3B5 | 0x3D5, false, _) => true,
        (0x3BA | 0x3DA, false, 8) => true,
        _ => false,
    }
}

unsafe fn attribute_read() -> i32 {
    let r = regs();
    let i = r.attribute_controller_index;
    if (0..0x10).contains(&i) {
        return DAC_MAP[i as usize] as i32 & 0xFF;
    }
    match i {
        0x10 => r.attribute_mode,
        0x12 => r.color_plane_enable,
        0x13 => r.horizontal_panning,
        0x14 => r.color_select,
        _ => 0xFF,
    }
}

unsafe fn crtc_read() -> i32 {
    let r = regs();
    match r.index_crtc {
        0x1 => r.horizontal_display_enable_end,
        0x2 => r.horizontal_blank_start,
        0x7 => {
            (r.vertical_display_enable_end >> 7 & 0x2)
                | (r.vertical_blank_start >> 5 & 0x8)
                | (r.line_compare >> 4 & 0x10)
                | (r.vertical_display_enable_end >> 3 & 0x40)
        },
        0x8 => r.preset_row_scan,
        0x9 => r.max_scan_line,
        0xA => r.cursor_scanline_start,
        0xB => r.cursor_scanline_end,
        0xC => r.start_address & 0xFF,
        0xD => r.start_address >> 8,
        0xE => r.cursor_address >> 8,
        0xF => r.cursor_address & 0xFF,
        0x12 => r.vertical_display_enable_end & 0xFF,
        0x13 => r.offset_register,
        0x14 => r.underline_location_register,
        0x15 => r.vertical_blank_start & 0xFF,
        0x17 => r.crtc_mode,
        0x18 => r.line_compare & 0xFF,
        i if (0..0x19).contains(&i) => r.crtc[i as usize] as i32,
        _ => 0,
    }
}

#[no_mangle]
#[allow(static_mut_refs)]
pub unsafe fn vga_port_read(port: i32, size: i32) -> i32 {
    let r = regs();
    let v = vga();
    match port {
        0x3C0 => {
            let low = (r.attribute_controller_index | r.palette_source) & 0xFF;
            if size == 16 { low | attribute_read() << 8 & 0xFF00 } else { low }
        },
        0x3C1 => attribute_read(),
        0x3C4 => r.sequencer_index,
        0x3C5 => match r.sequencer_index {
            0x01 => r.clocking_mode,
            0x02 => v.plane_write_bm,
            0x03 => r.character_map_select,
            0x04 => v.sequencer_memory_mode,
            0x06 => 0x12,
            _ => 0,
        },
        0x3C6 => r.dac_mask,
        0x3C7 => r.dac_state,
        0x3C8 => r.dac_color_index_write / 3 & 0xFF,
        0x3C9 => {
            let index = (r.dac_color_index_read / 3 & 0xFF) as usize;
            let offset = r.dac_color_index_read % 3;
            let color8 = PALETTE[index] >> ((2 - offset) * 8) & 0xFF;
            r.dac_color_index_read = (r.dac_color_index_read + 1) % (256 * 3);
            if r.dispi_enable_value & 0x20 != 0 { color8 } else { color8 >> 2 }
        },
        0x3CC => r.miscellaneous_output_register,
        0x3CE => r.graphics_index,
        0x3CF => match r.graphics_index {
            0 => v.planar_setreset,
            1 => v.planar_setreset_enable,
            2 => v.color_compare,
            3 => v.planar_rotate_reg,
            4 => v.plane_read,
            5 => v.planar_mode,
            6 => v.miscellaneous_graphics_register,
            7 => v.color_dont_care,
            8 => v.planar_bitmap,
            _ => 0,
        },
        0x3B4 | 0x3D4 => r.index_crtc,
        0x3B5 | 0x3D5 => crtc_read(),
        0x3BA | 0x3DA => {
            let value = r.port_3da_value;
            // bit 3 is set by the retrace of each frame (screen_fill_buffer); in text mode no frame may come,
            // so it is toggled here
            if v.graphical_mode == 0 {
                if r.port_3da_value & 1 != 0 {
                    r.port_3da_value ^= 8;
                }
                r.port_3da_value ^= 1;
            }
            else {
                r.port_3da_value ^= 1;
                r.port_3da_value &= 1;
            }
            r.attribute_controller_index = -1;
            value
        },
        _ => 0xFF,
    }
}

unsafe fn data_write(block: u8, index: i32, value: i32) {
    let r = regs();
    let v = vga();
    match block {
        0 => {
            if (0..0x10).contains(&index) {
                DAC_MAP[index as usize] = value as u8;
                if r.attribute_mode & 0x40 == 0 {
                    r.events |= EVENT_REDRAW;
                }
            }
            else {
                match index {
                    0x12 => {
                        if r.color_plane_enable != value {
                            r.color_plane_enable = value;
                            r.events |= EVENT_REDRAW;
                        }
                    },
                    0x14 => {
                        if r.color_select != value {
                            r.color_select = value;
                            r.events |= EVENT_REDRAW;
                        }
                    },
                    _ => {},
                }
            }
        },
        1 => match index {
            0x02 => v.plane_write_bm = value,
            0x04 => v.sequencer_memory_mode = value,
            _ => {},
        },
        2 => match index {
            0 => v.planar_setreset = value,
            1 => v.planar_setreset_enable = value,
            2 => v.color_compare = value,
            3 => v.planar_rotate_reg = value,
            4 => v.plane_read = value,
            5 => {
                let previous = v.planar_mode;
                v.planar_mode = value;
                if (previous ^ value) & 0x60 != 0 {
                    r.events |= EVENT_REPLOT;
                }
            },
            7 => v.color_dont_care = value,
            8 => v.planar_bitmap = value,
            _ => {},
        },
        _ => match index {
            0xA => {
                r.cursor_scanline_start = value;
                r.events |= EVENT_CURSOR_SCANLINE;
            },
            0xB => {
                r.cursor_scanline_end = value;
                r.events |= EVENT_CURSOR_SCANLINE;
            },
            0xE => {
                r.cursor_address = r.cursor_address & 0xFF | value << 8;
                r.events |= EVENT_CURSOR;
            },
            0xF => {
                r.cursor_address = r.cursor_address & 0xFF00 | value;
                r.events |= EVENT_CURSOR;
            },
            i if (0..0x19).contains(&i) => r.crtc[i as usize] = value as u8,
            _ => {},
        },
    }
}

unsafe fn dac_write(color_byte: i32) {
    let r = regs();
    let index = (r.dac_color_index_write / 3 & 0xFF) as usize;
    let offset = r.dac_color_index_write % 3;
    let mut color_byte = color_byte;
    if r.dispi_enable_value & 0x20 == 0 {
        color_byte &= 0x3F;
        let b = color_byte & 1;
        color_byte = color_byte << 2 | b << 1 | b;
    }
    let color = PALETTE[index];
    let color = match offset {
        0 => color & !0xFF0000 | color_byte << 16,
        1 => color & !0xFF00 | color_byte << 8,
        _ => color & !0xFF | color_byte,
    };
    if PALETTE[index] != color {
        PALETTE[index] = color;
        r.events |= EVENT_REDRAW;
    }
    r.dac_color_index_write = (r.dac_color_index_write + 1) % (256 * 3);
}

#[no_mangle]
pub unsafe fn vga_port_write(port: i32, size: i32, value: i32) {
    let r = regs();
    let low = value & 0xFF;
    match port {
        0x3C0 => {
            if r.attribute_controller_index == -1 {
                r.attribute_controller_index = low & 0x1F;
                r.palette_source = low & 0x20;
            }
            else {
                data_write(0, r.attribute_controller_index, low);
                r.attribute_controller_index = -1;
            }
        },
        0x3C2 => r.miscellaneous_output_register = low,
        0x3C4 => {
            r.sequencer_index = low;
            if size == 16 {
                data_write(1, low, value >> 8 & 0xFF);
            }
        },
        0x3C5 => data_write(1, r.sequencer_index, low),
        0x3C6 => {
            if r.dac_mask != low {
                r.dac_mask = low;
                r.events |= EVENT_REDRAW;
            }
        },
        0x3C7 => {
            r.dac_color_index_read = low * 3;
            r.dac_state = 0;
        },
        0x3C8 => {
            r.dac_color_index_write = low * 3;
            r.dac_state |= 0x3;
            if size == 16 {
                dac_write(value >> 8 & 0xFF);
            }
        },
        0x3C9 => dac_write(low),
        0x3CE => {
            r.graphics_index = low;
            if size == 16 {
                data_write(2, low, value >> 8 & 0xFF);
            }
        },
        0x3CF => data_write(2, r.graphics_index, low),
        0x3B4 | 0x3D4 => {
            r.index_crtc = low;
            if size == 16 {
                data_write(3, low, value >> 8 & 0xFF);
            }
        },
        0x3B5 | 0x3D5 => data_write(3, r.index_crtc, low),
        _ => {},
    }
}
