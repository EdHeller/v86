//! Cores in Web Workers (docs/parallel.md).
//!
//! The machine instance (the one with the devices and the first core) runs as before. Every other core runs in its own instance of the
//! module (src/parallel/relocate.js) in the same shared memory: it has the guest RAM in common with the machine and nothing else. What
//! belongs to the machine is reached by asking the machine thread (host calls below, answered by src/parallel/machine.js): the local
//! APIC registers of this core (the machine keeps all the APICs), the I/O APIC, the ports and memory mapped devices.

use crate::cpu::apic;
use crate::cpu::cpu::*;
use crate::cpu::global_pointers::*;
use crate::cpu::memory;

/// this instance runs one core of a parallel machine (and not the machine itself)
pub static mut WORKER: bool = false;
/// the core got its start-up IPI
static mut STARTED: bool = false;
/// the machine instance leaves cores other than the first to workers
pub static mut MACHINE: bool = false;

#[cfg(feature = "parallel")]
pub mod host {
    #[link(wasm_import_module = "env")]
    extern "C" {
        pub fn apic_read32(core: u32, addr: u32) -> u32;
        pub fn apic_write32(core: u32, addr: u32, value: u32);
        pub fn ioapic_read32(addr: u32) -> u32;
        pub fn ioapic_write32(addr: u32, value: u32);
        /// acknowledges the highest pending interrupt of the core: its vector, or -1
        pub fn apic_ack(core: u32) -> i32;
        /// non-zero when the machine has something for the core (an interrupt, INIT, start-up IPI)
        pub fn core_pending(core: u32) -> u32;
    }
}
#[cfg(not(feature = "parallel"))]
pub mod host {
    pub unsafe fn apic_read32(_core: u32, _addr: u32) -> u32 { 0 }
    pub unsafe fn apic_write32(_core: u32, _addr: u32, _value: u32) {}
    pub unsafe fn ioapic_read32(_addr: u32) -> u32 { 0 }
    pub unsafe fn ioapic_write32(_addr: u32, _value: u32) {}
    pub unsafe fn apic_ack(_core: u32) -> i32 { -1 }
    pub unsafe fn core_pending(_core: u32) -> u32 { 0 }
}

// shared code coherence --------------------------------------------------------------------------------------------
//
// Every instance compiles the code it runs. A write by one core to a page another instance compiled has to retire that code. The
// instances share, in the guest memory's shared space:
// - OWNERS: a byte per physical page, bit k set while instance k has compiled code of the page;
// - a ring of events (publish: "this page has code now" so that the TLB entries of the others mark it, invalidate: "a write hit a page
//   you may have compiled"). Every instance reads every event, at the start of each slice (`drain`).

pub static mut SHARED: *mut u8 = std::ptr::null_mut();
/// bit of this instance in OWNERS (core number)
static mut ME: u32 = 0;
/// this instance's read position in the ring
static mut TAIL: u64 = 0;
/// `drain` is running: retiring code because of another core's write must not be announced again
pub static mut DRAINING: bool = false;
const RING_ENTRIES: u64 = 4096;
const HEAD_OFFSET: usize = 0;
const RING_OFFSET: usize = 64;
const OWNERS_OFFSET: usize = RING_OFFSET + RING_ENTRIES as usize * 8;
const KIND_PUBLISH: u64 = 0;
const KIND_INVALIDATE: u64 = 1;

/// bytes of the shared space for `memory_size` bytes of guest memory
pub fn shared_size(bytes: u32) -> u32 { (OWNERS_OFFSET as u32) + (bytes >> 12) + 4096 }

pub fn active() -> bool { unsafe { !SHARED.is_null() } }

unsafe fn owner_byte(page: u32) -> &'static std::sync::atomic::AtomicU8 {
    &*(SHARED.add(OWNERS_OFFSET + page as usize) as *const std::sync::atomic::AtomicU8)
}

