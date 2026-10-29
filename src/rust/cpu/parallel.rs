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

/// Makes this instance core `core` of a machine with `cores` cores whose RAM is `memory_size` bytes at `memory_ptr`
#[no_mangle]
pub unsafe fn worker_init(core: u32, cores: u32, memory_ptr: u32, memory_size_: u32, tsc_low: u32, tsc_high: u32) {
    WORKER = true;
    STARTED = false;
    apic::CORE_COUNT = cores as usize;
    apic::CURRENT_CORE = core as usize;
    memory::mem8 = memory_ptr as *mut u8;
    *memory_size = memory_size_;
    *acpi_enabled = true;
    // no compiled code in the workers (yet)
    crate::jit::set_jit_config(0, 1);
    crate::jit::set_jit_config(4, u32::MAX);
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
pub unsafe fn machine_set_parallel(on: u32) { MACHINE = on != 0 }

#[no_mangle]
pub unsafe fn machine_ioapic_read32(addr: u32) -> u32 { crate::cpu::ioapic::read32(addr) }

#[no_mangle]
pub unsafe fn machine_ioapic_write32(addr: u32, value: u32) {
    crate::cpu::ioapic::write32(addr, value);
    request_irq_check();
}
