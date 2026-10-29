#![allow(non_upper_case_globals)]

// The CPU state lives in a block of the module's memory. In the normal build it is at fixed addresses (what the
// JavaScript side and the compiled code assume). In the `parallel` build (docs/parallel.md) the block is a static,
// STATE_BLOCK: tools/parallel_wasm.mjs lists every reference to it, and the loader (src/parallel/relocate.js) gives each
// instance of the module its own copy, in a low slot of the shared memory (slot 0, the machine's, is the fixed address).
#[cfg(feature = "parallel")]
#[repr(C, align(4096))]
pub struct StateBlock(pub [u8; 4096]);
#[cfg(feature = "parallel")]
pub static mut STATE_BLOCK: StateBlock = StateBlock([0; 4096]);

#[cfg(feature = "parallel")]
macro_rules! sp {
    ($t:ty, $off:expr) => {
        unsafe { (&raw mut STATE_BLOCK as *mut u8).wrapping_add($off) as *mut $t }
    };
}
#[cfg(not(feature = "parallel"))]
macro_rules! sp {
    ($t:ty, $off:expr) => {
        $off as *mut $t
    };
}

use crate::cpu::cpu::reg128;
use crate::softfloat::F80;
use crate::state_flags::CachedStateFlags;

pub const reg8: *mut u8 = sp!(u8, 64);
pub const reg16: *mut u16 = sp!(u16, 64);
pub const reg32: *mut i32 = sp!(i32, 64);

pub const last_op_size: *mut i32 = sp!(i32, 96);
pub const flags_changed: *mut i32 = sp!(i32, 100);
pub const last_op1: *mut i32 = sp!(i32, 104);
pub const state_flags: *mut CachedStateFlags = sp!(CachedStateFlags, 108);
pub const last_result: *mut i32 = sp!(i32, 112);
pub const flags: *mut i32 = sp!(i32, 120);

pub const segment_access_bytes: *mut u8 = sp!(u8, 512); // TODO: reorder below segment_limits

pub const apic_enabled: *mut bool = sp!(bool, 548);
pub const acpi_enabled: *mut bool = sp!(bool, 552);

pub const instruction_pointer: *mut i32 = sp!(i32, 556);
pub const previous_ip: *mut i32 = sp!(i32, 560);
pub const idtr_size: *mut i32 = sp!(i32, 564);
pub const idtr_offset: *mut i32 = sp!(i32, 568);
pub const gdtr_size: *mut i32 = sp!(i32, 572);
pub const gdtr_offset: *mut i32 = sp!(i32, 576);
pub const cr: *mut i32 = sp!(i32, 580);
pub const cpl: *mut u8 = sp!(u8, 612);
pub const in_hlt: *mut bool = sp!(bool, 616);
pub const last_virt_eip: *mut i32 = sp!(i32, 620);
pub const eip_phys: *mut i32 = sp!(i32, 624);

pub const sysenter_cs: *mut i32 = sp!(i32, 636);
pub const sysenter_esp: *mut i32 = sp!(i32, 640);
pub const sysenter_eip: *mut i32 = sp!(i32, 644);
pub const prefixes: *mut u8 = sp!(u8, 648);
pub const instruction_counter: *mut u32 = sp!(u32, 664);
pub const sreg: *mut u16 = sp!(u16, 668);
pub const dreg: *mut i32 = sp!(i32, 684);

// filled in by svga_fill_pixel_buffer, read by javacsript for optimised putImageData calls
pub const svga_dirty_bitmap_min_offset: *mut u32 = sp!(u32, 716);
pub const svga_dirty_bitmap_max_offset: *mut u32 = sp!(u32, 720);

pub const segment_is_null: *mut bool = sp!(bool, 724);
pub const segment_offsets: *mut i32 = sp!(i32, 736);
pub const segment_limits: *mut u32 = sp!(u32, 768);

pub const protected_mode: *mut bool = sp!(bool, 800);
pub const is_32: *mut bool = sp!(bool, 804);
pub const stack_size_32: *mut bool = sp!(bool, 808);
pub const memory_size: *mut u32 = sp!(u32, 812);
pub const fpu_stack_empty: *mut u8 = sp!(u8, 816);
pub const mxcsr: *mut i32 = sp!(i32, 824);

