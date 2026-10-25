//! Floating point exception flags, denormal handling and NaN rules of the sse arithmetic
//! instructions (add, sub, mul, div, sqrt, min and max, in packed and scalar form).
//!
//! The results are computed with the host's round-to-nearest arithmetic, MXCSR.RC is not taken
//! into account. MXCSR.DAZ and MXCSR.FTZ, the sticky exception flags and the exception masks
//! (#XM) are implemented.

use crate::cpu::cpu::*;
use crate::cpu::global_pointers::*;

#[derive(Copy, Clone, PartialEq)]
pub enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Sqrt,
    Min,
    Max,
}

const IE: u32 = 1;
const DE: u32 = 2;
const ZE: u32 = 4;
const OE: u32 = 8;
const UE: u32 = 16;
const PE: u32 = 32;

const DAZ: i32 = 1 << 6;
const FTZ: i32 = 1 << 15;

const DEFAULT_NAN32: u32 = 0xFFC0_0000;
const DEFAULT_NAN64: u64 = 0xFFF8_0000_0000_0000;

fn is_snan32(x: f32) -> bool { x.is_nan() && x.to_bits() & 0x0040_0000 == 0 }
fn is_snan64(x: f64) -> bool { x.is_nan() && x.to_bits() & 0x0008_0000_0000_0000 == 0 }
fn quiet32(x: f32) -> f32 { f32::from_bits(x.to_bits() | 0x0040_0000) }
fn quiet64(x: f64) -> f64 { f64::from_bits(x.to_bits() | 0x0008_0000_0000_0000) }
fn is_denormal32(x: f32) -> bool { x != 0.0 && x.abs() < f32::MIN_POSITIVE }
fn is_denormal64(x: f64) -> bool { x != 0.0 && x.abs() < f64::MIN_POSITIVE }

pub unsafe fn f32_op(op: Op, a: f32, b: f32, fl: &mut u32) -> f32 {
    let daz = *mxcsr & DAZ != 0;
    let (mut x, mut y) = (a, b);
    if is_snan32(x) || is_snan32(y) {
        *fl |= IE;
    }
    if matches!(op, Op::Min | Op::Max) && (x.is_nan() || y.is_nan()) {
        *fl |= IE;
        return y;
    }
    if op != Op::Sqrt && x.is_nan() {
        return quiet32(x);
    }
    if y.is_nan() {
        return quiet32(y);
    }
    for v in [&mut x, &mut y] {
        if is_denormal32(*v) {
            if daz {
                *v = 0.0f32.copysign(*v);
            }
            else {
                *fl |= DE;
            }
        }
    }
    let r = match op {
        Op::Add => x + y,
        Op::Sub => x - y,
        Op::Mul => x * y,
        Op::Div => x / y,
        Op::Sqrt => y.sqrt(),
        Op::Min => return if x < y { x } else { y },
        Op::Max => return if x > y { x } else { y },
    };
    if r.is_nan() {
        *fl |= IE;
        return f32::from_bits(DEFAULT_NAN32);
    }
    if op == Op::Div && y == 0.0 && x != 0.0 && x.is_finite() {
        *fl |= ZE;
        return r;
    }
    if r.is_infinite() {
        if x.is_finite() && y.is_finite() {
            *fl |= OE | PE;
        }
        return r;
    }
    let (xd, yd, rd) = (x as f64, y as f64, r as f64);
    let inexact = match op {
        Op::Add => rd != xd + yd,
        Op::Sub => rd != xd - yd,
        Op::Mul => rd != xd * yd,
        Op::Div => rd * yd != xd,
        _ => rd * rd != yd,
    };
    if inexact {
        *fl |= PE;
    }
    if is_denormal32(r) || (r == 0.0 && inexact) {
        if inexact {
            *fl |= UE | PE;
        }
        if *mxcsr & FTZ != 0 && is_denormal32(r) {
            *fl |= UE | PE;
            return 0.0f32.copysign(r);
        }
    }
    r
}

