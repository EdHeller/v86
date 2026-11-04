//! Three byte SSE opcodes (0F 38 xx and 0F 3A xx): SSSE3, SSE4.1 and parts of SSE4.2.
//!
//! The instructions are shared between the 16/32-bit interpreter (`instr_0F38` / `instr_0F3A`
//! in instructions_0f.rs) and the 64-bit mode interpreter (cpu/long_mode.rs). The register
//! operand `r` and a register source are xmm (or mmx for the prefix-less forms) register numbers,
//! memory operands are passed as 32-bit addresses.

use crate::cpu::cpu::*;
use crate::cpu::global_pointers::*;
use crate::cpu::sse_instr::sse_integer_round;
use crate::paging::OrPageFault;

#[derive(Copy, Clone)]
pub enum Src {
    Reg(i32),
    Mem(i32),
}

#[derive(Copy, Clone, PartialEq)]
pub enum Mandatory {
    None,
    P66,
    F2,
    F3,
}

unsafe fn read128(src: Src) -> OrPageFault<reg128> {
    match src {
        Src::Reg(n) => Ok(read_xmm128s(n)),
        // legacy sse: a 16 byte memory operand has to be 16 byte aligned
        Src::Mem(a) => safe_read128s_aligned(a),
    }
}

unsafe fn read64(src: Src) -> OrPageFault<u64> {
    match src {
        Src::Reg(n) => Ok(read_mmx64s(n)),
        Src::Mem(a) => safe_read64s(a),
    }
}

unsafe fn read_low64(src: Src) -> OrPageFault<u64> {
    match src {
        Src::Reg(n) => Ok(read_xmm64s(n)),
        Src::Mem(a) => safe_read64s(a),
    }
}

fn sat_i16(v: i32) -> i16 { v.clamp(i16::MIN as i32, i16::MAX as i32) as i16 }

fn to_reg(b: [u8; 16]) -> reg128 { reg128 { u8: b } }

unsafe fn mmx64(v: u64) -> reg128 { reg128 { u64: [v, 0] } }

/// Applies a 128 bit operation, to xmm registers with the 66 prefix and to mmx registers
/// (using the low 64 bits) without
unsafe fn simd(
    mandatory: Mandatory,
    r: i32,
    src: Src,
    f: impl Fn(reg128, reg128) -> reg128,
) -> OrPageFault<()> {
    if mandatory == Mandatory::P66 {
        let s = read128(src)?;
        let d = read_xmm128s(r);
        write_xmm_reg128(r, f(d, s));
    }
    else {
        let s = mmx64(read64(src)?);
        let d = mmx64(read_mmx64s(r));
        write_mmx_reg64(r, f(d, s).u64[0]);
        transition_fpu_to_mmx();
    }
    Ok(())
}

/// Lanes available to an operation: 16 bytes for xmm, 8 for mmx
fn lanes(mandatory: Mandatory) -> usize { if mandatory == Mandatory::P66 { 16 } else { 8 } }

