//! Cores that take turns on this thread (docs/multicore.md).
//!
//! The registers and the other per-core state live in the fixed block of the module's memory
//! (global_pointers.rs, `BLOCK_START..BLOCK_END`). Switching to another core copies the block of
//! the running core out and the block of the next core in; fields that belong to the machine (memory size,
//! the instruction counter that drives the time stamp counter, ...) are kept. Each core has its own local APIC
//! (apic.rs). Compiled code stays valid for every core (it checks the mappings it is entered with); the
//! translation caches are flushed on a switch.

use crate::cpu::apic;
use crate::cpu::cpu::*;
use crate::cpu::global_pointers::*;
use crate::cpu::{jit64, pic};

const BLOCK_START: usize = 64;
const BLOCK_END: usize = 1792;
const LEN: usize = BLOCK_END - BLOCK_START;
const MAX: usize = apic::MAX_CORES;

/// instructions per slice of a core
pub static mut SLICE: u32 = 20_011;
#[no_mangle]
pub unsafe fn smp_set_slice(n: u32) { SLICE = n }

/// (offset, size) of the fields of the block that belong to the machine, not to a core
const SHARED: [(usize, usize); 6] = [
    (548, 1),  // apic_enabled (the BSP's setting stays)
    (552, 1),  // acpi_enabled
    (664, 4),  // instruction_counter (time stamp counter)
    (716, 8),  // svga dirty bitmap offsets
    (812, 4),  // memory_size
    (960, 8),  // current_tsc
];

static mut SAVED: [[u8; LEN]; MAX] = [[0; LEN]; MAX];
static mut RESET_STATE: [u8; LEN] = [0; LEN];
static mut RUNNING: [bool; MAX] = [false; MAX];
static mut SHADOW: [bool; MAX] = [false; MAX];
static mut ROUND: usize = 0;
/// set by a core that has just sent an IPI or executed PAUSE: its slice ends after the current instruction
pub static mut YIELD: bool = false;
pub static mut SLICES: [u32; MAX] = [0; MAX];

unsafe fn block() -> &'static mut [u8] { std::slice::from_raw_parts_mut(BLOCK_START as *mut u8, LEN) }

pub unsafe fn count() -> usize { apic::CORE_COUNT }

/// Makes the machine a `count` core machine (called before the first reset)
#[no_mangle]
pub unsafe fn smp_init(count: u32) {
    apic::apic_set_core_count(count);
    machine_reset();
}

/// Called at the end of reset_cpu: the running core becomes the BSP, the others wait for a start-up IPI
pub unsafe fn machine_reset() {
    if apic::CORE_COUNT <= 1 {
        return;
    }
    let n = apic::CORE_COUNT as u32;
    apic::apic_set_core_count(n);
    RESET_STATE.copy_from_slice(block());
    for i in 0..MAX {
        SAVED[i] = RESET_STATE;
        RUNNING[i] = i == 0;
        SHADOW[i] = false;
        SLICES[i] = 0;
    }
    ROUND = 0;
}

/// current core after reset, for CPUID and the like
pub unsafe fn current_core() -> u32 { apic::CURRENT_CORE as u32 }

unsafe fn load(bytes: &[u8; LEN]) {
    let mut shared = [0u8; 64];
    let mut n = 0;
    for &(off, size) in SHARED.iter() {
        shared[n..n + size].copy_from_slice(std::slice::from_raw_parts(off as *const u8, size));
        n += size;
    }
    block().copy_from_slice(bytes);
    n = 0;
    for &(off, size) in SHARED.iter() {
        std::slice::from_raw_parts_mut(off as *mut u8, size).copy_from_slice(&shared[n..n + size]);
        n += size;
    }
}

const DEBUG_CLEAR_TLB_CODE: bool = false;
pub static mut TRANS_LOG: [[u32; 4]; 256] = [[0; 4]; 256];
pub static mut TRANS_COUNT: u32 = 0;
#[no_mangle]
pub unsafe fn smp_trans_log(i: u32, j: u32) -> u32 { if i == 0xFFFF { TRANS_COUNT } else { TRANS_LOG[i as usize & 255][j as usize] } }
pub static mut SWITCH_LOG: [[u32; 6]; 256] = [[0; 6]; 256];
pub static mut SWITCH_COUNT: u32 = 0;
#[no_mangle]
pub unsafe fn smp_switch_log(i: u32, j: u32) -> u32 { if i == 0xFFFF { SWITCH_COUNT } else { SWITCH_LOG[i as usize & 255][j as usize] } }

