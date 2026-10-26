//! Memory access for 64-bit mode (see long_mode.rs).
//!
//! The shared tlb (`tlb_data`) is indexed by 32-bit virtual page numbers, so it cannot hold
//! 64-bit mode translations (the kernel text at 0xffffffff81000000 and the direct map at
//! 0xffff888081000000 would alias). 64-bit mode therefore has its own, small tlb with full virtual
//! page numbers and a page walker that handles 4-level paging with 4K/2M/1G pages.
//!
//! Instruction implementations shared with the 32-bit interpreters (sse, x87, ...) take 32-bit
//! addresses. For those, `window` maps the pages of an operand to a fixed window in the shared tlb.

use crate::cpu::cpu::*;
use crate::cpu::global_pointers::*;
use crate::cpu::memory;
use crate::jit;
use crate::page::Page;
use crate::paging::OrPageFault;

pub fn is_canonical(a: u64) -> bool {
    let top = (a as i64) >> 47;
    top == 0 || top == -1
}

pub unsafe fn seg_base(seg: i32) -> u64 { if seg == FS { *fs_base } else { *gs_base } }

pub unsafe fn rip() -> u64 { *instruction_pointer as u32 as u64 | (*rip_hi as u32 as u64) << 32 }

// ---------------------------------------------------------------------------------------------
// page walk

const PTE_ADDR_MASK: u64 = 0x000F_FFFF_FFFF_F000;
// physical address bits 51:40 are reserved (MAXPHYADDR is 40)
const PTE_RESERVED: u64 = 0x000F_FF00_0000_0000;

const F_USER: u8 = 1;
const F_WRITE: u8 = 2;
const F_NX: u8 = 4;
const F_DIRTY: u8 = 8;

unsafe fn raise_pf(addr: u64, present: bool, write: bool, user: bool, fetch: bool, rsvd: bool) {
    *cr.offset(2) = addr as i32;
    *cr2_hi = (addr >> 32) as i32;
    let nx_visible = *cr.offset(4) & CR4_SMEP != 0 || *efer & EFER_NXE != 0;
    let error_code = (fetch && nx_visible) as i32 * 16
        | rsvd as i32 * 8
        | user as i32 * 4
        | write as i32 * 2
        | present as i32;
    *instruction_pointer = *previous_ip;
    if DEBUG && js::cpu_exception_hook(CPU_EXCEPTION_PF) {
        return;
    }
    call_interrupt_vector(CPU_EXCEPTION_PF, false, Some(error_code));
}

fn permitted(perm: u8, write: bool, user: bool, fetch: bool, wp: bool, nxe: bool) -> bool {
    if user && perm & F_USER == 0 {
        return false;
    }
    if write && (perm & F_DIRTY == 0 || perm & F_WRITE == 0 && (user || wp)) {
        return false;
    }
    !(fetch && nxe && perm & F_NX != 0)
}

