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

/// MXCSR as software usually leaves it: every exception masked, round to nearest, no DAZ, no FTZ (the status flags
/// aside). Then ordinary operands and results can raise nothing but the sticky inexact flag.
#[inline(always)]
unsafe fn usual_mxcsr() -> bool { *mxcsr & !0x3F == 0x1F80 }

/// finite and not denormal (zero is fine)
#[inline(always)]
fn ordinary64(x: f64) -> bool { x.is_finite() && (x == 0.0 || x.abs() >= f64::MIN_POSITIVE) }

/// The common case of the packed and scalar operations without the per-element exception path: the result, or
/// None when an operand or the result needs it (NaN, infinity, denormal, underflow, division, square root)
/// Whether x * y (rounded to r) was inexact, without a fused multiply-add (WebAssembly has none: f64::mul_add is a
/// slow library routine there): Dekker's product splits the operands into halves whose products are exact
#[inline(always)]
fn mul_inexact64(x: f64, y: f64, r: f64) -> bool {
    // (operands near the end of the range overflow in the split: let the exact path decide those)
    if x.abs() > 1e300 || y.abs() > 1e300 || r.abs() > 1e300 || (r != 0.0 && r.abs() < 1e-290) {
        return x.mul_add(y, -r) != 0.0;
    }
    let split = |v: f64| {
        let c = 134217729.0 * v; // 2^27 + 1
        let hi = c - (c - v);
        (hi, v - hi)
    };
    let (xh, xl) = split(x);
    let (yh, yl) = split(y);
    // (the evaluation order of TwoProduct, in which every step is exact)
    let err = xl * yl - (((r - xh * yh) - xl * yh) - xh * yl);
    err != 0.0
}

#[inline(always)]
unsafe fn fast_f64(op: Op, x: f64, y: f64, inexact: &mut bool, check_inexact: bool) -> Option<f64> {
    if !ordinary64(x) || !ordinary64(y) {
        return None;
    }
    let r = match op {
        Op::Add => x + y,
        Op::Sub => x - y,
        Op::Mul => x * y,
        Op::Min => return Some(if x < y { x } else { y }),
        Op::Max => return Some(if x > y { x } else { y }),
        _ => return None,
    };
    if !ordinary64(r) || (r == 0.0 && op == Op::Mul && x != 0.0 && y != 0.0) {
        return None;
    }
    if check_inexact {
        *inexact |= match op {
            Op::Add | Op::Sub => {
                let y = if op == Op::Sub { -y } else { y };
                let bb = r - x;
                (x - (r - bb)) + (y - bb) != 0.0
            },
            _ => mul_inexact64(x, y, r),
        };
    }
    Some(r)
}

/// The packed single precision add, subtract and multiply in the usual MXCSR state, four lanes at once with wasm
/// SIMD: the result, or None when a lane needs the exact per-element path (an operand or the result not finite or
/// denormal, an underflow to zero). The inexact flag is only looked for while it is still clear.
#[cfg(target_feature = "simd128")]
#[inline(always)]
unsafe fn ps_fast(op: Op, d: reg128, s: reg128) -> Option<(reg128, bool)> {
    use core::arch::wasm32::*;
    let a: v128 = std::mem::transmute(d);
    let b: v128 = std::mem::transmute(s);
    let r = match op {
        Op::Add => f32x4_add(a, b),
        Op::Sub => f32x4_sub(a, b),
        _ => f32x4_mul(a, b),
    };
    let exp = u32x4_splat(0x7F80_0000);
    let zero = u32x4_splat(0);
    // a lane is special when its exponent is all ones (infinity, NaN) or zero with a nonzero fraction (denormal)
    let special = |x: v128| {
        let e = v128_and(x, exp);
        let mag = v128_and(x, u32x4_splat(0x7FFF_FFFF));
        v128_or(u32x4_eq(e, exp), v128_and(u32x4_eq(e, zero), u32x4_ne(mag, zero)))
    };
    let mut bad = v128_or(v128_or(special(a), special(b)), special(r));
    if op == Op::Mul {
        // a product of nonzero numbers that became zero underflowed
        let fz = f32x4_splat(0.0);
        bad = v128_or(bad, v128_and(f32x4_eq(r, fz), v128_and(f32x4_ne(a, fz), f32x4_ne(b, fz))));
    }
    if v128_any_true(bad) {
        return None;
    }
    let mut inexact = false;
    if *mxcsr as u32 & PE == 0 {
        // in double precision the sum, difference and product of two singles are exact
        let hi = |x: v128| i32x4_shuffle::<2, 3, 0, 1>(x, x);
        let exact = |x: v128, y: v128, z: v128| {
            let (xd, yd, zd) = (f64x2_promote_low_f32x4(x), f64x2_promote_low_f32x4(y), f64x2_promote_low_f32x4(z));
            let e = match op {
                Op::Add => f64x2_add(xd, yd),
                Op::Sub => f64x2_sub(xd, yd),
                _ => f64x2_mul(xd, yd),
            };
            f64x2_eq(e, zd)
        };
        inexact = !i64x2_all_true(v128_and(exact(a, b, r), exact(hi(a), hi(b), hi(r))));
    }
    Some((std::mem::transmute(r), inexact))
}

