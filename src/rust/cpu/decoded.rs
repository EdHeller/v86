//! The interpreter's cache of decoded instructions (interpreter phase 2): per physical page, the instructions of
//! 32-bit code decoded once (opcode, modrm, sib, displacement, immediate) to a handler from gen/decoded.rs and its
//! operands, so that running them again skips the decoding.
//!
//! Only plain instructions are cached: 32-bit code (not long mode), no prefixes, within the page, in RAM. The pages are
//! marked as maybe having code (as jit::jit_increase_hotness_and_maybe_compile does), so that writes to them reach
//! jit_dirty_page, which calls `invalidate`.

use crate::cpu::cpu::{get_seg_prefix, read_reg32, SS, DS};
use crate::cpu::global_pointers::instruction_pointer;
use crate::cpu::memory;
use crate::gen::decoded::{self, Handler, TABLE};
use crate::page::Page;

/// set_jit_config(72)
pub static mut ENABLED: bool = true;

const NO_REG: u8 = 8;
/// address without a segment (lea)
const NO_SEG: u8 = 0xFF;

#[derive(Clone, Copy)]
pub struct Decoded {
    handler: Handler,
    len: u8,
    mem: bool,
    /// the modrm's register field
    r: u8,
    /// the modrm's register (not mem), or the address's base register (NO_REG for none)
    base: u8,
    index: u8,
    scale: u8,
    seg: u8,
    disp: i32,
    imm: i32,
}

const SLOTS: usize = 256;
/// slot values: 0 not decoded, 1 not cacheable, n the instruction at n - 2
const UNDECODED: u16 = 0;
const UNCACHEABLE: u16 = 1;

pub struct PageCache {
    page: u32,
    candidate: u32,
    visits: u32,
    slots: [u16; 4096],
    instructions: Vec<Decoded>,
}

static mut CACHE: Vec<PageCache> = Vec::new();


/// set_jit_config(73): interpreted runs of a page before it is decoded (cold code is cheaper to interpret)
pub static mut THRESHOLD: u32 = 32;

/// The cache of the page of phys_addr when it has been run often enough to be decoded, for one interpreted run
/// (which stays in the page); null otherwise
#[inline(always)]
pub unsafe fn page_for(phys_addr: u32) -> *mut PageCache {
    let page = phys_addr >> 12;
    #[allow(static_mut_refs)]
    let cache = &mut CACHE;
    if cache.is_empty() {
        for _ in 0..SLOTS {
            cache.push(PageCache {
                page: u32::MAX,
                candidate: u32::MAX,
                visits: 0,
                slots: [UNDECODED; 4096],
                instructions: Vec::new(),
            });
        }
    }
    let c = cache.get_unchecked_mut(page as usize % SLOTS);
    if c.page != page {
        // a page takes the slot after THRESHOLD runs (without another page's in between)
        if c.candidate != page {
            c.candidate = page;
            c.visits = 0;
        }
        c.visits += 1;
        if c.visits < THRESHOLD {
            return std::ptr::null_mut();
        }
        c.page = page;
        c.slots.fill(UNDECODED);
        c.instructions.clear();
        // writes to the page must reach invalidate (through jit_dirty_page)
        if !crate::jit::maybe_has_code(Page::of_u32(page)) {
            crate::jit::maybe_has_code_set(Page::of_u32(page));
            crate::cpu::cpu::tlb_set_has_code(Page::of_u32(page), true);
        }
    }
    c
}

/// A write to the page (or its code was dropped)
pub fn invalidate(page: Page) {
    unsafe {
        #[allow(static_mut_refs)]
        let cache = &mut CACHE;
        if cache.is_empty() {
            return;
        }
        let c = &mut cache[page.to_u32() as usize % SLOTS];
        if c.page == page.to_u32() {
            c.page = u32::MAX;
            c.candidate = u32::MAX;
        }
    }
}

pub fn clear() {
    unsafe {
        #[allow(static_mut_refs)]
        for c in CACHE.iter_mut() {
            c.page = u32::MAX;
            c.candidate = u32::MAX;
        }
    }
}

