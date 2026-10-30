mod ext {
    #[link(wasm_import_module = "env")]
    extern "C" {
        pub fn mmap_read8(addr: u32) -> i32;
        pub fn mmap_read32(addr: u32) -> i32;

        pub fn mmap_write8(addr: u32, value: i32);
        pub fn mmap_write16(addr: u32, value: i32);
        pub fn mmap_write32(addr: u32, value: i32);
        pub fn mmap_write64(addr: u32, v0: i32, v1: i32);
        pub fn mmap_write128(addr: u32, v0: i32, v1: i32, v2: i32, v3: i32);
    }
}

use crate::cpu::apic;
use crate::cpu::cpu::{
    request_irq_check, reg128, APIC_MEM_ADDRESS, APIC_MEM_SIZE, IOAPIC_MEM_ADDRESS, IOAPIC_MEM_SIZE,
};
use crate::cpu::global_pointers::memory_size;
use crate::cpu::ioapic;
use crate::cpu::vga;
use crate::cpu::mmio_ram;
use crate::jit;
use crate::page::Page;

use std::alloc;
use std::ptr;

#[allow(non_upper_case_globals)]
pub static mut mem8: *mut u8 = ptr::null_mut();

#[no_mangle]
pub fn allocate_memory(size: u32) -> u32 {
    unsafe {
        dbg_assert!(mem8.is_null());
    };
    dbg_log!("Allocate memory size={}m", size >> 20);
    let layout = alloc::Layout::from_size_align(size as usize, 0x1000).unwrap();
    let ptr = unsafe { alloc::alloc(layout) as u32 };
    unsafe {
        mem8 = ptr as *mut u8;
    };
    ptr
}

#[no_mangle]
pub unsafe fn zero_memory(addr: u32, size: u32) {
    ptr::write_bytes(mem8.offset(addr as isize), 0, size as usize);
}

#[allow(non_upper_case_globals)]
pub static mut vga_mem8: *mut u8 = ptr::null_mut();
#[allow(non_upper_case_globals)]
pub static mut vga_memory_size: u32 = 0;

#[no_mangle]
pub fn svga_allocate_memory(size: u32) -> u32 {
    unsafe {
        dbg_assert!(vga_mem8.is_null());
    };
    let layout = alloc::Layout::from_size_align(size as usize, 0x1000).unwrap();
    let ptr = unsafe { alloc::alloc(layout) };
    dbg_assert!(
        size & (1 << 12 << 6) == 0,
        "size not aligned to dirty_bitmap"
    );
    unsafe {
        vga_mem8 = ptr;
        vga_memory_size = size;
        vga::set_dirty_bitmap_size(size >> 12 >> 6);
    };
    ptr as u32
}

/// Pages 0xC0..0xFF (0xC0000-0xFFFFF) whose i440FX PAM setting is read-only (01): reads come from
/// RAM, writes are dropped. Bit n is page 0xC0 + n.
pub static mut PAM_READ_ONLY: u64 = 0;

/// is `addr` in a page that ignores writes (shadowed ROM)? Pages like this also carry TLB_HAS_CODE,
/// so that every write takes the slow path
#[inline]
pub fn write_ignored(addr: u32) -> bool {
    let page = (addr >> 12).wrapping_sub(0xC0);
    page < 64 && unsafe { PAM_READ_ONLY } >> page & 1 != 0
}

#[no_mangle]
pub unsafe fn set_pam_read_only(lo: u32, hi: u32) {
    PAM_READ_ONLY = lo as u64 | (hi as u64) << 32;
    crate::cpu::cpu::full_clear_tlb();
}

#[no_mangle]
pub fn in_mapped_range(addr: u32) -> bool {
    return addr >= 0xA0000 && addr < 0xC0000 || addr >= unsafe { *memory_size };
}