pub unsafe fn exec_0f38(op: u8, mandatory: Mandatory, r: i32, src: Src) -> OrPageFault<()> {
    let n = lanes(mandatory);
    match op {
        // ---- SSSE3 (mmx and xmm forms)
        0x00 => simd(mandatory, r, src, |d, s| unsafe {
            // pshufb
            let mut out = [0u8; 16];
            for i in 0..n {
                let c = s.u8[i];
                out[i] = if c & 0x80 != 0 { 0 } else { d.u8[(c as usize) & (n - 1)] };
            }
            to_reg(out)
        }),
        0x01 | 0x05 => simd(mandatory, r, src, |d, s| unsafe {
            // phaddw / phsubw
            let w = n / 2;
            let mut out = [0i16; 8];
            for i in 0..w / 2 {
                let (a, b) = (d.i16[2 * i], d.i16[2 * i + 1]);
                out[i] = if op == 0x01 { a.wrapping_add(b) } else { a.wrapping_sub(b) };
                let (a, b) = (s.i16[2 * i], s.i16[2 * i + 1]);
                out[w / 2 + i] = if op == 0x01 { a.wrapping_add(b) } else { a.wrapping_sub(b) };
            }
            reg128 { i16: out }
        }),
        0x02 | 0x06 => simd(mandatory, r, src, |d, s| unsafe {
            // phaddd / phsubd
            let w = n / 4;
            let mut out = [0i32; 4];
            for i in 0..w / 2 {
                let (a, b) = (d.i32[2 * i], d.i32[2 * i + 1]);
                out[i] = if op == 0x02 { a.wrapping_add(b) } else { a.wrapping_sub(b) };
                let (a, b) = (s.i32[2 * i], s.i32[2 * i + 1]);
                out[w / 2 + i] = if op == 0x02 { a.wrapping_add(b) } else { a.wrapping_sub(b) };
            }
            reg128 { i32: out }
        }),
        0x03 | 0x07 => simd(mandatory, r, src, |d, s| unsafe {
            // phaddsw / phsubsw
            let w = n / 2;
            let mut out = [0i16; 8];
            for i in 0..w / 2 {
                let (a, b) = (d.i16[2 * i] as i32, d.i16[2 * i + 1] as i32);
                out[i] = sat_i16(if op == 0x03 { a + b } else { a - b });
                let (a, b) = (s.i16[2 * i] as i32, s.i16[2 * i + 1] as i32);
                out[w / 2 + i] = sat_i16(if op == 0x03 { a + b } else { a - b });
            }
            reg128 { i16: out }
        }),
        0x04 => simd(mandatory, r, src, |d, s| unsafe {
            // pmaddubsw: unsigned bytes of the destination times signed bytes of the source
            let mut out = [0i16; 8];
            for i in 0..n / 2 {
                let v = d.u8[2 * i] as i32 * s.i8[2 * i] as i32
                    + d.u8[2 * i + 1] as i32 * s.i8[2 * i + 1] as i32;
                out[i] = sat_i16(v);
            }
            reg128 { i16: out }
        }),
        0x08 | 0x09 | 0x0A => simd(mandatory, r, src, |d, s| unsafe {
            // psignb / psignw / psignd
            let mut out = d;
            match op {
                0x08 => {
                    for i in 0..n {
                        out.i8[i] = match s.i8[i] {
                            0 => 0,
                            x if x < 0 => d.i8[i].wrapping_neg(),
                            _ => d.i8[i],
                        };
                    }
                },
                0x09 => {
                    for i in 0..n / 2 {
                        out.i16[i] = match s.i16[i] {
                            0 => 0,
                            x if x < 0 => d.i16[i].wrapping_neg(),
                            _ => d.i16[i],
                        };
                    }
                },
                _ => {
                    for i in 0..n / 4 {
                        out.i32[i] = match s.i32[i] {
                            0 => 0,
                            x if x < 0 => d.i32[i].wrapping_neg(),
                            _ => d.i32[i],
                        };
                    }
                },
            }
            out
        }),
        0x0B => simd(mandatory, r, src, |d, s| unsafe {
            // pmulhrsw
            let mut out = d;
            for i in 0..n / 2 {
                let v = (d.i16[i] as i32 * s.i16[i] as i32 >> 14) + 1 >> 1;
                out.i16[i] = v as i16;
            }
            out
        }),
        0x1C | 0x1D | 0x1E => simd(mandatory, r, src, |_d, s| unsafe {
            // pabsb / pabsw / pabsd
            let mut out = s;
            match op {
                0x1C => (0..n).for_each(|i| out.u8[i] = s.i8[i].wrapping_abs() as u8),
                0x1D => (0..n / 2).for_each(|i| out.u16[i] = s.i16[i].wrapping_abs() as u16),
                _ => (0..n / 4).for_each(|i| out.u32[i] = s.i32[i].wrapping_abs() as u32),
            }
            out
        }),
        // ---- SHA extensions (no prefix; sha256rnds2 reads xmm0)
        0xC8..=0xCD if mandatory == Mandatory::None => {
            let sv = read128(src)?;
            let dv = read_xmm128s(r);
            let x0 = read_xmm128s(0);
            let out = sha(op as u32, unsafe { dv.u32 }, unsafe { sv.u32 }, unsafe { x0.u32 }, 0);
            write_xmm_reg128(r, reg128 { u32: out });
            Ok(())
        },
        // ---- SSE4.1 / SSE4.2 (all need the 66 prefix)
        _ if mandatory != Mandatory::P66 => {
            trigger_ud();
            Err(())
        },
        0xDB..=0xDF => {
            // aesimc, aesenc, aesenclast, aesdec, aesdeclast
            let key = read128(src)?;
            let state = read_xmm128s(r);
            write_xmm_reg128(r, to_reg(aes(op, unsafe { state.u8 }, unsafe { key.u8 }, -1)));
            Ok(())
        },
        0x10 | 0x14 | 0x15 => {
            // pblendvb / blendvps / blendvpd with the mask in xmm0
            let s = read128(src)?;
            let d = read_xmm128s(r);
            let m = read_xmm128s(0);
            let mut out = d;
            unsafe {
                match op {
                    0x10 => (0..16).for_each(|i| if m.u8[i] & 0x80 != 0 { out.u8[i] = s.u8[i] }),
                    0x14 => (0..4).for_each(|i| if m.u32[i] & 0x8000_0000 != 0 { out.u32[i] = s.u32[i] }),
                    _ => (0..2).for_each(|i| if m.u64[i] >> 63 != 0 { out.u64[i] = s.u64[i] }),
                }
            }
            write_xmm_reg128(r, out);
            Ok(())
        },
        0x17 => {
            // ptest
            let s = read128(src)?;
            let d = read_xmm128s(r);
            let (zf, cf) = unsafe {
                (
                    d.u64[0] & s.u64[0] == 0 && d.u64[1] & s.u64[1] == 0,
                    !d.u64[0] & s.u64[0] == 0 && !d.u64[1] & s.u64[1] == 0,
                )
            };
            *flags = *flags & !FLAGS_ALL | zf as i32 * FLAG_ZERO | cf as i32 * FLAG_CARRY;
            *flags_changed = 0;
            Ok(())
        },
        0x20..=0x25 | 0x30..=0x35 => {
            // pmovsx / pmovzx: (source element size, destination element size)
            let signed = op < 0x30;
            let kind = op & 0xF;
            let s = read_low64(src)?;
            let b = s.to_le_bytes();
            let mut out = reg128 { u64: [0, 0] };
            unsafe {
                match kind {
                    0 => (0..8).for_each(|i| out.i16[i] = if signed { b[i] as i8 as i16 } else { b[i] as i16 }),
                    1 => (0..4).for_each(|i| out.i32[i] = if signed { b[i] as i8 as i32 } else { b[i] as i32 }),
                    2 => (0..2).for_each(|i| out.i64[i] = if signed { b[i] as i8 as i64 } else { b[i] as i64 }),
                    3 => (0..4).for_each(|i| {
                        let w = u16::from_le_bytes([b[2 * i], b[2 * i + 1]]);
                        out.i32[i] = if signed { w as i16 as i32 } else { w as i32 }
                    }),
                    4 => (0..2).for_each(|i| {
                        let w = u16::from_le_bytes([b[2 * i], b[2 * i + 1]]);
                        out.i64[i] = if signed { w as i16 as i64 } else { w as i64 }
                    }),
                    _ => (0..2).for_each(|i| {
                        let d = u32::from_le_bytes([b[4 * i], b[4 * i + 1], b[4 * i + 2], b[4 * i + 3]]);
                        out.i64[i] = if signed { d as i32 as i64 } else { d as i64 }
                    }),
                }
            }
            write_xmm_reg128(r, out);
            Ok(())
        },
        0x28 => {
            // pmuldq
            let s = read128(src)?;
            let d = read_xmm128s(r);
            unsafe {
                write_xmm_reg128(
                    r,
                    reg128 {
                        i64: [d.i32[0] as i64 * s.i32[0] as i64, d.i32[2] as i64 * s.i32[2] as i64],
                    },
                );
            }
            Ok(())
        },
        0x29 | 0x37 => {
            // pcmpeqq / pcmpgtq
            let s = read128(src)?;
            let d = read_xmm128s(r);
            let mut out = d;
            unsafe {
                for i in 0..2 {
                    let t = if op == 0x29 { d.i64[i] == s.i64[i] } else { d.i64[i] > s.i64[i] };
                    out.u64[i] = if t { u64::MAX } else { 0 };
                }
            }
            write_xmm_reg128(r, out);
            Ok(())
        },
        0x2A => match src {
            // movntdqa xmm, m128
            Src::Mem(a) => {
                write_xmm_reg128(r, safe_read128s_aligned(a)?);
                Ok(())
            },
            Src::Reg(_) => {
                trigger_ud();
                Err(())
            },
        },
        0x2B => {
            // packusdw
            let s = read128(src)?;
            let d = read_xmm128s(r);
            let sat = |v: i32| v.clamp(0, 0xFFFF) as u16;
            unsafe {
                write_xmm_reg128(
                    r,
                    reg128 {
                        u16: [
                            sat(d.i32[0]),
                            sat(d.i32[1]),
                            sat(d.i32[2]),
                            sat(d.i32[3]),
                            sat(s.i32[0]),
                            sat(s.i32[1]),
                            sat(s.i32[2]),
                            sat(s.i32[3]),
                        ],
                    },
                );
            }
            Ok(())
        },
        0x38..=0x3F | 0x40 => {
            // pminsb pminsd pminuw pminud pmaxsb pmaxsd pmaxuw pmaxud pmulld
            let s = read128(src)?;
            let d = read_xmm128s(r);
            let mut out = d;
            unsafe {
                match op {
                    0x38 => (0..16).for_each(|i| out.i8[i] = d.i8[i].min(s.i8[i])),
                    0x39 => (0..4).for_each(|i| out.i32[i] = d.i32[i].min(s.i32[i])),
                    0x3A => (0..8).for_each(|i| out.u16[i] = d.u16[i].min(s.u16[i])),
                    0x3B => (0..4).for_each(|i| out.u32[i] = d.u32[i].min(s.u32[i])),
                    0x3C => (0..16).for_each(|i| out.i8[i] = d.i8[i].max(s.i8[i])),
                    0x3D => (0..4).for_each(|i| out.i32[i] = d.i32[i].max(s.i32[i])),
                    0x3E => (0..8).for_each(|i| out.u16[i] = d.u16[i].max(s.u16[i])),
                    0x3F => (0..4).for_each(|i| out.u32[i] = d.u32[i].max(s.u32[i])),
                    _ => (0..4).for_each(|i| out.i32[i] = d.i32[i].wrapping_mul(s.i32[i])),
                }
            }
            write_xmm_reg128(r, out);
            Ok(())
        },
        0x41 => {
            // phminposuw
            let s = read128(src)?;
            let mut min = unsafe { s.u16[0] };
            let mut idx = 0;
            for i in 1..8 {
                let v = unsafe { s.u16[i] };
                if v < min {
                    min = v;
                    idx = i;
                }
            }
            write_xmm_reg128(r, reg128 { u16: [min, idx as u16, 0, 0, 0, 0, 0, 0] });
            Ok(())
        },
        _ => {
            dbg_log!("Unimplemented 0F38 opcode {:x}", op);
            trigger_ud();
            Err(())
        },
    }
}

