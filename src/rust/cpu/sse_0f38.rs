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
        // ---- SSE4.1 / SSE4.2 (all need the 66 prefix)
        _ if mandatory != Mandatory::P66 => {
            trigger_ud();
            Err(())
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