pub const reg_xmm: *mut reg128 = sp!(reg128, 1536); // 16 entries, xmm8-15 are only used in 64-bit mode
pub const current_tsc: *mut u64 = sp!(u64, 960);

pub const reg_pdpte: *mut u64 = sp!(u64, 968); // 4 64-bit entries

pub const fpu_stack_ptr: *mut u8 = sp!(u8, 1032);
pub const fpu_control_word: *mut u16 = sp!(u16, 1036);
pub const fpu_status_word: *mut u16 = sp!(u16, 1040);
pub const fpu_opcode: *mut i32 = sp!(i32, 1044);
pub const fpu_ip: *mut i32 = sp!(i32, 1048);
pub const fpu_ip_selector: *mut i32 = sp!(i32, 1052);
pub const fpu_dp: *mut i32 = sp!(i32, 1056);
pub const fpu_dp_selector: *mut i32 = sp!(i32, 1060);
pub const tss_size_32: *mut bool = sp!(bool, 1128);

pub const sse_scratch_register: *mut reg128 = sp!(reg128, 1136);

pub const fpu_st: *mut F80 = sp!(F80, 1152);
pub const efer: *mut u64 = sp!(u64, 1280);
pub const pat: *mut u64 = sp!(u64, 1288);

// Long mode state (see cpu/long_mode.rs). The low 32 bits of rax..rdi live in `reg32`.
pub const reg_hi: *mut i32 = sp!(i32, 1344); // upper 32 bits of r0..r15 (16 entries)
pub const reg_ext: *mut i32 = sp!(i32, 1408); // lower 32 bits of r8..r15 (8 entries)
pub const rip_hi: *mut i32 = sp!(i32, 1440); // upper 32 bits of rip, always 0 for now
pub const is_64: *mut bool = sp!(bool, 1444); // executing a 64-bit code segment (EFER.LMA && CS.L)
pub const msr_star: *mut u64 = sp!(u64, 1448);
pub const msr_lstar: *mut u64 = sp!(u64, 1456);
pub const msr_cstar: *mut u64 = sp!(u64, 1464);
pub const msr_sfmask: *mut u64 = sp!(u64, 1472);
pub const fs_base: *mut u64 = sp!(u64, 1480); // 64-bit fs/gs base, used by 64-bit code
pub const gs_base: *mut u64 = sp!(u64, 1488);
pub const kernel_gs_base: *mut u64 = sp!(u64, 1496);
pub const cr2_hi: *mut i32 = sp!(i32, 1504); // upper half of cr2
pub const cr8_tpr: *mut i32 = sp!(i32, 1508); // cr8 (task priority), 4 bits
pub const gdtr_hi: *mut i32 = sp!(i32, 1512); // upper halves of the 64-bit descriptor table bases
pub const idtr_hi: *mut i32 = sp!(i32, 1516);
pub const tr_base_hi: *mut i32 = sp!(i32, 1520);
pub const sse_exception_pending: *mut i32 = sp!(i32, 1524); // set by sse instructions called from the jit
pub const xcr0: *mut u64 = sp!(u64, 1528); // enabled xsave state components (saved with the state)

pub fn get_reg32_offset(r: u32) -> u32 {
    dbg_assert!(r < 8);
    (unsafe { reg32.offset(r as isize) }) as u32
}

pub fn get_reg_mmx_offset(r: u32) -> u32 {
    dbg_assert!(r < 8);
    (unsafe { fpu_st.offset(r as isize) }) as u32
}

pub fn get_reg_xmm_offset(r: u32) -> u32 {
    dbg_assert!(r < 8);
    (unsafe { reg_xmm.offset(r as isize) }) as u32
}

pub fn get_sreg_offset(s: u32) -> u32 {
    dbg_assert!(s < 6);
    (unsafe { sreg.offset(s as isize) }) as u32
}

pub fn get_seg_offset(s: u32) -> u32 {
    dbg_assert!(s < 8);
    (unsafe { segment_offsets.offset(s as isize) }) as u32
}

pub fn get_segment_is_null_offset(s: u32) -> u32 {
    dbg_assert!(s < 8);
    (unsafe { segment_is_null.offset(s as isize) }) as u32
}

pub fn get_creg_offset(i: u32) -> u32 {
    dbg_assert!(i < 8);
    (unsafe { cr.offset(i as isize) }) as u32
}