unsafe fn round_f64(x: f64, imm: i32) -> f64 {
    let mode = if imm & 4 != 0 { *mxcsr >> 13 & 3 } else { imm & 3 };
    match mode {
        0 => x.round_ties_even(),
        1 => x.floor(),
        2 => x.ceil(),
        _ => x.trunc(),
    }
}

unsafe fn round_f32(x: f32, imm: i32) -> f32 {
    let mode = if imm & 4 != 0 { *mxcsr >> 13 & 3 } else { imm & 3 };
    match mode {
        0 => x.round_ties_even(),
        1 => x.floor(),
        2 => x.ceil(),
        _ => x.trunc(),
    }
}

pub unsafe fn exec_0f3a(op: u8, mandatory: Mandatory, r: i32, src: Src, imm: i32) -> OrPageFault<()> {
    let _ = sse_integer_round;
    match op {
        0x0F => {
            // palignr
            let n = lanes(mandatory);
            let shift = imm as usize & 0xFF;
            return simd(mandatory, r, src, |d, s| unsafe {
                let mut cat = [0u8; 32];
                for i in 0..n {
                    cat[i] = s.u8[i];
                    cat[n + i] = d.u8[i];
                }
                let mut out = [0u8; 16];
                for i in 0..n {
                    out[i] = if i + shift < 2 * n { cat[i + shift] } else { 0 };
                }
                to_reg(out)
            });
        },
        0xCC if mandatory == Mandatory::None => {
            // sha1rnds4
            let sv = read128(src)?;
            let dv = read_xmm128s(r);
            let out = sha(0x1CC, unsafe { dv.u32 }, unsafe { sv.u32 }, [0; 4], imm);
            write_xmm_reg128(r, reg128 { u32: out });
            return Ok(());
        },
        _ if mandatory != Mandatory::P66 => {
            trigger_ud();
            return Err(());
        },
        _ => {},
    }
    match op {
        0x08 => {
            // roundps
            let s = read128(src)?;
            let mut out = s;
            unsafe { (0..4).for_each(|i| out.f32[i] = round_f32(s.f32[i], imm)) };
            write_xmm_reg128(r, out);
        },
        0x09 => {
            // roundpd
            let s = read128(src)?;
            let mut out = s;
            unsafe { (0..2).for_each(|i| out.f64[i] = round_f64(s.f64[i], imm)) };
            write_xmm_reg128(r, out);
        },
        0x0A => {
            // roundss
            let v = match src {
                Src::Reg(n) => read_xmm_f32(n),
                Src::Mem(a) => safe_read_f32(a)?,
            };
            write_xmm_f32(r, round_f32(v, imm));
        },
        0x0B => {
            // roundsd
            let v = f64::from_bits(read_low64(src)?);
            write_xmm_f64(r, round_f64(v, imm));
        },
        0x0C | 0x0D | 0x0E => {
            // blendps / blendpd / pblendw
            let s = read128(src)?;
            let mut out = read_xmm128s(r);
            unsafe {
                match op {
                    0x0C => (0..4).for_each(|i| if imm >> i & 1 != 0 { out.u32[i] = s.u32[i] }),
                    0x0D => (0..2).for_each(|i| if imm >> i & 1 != 0 { out.u64[i] = s.u64[i] }),
                    _ => (0..8).for_each(|i| if imm >> i & 1 != 0 { out.u16[i] = s.u16[i] }),
                }
            }
            write_xmm_reg128(r, out);
        },
        0x21 => {
            // insertps: the memory form is a 32 bit operand (no alignment, no read past it)
            let s = match src {
                Src::Reg(n) => read_xmm128s(n),
                Src::Mem(a) => reg128 { u32: [safe_read32s(a)? as u32, 0, 0, 0] },
            };
            let (count_s, count_d, zmask) = match src {
                Src::Reg(_) => (imm >> 6 & 3, imm >> 4 & 3, imm & 15),
                Src::Mem(_) => (0, imm >> 4 & 3, imm & 15),
            };
            let mut out = read_xmm128s(r);
            unsafe {
                out.u32[count_d as usize] = s.u32[count_s as usize];
                for i in 0..4 {
                    if zmask >> i & 1 != 0 {
                        out.u32[i] = 0;
                    }
                }
            }
            write_xmm_reg128(r, out);
        },
        0x40 => {
            // dpps
            let s = read128(src)?;
            let d = read_xmm128s(r);
            let mut p = [0f32; 4];
            unsafe {
                for i in 0..4 {
                    p[i] = if imm >> (4 + i) & 1 != 0 { d.f32[i] * s.f32[i] } else { 0.0 };
                }
            }
            let sum = (p[0] + p[1]) + (p[2] + p[3]);
            let mut out = reg128 { u64: [0, 0] };
            unsafe {
                for i in 0..4 {
                    out.f32[i] = if imm >> i & 1 != 0 { sum } else { 0.0 };
                }
            }
            write_xmm_reg128(r, out);
        },
        0x41 => {
            // dppd
            let s = read128(src)?;
            let d = read_xmm128s(r);
            let mut sum = 0f64;
            unsafe {
                for i in 0..2 {
                    if imm >> (4 + i) & 1 != 0 {
                        sum += d.f64[i] * s.f64[i];
                    }
                }
            }
            let mut out = reg128 { u64: [0, 0] };
            unsafe {
                for i in 0..2 {
                    out.f64[i] = if imm >> i & 1 != 0 { sum } else { 0.0 };
                }
            }
            write_xmm_reg128(r, out);
        },
        0x42 => {
            // mpsadbw
            let s = read128(src)?;
            let d = read_xmm128s(r);
            let s_off = (imm as usize & 3) * 4;
            let d_off = (imm as usize >> 2 & 1) * 4;
            let mut out = [0u16; 8];
            unsafe {
                for j in 0..8 {
                    let mut sum = 0u16;
                    for k in 0..4 {
                        sum += (d.u8[d_off + j + k] as i16 - s.u8[s_off + k] as i16).unsigned_abs();
                    }
                    out[j] = sum;
                }
            }
            write_xmm_reg128(r, reg128 { u16: out });
        },
        0xDF => {
            // aeskeygenassist
            let sv = read128(src)?;
            write_xmm_reg128(r, to_reg(aes(0xDF, [0; 16], unsafe { sv.u8 }, imm & 0xFF)));
        },
        0x44 => {
            // pclmulqdq
            let s = read128(src)?;
            let d = read_xmm128s(r);
            let a = unsafe { d.u64[(imm & 1) as usize] };
            let b = unsafe { s.u64[(imm >> 4 & 1) as usize] };
            let mut lo = 0u64;
            let mut hi = 0u64;
            for i in 0..64 {
                if b >> i & 1 != 0 {
                    lo ^= a << i;
                    if i > 0 {
                        hi ^= a >> (64 - i);
                    }
                }
            }
            write_xmm_reg128(r, reg128 { u64: [lo, hi] });
        },
        _ => {
            dbg_log!("Unimplemented 0F3A opcode {:x}", op);
            trigger_ud();
            return Err(());
        },
    }
    Ok(())
}