/// 4-level page table walk for a canonical address. With `fault` the appropriate page fault is
/// raised on failure, otherwise `Err` is returned silently. Returns the physical address and the
/// accumulated permissions.
unsafe fn walk(
    addr: u64,
    write: bool,
    user: bool,
    fetch: bool,
    fault: bool,
    set_bits: bool,
) -> OrPageFault<(u32, u8)> {
    let nxe = *efer & EFER_NXE != 0;
    let wp = *cr & CR0_WP != 0;
    let mut table = (*cr.offset(3) as u32 & 0xFFFFF000) as u64;
    let mut user_ok = true;
    let mut write_ok = true;
    let mut no_exec = false;
    let mut entries = [(0u32, 0u64); 4];
    let mut count = 0;
    let phys;
    let mut dirty_leaf = false;

    let mut level = 0;
    loop {
        let shift = 39 - 9 * level;
        let entry_addr = table + (addr >> shift & 0x1FF) * 8;
        dbg_assert!(entry_addr >> 32 == 0, "page table above 4GB");
        let entry = memory::read64s(entry_addr as u32) as u64;
        if entry & 1 == 0 {
            if fault {
                raise_pf(addr, false, write, user, fetch, false);
            }
            return Err(());
        }
        let ps = entry & 0x80 != 0;
        let reserved = entry & PTE_RESERVED != 0
            || !nxe && entry >> 63 != 0
            || level == 0 && ps
            || level == 1 && ps && entry & 0x3FFFE000 != 0
            || level == 2 && ps && entry & 0x1FE000 != 0;
        if reserved {
            if fault {
                raise_pf(addr, true, write, user, fetch, true);
            }
            return Err(());
        }
        user_ok &= entry & 4 != 0;
        write_ok &= entry & 2 != 0;
        no_exec |= entry >> 63 != 0;
        entries[count] = (entry_addr as u32, entry);
        count += 1;

        if level == 1 && ps {
            phys = (entry & PTE_ADDR_MASK & !0x3FFF_FFFF) | (addr & 0x3FFF_FFFF);
            break;
        }
        if level == 2 && ps {
            phys = (entry & PTE_ADDR_MASK & !0x1FFFFF) | (addr & 0x1FFFFF);
            break;
        }
        if level == 3 {
            phys = (entry & PTE_ADDR_MASK) | (addr & 0xFFF);
            break;
        }
        table = entry & PTE_ADDR_MASK;
        level += 1;
    }

    let mut perm = user_ok as u8 * F_USER | write_ok as u8 * F_WRITE | no_exec as u8 * F_NX;
    // the dirty bit of the leaf is only ever set by write accesses
    let leaf_entry = entries[count - 1].1;
    if leaf_entry & 0x40 != 0 {
        perm |= F_DIRTY;
    }
    if write {
        dirty_leaf = true;
        perm |= F_DIRTY;
    }
    if !permitted(perm, write, user, fetch, wp, nxe) {
        if fault {
            raise_pf(addr, true, write, user, fetch, false);
        }
        return Err(());
    }
    dbg_assert!(phys >> 32 == 0, "physical address above 4GB");

    if set_bits {
        for (i, &(entry_addr, entry)) in entries[..count].iter().enumerate() {
            let new = entry | 0x20 | if dirty_leaf && i == count - 1 { 0x40 } else { 0 };
            if new != entry {
                memory::write8(entry_addr, new as i32);
            }
        }
    }
    Ok((phys as u32, perm))
}

// ---------------------------------------------------------------------------------------------
// tlb

#[derive(Clone, Copy)]
struct Entry {
    vpn: u64,
    phys: u32,
    perm: u8,
}

const TLB_SIZE: usize = 4096;
const INVALID: u64 = u64::MAX;
static mut TLB: [Entry; TLB_SIZE] = [Entry { vpn: INVALID, phys: 0, perm: 0 }; TLB_SIZE];
static mut FETCH_VPN: u64 = INVALID;
static mut FETCH_PHYS: u32 = 0;
static mut FETCH_USER: bool = false;

pub unsafe fn flush() {
    #[allow(static_mut_refs)]
    for e in TLB.iter_mut() {
        e.vpn = INVALID;
    }
    FETCH_VPN = INVALID;
    super::jit64::tlb_flushed();
}

pub unsafe fn invlpg64(addr: u64) {
    let vpn = addr >> 12;
    let e = &raw mut TLB[(vpn as usize) & (TLB_SIZE - 1)];
    if (*e).vpn == vpn {
        (*e).vpn = INVALID;
    }
    FETCH_VPN = INVALID;
    super::jit64::tlb_flushed();
}

/// Translates a canonical address; raises a page fault on failure
pub unsafe fn translate(addr: u64, write: bool, user: bool, fetch: bool) -> OrPageFault<u32> {
    let vpn = addr >> 12;
    let e = &raw mut TLB[(vpn as usize) & (TLB_SIZE - 1)];
    let nxe = *efer & EFER_NXE != 0;
    let wp = *cr & CR0_WP != 0;
    if (*e).vpn == vpn && permitted((*e).perm, write, user, fetch, wp, nxe) {
        return Ok((*e).phys | (addr & 0xFFF) as u32);
    }
    let (phys, perm) = walk(addr, write, user, fetch, true, true)?;
    *e = Entry { vpn, phys: phys & !0xFFF, perm };
    Ok(phys)
}

/// Like `translate`, but never raises an exception and does not modify page table entries
/// Like `probe`, for an instruction fetch (checks NX)
pub unsafe fn probe_fetch(addr: u64, user: bool) -> Option<u32> {
    if !is_canonical(addr) {
        return None;
    }
    walk(addr, false, user, true, false, false).ok().map(|(p, _)| p)
}

pub unsafe fn probe(addr: u64, write: bool, user: bool) -> Option<u32> {
    if !is_canonical(addr) {
        return None;
    }
    walk(addr, write, user, false, false, false).ok().map(|(p, _)| p)
}