#[cfg(not(target_feature = "simd128"))]
#[inline(always)]
unsafe fn ps_fast(_op: Op, _d: reg128, _s: reg128) -> Option<(reg128, bool)> { None }

pub unsafe fn ps(op: Op, source: reg128, r: i32) {
    let d = read_xmm128s(r);
    if usual_mxcsr() && matches!(op, Op::Add | Op::Sub | Op::Mul) {
        if let Some((out, inexact)) = ps_fast(op, d, source) {
            if inexact {
                *mxcsr |= PE as i32;
            }
            write_xmm_reg128(r, out);
            return;
        }
    }
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
    if usual_mxcsr() {
        let check = *mxcsr as u32 & PE == 0;
        let mut inexact = false;
        let mut out = d;
        let mut ok = true;
        for i in 0..2 {
            match fast_f64(op, d.f64[i], source.f64[i], &mut inexact, check) {
                Some(v) => out.f64[i] = v,
                None => {
                    ok = false;
                    break;
                },
            }
        }
        if ok {
            if inexact {
                *mxcsr |= PE as i32;
            }
            write_xmm_reg128(r, out);
            return;
        }
    }
    let mut fl = 0;
    let mut out = d;
    for i in 0..2 {
        out.f64[i] = f64_op(op, d.f64[i], source.f64[i], &mut fl);
    }
    if commit(fl) {
        write_xmm_reg128(r, out);
    }
}

/// The scalar single precision operations' fast path (as ps_fast, one lane)
#[inline(always)]
unsafe fn ss_fast(op: Op, x: f32, y: f32) -> Option<(f32, bool)> {
    let ordinary = |v: f32| v.is_finite() && (v == 0.0 || v.abs() >= f32::MIN_POSITIVE);
    if !ordinary(x) || !ordinary(y) {
        return None;
    }
    let r = match op {
        Op::Add => x + y,
        Op::Sub => x - y,
        Op::Mul => x * y,
        Op::Min => return Some((if x < y { x } else { y }, false)),
        Op::Max => return Some((if x > y { x } else { y }, false)),
        _ => return None,
    };
    if !ordinary(r) || (r == 0.0 && op == Op::Mul && x != 0.0 && y != 0.0) {
        return None;
    }
    let mut inexact = false;
    if *mxcsr as u32 & PE == 0 {
        let (xd, yd, rd) = (x as f64, y as f64, r as f64);
        inexact = rd != match op {
            Op::Add => xd + yd,
            Op::Sub => xd - yd,
            _ => xd * yd,
        };
    }
    Some((r, inexact))
}

pub unsafe fn ss(op: Op, source: f32, r: i32) {
    if usual_mxcsr() {
        if let Some((v, inexact)) = ss_fast(op, read_xmm_f32(r), source) {
            if inexact {
                *mxcsr |= PE as i32;
            }
            write_xmm_f32(r, v);
            return;
        }
    }
    let mut fl = 0;
    let v = f32_op(op, read_xmm_f32(r), source, &mut fl);
    if commit(fl) {
        write_xmm_f32(r, v);
    }
}

pub unsafe fn sd(op: Op, source: f64, r: i32) {
    if usual_mxcsr() {
        let check = *mxcsr as u32 & PE == 0;
        let mut inexact = false;
        if let Some(v) = fast_f64(op, f64::from_bits(read_xmm64s(r)), source, &mut inexact, check) {
            if inexact {
                *mxcsr |= PE as i32;
            }
            write_xmm_f64(r, v);
            return;
        }
    }
    let mut fl = 0;
    let v = f64_op(op, f64::from_bits(read_xmm64s(r)), source, &mut fl);
    if commit(fl) {
        write_xmm_f64(r, v);
    }
}