/// Where instruction fetches read `phys`, as an offset from mem8: the
/// address itself for RAM, the region's memory for code in device memory
/// (the UEFI firmware's flash). Such addresses are beyond the guest's RAM,
/// so nothing compiles from them and they read as mapped.
#[inline(always)]
pub fn code_phys(phys: u32) -> u32 {
    if in_mapped_range(phys) { code_phys_slow(phys) } else { phys }
}
#[cold]
fn code_phys_slow(phys: u32) -> u32 {
    match unsafe { mmio_ram::read_host(phys & !0xFFF, 4096) } {
        Some(host) => (host as u32).wrapping_sub(unsafe { mem8 } as u32) | phys & 0xFFF,
        None => phys,
    }
}
/// RAM, or code in device memory as code_phys gives it
pub fn fetchable(phys: u32) -> bool { !in_mapped_range(phys) || phys >= unsafe { *memory_size } }

pub const VGA_LFB_ADDRESS: u32 = 0xE0000000;
pub fn in_svga_lfb(addr: u32) -> bool {
    addr >= VGA_LFB_ADDRESS && unsafe { vga_memory_size != 0 && addr <= VGA_LFB_ADDRESS + (vga_memory_size - 1) }
}


// Guest RAM accesses. With cores in other instances and a host with a weaker memory order than x86's (the `seqcst` build) every aligned access is a sequentially
// consistent atomic, so that code that relies on x86's ordering (a flag stored after the data it announces)
// also works on hosts with a weaker one (arm64); an unaligned one is followed by a fence.
#[cfg(feature = "seqcst")]
mod ram {
    use std::ptr;
    use std::sync::atomic::{fence, AtomicU16, AtomicU32, AtomicU64, Ordering::SeqCst};
    #[inline(always)]
    pub unsafe fn load8(p: *const u8) -> u8 { std::sync::atomic::AtomicU8::from_ptr(p as *mut u8).load(SeqCst) }
    #[inline(always)]
    pub unsafe fn load16(p: *const u8) -> u16 {
        if p as usize & 1 == 0 { AtomicU16::from_ptr(p as *mut u16).load(SeqCst) } else { let v = ptr::read_unaligned(p as *const u16); fence(SeqCst); v }
    }
    #[inline(always)]
    pub unsafe fn load32(p: *const u8) -> u32 {
        if p as usize & 3 == 0 { AtomicU32::from_ptr(p as *mut u32).load(SeqCst) } else { let v = ptr::read_unaligned(p as *const u32); fence(SeqCst); v }
    }
    #[inline(always)]
    pub unsafe fn load64(p: *const u8) -> u64 {
        if p as usize & 7 == 0 { AtomicU64::from_ptr(p as *mut u64).load(SeqCst) } else { let v = ptr::read_unaligned(p as *const u64); fence(SeqCst); v }
    }
    #[inline(always)]
    pub unsafe fn store8(p: *mut u8, v: u8) { std::sync::atomic::AtomicU8::from_ptr(p).store(v, SeqCst) }
    #[inline(always)]
    pub unsafe fn store16(p: *mut u8, v: u16) {
        if p as usize & 1 == 0 { AtomicU16::from_ptr(p as *mut u16).store(v, SeqCst) } else { ptr::write_unaligned(p as *mut u16, v); fence(SeqCst) }
    }
    #[inline(always)]
    pub unsafe fn store32(p: *mut u8, v: u32) {
        if p as usize & 3 == 0 { AtomicU32::from_ptr(p as *mut u32).store(v, SeqCst) } else { ptr::write_unaligned(p as *mut u32, v); fence(SeqCst) }
    }
    #[inline(always)]
    pub unsafe fn store64(p: *mut u8, v: u64) {
        if p as usize & 7 == 0 { AtomicU64::from_ptr(p as *mut u64).store(v, SeqCst) } else { ptr::write_unaligned(p as *mut u64, v); fence(SeqCst) }
    }
}
#[cfg(not(feature = "seqcst"))]
mod ram {
    use std::ptr;
    #[inline(always)]
    pub unsafe fn load8(p: *const u8) -> u8 { *p }
    #[inline(always)]
    pub unsafe fn load16(p: *const u8) -> u16 { ptr::read_unaligned(p as *const u16) }
    #[inline(always)]
    pub unsafe fn load32(p: *const u8) -> u32 { ptr::read_unaligned(p as *const u32) }
    #[inline(always)]
    pub unsafe fn load64(p: *const u8) -> u64 { ptr::read_unaligned(p as *const u64) }
    #[inline(always)]
    pub unsafe fn store8(p: *mut u8, v: u8) { *p = v }
    #[inline(always)]
    pub unsafe fn store16(p: *mut u8, v: u16) { ptr::write_unaligned(p as *mut u16, v) }
    #[inline(always)]
    pub unsafe fn store32(p: *mut u8, v: u32) { ptr::write_unaligned(p as *mut u32, v) }
    #[inline(always)]
    pub unsafe fn store64(p: *mut u8, v: u64) { ptr::write_unaligned(p as *mut u64, v) }
}