pub unsafe fn f64_op(op: Op, a: f64, b: f64, fl: &mut u32) -> f64 {
    let daz = *mxcsr & DAZ != 0;
    let (mut x, mut y) = (a, b);
    if is_snan64(x) || is_snan64(y) {
        *fl |= IE;
    }
    if matches!(op, Op::Min | Op::Max) && (x.is_nan() || y.is_nan()) {
        *fl |= IE;
        return y;
    }
    if op != Op::Sqrt && x.is_nan() {
        return quiet64(x);
    }
    if y.is_nan() {
        return quiet64(y);
    }
    for v in [&mut x, &mut y] {
        if is_denormal64(*v) {
            if daz {
                *v = 0.0f64.copysign(*v);
            }
            else {
                *fl |= DE;
            }
        }
    }
    let r = match op {
        Op::Add => x + y,
        Op::Sub => x - y,
        Op::Mul => x * y,
        Op::Div => x / y,
        Op::Sqrt => y.sqrt(),
        Op::Min => return if x < y { x } else { y },
        Op::Max => return if x > y { x } else { y },
    };
    if r.is_nan() {
        *fl |= IE;
        return f64::from_bits(DEFAULT_NAN64);
    }
    if op == Op::Div && y == 0.0 && x != 0.0 && x.is_finite() {
        *fl |= ZE;
        return r;
    }
    if r.is_infinite() {
        if x.is_finite() && y.is_finite() {
            *fl |= OE | PE;
        }
        return r;
    }
    // exactness through error terms (fused multiply add gives the exact rounding error)
    let inexact = match op {
        Op::Add | Op::Sub => {
            let y = if op == Op::Sub { -y } else { y };
            let bb = r - x;
            (x - (r - bb)) + (y - bb) != 0.0
        },
        Op::Mul => x.mul_add(y, -r) != 0.0,
        Op::Div => r.mul_add(y, -x) != 0.0,
        _ => r.mul_add(r, -y) != 0.0,
    };
    if inexact {
        *fl |= PE;
    }
    if is_denormal64(r) || (r == 0.0 && inexact) {
        if inexact {
            *fl |= UE | PE;
        }
        if *mxcsr & FTZ != 0 && is_denormal64(r) {
            *fl |= UE | PE;
            return 0.0f64.copysign(r);
        }
    }
    r
}

/// Records the exception flags. Returns false if an unmasked exception was raised, in which
/// case the instruction must not write its result.
pub unsafe fn commit(fl: u32) -> bool {
    let unmasked = fl & !((*mxcsr as u32) >> 7) & 0x3F;
    *mxcsr |= fl as i32;
    if unmasked == 0 {
        return true;
    }
    if in_jit {
        // the jitted code checks this after the call and leaves the block
        *sse_exception_pending = 1;
    }
    else {
        *instruction_pointer = *previous_ip;
        if *cr.offset(4) & CR4_OSXMMEXCPT != 0 {
            call_interrupt_vector(CPU_EXCEPTION_XM, false, None);
        }
        else {
            trigger_ud();
        }
    }
    false
}

#[no_mangle]
pub unsafe fn trigger_xm_jit(eip_offset_in_page: i32) {
    *sse_exception_pending = 0;
    dbg_assert!(eip_offset_in_page >= 0 && eip_offset_in_page < 0x1000);
    *instruction_pointer = *instruction_pointer & !0xFFF | eip_offset_in_page;
    jit_exit_reason = JitExitReason::CpuException {
        code: if *cr.offset(4) & CR4_OSXMMEXCPT != 0 { CPU_EXCEPTION_XM } else { CPU_EXCEPTION_UD },
        error_code: None,
    };
}

pub unsafe fn ps(op: Op, source: reg128, r: i32) {
    let d = read_xmm128s(r);
    let mut fl = 0;
    let mut out = d;
    for i in 0..4 {
        out.f32[i] = f32_op(op, d.f32[i], source.f32[i], &mut fl);
    }
    if commit(fl) {
        write_xmm_reg128(r, out);
    }
}

pub unsafe fn pd(op: Op, source: reg128, r: i32) {
    let d = read_xmm128s(r);
    let mut fl = 0;
    let mut out = d;
    for i in 0..2 {
        out.f64[i] = f64_op(op, d.f64[i], source.f64[i], &mut fl);
    }
    if commit(fl) {
        write_xmm_reg128(r, out);
    }
}

pub unsafe fn ss(op: Op, source: f32, r: i32) {
    let mut fl = 0;
    let v = f32_op(op, read_xmm_f32(r), source, &mut fl);
    if commit(fl) {
        write_xmm_f32(r, v);
    }
}

pub unsafe fn sd(op: Op, source: f64, r: i32) {
    let mut fl = 0;
    let v = f64_op(op, f64::from_bits(read_xmm64s(r)), source, &mut fl);
    if commit(fl) {
        write_xmm_f64(r, v);
    }
}