/// The instruction at phys_addr (in c's page), decoded (None: run it by the interpreter)
#[inline(always)]
pub unsafe fn lookup(c: *mut PageCache, phys_addr: u32) -> Option<Decoded> {
    let c = &mut *c;
    if c.page != phys_addr >> 12 {
        // written in this run
        return None;
    }
    let offset = (phys_addr & 0xFFF) as usize;
    let mut s = *c.slots.get_unchecked(offset);
    if s == UNDECODED {
        s = match decode(phys_addr) {
            Some(d) => {
                c.instructions.push(d);
                c.instructions.len() as u16 + 1
            },
            None => UNCACHEABLE,
        };
        c.slots[offset] = s;
    }
    if s == UNCACHEABLE {
        None
    }
    else {
        Some(*c.instructions.get_unchecked(s as usize - 2))
    }
}

unsafe fn decode(phys_addr: u32) -> Option<Decoded> {
    if phys_addr >= *crate::cpu::global_pointers::memory_size {
        return None;
    }
    let offset = phys_addr & 0xFFF;
    let available = 0x1000 - offset;
    let mut p = 0;
    let mut next = || -> Option<u8> {
        if p >= available {
            return None;
        }
        let b = *memory::mem8.offset((phys_addr + p) as isize);
        p += 1;
        Some(b)
    };

    let b0 = next()?;
    let index = if b0 == 0x0F { 256 + next()? as usize } else { b0 as usize };
    let entry = TABLE[index]?;

    let mut d_mem = false;
    let mut r = 0;
    let mut base = NO_REG;
    let mut idx = NO_REG;
    let mut scale = 0;
    let mut seg = DS as u8;
    let mut disp = 0i32;

    let take32 = |next: &mut dyn FnMut() -> Option<u8>| -> Option<i32> {
        let a = next()? as u32;
        let b = next()? as u32;
        let c = next()? as u32;
        let e = next()? as u32;
        Some((a | b << 8 | c << 16 | e << 24) as i32)
    };

    if entry.modrm {
        let modrm = next()?;
        r = modrm >> 3 & 7;
        let mo = modrm >> 6;
        let rm = modrm & 7;
        if mo == 3 {
            base = rm;
        }
        else {
            d_mem = true;
            if rm == 4 {
                let sib = next()?;
                let sb = sib & 7;
                let si = sib >> 3 & 7;
                if si != 4 {
                    idx = si;
                    scale = sib >> 6;
                }
                if sb == 4 {
                    base = 4;
                    seg = SS as u8;
                }
                else if sb == 5 {
                    if mo == 0 {
                        disp = take32(&mut next)?;
                    }
                    else {
                        base = 5;
                        seg = SS as u8;
                    }
                }
                else {
                    base = sb;
                }
            }
            else if rm == 5 && mo == 0 {
                disp = take32(&mut next)?;
            }
            else {
                base = rm;
                if rm == 5 {
                    seg = SS as u8;
                }
            }
            if mo == 1 {
                disp = disp.wrapping_add(next()? as i8 as i32);
            }
            else if mo == 2 {
                disp = disp.wrapping_add(take32(&mut next)?);
            }
        }
    }

    let (handler, imm_kind) = entry.handlers[if entry.group { r as usize } else { 0 }]?;

    let imm = match imm_kind {
        0 => 0,
        1 => next()? as i32,
        2 => next()? as i8 as i32,
        3 => {
            let a = next()? as i32;
            let b = next()? as i32;
            a | b << 8
        },
        4 => take32(&mut next)?,
        _ => return None,
    };

    if index == 0x8D {
        if !d_mem {
            return None;
        }
        seg = NO_SEG;
    }

    Some(Decoded { handler, len: p as u8, mem: d_mem, r, base, index: idx, scale, seg, disp, imm })
}

/// Run a decoded instruction that starts at *instruction_pointer
#[inline(always)]
pub unsafe fn run(d: &Decoded) {
    *instruction_pointer += d.len as i32;
    let x = if d.mem {
        let mut a = d.disp;
        if d.base != NO_REG {
            a = a.wrapping_add(read_reg32(d.base as i32));
        }
        if d.index != NO_REG {
            a = a.wrapping_add(read_reg32(d.index as i32) << d.scale);
        }
        if d.seg != NO_SEG {
            match get_seg_prefix(d.seg as i32) {
                Ok(s) => a = a.wrapping_add(s),
                Err(()) => return,
            }
        }
        a
    }
    else {
        d.base as i32
    };
    decoded::run(d.handler, x, d.mem, d.r as i32, d.imm)
}