// ---------------------------------------------------------------------------------------------
// data access

unsafe fn check_canonical(addr: u64, bytes: u64) -> OrPageFault<()> {
    if !is_canonical(addr) || !is_canonical(addr.wrapping_add(bytes - 1)) {
        dbg_log!("#gp: non-canonical address {:#x}", addr);
        trigger_gp(0);
        return Err(());
    }
    Ok(())
}

unsafe fn read_phys(phys: u32, bits: u32) -> u64 {
    match bits {
        8 => memory::read8(phys) as u64 & 0xFF,
        16 => memory::read16(phys) as u64 & 0xFFFF,
        32 => memory::read32s(phys) as u32 as u64,
        _ => memory::read64s(phys) as u64,
    }
}

unsafe fn write_phys(phys: u32, bits: u32, v: u64) {
    match bits {
        8 => memory::write8(phys, v as i32),
        16 => memory::write16(phys, v as i32),
        32 => memory::write32(phys, v as i32),
        _ => {
            memory::write32(phys, v as i32);
            memory::write32(phys + 4, (v >> 32) as i32);
        },
    }
}

pub unsafe fn mem_read(addr: u64, bits: u32) -> OrPageFault<u64> {
    let bytes = bits as u64 / 8;
    check_canonical(addr, bytes)?;
    let user = *cpl == 3;
    if (addr & 0xFFF) + bytes <= 0x1000 {
        return Ok(read_phys(translate(addr, false, user, false)?, bits));
    }
    let first = translate(addr, false, user, false)?;
    let second = translate((addr & !0xFFF) + 0x1000, false, user, false)?;
    let mut v = 0u64;
    for i in 0..bytes {
        let off = (addr & 0xFFF) + i;
        let phys = if off < 0x1000 { first + i as u32 } else { second & !0xFFF | (off - 0x1000) as u32 };
        v |= (memory::read8(phys) as u64 & 0xFF) << (8 * i);
    }
    Ok(v)
}

pub unsafe fn mem_write(addr: u64, bits: u32, v: u64) -> OrPageFault<()> {
    let bytes = bits as u64 / 8;
    check_canonical(addr, bytes)?;
    let user = *cpl == 3;
    if (addr & 0xFFF) + bytes <= 0x1000 {
        write_phys(translate(addr, true, user, false)?, bits, v);
        return Ok(());
    }
    // translate both pages before writing anything
    let first = translate(addr, true, user, false)?;
    let second = translate((addr & !0xFFF) + 0x1000, true, user, false)?;
    for i in 0..bytes {
        let off = (addr & 0xFFF) + i;
        let phys = if off < 0x1000 { first + i as u32 } else { second & !0xFFF | (off - 0x1000) as u32 };
        memory::write8(phys, (v >> (8 * i)) as u8 as i32);
    }
    Ok(())
}

/// Supervisor read, used for descriptor tables, the IDT and the TSS
pub unsafe fn read_system(addr: u64, bits: u32) -> OrPageFault<u64> {
    let bytes = bits as u64 / 8;
    check_canonical(addr, bytes)?;
    if (addr & 0xFFF) + bytes <= 0x1000 {
        return Ok(read_phys(translate(addr, false, false, false)?, bits));
    }
    let mut v = 0u64;
    for i in 0..bytes {
        let phys = translate(addr + i, false, false, false)?;
        v |= (memory::read8(phys) as u64 & 0xFF) << (8 * i);
    }
    Ok(v)
}

pub unsafe fn write_system8(addr: u64, v: u8) -> OrPageFault<()> {
    check_canonical(addr, 1)?;
    memory::write8(translate(addr, true, false, false)?, v as i32);
    Ok(())
}

static mut LAST_DESCRIPTOR_ADDRESS: u64 = 0;
pub unsafe fn set_last_descriptor_address(a: u64) { LAST_DESCRIPTOR_ADDRESS = a; }
pub unsafe fn last_descriptor_address() -> u64 { LAST_DESCRIPTOR_ADDRESS }

// ---------------------------------------------------------------------------------------------
// instruction fetch