unsafe fn switch_to(to: usize) {
    let from = apic::CURRENT_CORE;
    if from == to {
        return;
    }
    let eip_from = *instruction_pointer as u32;
    let esp_from = *reg32.offset(4) as u32;
    SAVED[from].copy_from_slice(block());
    SHADOW[from] = interrupt_shadow;
    let next = SAVED[to];
    load(&next);
    apic::apic_set_current_core(to as u32);
    interrupt_shadow = SHADOW[to];
    IN_SHADOW_STEP = false;
    full_clear_tlb();
    if DEBUG_CLEAR_TLB_CODE { for p in 0..0x2000 { clear_tlb_code(p) } }
    jit64::core_switched();
    SWITCH_LOG[SWITCH_COUNT as usize & 255] = [from as u32, to as u32, eip_from, esp_from, *instruction_pointer as u32, *reg32.offset(4) as u32];
    SWITCH_COUNT += 1;
}

/// a field of a core's block, whether or not the core is the running one
unsafe fn field32(core: usize, off: usize) -> i32 {
    if core == apic::CURRENT_CORE {
        *(off as *const i32)
    }
    else {
        let at = off - BLOCK_START;
        i32::from_le_bytes([SAVED[core][at], SAVED[core][at + 1], SAVED[core][at + 2], SAVED[core][at + 3]])
    }
}

unsafe fn field8(core: usize, off: usize) -> u8 {
    if core == apic::CURRENT_CORE { *(off as *const u8) } else { SAVED[core][off - BLOCK_START] }
}

unsafe fn take_events(core: usize) {
    let events = apic::apic_take_core_events(core as u32);
    if events == 0 {
        return;
    }
    if events & apic::CORE_EVENT_INIT != 0 {
        dbg_log!("core {}: INIT", core);
        apic::apic_init_core(core as u32);
        // everything takes its reset value (the x87/SSE state would survive on hardware)
        if core == apic::CURRENT_CORE {
            let r = RESET_STATE;
            load(&r);
            full_clear_tlb();
        }
        else {
            SAVED[core] = RESET_STATE;
        }
        SHADOW[core] = false;
        // an AP waits for a start-up IPI, the BSP restarts at the reset vector
        RUNNING[core] = core == 0;
    }
    if events & apic::CORE_EVENT_SIPI != 0 && !RUNNING[core] {
        let vector = (events >> 8 & 0xFF) as i32;
        dbg_log!("core {}: start-up IPI, vector {:x}", core, vector);
        switch_to(core);
        *sreg.offset(CS as isize) = (vector << 8) as u16;
        *segment_offsets.offset(CS as isize) = vector << 12;
        *instruction_pointer = vector << 12;
        *previous_ip = vector << 12;
        update_state_flags();
        RUNNING[core] = true;
    }
}

unsafe fn runnable(core: usize) -> bool {
    if !RUNNING[core] {
        return false;
    }
    if field8(core, in_hlt as usize) == 0 {
        return true;
    }
    if apic::apic_core_nmi_pending(core as u32) {
        return true;
    }
    if field32(core, flags as usize) & FLAG_INTERRUPT == 0 {
        return false;
    }
    apic::apic_core_interrupt_pending(core as u32) || core == 0 && pic::pic_has_pending()
}

/// one round of slices; returns the time until the machine has to run again (0 when a core is busy)
unsafe fn round() -> f64 {
    let n = count();
    let now = js::microtick();
    let mut next = js::run_hardware_timers(*acpi_enabled, now);
    let first = ROUND % n;
    ROUND = ROUND.wrapping_add(1);
    for turn in 0..n {
        let core = (first + turn) % n;
        take_events(core);
        if !runnable(core) {
            continue;
        }
        switch_to(core);
        if *in_hlt {
            handle_irqs();
            if *in_hlt {
                continue;
            }
        }
        let start = *instruction_counter;
        YIELD = false;
        while (*instruction_counter).wrapping_sub(start) < SLICE && !*in_hlt && !YIELD {
            cycle_internal();
            if IRQ_CHECK {
                IRQ_CHECK = false;
                handle_irqs();
            }
        }
        handle_irqs();
        SLICES[core] = SLICES[core].wrapping_add(1);
        if !*in_hlt {
            next = 0.0;
        }
    }
    for core in 0..n {
        if apic::apic_peek_core_events(core as u32) || runnable(core) && (apic::apic_core_interrupt_pending(core as u32) || core == 0 && pic::pic_has_pending()) {
            next = 0.0;
        }
    }
    next
}

/// main_loop for machines with more than one core
pub unsafe fn main_loop() -> f64 {
    let start = js::microtick();
    loop {
        let t = round();
        if t > 0.0 {
            return t;
        }
        if js::microtick() - start > TIME_PER_FRAME {
            return 0.0;
        }
    }
}

/// debugging: a 32-bit field of a core's block (`core` need not be the running one)
#[no_mangle]
pub unsafe fn smp_debug_field(core: u32, off: u32) -> i32 { field32(core as usize, off as usize) }