unsafe fn post(kind: u64, page: u32) {
    use std::sync::atomic::{AtomicU64, Ordering::SeqCst};
    let head = &*(SHARED.add(HEAD_OFFSET) as *const AtomicU64);
    let index = head.fetch_add(1, SeqCst);
    let slot = &*(SHARED.add(RING_OFFSET + (index % RING_ENTRIES) as usize * 8) as *const AtomicU64);
    // (the sequence number is in the high half: a reader can tell a written entry from an old one)
    slot.store((index + 1) << 32 | (page as u64) << 1 | kind, SeqCst);
}

/// this instance has code of `page`
pub unsafe fn set_owner(page: u32) {
    use std::sync::atomic::Ordering::SeqCst;
    let before = owner_byte(page).fetch_or(1 << ME, SeqCst);
    if before == 0 {
        // the first code of the page: the others' TLB entries for it have to mark it
        post(KIND_PUBLISH, page);
    }
}

/// this instance has no more code of `page`
pub unsafe fn clear_owner(page: u32) { owner_byte(page).fetch_and(!(1u8 << ME), std::sync::atomic::Ordering::SeqCst); }

pub unsafe fn any_owner(page: u32) -> bool { owner_byte(page).load(std::sync::atomic::Ordering::SeqCst) != 0 }

pub unsafe fn other_owner(page: u32) -> bool { owner_byte(page).load(std::sync::atomic::Ordering::SeqCst) & !(1u8 << ME) != 0 }

/// a write hit `page`: the other instances that have code of it retire it (when they next `drain`)
pub unsafe fn announce_write(page: u32) {
    if !DRAINING && other_owner(page) {
        post(KIND_INVALIDATE, page);
    }
}

/// Acts on the events of the other instances: marks the TLB entries of published pages, retires code of written pages. Called between
/// slices (the JIT state must not be in use)
#[no_mangle]
pub unsafe fn parallel_drain() {
    use std::sync::atomic::{AtomicU64, Ordering::SeqCst};
    if SHARED.is_null() {
        return;
    }
    let head = (&*(SHARED.add(HEAD_OFFSET) as *const AtomicU64)).load(SeqCst);
    if TAIL == head {
        return;
    }
    DRAINING = true;
    if head - TAIL > RING_ENTRIES {
        // lapped: events were lost, so retire all code
        crate::jit::jit_clear_cache_js();
        TAIL = head;
    }
    while TAIL < head {
        let slot = &*(SHARED.add(RING_OFFSET + (TAIL % RING_ENTRIES) as usize * 8) as *const AtomicU64);
        let value = slot.load(SeqCst);
        if value >> 32 != TAIL + 1 {
            // (not written yet: it will be next time)
            break;
        }
        let page = (value as u32) >> 1;
        if value & 1 == KIND_PUBLISH {
            crate::cpu::cpu::tlb_set_has_code(crate::page::Page::of_u32(page), true);
        }
        else {
            crate::jit::jit_dirty_page(crate::page::Page::of_u32(page));
        }
        TAIL += 1;
    }
    DRAINING = false;
}

/// the shared space of a parallel machine: set by the machine, and by every worker when it starts
#[no_mangle]
pub unsafe fn parallel_set_shared(ptr: u32, me: u32) {
    SHARED = ptr as *mut u8;
    ME = me;
    TAIL = (&*(SHARED.add(HEAD_OFFSET) as *const std::sync::atomic::AtomicU64)).load(std::sync::atomic::Ordering::SeqCst);
}

#[no_mangle]
pub fn parallel_shared_size(bytes: u32) -> u32 { shared_size(bytes) }

/// Makes this instance core `core` of a machine with `cores` cores whose RAM is `memory_size` bytes at `memory_ptr`
#[no_mangle]
pub unsafe fn worker_init(core: u32, cores: u32, memory_ptr: u32, memory_size_: u32, tsc_low: u32, tsc_high: u32) {
    WORKER = true;
    STARTED = false;
    memory::ATOMIC_RMW = true;
    apic::CORE_COUNT = cores as usize;
    apic::CURRENT_CORE = core as usize;
    memory::mem8 = memory_ptr as *mut u8;
    *memory_size = memory_size_;
    *acpi_enabled = true;
    reset_cpu();
    tsc_offset = tsc_low as u64 | (tsc_high as u64) << 32;
}