/// crc32 instruction: CRC-32C update with a value of `bytes` bytes
pub fn crc32c(mut crc: u32, data: u64, bytes: u32) -> u32 {
    for i in 0..bytes {
        crc ^= (data >> (8 * i)) as u8 as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { crc >> 1 ^ 0x82F63B78 } else { crc >> 1 };
        }
    }
    crc
}

// ---------------------------------------------------------------------------------------------
// AES-NI (66 0F 38 DB..DF, 66 0F 3A DF). The state is the register's 16 bytes, byte r + 4c being row r of column c.

const fn xtime(x: u8) -> u8 { (x << 1) ^ if x & 0x80 != 0 { 0x1B } else { 0 } }

const fn gmul(mut a: u8, mut b: u8) -> u8 {
    let mut p = 0u8;
    while b != 0 {
        if b & 1 != 0 {
            p ^= a;
        }
        a = xtime(a);
        b >>= 1;
    }
    p
}

const fn make_sbox() -> [u8; 256] {
    let mut s = [0u8; 256];
    let mut x = 0;
    while x < 256 {
        // the multiplicative inverse (x^254), then the affine transformation
        let mut inv = 1u8;
        let mut k = 0;
        while k < 254 {
            inv = gmul(inv, x as u8);
            k += 1;
        }
        if x == 0 {
            inv = 0;
        }
        let b = inv;
        s[x] = b ^ b.rotate_left(1) ^ b.rotate_left(2) ^ b.rotate_left(3) ^ b.rotate_left(4) ^ 0x63;
        x += 1;
    }
    s
}

