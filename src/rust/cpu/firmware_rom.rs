//! The firmware's memory-mapped ranges outside RAM (SeaBIOS; not with UEFI's flash): the VGA option ROM at the
//! address of the VGA's PCI ROM BAR (src/vga.js), and the BIOS's alias below 4 GB (the top 1 MB of the first MB,
//! readable and writable as there). src/cpu.js's load_bios hands them over; JavaScript still registers the ranges.
//! (SeaBIOS reads the option ROM byte by byte: about 9,000 calls into JavaScript per boot before)

use crate::cpu::memory;

const VGA_ROM_BASE: u32 = 0xFEB0_0000;
const VGA_ROM_END: u32 = 0xFEC0_0000;
const BIOS_ALIAS_BASE: u32 = 0xFFF0_0000;

static mut VGA_ROM: Vec<u8> = Vec::new();
static mut HAVE_VGA_ROM: bool = false;
static mut HAVE_BIOS_ALIAS: bool = false;

/// Room for the VGA option ROM's bytes (then served at 0xFEB00000)
#[no_mangle]
#[allow(static_mut_refs)]
pub unsafe fn firmware_vga_rom_buffer(length: u32) -> u32 {
    VGA_ROM.clear();
    VGA_ROM.resize(length as usize, 0);
    HAVE_VGA_ROM = true;
    VGA_ROM.as_mut_ptr() as u32
}

/// The BIOS's alias at 0xFFF00000 is served from memory
#[no_mangle]
pub unsafe fn firmware_bios_alias() { HAVE_BIOS_ALIAS = true; }

#[inline(always)]
pub unsafe fn serves(addr: u32) -> bool {
    addr >= VGA_ROM_BASE
        && (addr >= BIOS_ALIAS_BASE && HAVE_BIOS_ALIAS || addr < VGA_ROM_END && HAVE_VGA_ROM)
}

#[allow(static_mut_refs)]
pub unsafe fn read8(addr: u32) -> i32 {
    if addr >= BIOS_ALIAS_BASE {
        *memory::mem8.add((addr & 0xFFFFF) as usize) as i32
    }
    else {
        VGA_ROM.get((addr - VGA_ROM_BASE) as usize).copied().unwrap_or(0) as i32
    }
}

pub unsafe fn read32(addr: u32) -> i32 {
    read8(addr) | read8(addr.wrapping_add(1)) << 8 | read8(addr.wrapping_add(2)) << 16 | read8(addr.wrapping_add(3)) << 24
}

/// The ROM ignores writes; the alias writes the first MB
pub unsafe fn write(addr: u32, bytes: u32, value: i32) {
    for i in 0..bytes {
        let a = addr.wrapping_add(i);
        if a >= BIOS_ALIAS_BASE {
            *memory::mem8.add((a & 0xFFFFF) as usize) = (value >> (8 * i)) as u8;
        }
    }
}