#[inline(always)]
unsafe fn fetch_byte() -> OrPageFault<u8> {
    let lo = *instruction_pointer as u32;
    let hi = *rip_hi as u32;
    let r = (hi as u64) << 32 | lo as u64;
    let user = *cpl == 3;
    if r >> 12 != FETCH_VPN || user != FETCH_USER {
        if !is_canonical(r) {
            trigger_gp(0);
            return Err(());
        }
        let phys = translate(r, false, user, true)?;
        FETCH_VPN = r >> 12;
        FETCH_PHYS = phys & !0xFFF;
        FETCH_USER = user;
    }
    let phys = FETCH_PHYS | (lo & 0xFFF);
    dbg_assert!(!memory::in_mapped_range(phys), "code in mapped memory");
    let b = *memory::mem8.offset(phys as isize);
    if lo & 0xFFF != 0xFFF {
        // cannot carry into the upper half
        *instruction_pointer = lo.wrapping_add(1) as i32;
    }
    else {
        let (low, carry) = lo.overflowing_add(1);
        *instruction_pointer = low as i32;
        if carry {
            *rip_hi = (*rip_hi).wrapping_add(1);
        }
    }
    Ok(b)
}

#[inline(always)]
pub unsafe fn fetch8() -> OrPageFault<u64> { Ok(fetch_byte()? as u64) }
pub unsafe fn fetch16() -> OrPageFault<u64> { Ok(fetch8()? | fetch8()? << 8) }
pub unsafe fn fetch32() -> OrPageFault<u64> { Ok(fetch16()? | fetch16()? << 16) }
pub unsafe fn fetch64() -> OrPageFault<u64> { Ok(fetch32()? | fetch32()? << 32) }

// ---------------------------------------------------------------------------------------------
// window into the shared tlb for 32-bit instruction implementations

const WINDOW_PAGE: i32 = 0xFFFF8;
static mut WINDOW_ACTIVE: bool = false;
static mut WINDOW_TARGET: u64 = 0;

fn window_entry(page: i32, phys: u32, writable: bool) -> i32 {
    unsafe {
        let high = phys & !0xFFF;
        let mapped = memory::in_mapped_range(high);
        let has_code = !mapped && jit::jit_page_has_code(Page::page_of(high));
        let info = TLB_VALID
            | if writable { 0 } else { TLB_READONLY }
            | if mapped { TLB_IN_MAPPED_RANGE } else { 0 }
            | if has_code { TLB_HAS_CODE } else { 0 };
        (high.wrapping_add(memory::mem8 as u32)) as i32 ^ page << 12 | info
    }
}

/// Returns a 32-bit address through which the operand at `addr` can be accessed by instruction
/// implementations that use the shared tlb. The two pages of the window are not mapped upfront:
/// when such an instruction misses in the tlb, `window_walk` (called from the 32-bit page walker)
/// translates the real 64-bit address with the right access type. The window is valid until
/// `window_close`.
pub unsafe fn window(addr: u64) -> OrPageFault<i32> {
    check_canonical(addr, 1)?;
    WINDOW_TARGET = addr & !0xFFF;
    WINDOW_ACTIVE = true;
    tlb_data[WINDOW_PAGE as usize] = 0;
    tlb_data[WINDOW_PAGE as usize + 1] = 0;
    Ok((WINDOW_PAGE << 12) | (addr & 0xFFF) as i32)
}

pub unsafe fn window_close() {
    WINDOW_ACTIVE = false;
    tlb_data[WINDOW_PAGE as usize] = 0;
    tlb_data[WINDOW_PAGE as usize + 1] = 0;
}

/// Hook for the 32-bit page walker: resolves tlb misses inside the window
pub unsafe fn window_walk(
    addr: i32,
    for_writing: bool,
    user: bool,
    fetch: bool,
    side_effects: bool,
) -> Option<OrPageFault<std::num::NonZeroI32>> {
    let page = (addr as u32 >> 12) as i32;
    if !WINDOW_ACTIVE || page != WINDOW_PAGE && page != WINDOW_PAGE + 1 {
        return None;
    }
    let target = WINDOW_TARGET + (page - WINDOW_PAGE) as u64 * 0x1000 + (addr & 0xFFF) as u64;
    let phys = if side_effects {
        match translate(target, for_writing, user, fetch) {
            Ok(p) => p,
            Err(()) => return Some(Err(())),
        }
    }
    else {
        match probe(target, for_writing, user) {
            Some(p) => p,
            None => return Some(Err(())),
        }
    };
    let entry = window_entry(page, phys, for_writing);
    if side_effects {
        tlb_data[page as usize] = entry;
    }
    Some(Ok(std::num::NonZeroI32::new(entry).unwrap()))
}