const fn make_inv_sbox(s: &[u8; 256]) -> [u8; 256] {
    let mut inv = [0u8; 256];
    let mut x = 0;
    while x < 256 {
        inv[s[x] as usize] = x as u8;
        x += 1;
    }
    inv
}

static SBOX: [u8; 256] = make_sbox();
static INV_SBOX: [u8; 256] = make_inv_sbox(&SBOX);

fn shift_rows(s: [u8; 16], inverse: bool) -> [u8; 16] {
    let mut out = [0u8; 16];
    for c in 0..4 {
        for r in 0..4 {
            let from = if inverse { (c + 4 - r) % 4 } else { (c + r) % 4 };
            out[r + 4 * c] = s[r + 4 * from];
        }
    }
    out
}

fn mix_columns(s: [u8; 16], inverse: bool) -> [u8; 16] {
    let m: [u8; 4] = if inverse { [14, 11, 13, 9] } else { [2, 3, 1, 1] };
    let mut out = [0u8; 16];
    for c in 0..4 {
        for r in 0..4 {
            let mut v = 0;
            for k in 0..4 {
                v ^= gmul(m[(k + 4 - r) % 4], s[k + 4 * c]);
            }
            out[r + 4 * c] = v;
        }
    }
    out
}

fn sub_bytes(s: [u8; 16], table: &[u8; 256]) -> [u8; 16] { s.map(|b| table[b as usize]) }