#[no_mangle]
pub fn read8(addr: u32) -> i32 {
    if in_mapped_range(addr) {
        if in_svga_lfb(addr) {
            unsafe { *vga_mem8.offset((addr - VGA_LFB_ADDRESS) as isize) as i32 }
        }
        else if let Some(host) = unsafe { mmio_ram::read_host(addr, 1) } {
            unsafe { *host as i32 }
        }
        else if addr >= APIC_MEM_ADDRESS && addr < APIC_MEM_ADDRESS + APIC_MEM_SIZE {
            apic::read32((addr - APIC_MEM_ADDRESS) & !3) as i32 >> 8 * (addr & 3) & 0xFF
        }
        else if addr >= IOAPIC_MEM_ADDRESS && addr < IOAPIC_MEM_ADDRESS + IOAPIC_MEM_SIZE {
            ioapic::read32((addr - IOAPIC_MEM_ADDRESS) & !3) as i32 >> 8 * (addr & 3) & 0xFF
        }
        else {
            unsafe { ext::mmap_read8(addr) }
        }
    }
    else {
        read8_no_mmap_check(addr)
    }
}
pub fn read8_no_mmap_check(addr: u32) -> i32 { unsafe { ram::load8(mem8.offset(addr as isize)) as i32 } }

#[no_mangle]
pub fn read16(addr: u32) -> i32 {
    if in_mapped_range(addr) {
        if in_svga_lfb(addr) {
            unsafe {
                ptr::read_unaligned(vga_mem8.offset((addr - VGA_LFB_ADDRESS) as isize) as *const u16)
                    as i32
            }
        }
        else if let Some(host) = unsafe { mmio_ram::read_host(addr, 2) } {
            unsafe { ptr::read_unaligned(host as *const u16) as i32 }
        }
        else {
            read8(addr) | read8(addr + 1) << 8
        }
    }
    else {
        read16_no_mmap_check(addr)
    }
}
pub fn read16_no_mmap_check(addr: u32) -> i32 {
    unsafe { ram::load16(mem8.offset(addr as isize)) as i32 }
}

#[no_mangle]
pub fn read32s(addr: u32) -> i32 {
    if in_mapped_range(addr) {
        if in_svga_lfb(addr) {
            unsafe {
                ptr::read_unaligned(vga_mem8.offset((addr - VGA_LFB_ADDRESS) as isize) as *const i32)
            } // XXX
        }
        else if let Some(host) = unsafe { mmio_ram::read_host(addr, 4) } {
            unsafe { ptr::read_unaligned(host as *const i32) }
        }
        else if addr >= APIC_MEM_ADDRESS && addr < APIC_MEM_ADDRESS + APIC_MEM_SIZE {
            apic::read32(addr - APIC_MEM_ADDRESS) as i32
        }
        else if addr >= IOAPIC_MEM_ADDRESS && addr < IOAPIC_MEM_ADDRESS + IOAPIC_MEM_SIZE {
            ioapic::read32(addr - IOAPIC_MEM_ADDRESS) as i32
        }
        else {
            unsafe { ext::mmap_read32(addr) }
        }
    }
    else {
        read32_no_mmap_check(addr)
    }
}
pub fn read32_no_mmap_check(addr: u32) -> i32 {
    unsafe { ram::load32(mem8.offset(addr as isize)) as i32 }
}

pub unsafe fn read64s(addr: u32) -> i64 {
    if in_mapped_range(addr) {
        if in_svga_lfb(addr) {
            ptr::read_unaligned(vga_mem8.offset((addr - VGA_LFB_ADDRESS) as isize) as *const i64)
        }
        else if let Some(host) = mmio_ram::read_host(addr, 8) {
            ptr::read_unaligned(host as *const i64)
        }
        else {
            read32s(addr) as i64 | (read32s(addr + 4) as i64) << 32
        }
    }
    else {
        ram::load64(mem8.offset(addr as isize)) as i64
    }
}