/// the machine's tsc offset, so that every core sees the same time stamp counter
#[no_mangle]
pub unsafe fn get_tsc_offset(high: u32) -> u32 { if high != 0 { (tsc_offset >> 32) as u32 } else { tsc_offset as u32 } }

/// INIT (bit 0) and start-up IPI (bit 1, vector in bits 8..16), as taken from the machine's APIC
#[no_mangle]
pub unsafe fn worker_events(events: u32) {
    if events & apic::CORE_EVENT_INIT != 0 {
        let offset = tsc_offset;
        reset_cpu();
        tsc_offset = offset;
        STARTED = false;
    }
    if events & apic::CORE_EVENT_SIPI != 0 && !STARTED {
        let vector = (events >> 8 & 0xFF) as i32;
        *sreg.offset(CS as isize) = (vector << 8) as u16;
        *segment_offsets.offset(CS as isize) = vector << 12;
        *instruction_pointer = vector << 12;
        *previous_ip = vector << 12;
        update_state_flags();
        STARTED = true;
    }
}

/// Runs the core for about `budget` instructions. Returns 0 when the budget is used up, 1 when the core halted
/// (it waits for an interrupt), 2 when it waits for its start-up IPI
#[no_mangle]
pub unsafe fn worker_run(budget: u32) -> u32 {
    if !STARTED {
        return 2;
    }
    if *in_hlt {
        handle_irqs();
        if *in_hlt {
            return 1;
        }
    }
    parallel_drain();
    let start = *instruction_counter;
    while (*instruction_counter).wrapping_sub(start) < budget && !*in_hlt {
        cycle_internal();
        if IRQ_CHECK {
            IRQ_CHECK = false;
            handle_irqs();
        }
    }
    handle_irqs();
    if *in_hlt { 1 } else { 0 }
}

// the machine's side: answers for the cores in workers ---------------------------------------------

/// the register of the APIC of `core`
#[no_mangle]
pub unsafe fn machine_apic_read32(core: u32, addr: u32) -> u32 {
    let saved = apic::CURRENT_CORE;
    apic::CURRENT_CORE = core as usize;
    let value = apic::read32(addr);
    apic::CURRENT_CORE = saved;
    value
}

#[no_mangle]
pub unsafe fn machine_apic_write32(core: u32, addr: u32, value: u32) {
    let saved = apic::CURRENT_CORE;
    apic::CURRENT_CORE = core as usize;
    apic::write32(addr, value);
    apic::CURRENT_CORE = saved;
}

#[no_mangle]
pub unsafe fn machine_apic_ack(core: u32) -> i32 {
    let saved = apic::CURRENT_CORE;
    apic::CURRENT_CORE = core as usize;
    let r = match apic::acknowledge_irq() { Some(v) => v as i32, None => -1 };
    apic::CURRENT_CORE = saved;
    r
}

/// INIT / start-up events of `core`, cleared
#[no_mangle]
pub unsafe fn machine_take_events(core: u32) -> u32 { apic::apic_take_core_events(core) }

/// bit k: core k (1..8) has an interrupt, an NMI or an INIT / start-up IPI waiting
#[no_mangle]
pub unsafe fn machine_pending_mask() -> u32 {
    let mut mask = 0;
    for core in 1..apic::CORE_COUNT {
        if apic::apic_core_interrupt_pending(core as u32) || apic::apic_core_nmi_pending(core as u32) || apic::apic_peek_core_events(core as u32) {
            mask |= 1 << core;
        }
    }
    mask
}

/// the machine leaves the cores other than the first to workers
#[no_mangle]
pub unsafe fn machine_set_parallel(on: u32) {
    MACHINE = on != 0;
    memory::ATOMIC_RMW = on != 0;
}

#[no_mangle]
pub unsafe fn machine_ioapic_read32(addr: u32) -> u32 { crate::cpu::ioapic::read32(addr) }

#[no_mangle]
pub unsafe fn machine_ioapic_write32(addr: u32, value: u32) {
    crate::cpu::ioapic::write32(addr, value);
    request_irq_check();
}