fn xor16(a: [u8; 16], b: [u8; 16]) -> [u8; 16] {
    let mut out = a;
    for i in 0..16 {
        out[i] ^= b[i];
    }
    out
}

fn sub_word(w: u32) -> u32 { u32::from_le_bytes(w.to_le_bytes().map(|b| SBOX[b as usize])) }

/// aesenc, aesenclast, aesdec, aesdeclast, aesimc (0F 38 op), aeskeygenassist (`imm` given)
pub fn aes(op: u8, state: [u8; 16], key: [u8; 16], imm: i32) -> [u8; 16] {
    match op {
        0xDC => xor16(mix_columns(sub_bytes(shift_rows(state, false), &SBOX), false), key),
        0xDD => xor16(sub_bytes(shift_rows(state, false), &SBOX), key),
        0xDE => xor16(mix_columns(sub_bytes(shift_rows(state, true), &INV_SBOX), true), key),
        0xDF if imm < 0 => xor16(sub_bytes(shift_rows(state, true), &INV_SBOX), key),
        0xDB => mix_columns(key, true),
        _ => {
            // aeskeygenassist: of the source's dwords 1 and 3
            let x1 = u32::from_le_bytes([key[4], key[5], key[6], key[7]]);
            let x3 = u32::from_le_bytes([key[12], key[13], key[14], key[15]]);
            let rcon = imm as u32 & 0xFF;
            let w = [sub_word(x1), sub_word(x1).rotate_right(8) ^ rcon, sub_word(x3), sub_word(x3).rotate_right(8) ^ rcon];
            let mut out = [0u8; 16];
            for i in 0..4 {
                out[4 * i..4 * i + 4].copy_from_slice(&w[i].to_le_bytes());
            }
            out
        },
    }
}