pub unsafe fn read128(addr: u32) -> reg128 {
    if in_mapped_range(addr) {
        if in_svga_lfb(addr) {
            ptr::read_unaligned(vga_mem8.offset((addr - VGA_LFB_ADDRESS) as isize) as *const reg128)
        }
        else if let Some(host) = mmio_ram::read_host(addr, 16) {
            ptr::read_unaligned(host as *const reg128)
        }
        else {
            reg128 {
                i32: [
                    read32s(addr + 0),
                    read32s(addr + 4),
                    read32s(addr + 8),
                    read32s(addr + 12),
                ],
            }
        }
    }
    else {
        ptr::read_unaligned(mem8.offset(addr as isize) as *const reg128)
    }
}

#[no_mangle]
pub unsafe fn write8(addr: u32, value: i32) {
    if in_mapped_range(addr) {
        mmap_write8(addr, value & 0xFF);
    }
    else {
        jit::jit_dirty_page(Page::page_of(addr));
        write8_no_mmap_or_dirty_check(addr, value);
    };
}

pub unsafe fn write8_no_mmap_or_dirty_check(addr: u32, value: i32) {
    if write_ignored(addr) { return }
    ram::store8(mem8.offset(addr as isize), value as u8)
}

#[no_mangle]
pub unsafe fn write16(addr: u32, value: i32) {
    if in_mapped_range(addr) {
        mmap_write16(addr, value & 0xFFFF);
    }
    else {
        jit::jit_dirty_cache_small(addr, addr + 2);
        write16_no_mmap_or_dirty_check(addr, value);
    };
}
pub unsafe fn write16_no_mmap_or_dirty_check(addr: u32, value: i32) {
    if write_ignored(addr) { return }
    ram::store16(mem8.offset(addr as isize), value as u16)
}

#[no_mangle]
pub unsafe fn write32(addr: u32, value: i32) {
    if in_mapped_range(addr) {
        mmap_write32(addr, value);
    }
    else {
        jit::jit_dirty_cache_small(addr, addr + 4);
        write32_no_mmap_or_dirty_check(addr, value);
    }
}

pub unsafe fn write32_no_mmap_or_dirty_check(addr: u32, value: i32) {
    if write_ignored(addr) { return }
    ram::store32(mem8.offset(addr as isize), value as u32)
}

pub unsafe fn write64_no_mmap_or_dirty_check(addr: u32, value: u64) {
    if write_ignored(addr) { return }
    ram::store64(mem8.offset(addr as isize), value)
}

pub unsafe fn write128_no_mmap_or_dirty_check(addr: u32, value: reg128) {
    if write_ignored(addr) { return }
    ptr::write_unaligned(mem8.offset(addr as isize) as *mut reg128, value)
}

pub unsafe fn memset_no_mmap_or_dirty_check(addr: u32, value: u8, count: u32) {
    if PAM_READ_ONLY != 0 && addr < 0x100000 && addr as u64 + count as u64 > 0xC0000 {
        for i in 0..count { write8_no_mmap_or_dirty_check(addr + i, value as i32) }
        return;
    }
    ptr::write_bytes(mem8.offset(addr as isize), value, count as usize);
}

pub unsafe fn memcpy_no_mmap_or_dirty_check(src_addr: u32, dst_addr: u32, count: u32) {
    dbg_assert!(src_addr < *memory_size);
    dbg_assert!(dst_addr < *memory_size);
    if PAM_READ_ONLY != 0 && dst_addr < 0x100000 && dst_addr as u64 + count as u64 > 0xC0000 {
        // a forward copy, like the overlapping-safe ptr::copy of the common case, one byte at a time
        if dst_addr <= src_addr {
            for i in 0..count { write8_no_mmap_or_dirty_check(dst_addr + i, *mem8.offset((src_addr + i) as isize) as i32) }
        }
        else {
            for i in (0..count).rev() { write8_no_mmap_or_dirty_check(dst_addr + i, *mem8.offset((src_addr + i) as isize) as i32) }
        }
        return;
    }
    ptr::copy(
        mem8.offset(src_addr as isize),
        mem8.offset(dst_addr as isize),
        count as usize,
    )
}

