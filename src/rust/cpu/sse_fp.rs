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

/// Whether the single precision sum or difference r of x and y (finite) was rounded. Not by redoing it in double
/// precision: that is exact only while the exponents are close (-3.7e9 + 2.9e-24 rounds in both). TwoSum: with
/// bb = r - x the rounding error (x - (r - bb)) + (y - bb) is exact, zero exactly when x - (r - bb) == bb - y.
#[inline(always)]
fn add_inexact32(op: Op, x: f32, y: f32, r: f32) -> bool {
    let y = if op == Op::Sub { -y } else { y };
    let bb = r - x;
    x - (r - bb) != bb - y
}

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
        Op::Add | Op::Sub => add_inexact32(op, x, y, r),
        // (products and quotient checks of singles are exact in double precision: 48 bits)
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
        // (a product that comes out denormal or zero: its error term would underflow too and look exact, so the
        // smaller operand (both when the larger is below 1) and the result are scaled by 2^1000 first, exactly)
        Op::Mul if (is_denormal64(r) || r == 0.0) && x != 0.0 && y != 0.0 => {
            const SCALE: f64 = 1.0715086071862673e301; // 2^1000
            let (small, large) = if x.abs() < y.abs() { (x, y) } else { (y, x) };
            if large.abs() < 1.0 {
                // (both tiny: both scaled, the product by 2^2000, still below the overflow)
                (small * SCALE).mul_add(large * SCALE, -(r * SCALE * SCALE)) != 0.0
            }
            else {
                (small * SCALE).mul_add(large, -(r * SCALE)) != 0.0
            }
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

/// The status flags of the conversions to integers of the instruction running (reset by it, committed before it
/// writes its destination)
pub static mut CVT_FLAGS: u32 = 0;

/// A conversion to a 32-bit integer (cvt(t)ss2si, cvt(t)sd2si, the packed ones): truncated or rounded as MXCSR says;
/// NaN or out of range raises invalid and gives the integer indefinite 0x80000000, a fraction raises inexact (into
/// CVT_FLAGS)
pub unsafe fn cvt_i32_acc(x: f64, truncate: bool) -> i32 {
    let v = if truncate { x.trunc() } else { crate::cpu::sse_instr::sse_integer_round(x) };
    if !(v >= -2147483648.0 && v < 2147483648.0) {
        CVT_FLAGS |= IE;
        return i32::MIN;
    }
    if v != x {
        CVT_FLAGS |= PE;
    }
    v as i32
}

/// The same for a helper called by compiled code: the flags committed at once (an unmasked exception is left
/// pending for the code to raise before it writes the destination)
pub unsafe fn cvt_i32_jit(x: f64, truncate: bool) -> i32 {
    CVT_FLAGS = 0;
    let v = cvt_i32_acc(x, truncate);
    commit(CVT_FLAGS);
    v
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

/// Off: every operation through the exact per-element path (set_jit_config(60, 1); tests/x64/js/sse_fast_paths.js
/// compares the two)
pub static mut FAST_PATHS_OFF: bool = false;

/// MXCSR as software usually leaves it: every exception masked, round to nearest, no DAZ, no FTZ (the status flags
/// aside). Then ordinary operands and results can raise nothing but the sticky inexact flag.
#[inline(always)]
unsafe fn usual_mxcsr() -> bool { *mxcsr & !0x3F == 0x1F80 && !FAST_PATHS_OFF }

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
/// SIMD: the result and the status flags it raises, as f32_op lane by lane, or None when a lane needs the exact
/// per-element path (a tiny result: denormal, or an underflow to zero). NaNs and infinities stay here (once a
/// computation has overflowed every following operation would otherwise take the slow path): a NaN operand is
/// returned quieted (the destination's first), an invalid operation gives the default NaN. The inexact flag is
/// only looked for while it is still clear.
/// The lanes of r = a op b (finite lanes) that were rounded: TwoSum for sums and differences (see add_inexact32),
/// the product in double precision (exact there: 48 bits)
#[cfg(target_feature = "simd128")]
#[inline(always)]
fn inexact_lanes(op: Op, a: core::arch::wasm32::v128, b: core::arch::wasm32::v128, r: core::arch::wasm32::v128) -> core::arch::wasm32::v128 {
    use core::arch::wasm32::*;
    match op {
        Op::Add | Op::Sub => {
            let bb = f32x4_sub(r, a);
            let t = f32x4_sub(a, f32x4_sub(r, bb));
            f32x4_ne(t, if op == Op::Add { f32x4_sub(bb, b) } else { f32x4_add(b, bb) })
        },
        _ => {
            let hi = |x: v128| i32x4_shuffle::<2, 3, 0, 1>(x, x);
            let differs = |x: v128, y: v128, z: v128| {
                f64x2_ne(f64x2_mul(f64x2_promote_low_f32x4(x), f64x2_promote_low_f32x4(y)), f64x2_promote_low_f32x4(z))
            };
            // (the 64-bit lane masks of both halves back to four 32-bit lanes)
            i32x4_shuffle::<0, 2, 4, 6>(differs(a, b, r), differs(hi(a), hi(b), hi(r)))
        },
    }
}

#[cfg(target_feature = "simd128")]
#[inline(always)]
unsafe fn ps_fast(op: Op, d: reg128, s: reg128) -> Option<(reg128, u32)> {
    use core::arch::wasm32::*;
    let a: v128 = std::mem::transmute(d);
    let b: v128 = std::mem::transmute(s);
    let r = match op {
        Op::Add => f32x4_add(a, b),
        Op::Sub => f32x4_sub(a, b),
        _ => f32x4_mul(a, b),
    };
    // a lane is ordinary when it is finite and normal or zero (float compares: fewer operations than the bits)
    let abs_mask = u32x4_splat(0x7FFF_FFFF);
    let (aa, ab, ar) = (v128_and(a, abs_mask), v128_and(b, abs_mask), v128_and(r, abs_mask));
    let (inf, min_normal, fz) = (f32x4_splat(f32::INFINITY), f32x4_splat(f32::MIN_POSITIVE), f32x4_splat(0.0));
    let (za, zb) = (f32x4_eq(aa, fz), f32x4_eq(ab, fz));
    let ordinary = |x: v128, zero: v128| v128_and(f32x4_lt(x, inf), v128_or(f32x4_ge(x, min_normal), zero));
    let mut ok = v128_and(ordinary(aa, za), ordinary(ab, zb));
    ok = v128_and(ok, if op == Op::Mul {
        // a zero product of nonzero numbers underflowed
        v128_and(f32x4_lt(ar, inf), v128_or(f32x4_ge(ar, min_normal), v128_or(za, zb)))
    }
    else {
        ordinary(ar, f32x4_eq(ar, fz))
    });
    if !i32x4_all_true(ok) {
        // (NaNs, infinities, denormals: rarer, and once a computation has overflowed every following operation has
        // them)
        return ps_special(op, a, b, r);
    }
    let mut fl = 0;
    if *mxcsr as u32 & PE == 0 && v128_any_true(inexact_lanes(op, a, b, r)) {
        fl = PE;
    }
    Some((std::mem::transmute(r), fl))
}

#[cfg(target_feature = "simd128")]
#[inline(never)]
unsafe fn ps_special(op: Op, a: core::arch::wasm32::v128, b: core::arch::wasm32::v128, r: core::arch::wasm32::v128) -> Option<(reg128, u32)> {
    use core::arch::wasm32::*;
    let exp = u32x4_splat(0x7F80_0000);
    let magnitude = u32x4_splat(0x7FFF_FFFF);
    let quiet = u32x4_splat(0x0040_0000);
    let zero = u32x4_splat(0);
    let nan = |x: v128| f32x4_ne(x, x);
    let denormal = |x: v128| v128_and(u32x4_eq(v128_and(x, exp), zero), u32x4_ne(v128_and(x, magnitude), zero));
    let infinite = |x: v128| u32x4_eq(v128_and(x, magnitude), exp);
    let (nan_a, nan_b, nan_r) = (nan(a), nan(b), nan(r));
    let nan_in = v128_or(nan_a, nan_b);
    // the lanes computed from numbers (not from a NaN operand)
    let numbers = v128_not(nan_in);
    let mut tiny = denormal(r);
    if op == Op::Mul {
        // a product of nonzero numbers that became zero underflowed
        let fz = f32x4_splat(0.0);
        tiny = v128_or(tiny, v128_and(f32x4_eq(r, fz), v128_and(f32x4_ne(a, fz), f32x4_ne(b, fz))));
    }
    if v128_any_true(v128_and(tiny, numbers)) {
        return None;
    }
    let mut fl = 0;
    // a signaling NaN operand, or an invalid operation (a NaN from numbers: inf - inf, 0 * inf)
    let snan = |x: v128, n: v128| v128_and(n, u32x4_eq(v128_and(x, quiet), zero));
    if v128_any_true(v128_or(v128_or(snan(a, nan_a), snan(b, nan_b)), v128_and(nan_r, numbers))) {
        fl |= IE;
    }
    if v128_any_true(v128_and(numbers, v128_or(denormal(a), denormal(b)))) {
        fl |= DE;
    }
    let finite_in = v128_not(v128_or(v128_or(infinite(a), infinite(b)), nan_in));
    let inf_r = infinite(r);
    if v128_any_true(v128_and(inf_r, finite_in)) {
        fl |= OE | PE;
    }
    // the result lanes: the destination's NaN quieted, else the source's, else the default NaN for an invalid
    // operation, else the sum, difference or product
    let mut out = v128_bitselect(u32x4_splat(DEFAULT_NAN32), r, nan_r);
    out = v128_bitselect(v128_or(b, quiet), out, nan_b);
    out = v128_bitselect(v128_or(a, quiet), out, nan_a);
    if (*mxcsr as u32 | fl) & PE == 0 {
        // (the lanes of numbers with a finite result: the others raise no inexact)
        let ordinary = v128_and(numbers, v128_not(v128_or(nan_r, inf_r)));
        if v128_any_true(v128_and(inexact_lanes(op, a, b, r), ordinary)) {
            fl |= PE;
        }
    }
    Some((std::mem::transmute(out), fl))
}

#[cfg(not(target_feature = "simd128"))]
#[inline(always)]
unsafe fn ps_fast(_op: Op, _d: reg128, _s: reg128) -> Option<(reg128, u32)> { None }

pub unsafe fn ps(op: Op, source: reg128, r: i32) {
    let d = read_xmm128s(r);
    if usual_mxcsr() && matches!(op, Op::Add | Op::Sub | Op::Mul) {
        if let Some((out, fl)) = ps_fast(op, d, source) {
            // (every exception is masked: the flags are only recorded)
            *mxcsr |= fl as i32;
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
        inexact = match op {
            Op::Add | Op::Sub => add_inexact32(op, x, y, r),
            _ => r as f64 != x as f64 * y as f64,
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