// ---------------------------------------------------------------------------------------------
// SHA extensions (0F 38 C8..CD, 0F 3A CC); dword 3 is the register's high dword (bits 127:96)

/// sha1nexte, sha1msg1, sha1msg2, sha256rnds2 (with xmm0), sha256msg1, sha256msg2; sha1rnds4 as op 0xCC + 0x100
pub fn sha(op: u32, d: [u32; 4], s: [u32; 4], xmm0: [u32; 4], imm: i32) -> [u32; 4] {
    match op {
        0xC8 => [s[0], s[1], s[2], s[3].wrapping_add(d[3].rotate_left(30))],
        0xC9 => [d[0] ^ s[2], d[1] ^ s[3], d[2] ^ d[0], d[3] ^ d[1]],
        0xCA => {
            let w16 = (d[3] ^ s[2]).rotate_left(1);
            let w17 = (d[2] ^ s[1]).rotate_left(1);
            let w18 = (d[1] ^ s[0]).rotate_left(1);
            let w19 = (d[0] ^ w16).rotate_left(1);
            [w19, w18, w17, w16]
        },
        0xCB => {
            let (mut a, mut b, mut c, mut dd) = (s[3], s[2], d[3], d[2]);
            let (mut e, mut f, mut g, mut h) = (s[1], s[0], d[1], d[0]);
            for wk in [xmm0[0], xmm0[1]] {
                let ch = (e & f) ^ (!e & g);
                let maj = (a & b) ^ (a & c) ^ (b & c);
                let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
                let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
                let t = ch.wrapping_add(s1).wrapping_add(wk).wrapping_add(h);
                let a2 = t.wrapping_add(maj).wrapping_add(s0);
                let e2 = t.wrapping_add(dd);
                h = g;
                g = f;
                f = e;
                e = e2;
                dd = c;
                c = b;
                b = a;
                a = a2;
            }
            [f, e, b, a]
        },
        0xCC => {
            let sigma0 = |x: u32| x.rotate_right(7) ^ x.rotate_right(18) ^ x >> 3;
            [
                d[0].wrapping_add(sigma0(d[1])),
                d[1].wrapping_add(sigma0(d[2])),
                d[2].wrapping_add(sigma0(d[3])),
                d[3].wrapping_add(sigma0(s[0])),
            ]
        },
        0xCD => {
            let sigma1 = |x: u32| x.rotate_right(17) ^ x.rotate_right(19) ^ x >> 10;
            let w16 = d[0].wrapping_add(sigma1(s[2]));
            let w17 = d[1].wrapping_add(sigma1(s[3]));
            let w18 = d[2].wrapping_add(sigma1(w16));
            let w19 = d[3].wrapping_add(sigma1(w17));
            [w16, w17, w18, w19]
        },
        _ => {
            // sha1rnds4: four rounds with the function and constant imm selects
            let k = [0x5A827999u32, 0x6ED9EBA1, 0x8F1BBCDC, 0xCA62C1D6][(imm & 3) as usize];
            let f = |b: u32, c: u32, dd: u32| match imm & 3 {
                0 => (b & c) ^ (!b & dd),
                2 => (b & c) ^ (b & dd) ^ (c & dd),
                _ => b ^ c ^ dd,
            };
            let (mut a, mut b, mut c, mut dd) = (d[3], d[2], d[1], d[0]);
            let w = [s[3], s[2], s[1], s[0]];
            let mut e = 0;
            for i in 0..4 {
                let a2 = f(b, c, dd).wrapping_add(a.rotate_left(5)).wrapping_add(w[i]).wrapping_add(e).wrapping_add(k);
                e = dd;
                dd = c;
                c = b.rotate_left(30);
                b = a;
                a = a2;
            }
            [dd, c, b, a]
        },
    }
}

// ---------------------------------------------------------------------------------------------
// SSE4.2 string compares (66 0F 3A 60..63): the shared core; the callers read the lengths (pcmpestr*: eax, edx)
// and write ecx, xmm0 and the flags