pub unsafe fn memcpy_into_svga_lfb(src_addr: u32, dst_addr: u32, count: u32) {
    dbg_assert!(src_addr < *memory_size);
    dbg_assert!(in_svga_lfb(dst_addr));
    dbg_assert!(Page::page_of(dst_addr) == Page::page_of(dst_addr + count - 1));
    vga::mark_dirty(dst_addr);
    ptr::copy_nonoverlapping(
        mem8.offset(src_addr as isize),
        vga_mem8.offset((dst_addr - VGA_LFB_ADDRESS) as isize),
        count as usize,
    )
}

pub unsafe fn mmap_write8(addr: u32, value: i32) {
    if in_svga_lfb(addr) {
        vga::mark_dirty(addr);
        *vga_mem8.offset((addr - VGA_LFB_ADDRESS) as isize) = value as u8
    }
    else if let Some(host) = mmio_ram::write_host(addr, 1) {
        *host = value as u8
    }
    else {
        ext::mmap_write8(addr, value)
    }
}
pub unsafe fn mmap_write16(addr: u32, value: i32) {
    if in_svga_lfb(addr) {
        vga::mark_dirty(addr);
        ptr::write_unaligned(
            vga_mem8.offset((addr - VGA_LFB_ADDRESS) as isize) as *mut u16,
            value as u16,
        )
    }
    else if let Some(host) = mmio_ram::write_host(addr, 2) {
        ptr::write_unaligned(host as *mut u16, value as u16)
    }
    else {
        ext::mmap_write16(addr, value)
    }
}
pub unsafe fn mmap_write32(addr: u32, value: i32) {
    if in_svga_lfb(addr) {
        vga::mark_dirty(addr);
        ptr::write_unaligned(
            vga_mem8.offset((addr - VGA_LFB_ADDRESS) as isize) as *mut i32,
            value,
        )
    }
    else if let Some(host) = mmio_ram::write_host(addr, 4) {
        ptr::write_unaligned(host as *mut i32, value)
    }
    else if addr >= APIC_MEM_ADDRESS && addr < APIC_MEM_ADDRESS + APIC_MEM_SIZE {
        apic::write32(addr - APIC_MEM_ADDRESS, value as u32);
        request_irq_check();
    }
    else if addr >= IOAPIC_MEM_ADDRESS && addr < IOAPIC_MEM_ADDRESS + IOAPIC_MEM_SIZE {
        ioapic::write32(addr - IOAPIC_MEM_ADDRESS, value as u32);
        request_irq_check();
    }
    else {
        ext::mmap_write32(addr, value)
    }
}
pub unsafe fn mmap_write64(addr: u32, value: u64) {
    if in_svga_lfb(addr) {
        vga::mark_dirty(addr);
        ptr::write_unaligned(
            vga_mem8.offset((addr - VGA_LFB_ADDRESS) as isize) as *mut u64,
            value,
        )
    }
    else if let Some(host) = mmio_ram::write_host(addr, 8) {
        ptr::write_unaligned(host as *mut u64, value)
    }
    else {
        ext::mmap_write64(addr, value as i32, (value >> 32) as i32)
    }
}
pub unsafe fn mmap_write128(addr: u32, v0: u64, v1: u64) {
    if in_svga_lfb(addr) {
        vga::mark_dirty(addr);
        ptr::write_unaligned(
            vga_mem8.offset((addr - VGA_LFB_ADDRESS) as isize) as *mut u64,
            v0,
        );
        ptr::write_unaligned(
            vga_mem8.offset((addr - VGA_LFB_ADDRESS + 8) as isize) as *mut u64,
            v1,
        )
    }
    else if let Some(host) = mmio_ram::write_host(addr, 16) {
        ptr::write_unaligned(host as *mut u64, v0);
        ptr::write_unaligned(host.add(8) as *mut u64, v1);
    }
    else {
        ext::mmap_write128(
            addr,
            v0 as i32,
            (v0 >> 32) as i32,
            v1 as i32,
            (v1 >> 32) as i32,
        )
    }
}