pub struct StrCmp {
    /// IntRes2, one bit per element
    pub res: u32,
    /// elements per register: 16 bytes or 8 words
    pub n: u32,
    pub la: u32,
    pub lb: u32,
}

/// `la` / `lb`: the explicit lengths (pcmpestr*), None for null terminated (pcmpistr*)
pub fn pcmpstr(a: reg128, b: reg128, la: Option<i64>, lb: Option<i64>, imm: i32) -> StrCmp {
    let words = imm & 1 != 0;
    let signed = imm & 2 != 0;
    let n: u32 = if words { 8 } else { 16 };
    let elem = |x: &reg128, i: u32| -> i32 {
        unsafe {
            match (words, signed) {
                (false, false) => x.u8[i as usize] as i32,
                (false, true) => x.i8[i as usize] as i32,
                (true, false) => x.u16[i as usize] as i32,
                (true, true) => x.i16[i as usize] as i32,
            }
        }
    };
    let length = |x: &reg128, explicit: Option<i64>| -> u32 {
        match explicit {
            Some(v) => v.unsigned_abs().min(n as u64) as u32,
            None => (0..n).find(|&i| elem(x, i) == 0).unwrap_or(n),
        }
    };
    let la = length(&a, la);
    let lb = length(&b, lb);
    let mut res1 = 0u32;
    match imm >> 2 & 3 {
        0 => {
            // equal any: b[i] is one of a's valid elements
            for i in 0..lb {
                if (0..la).any(|j| elem(&a, j) == elem(&b, i)) {
                    res1 |= 1 << i;
                }
            }
        },
        1 => {
            // ranges: a holds pairs (low, high)
            for i in 0..lb {
                let v = elem(&b, i);
                let mut j = 0;
                while j + 1 < la {
                    if elem(&a, j) <= v && v <= elem(&a, j + 1) {
                        res1 |= 1 << i;
                        break;
                    }
                    j += 2;
                }
            }
        },
        2 => {
            // equal each: both invalid compares true, one invalid false
            for i in 0..n {
                let ok = match (i < la, i < lb) {
                    (true, true) => elem(&a, i) == elem(&b, i),
                    (false, false) => true,
                    _ => false,
                };
                if ok {
                    res1 |= 1 << i;
                }
            }
        },
        _ => {
            // equal ordered: a occurs in b at i (a's invalid elements match anything, b's invalid ones nothing)
            for i in 0..n {
                let mut ok = true;
                for k in 0..n - i {
                    if k >= la {
                        break;
                    }
                    if i + k >= lb || elem(&a, k) != elem(&b, i + k) {
                        ok = false;
                        break;
                    }
                }
                if ok {
                    res1 |= 1 << i;
                }
            }
        },
    }
    let all = (1u32 << n) - 1;
    let res = match imm >> 4 & 3 {
        1 => !res1 & all,
        3 => res1 ^ ((1u32 << lb) - 1),
        _ => res1,
    };
    StrCmp { res, n, la, lb }
}

impl StrCmp {
    /// pcmpestri / pcmpistri: the index for ecx
    pub fn index(&self, imm: i32) -> u32 {
        if self.res == 0 {
            self.n
        }
        else if imm & 0x40 != 0 {
            31 - self.res.leading_zeros()
        }
        else {
            self.res.trailing_zeros()
        }
    }

    /// pcmpestrm / pcmpistrm: the mask for xmm0
    pub fn mask(&self, imm: i32) -> reg128 {
        let mut out = reg128 { u64: [0, 0] };
        if imm & 0x40 == 0 {
            unsafe { out.u64[0] = self.res as u64 };
        }
        else {
            for i in 0..self.n {
                if self.res >> i & 1 != 0 {
                    unsafe {
                        if self.n == 8 {
                            out.u16[i as usize] = 0xFFFF;
                        }
                        else {
                            out.u8[i as usize] = 0xFF;
                        }
                    }
                }
            }
        }
        out
    }

    /// CF: any match, ZF: b ended (lb < n), SF: a ended, OF: bit 0; AF and PF clear
    pub unsafe fn set_flags(&self) {
        let mut f = 0;
        if self.res != 0 {
            f |= FLAG_CARRY;
        }
        if self.lb < self.n {
            f |= FLAG_ZERO;
        }
        if self.la < self.n {
            f |= FLAG_SIGN;
        }
        if self.res & 1 != 0 {
            f |= FLAG_OVERFLOW;
        }
        *flags_changed = 0;
        *flags = *flags & !FLAGS_ALL | f;
    }
}