#[no_mangle]
pub unsafe fn is_memory_zeroed(addr: u32, length: u32) -> bool {
    dbg_assert!(addr % 8 == 0);
    dbg_assert!(length % 8 == 0);
    for i in (addr..addr + length).step_by(8) {
        if *(mem8.offset(i as isize) as *const i64) != 0 {
            return false;
        }
    }
    return true;
}


/// Several cores share the guest RAM (a parallel machine, docs/parallel.md): read-modify-write instructions
/// that must be atomic commit through compare-exchange
pub static mut ATOMIC_RMW: bool = false;

/// Compare-exchange of the `bits` bit value at the physical address `phys` (in RAM, not in a memory mapped range). Returns None when it
/// cannot be done atomically (crosses a 32 bit boundary, mapped range): the caller then does it the plain way
pub unsafe fn atomic_cas(phys: u32, bits: u32, expected: u64, new: u64) -> Option<bool> {
    use std::sync::atomic::{AtomicU32, AtomicU64, Ordering::SeqCst};
    if in_mapped_range(phys) {
        return None;
    }
    if bits == 64 {
        if phys & 7 != 0 {
            return None;
        }
        let p = mem8.offset(phys as isize) as *const AtomicU64;
        return Some((*p).compare_exchange(expected, new, SeqCst, SeqCst).is_ok());
    }
    let shift = (phys & 3) * 8;
    if shift + bits > 32 {
        return None;
    }
    let mask = if bits == 32 { 0xFFFF_FFFFu32 } else { (1u32 << bits) - 1 };
    let p = mem8.offset((phys & !3) as isize) as *const AtomicU32;
    loop {
        let cur = (*p).load(SeqCst);
        if (cur >> shift) & mask != expected as u32 & mask {
            return Some(false);
        }
        let next = (cur & !(mask << shift)) | ((new as u32 & mask) << shift);
        match (*p).compare_exchange(cur, next, SeqCst, SeqCst) {
            Ok(_) => return Some(true),
            // (another part of the word changed: look again)
            Err(_) => continue,
        }
    }
}

/// Atomic read-modify-write of 8, 16 or 32 bits at `phys`: `f` maps the old value to the new one (it may run more than once). False when
/// not possible atomically
pub unsafe fn atomic_rmw(phys: u32, bits: u32, f: &dyn Fn(i32) -> i32) -> bool {
    use std::sync::atomic::{AtomicU32, Ordering::SeqCst};
    let shift = (phys & 3) * 8;
    if in_mapped_range(phys) || shift + bits > 32 {
        return false;
    }
    let mask = if bits == 32 { 0xFFFF_FFFFu32 } else { (1u32 << bits) - 1 };
    let p = mem8.offset((phys & !3) as isize) as *const AtomicU32;
    // (instructions such as xchg, xadd and cmpxchg change registers inside `f`: a retry starts from the registers of the first attempt)
    let mut saved = [0i32; 8];
    for i in 0..8 {
        saved[i] = *crate::cpu::global_pointers::reg32.add(i);
    }
    loop {
        let cur = (*p).load(SeqCst);
        let old = ((cur >> shift) & mask) as i32;
        let new = f(old) as u32 & mask;
        let next = (cur & !(mask << shift)) | (new << shift);
        if (*p).compare_exchange(cur, next, SeqCst, SeqCst).is_ok() {
            return true;
        }
        for i in 0..8 {
            *crate::cpu::global_pointers::reg32.add(i) = saved[i];
        }
    }
}

/// Sets bits (the accessed and dirty bits) in the low byte of a page table entry. Another core may change the entry at the same time: with
/// several cores the update is an atomic OR, like the hardware's
pub unsafe fn set_entry_bits(phys: u32, bits: u8) {
    use std::sync::atomic::{AtomicU8, Ordering::SeqCst};
    if ATOMIC_RMW && !in_mapped_range(phys) {
        jit::jit_dirty_page(Page::page_of(phys));
        (*(mem8.offset(phys as isize) as *const AtomicU8)).fetch_or(bits, SeqCst);
    }
    else {
        write8(phys, (read8(phys) | bits as i32) & 0xFF);
    }
}
