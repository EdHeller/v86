//! 64-bit mode (EFER.LMA with CS.L=1).
//!
//! The JIT and the generated interpreter only know 16 and 32 bit code, so 64-bit code is run by
//! the dedicated, hand written interpreter in this file: `step64` executes exactly one
//! instruction and is called from `cycle_internal` while `is_64` is set. The JIT is never entered.
//!
//! Register file: the low halves of rax..rdi stay in `reg32`, the upper halves of r0..r15 are in
//! `reg_hi` and the low halves of r8..r15 in `reg_ext`. Flags are computed eagerly
//! (`flags_changed` is always 0 after an instruction that modifies them).
//!
//! Current limitations (see tests/x64):
//! - linear addresses and rip are limited to the low 4GB; anything above raises #GP
//! - fs/gs bases only come from the segment descriptor (not from the base MSRs)
//! - ring 3 / stack switches (TSS, IST) in interrupt delivery and iretq are not implemented
//! - only a subset of the instruction set is implemented, unknown opcodes assert in debug builds
//!   and raise #UD otherwise

use crate::cpu::cpu::*;
use crate::cpu::long_mem::*;
use crate::cpu::sse_instr::sse_integer_round;
use crate::gen;
use crate::prefix;
use crate::cpu::global_pointers::*;
use crate::cpu::instructions;
use crate::cpu::instructions_0f;
use crate::cpu::misc_instr::{verr, verw};
use crate::paging::OrPageFault;

const RAX: u8 = 0;
const RCX: u8 = 1;
const RDX: u8 = 2;
const RSP: u8 = 4;
const RSI: u8 = 6;
const RDI: u8 = 7;

// ---------------------------------------------------------------------------------------------
// register access

pub unsafe fn r64(n: u8) -> u64 {
    let lo = if n < 8 { *reg32.offset(n as isize) } else { *reg_ext.offset(n as isize - 8) };
    (*reg_hi.offset(n as isize) as u32 as u64) << 32 | lo as u32 as u64
}

pub unsafe fn set_r64(n: u8, v: u64) {
    if n < 8 {
        *reg32.offset(n as isize) = v as i32;
    }
    else {
        *reg_ext.offset(n as isize - 8) = v as i32;
    }
    *reg_hi.offset(n as isize) = (v >> 32) as i32;
}

unsafe fn get_reg(n: u8, bits: u32, rex: bool) -> u64 {
    match bits {
        8 => {
            if !rex && (4..8).contains(&n) {
                r64(n - 4) >> 8 & 0xFF
            }
            else {
                r64(n) & 0xFF
            }
        },
        16 => r64(n) & 0xFFFF,
        32 => r64(n) & 0xFFFF_FFFF,
        _ => r64(n),
    }
}

unsafe fn set_reg(n: u8, bits: u32, rex: bool, v: u64) {
    match bits {
        8 => {
            if !rex && (4..8).contains(&n) {
                let old = r64(n - 4);
                set_r64(n - 4, old & !0xFF00 | (v & 0xFF) << 8);
            }
            else {
                set_r64(n, r64(n) & !0xFF | v & 0xFF);
            }
        },
        16 => set_r64(n, r64(n) & !0xFFFF | v & 0xFFFF),
        // writes of 32-bit registers clear the upper half
        32 => set_r64(n, v & 0xFFFF_FFFF),
        _ => set_r64(n, v),
    }
}

fn mask(bits: u32) -> u64 { if bits == 64 { u64::MAX } else { (1u64 << bits) - 1 } }

fn sign_extend(v: u64, bits: u32) -> u64 {
    if bits == 64 { v } else { ((v << (64 - bits)) as i64 >> (64 - bits)) as u64 }
}

// ---------------------------------------------------------------------------------------------
// decoder state

#[derive(Clone, Copy)]
struct Prefixes {
    rex: u8,
    has_rex: bool,
    osz16: bool,
    asz32: bool,
    seg: i32, // -1 none, otherwise FS or GS
    rep: u8,  // 0, 0xF2 or 0xF3
    lock: bool,
}

impl Prefixes {
    fn w(&self) -> bool { self.rex & 8 != 0 }
    fn r(&self) -> u8 { (self.rex >> 2 & 1) << 3 }
    fn x(&self) -> u8 { (self.rex >> 1 & 1) << 3 }
    fn b(&self) -> u8 { (self.rex & 1) << 3 }
    fn opsize(&self) -> u32 {
        if self.w() {
            64
        }
        else if self.osz16 {
            16
        }
        else {
            32
        }
    }
}

#[derive(Clone, Copy)]
enum Loc {
    Reg(u8),
    Mem(u64),
}

#[derive(Clone, Copy)]
struct ModRM {
    reg: u8, // with REX.R
    md: u8,
    loc: Loc,
}

unsafe fn fetch_imm(bits: u32) -> OrPageFault<u64> {
    // immediates are at most 32 bits, sign extended to the operand size
    Ok(match bits {
        8 => fetch8()?,
        16 => fetch16()?,
        _ => sign_extend(fetch32()?, 32),
    })
}


unsafe fn set_rip(target: u64) -> OrPageFault<()> {
    if !is_canonical(target) {
        dbg_log!("#gp: non-canonical rip {:#x}", target);
        trigger_gp(0);
        return Err(());
    }
    *instruction_pointer = target as i32;
    *rip_hi = (target >> 32) as i32;
    Ok(())
}

unsafe fn decode_modrm(p: &Prefixes, imm_len: u64) -> OrPageFault<ModRM> {
    let m = fetch8()? as u8;
    decode_modrm_byte(p, m, imm_len)
}

unsafe fn decode_modrm_byte(p: &Prefixes, m: u8, imm_len: u64) -> OrPageFault<ModRM> {
    let md = m >> 6;
    let reg = (m >> 3 & 7) | p.r();
    let rm = m & 7;

    if md == 3 {
        return Ok(ModRM { reg, md, loc: Loc::Reg(rm | p.b()) });
    }

    let mut addr: u64;
    let mut rip_relative = false;
    if rm == 4 {
        let sib = fetch8()? as u8;
        let scale = sib >> 6;
        let index = (sib >> 3 & 7) | p.x();
        let base = (sib & 7) | p.b();
        let base_val = if sib & 7 == 5 && md == 0 { 0 } else { r64(base) };
        let index_val = if index == 4 { 0 } else { r64(index) << scale };
        addr = base_val.wrapping_add(index_val);
        if sib & 7 == 5 && md == 0 {
            addr = addr.wrapping_add(sign_extend(fetch32()?, 32));
        }
    }
    else if rm == 5 && md == 0 {
        addr = sign_extend(fetch32()?, 32);
        rip_relative = true;
    }
    else {
        addr = r64(rm | p.b());
    }

    match md {
        1 => addr = addr.wrapping_add(sign_extend(fetch8()?, 8)),
        2 => addr = addr.wrapping_add(sign_extend(fetch32()?, 32)),
        _ => {},
    }

    if rip_relative {
        addr = addr.wrapping_add(rip() + imm_len);
    }
    if p.asz32 {
        addr &= 0xFFFF_FFFF;
    }
    if p.seg >= 0 {
        addr = addr.wrapping_add(seg_base(p.seg));
    }
    Ok(ModRM { reg, md, loc: Loc::Mem(addr) })
}

// ---------------------------------------------------------------------------------------------
// memory

unsafe fn rm_read(p: &Prefixes, m: &ModRM, bits: u32) -> OrPageFault<u64> {
    match m.loc {
        Loc::Reg(n) => Ok(get_reg(n, bits, p.has_rex)),
        Loc::Mem(a) => mem_read(a, bits),
    }
}

unsafe fn rm_write(p: &Prefixes, m: &ModRM, bits: u32, v: u64) -> OrPageFault<()> {
    match m.loc {
        Loc::Reg(n) => {
            set_reg(n, bits, p.has_rex, v);
            Ok(())
        },
        Loc::Mem(a) => mem_write(a, bits, v),
    }
}

unsafe fn push_val(v: u64, bits: u32) -> OrPageFault<()> {
    let new_sp = r64(RSP).wrapping_sub(bits as u64 / 8);
    mem_write(new_sp, bits, v)?;
    set_r64(RSP, new_sp);
    Ok(())
}

unsafe fn pop_val(bits: u32) -> OrPageFault<u64> {
    let sp = r64(RSP);
    let v = mem_read(sp, bits)?;
    set_r64(RSP, sp.wrapping_add(bits as u64 / 8));
    Ok(v)
}

// ---------------------------------------------------------------------------------------------
// flags

unsafe fn set_flags(cf: bool, af: bool, of: bool, result: u64, bits: u32) {
    let r = result & mask(bits);
    let pf = (r as u8).count_ones() % 2 == 0;
    let zf = r == 0;
    let sf = r >> (bits - 1) & 1 != 0;
    *flags = *flags & !FLAGS_ALL
        | cf as i32 * FLAG_CARRY
        | pf as i32 * FLAG_PARITY
        | af as i32 * FLAG_ADJUST
        | zf as i32 * FLAG_ZERO
        | sf as i32 * FLAG_SIGN
        | of as i32 * FLAG_OVERFLOW;
    *flags_changed = 0;
}

/// op: 0 add, 1 or, 2 adc, 3 sbb, 4 and, 5 sub, 6 xor, 7 cmp
unsafe fn alu(op: u8, a: u64, b: u64, bits: u32) -> u64 {
    let m = mask(bits);
    let a = a & m;
    let b = b & m;
    let carry_in = (get_eflags() & FLAG_CARRY) as u128;
    let msb = bits - 1;
    let (r, cf, of) = match op {
        0 | 2 => {
            let cin = if op == 2 { carry_in } else { 0 };
            let wide = a as u128 + b as u128 + cin;
            let r = wide as u64 & m;
            (r, wide >> bits & 1 != 0, ((a ^ r) & (b ^ r)) >> msb & 1 != 0)
        },
        3 | 5 | 7 => {
            let cin = if op == 3 { carry_in } else { 0 };
            let r = (a as u128).wrapping_sub(b as u128).wrapping_sub(cin) as u64 & m;
            (r, (a as u128) < b as u128 + cin, ((a ^ b) & (a ^ r)) >> msb & 1 != 0)
        },
        1 => (a | b, false, false),
        4 => (a & b, false, false),
        _ => (a ^ b, false, false),
    };
    let af = (a ^ b ^ r) & 0x10 != 0;
    let (af, of) = match op {
        1 | 4 | 6 => (false, false),
        _ => (af, of),
    };
    set_flags(cf, af, of, r, bits);
    r
}

/// Replace the flags with an already materialised value
unsafe fn set_eflags_raw(f: i32) {
    *flags = f;
    *flags_changed = 0;
}

unsafe fn incdec(dec: bool, a: u64, bits: u32) -> u64 {
    let cf = get_eflags() & FLAG_CARRY != 0;
    let r = alu(if dec { 5 } else { 0 }, a, 1, bits);
    *flags = *flags & !FLAG_CARRY | cf as i32 * FLAG_CARRY;
    *flags_changed = 0;
    r
}

unsafe fn cond(cc: u8) -> bool {
    let f = get_eflags();
    let cf = f & FLAG_CARRY != 0;
    let pf = f & FLAG_PARITY != 0;
    let zf = f & FLAG_ZERO != 0;
    let sf = f & FLAG_SIGN != 0;
    let of = f & FLAG_OVERFLOW != 0;
    let r = match cc >> 1 {
        0 => of,
        1 => cf,
        2 => zf,
        3 => cf || zf,
        4 => sf,
        5 => pf,
        6 => sf != of,
        _ => zf || sf != of,
    };
    r != (cc & 1 != 0)
}

unsafe fn shift(op: u8, a: u64, count: u32, bits: u32) -> u64 {
    let m = mask(bits);
    let a = a & m;
    let count = count & if bits == 64 { 63 } else { 31 };
    if count == 0 {
        return a;
    }
    let msb = bits - 1;
    let old_cf = get_eflags() & FLAG_CARRY != 0;
    let old_of = get_eflags() & FLAG_OVERFLOW != 0;
    match op {
        // rol
        0 => {
            let c = count % bits;
            let r = if c == 0 { a } else { (a << c | a >> (bits - c)) & m };
            let cf = r & 1 != 0;
            let of = (r >> msb & 1 != 0) != cf;
            *flags = *flags & !(FLAG_CARRY | FLAG_OVERFLOW)
                | cf as i32 * FLAG_CARRY
                | (if count == 1 { of } else { old_of }) as i32 * FLAG_OVERFLOW;
            *flags_changed = 0;
            r
        },
        // ror
        1 => {
            let c = count % bits;
            let r = if c == 0 { a } else { (a >> c | a << (bits - c)) & m };
            let cf = r >> msb & 1 != 0;
            let of = (r >> msb & 1) != (r >> (bits - 2) & 1);
            *flags = *flags & !(FLAG_CARRY | FLAG_OVERFLOW)
                | cf as i32 * FLAG_CARRY
                | (if count == 1 { of } else { old_of }) as i32 * FLAG_OVERFLOW;
            *flags_changed = 0;
            r
        },
        // rcl / rcr: rotate through carry, one bit at a time
        2 | 3 => {
            let mut v = a;
            let mut cf = old_cf;
            let mut of = old_of;
            for _ in 0..count % (bits + 1) {
                if op == 2 {
                    let out = v >> msb & 1 != 0;
                    v = (v << 1 | cf as u64) & m;
                    of = (v >> msb & 1 != 0) != out;
                    cf = out;
                }
                else {
                    let out = v & 1 != 0;
                    of = (v >> msb & 1 != 0) != cf;
                    v = v >> 1 | (cf as u64) << msb;
                    cf = out;
                }
            }
            *flags = *flags & !(FLAG_CARRY | FLAG_OVERFLOW)
                | cf as i32 * FLAG_CARRY
                | of as i32 * FLAG_OVERFLOW;
            *flags_changed = 0;
            v
        },
        // shl / sal, shr, sar
        _ => {
            let (r, cf, of) = match op {
                4 | 6 => {
                    let r = if count >= 64 { 0 } else { a << count & m };
                    let cf = if count > bits { false } else { a >> (bits - count) & 1 != 0 };
                    (r, cf, (r >> msb & 1 != 0) != cf)
                },
                5 => {
                    let r = if count >= 64 { 0 } else { a >> count };
                    let cf = if count > bits { false } else { a >> (count - 1) & 1 != 0 };
                    (r, cf, a >> msb & 1 != 0)
                },
                _ => {
                    let s = sign_extend(a, bits) as i64;
                    let c = count.min(63);
                    let r = (s >> c) as u64 & m;
                    let cf = (s >> (c - 1)) & 1 != 0;
                    (r, cf, false)
                },
            };
            set_flags(cf, false, count == 1 && of, r, bits);
            r
        },
    }
}

// ---------------------------------------------------------------------------------------------
// instruction execution

pub unsafe fn step64() {
    *previous_ip = *instruction_pointer;
    *instruction_counter += 1;
    // An Err means an exception has already been raised (and rip reset)
    let _ = exec();
}

unsafe fn unimplemented_opcode(opcode: u32) -> OrPageFault<()> {
    dbg_log!("long mode: unimplemented opcode {:#x} at {:#x}", opcode, rip());
    dbg_assert!(false, "long mode: unimplemented opcode {:#x}", opcode);
    trigger_ud();
    Err(())
}

unsafe fn ud() -> OrPageFault<()> {
    trigger_ud();
    Err(())
}

unsafe fn exec() -> OrPageFault<()> {
    let mut p = Prefixes { rex: 0, has_rex: false, osz16: false, asz32: false, seg: -1, rep: 0, lock: false };

    let opcode = loop {
        let b = fetch8()? as u8;
        match b {
            0x66 => p.osz16 = true,
            0x67 => p.asz32 = true,
            0xF2 | 0xF3 => p.rep = b,
            0xF0 => p.lock = true,
            0x64 => p.seg = FS,
            0x65 => p.seg = GS,
            0x26 | 0x2E | 0x36 | 0x3E => {}, // ignored in 64-bit mode
            0x40..=0x4F => {
                // REX must be the last prefix
                p.rex = b;
                p.has_rex = true;
                let op = fetch8()? as u8;
                break op;
            },
            _ => break b,
        }
    };

    let _ = p.lock;
    let osz = p.opsize();

    match opcode {
        // ALU: 00-3F, op in bits 5:3, form in bits 2:0
        0x00..=0x3F if opcode & 7 < 6 => {
            let op = opcode >> 3;
            match opcode & 7 {
                0 | 1 => {
                    let bits = if opcode & 1 == 0 { 8 } else { osz };
                    let m = decode_modrm(&p, 0)?;
                    let a = rm_read(&p, &m, bits)?;
                    let b = get_reg(m.reg, bits, p.has_rex);
                    let r = alu(op, a, b, bits);
                    if op != 7 {
                        rm_write(&p, &m, bits, r)?;
                    }
                },
                2 | 3 => {
                    let bits = if opcode & 1 == 0 { 8 } else { osz };
                    let m = decode_modrm(&p, 0)?;
                    let b = rm_read(&p, &m, bits)?;
                    let a = get_reg(m.reg, bits, p.has_rex);
                    let r = alu(op, a, b, bits);
                    if op != 7 {
                        set_reg(m.reg, bits, p.has_rex, r);
                    }
                },
                4 => {
                    let b = fetch8()?;
                    let r = alu(op, get_reg(RAX, 8, true), b, 8);
                    if op != 7 {
                        set_reg(RAX, 8, true, r);
                    }
                },
                _ => {
                    let b = fetch_imm(if osz == 16 { 16 } else { 32 })?;
                    let r = alu(op, get_reg(RAX, osz, true), b, osz);
                    if op != 7 {
                        set_reg(RAX, osz, true, r);
                    }
                },
            }
        },
        // one-byte opcodes that are invalid in 64-bit mode
        0x06 | 0x07 | 0x0E | 0x16 | 0x17 | 0x1E | 0x1F | 0x27 | 0x2F | 0x37 | 0x3F | 0x60
        | 0x61 | 0x62 | 0x82 | 0x9A | 0xC4 | 0xC5 | 0xCE | 0xD4 | 0xD5 | 0xD6 | 0xEA => return ud(),
        0x0F => return exec_0f(&p),
        // push / pop
        0x50..=0x57 => {
            let n = (opcode & 7) | p.b();
            let v = r64(n);
            push_val(v, if p.osz16 { 16 } else { 64 })?;
        },
        0x58..=0x5F => {
            let n = (opcode & 7) | p.b();
            let bits = if p.osz16 { 16 } else { 64 };
            let v = pop_val(bits)?;
            set_reg(n, bits, true, v);
        },
        0x63 => {
            // movsxd
            let m = decode_modrm(&p, 0)?;
            let v = rm_read(&p, &m, 32)?;
            if p.w() {
                set_r64(m.reg, sign_extend(v, 32));
            }
            else {
                set_reg(m.reg, osz, true, v);
            }
        },
        0x68 => {
            let v = fetch_imm(if p.osz16 { 16 } else { 32 })?;
            push_val(v, if p.osz16 { 16 } else { 64 })?;
        },
        0x6A => {
            let v = sign_extend(fetch8()?, 8);
            push_val(v, if p.osz16 { 16 } else { 64 })?;
        },
        0x69 | 0x6B => {
            let imm_len = if opcode == 0x6B { 1 } else if osz == 16 { 2 } else { 4 };
            let m = decode_modrm(&p, imm_len)?;
            let a = rm_read(&p, &m, osz)?;
            let b = if opcode == 0x6B { sign_extend(fetch8()?, 8) } else { fetch_imm(if osz == 16 { 16 } else { 32 })? };
            let r = imul(a, b, osz);
            set_reg(m.reg, osz, true, r);
        },
        0x70..=0x7F => {
            let d = sign_extend(fetch8()?, 8);
            if cond(opcode & 0xF) {
                set_rip(rip().wrapping_add(d))?;
            }
        },
        // group 1
        0x80 | 0x81 | 0x83 => {
            let bits = if opcode == 0x80 { 8 } else { osz };
            let imm_len = if opcode == 0x81 { if osz == 16 { 2 } else { 4 } } else { 1 };
            let m = decode_modrm(&p, imm_len)?;
            let imm = match opcode {
                0x80 => fetch8()?,
                0x81 => fetch_imm(if osz == 16 { 16 } else { 32 })?,
                _ => sign_extend(fetch8()?, 8),
            };
            let a = rm_read(&p, &m, bits)?;
            let op = m.reg & 7;
            let r = alu(op, a, imm, bits);
            if op != 7 {
                rm_write(&p, &m, bits, r)?;
            }
        },
        0x84 | 0x85 => {
            let bits = if opcode == 0x84 { 8 } else { osz };
            let m = decode_modrm(&p, 0)?;
            let a = rm_read(&p, &m, bits)?;
            alu(4, a, get_reg(m.reg, bits, p.has_rex), bits);
        },
        0x86 | 0x87 => {
            let bits = if opcode == 0x86 { 8 } else { osz };
            let m = decode_modrm(&p, 0)?;
            let a = rm_read(&p, &m, bits)?;
            let b = get_reg(m.reg, bits, p.has_rex);
            rm_write(&p, &m, bits, b)?;
            set_reg(m.reg, bits, p.has_rex, a);
        },
        0x88 | 0x89 => {
            let bits = if opcode == 0x88 { 8 } else { osz };
            let m = decode_modrm(&p, 0)?;
            let v = get_reg(m.reg, bits, p.has_rex);
            rm_write(&p, &m, bits, v)?;
        },
        0x8A | 0x8B => {
            let bits = if opcode == 0x8A { 8 } else { osz };
            let m = decode_modrm(&p, 0)?;
            let v = rm_read(&p, &m, bits)?;
            set_reg(m.reg, bits, p.has_rex, v);
        },
        0x8C => {
            let m = decode_modrm(&p, 0)?;
            if m.reg & 7 > 5 {
                return ud();
            }
            let v = *sreg.offset((m.reg & 7) as isize) as u64;
            match m.loc {
                Loc::Reg(_) => rm_write(&p, &m, osz, v)?,
                Loc::Mem(_) => rm_write(&p, &m, 16, v)?,
            }
        },
        0x8D => {
            let m = decode_modrm(&p, 0)?;
            match m.loc {
                Loc::Mem(a) => {
                    // lea ignores the segment override
                    let a = if p.seg >= 0 { a.wrapping_sub(seg_base(p.seg)) } else { a };
                    set_reg(m.reg, osz, true, a)
                },
                Loc::Reg(_) => return ud(),
            }
        },
        0x8E => {
            let m = decode_modrm(&p, 0)?;
            let reg = (m.reg & 7) as i32;
            if reg == CS || reg > 5 {
                return ud();
            }
            let sel = rm_read(&p, &m, 16)? as i32;
            if !switch_seg(reg, sel) {
                return Err(());
            }
            if (reg == FS || reg == GS) && sel & 0xFFFC != 0 {
                // the 64-bit base is replaced by the (32-bit) base of the descriptor
                let base = *segment_offsets.offset(reg as isize) as u32 as u64;
                if reg == FS {
                    *fs_base = base;
                }
                else {
                    *gs_base = base;
                }
            }
        },
        0x8F => {
            let m = decode_modrm(&p, 0)?;
            if m.reg & 7 != 0 {
                return ud();
            }
            let bits = if p.osz16 { 16 } else { 64 };
            let v = pop_val(bits)?;
            rm_write(&p, &m, bits, v)?;
        },
        0x90 => {
            if p.b() != 0 {
                let n = p.b();
                let a = get_reg(RAX, osz, true);
                let b = get_reg(n, osz, true);
                set_reg(RAX, osz, true, b);
                set_reg(n, osz, true, a);
            }
            else if p.rep == 0xF3 {
                // pause
            }
        },
        0x91..=0x97 => {
            let n = (opcode & 7) | p.b();
            let a = get_reg(RAX, osz, true);
            let b = get_reg(n, osz, true);
            set_reg(RAX, osz, true, b);
            set_reg(n, osz, true, a);
        },
        0x98 => {
            // cbw / cwde / cdqe
            match osz {
                16 => set_reg(RAX, 16, true, sign_extend(r64(RAX) & 0xFF, 8)),
                32 => set_reg(RAX, 32, true, sign_extend(r64(RAX) & 0xFFFF, 16)),
                _ => set_r64(RAX, sign_extend(r64(RAX) & 0xFFFF_FFFF, 32)),
            }
        },
        0x99 => {
            // cwd / cdq / cqo
            let sign = get_reg(RAX, osz, true) >> (osz - 1) & 1 != 0;
            set_reg(RDX, osz, true, if sign { u64::MAX } else { 0 });
        },
        0x9C => {
            let f = (get_eflags() & !(FLAG_RF | FLAG_VM)) as u32 as u64;
            push_val(f, if p.osz16 { 16 } else { 64 })?;
        },
        0x9D => {
            let bits = if p.osz16 { 16 } else { 64 };
            let v = pop_val(bits)?;
            if bits == 16 {
                update_eflags(*flags & !0xFFFF | v as i32 & 0xFFFF);
            }
            else {
                update_eflags(v as i32);
            }
        },
        0xA8 => {
            let b = fetch8()?;
            alu(4, get_reg(RAX, 8, true), b, 8);
        },
        0xA9 => {
            let b = fetch_imm(if osz == 16 { 16 } else { 32 })?;
            alu(4, get_reg(RAX, osz, true), b, osz);
        },
        0xA4 | 0xA5 | 0xA6 | 0xA7 | 0xAA | 0xAB | 0xAC | 0xAD | 0xAE | 0xAF | 0x6C..=0x6F => {
            string_op(&p, opcode)?;
        },
        0xA0..=0xA3 => {
            // mov al/ax/eax/rax <-> moffs
            let bits = if opcode & 1 == 0 { 8 } else { osz };
            let mut addr = if p.asz32 { fetch32()? } else { fetch64()? };
            if p.seg >= 0 {
                addr = addr.wrapping_add(seg_base(p.seg));
            }
            if opcode < 0xA2 {
                let v = mem_read(addr, bits)?;
                set_reg(RAX, bits, true, v);
            }
            else {
                mem_write(addr, bits, get_reg(RAX, bits, true))?;
            }
        },
        0xE4 | 0xE5 | 0xE6 | 0xE7 | 0xEC | 0xED | 0xEE | 0xEF => {
            let port = if opcode < 0xE8 { fetch8()? as i32 } else { (r64(RDX) & 0xFFFF) as i32 };
            let bits = if opcode & 1 == 0 { 8 } else if p.osz16 { 16 } else { 32 };
            if !test_privileges_for_io(port, bits as i32 / 8) {
                return Err(());
            }
            if matches!(opcode, 0xE4 | 0xE5 | 0xEC | 0xED) {
                let v = match bits {
                    8 => io_port_read8(port),
                    16 => io_port_read16(port),
                    _ => io_port_read32(port),
                };
                set_reg(RAX, bits, true, v as u32 as u64 & mask(bits));
            }
            else {
                let v = get_reg(RAX, bits, true) as i32;
                match bits {
                    8 => io_port_write8(port, v),
                    16 => io_port_write16(port, v),
                    _ => io_port_write32(port, v),
                }
            }
        },
        0xB0..=0xB7 => {
            let v = fetch8()?;
            set_reg((opcode & 7) | p.b(), 8, p.has_rex, v);
        },
        0xB8..=0xBF => {
            let n = (opcode & 7) | p.b();
            let v = match osz {
                64 => fetch64()?,
                16 => fetch16()?,
                _ => fetch32()?,
            };
            set_reg(n, osz, true, v);
        },
        // shifts and rotates
        0xC0 | 0xC1 | 0xD0 | 0xD1 | 0xD2 | 0xD3 => {
            let bits = if opcode & 1 == 0 { 8 } else { osz };
            let imm_len = if opcode < 0xD0 { 1 } else { 0 };
            let m = decode_modrm(&p, imm_len)?;
            let count = match opcode {
                0xC0 | 0xC1 => fetch8()? as u32,
                0xD0 | 0xD1 => 1,
                _ => (r64(RCX) & 0xFF) as u32,
            };
            let a = rm_read(&p, &m, bits)?;
            let r = shift(m.reg & 7, a, count, bits);
            if count & if bits == 64 { 63 } else { 31 } != 0 {
                rm_write(&p, &m, bits, r)?;
            }
        },
        0xC2 | 0xC3 => {
            let adj = if opcode == 0xC2 { fetch16()? } else { 0 };
            let target = pop_val(64)?;
            set_r64(RSP, r64(RSP).wrapping_add(adj));
            set_rip(target)?;
        },
        0xC6 | 0xC7 => {
            let bits = if opcode == 0xC6 { 8 } else { osz };
            let imm_len = if bits == 8 { 1 } else if bits == 16 { 2 } else { 4 };
            let m = decode_modrm(&p, imm_len)?;
            if m.reg & 7 != 0 {
                return ud();
            }
            let v = fetch_imm(bits)?;
            rm_write(&p, &m, bits, v)?;
        },
        0xC9 => {
            // leave
            let bp = r64(5);
            let bits = if p.osz16 { 16 } else { 64 };
            set_r64(RSP, bp);
            let v = pop_val(bits)?;
            set_reg(5, bits, true, v);
        },
        0xCA | 0xCB => {
            // far return
            let adj = if opcode == 0xCA { fetch16()? } else { 0 };
            let bits = if p.w() { 64 } else if p.osz16 { 16 } else { 32 };
            let sp = r64(RSP);
            let new_rip = mem_read(sp, bits)?;
            let new_cs = mem_read(sp + bits as u64 / 8, bits)? as i32 & 0xFFFF;
            if new_cs & 3 != *cpl as i32 {
                panic!("Unimplemented: far return to a different privilege level");
            }
            load_cs_64(new_cs, new_rip)?;
            set_r64(RSP, sp.wrapping_add(2 * (bits as u64 / 8)).wrapping_add(adj));
        },
        0xCC => {
            call_interrupt_vector_64(3, true, None);
        },
        0xCD => {
            let n = fetch8()? as i32;
            call_interrupt_vector_64(n, true, None);
        },
        0xCF => iretq(&p)?,
        0xE0..=0xE3 => {
            let d = sign_extend(fetch8()?, 8);
            let bits = if p.asz32 { 32 } else { 64 };
            let take = if opcode == 0xE3 {
                get_reg(RCX, bits, true) == 0
            }
            else {
                let c = get_reg(RCX, bits, true).wrapping_sub(1) & mask(bits);
                set_reg(RCX, bits, true, c);
                let zf = get_eflags() & FLAG_ZERO != 0;
                c != 0 && match opcode {
                    0xE0 => !zf,
                    0xE1 => zf,
                    _ => true,
                }
            };
            if take {
                set_rip(rip().wrapping_add(d))?;
            }
        },
        0xE8 => {
            let d = sign_extend(fetch32()?, 32);
            let ret = rip();
            push_val(ret, 64)?;
            set_rip(ret.wrapping_add(d))?;
        },
        0xE9 => {
            let d = sign_extend(fetch32()?, 32);
            set_rip(rip().wrapping_add(d))?;
        },
        0xEB => {
            let d = sign_extend(fetch8()?, 8);
            set_rip(rip().wrapping_add(d))?;
        },
        0x9B => instructions::instr_9B(),
        0xD8..=0xDF => return bridge_x87(&p, opcode),
        0xF4 => instructions::instr_F4(),
        0xF5 => set_eflags_raw(get_eflags() ^ FLAG_CARRY),
        0xF6 | 0xF7 => group3(&p, opcode)?,
        0xF8 => set_eflags_raw(get_eflags() & !FLAG_CARRY),
        0xF9 => set_eflags_raw(get_eflags() | FLAG_CARRY),
        0xFA => instructions::instr_FA(),
        0xFB => {
            // sti: only the interrupt flag, the interrupt shadow is not emulated
            if !instructions::instr_FB_without_fault() {
                trigger_gp(0);
                return Err(());
            }
        },
        0xFC => set_eflags_raw(get_eflags() & !FLAG_DIRECTION),
        0xFD => set_eflags_raw(get_eflags() | FLAG_DIRECTION),
        0xFE => {
            let m = decode_modrm(&p, 0)?;
            let a = rm_read(&p, &m, 8)?;
            match m.reg & 7 {
                0 => rm_write(&p, &m, 8, incdec(false, a, 8))?,
                1 => rm_write(&p, &m, 8, incdec(true, a, 8))?,
                _ => return ud(),
            }
        },
        0xFF => {
            let m = decode_modrm(&p, 0)?;
            match m.reg & 7 {
                0 | 1 => {
                    let a = rm_read(&p, &m, osz)?;
                    rm_write(&p, &m, osz, incdec(m.reg & 7 == 1, a, osz))?;
                },
                2 => {
                    // call near indirect: operand size is always 64 bits
                    let target = rm_read(&p, &m, 64)?;
                    push_val(rip(), 64)?;
                    set_rip(target)?;
                },
                4 => {
                    let target = rm_read(&p, &m, 64)?;
                    set_rip(target)?;
                },
                6 => {
                    let bits = if p.osz16 { 16 } else { 64 };
                    let v = rm_read(&p, &m, bits)?;
                    push_val(v, bits)?;
                },
                _ => return unimplemented_opcode(0xFF00 | (m.reg & 7) as u32),
            }
        },
        _ => return unimplemented_opcode(opcode as u32),
    }
    Ok(())
}

unsafe fn imul(a: u64, b: u64, bits: u32) -> u64 {
    let sa = sign_extend(a, bits) as i64 as i128;
    let sb = sign_extend(b, bits) as i64 as i128;
    let wide = sa * sb;
    let r = wide as u64 & mask(bits);
    let overflow = sign_extend(r, bits) as i64 as i128 != wide;
    set_flags(overflow, false, overflow, r, bits);
    r
}

unsafe fn group3(p: &Prefixes, opcode: u8) -> OrPageFault<()> {
    let bits = if opcode == 0xF6 { 8 } else { p.opsize() };
    let imm_len = if bits == 8 { 1 } else if bits == 16 { 2 } else { 4 };
    // test has an immediate, which matters for rip-relative addressing
    let peek_reg = {
        let (ip, hi) = (*instruction_pointer, *rip_hi);
        let b = fetch8()? as u8 >> 3 & 7;
        *instruction_pointer = ip;
        *rip_hi = hi;
        b
    };
    let m = decode_modrm(p, if peek_reg < 2 { imm_len } else { 0 })?;
    let a = rm_read(p, &m, bits)?;
    match m.reg & 7 {
        0 | 1 => {
            let imm = fetch_imm(bits)?;
            alu(4, a, imm, bits);
        },
        2 => rm_write(p, &m, bits, !a)?,
        3 => {
            let r = alu(5, 0, a, bits);
            rm_write(p, &m, bits, r)?;
        },
        4 | 5 => {
            // mul / imul with implicit accumulator
            let signed = m.reg & 7 == 5;
            let acc = get_reg(RAX, bits, true);
            let (lo, hi, overflow) = if signed {
                let wide = sign_extend(acc, bits) as i64 as i128 * sign_extend(a, bits) as i64 as i128;
                let lo = wide as u64 & mask(bits);
                let hi = (wide >> bits) as u64 & mask(bits);
                (lo, hi, sign_extend(lo, bits) as i64 as i128 != wide)
            }
            else {
                let wide = acc as u128 * a as u128;
                let lo = wide as u64 & mask(bits);
                let hi = (wide >> bits) as u64 & mask(bits);
                (lo, hi, hi != 0)
            };
            if bits == 8 {
                set_reg(RAX, 16, true, hi << 8 | lo);
            }
            else {
                set_reg(RAX, bits, true, lo);
                set_reg(RDX, bits, true, hi);
            }
            set_flags(overflow, false, overflow, lo, bits);
        },
        _ => {
            // div / idiv
            let signed = m.reg & 7 == 7;
            if a == 0 {
                trigger_de();
                return Err(());
            }
            let dividend: u128 = if bits == 8 {
                get_reg(RAX, 16, true) as u128
            }
            else {
                (get_reg(RDX, bits, true) as u128) << bits | get_reg(RAX, bits, true) as u128
            };
            let (q, r) = if signed {
                let width = bits * 2;
                let dv = ((dividend << (128 - width)) as i128) >> (128 - width);
                let ds = sign_extend(a, bits) as i64 as i128;
                let q = dv / ds;
                let r = dv % ds;
                let lo = -(1i128 << (bits - 1));
                let hi = (1i128 << (bits - 1)) - 1;
                if q < lo || q > hi {
                    trigger_de();
                    return Err(());
                }
                (q as u64 & mask(bits), r as u64 & mask(bits))
            }
            else {
                let q = dividend / a as u128;
                let r = dividend % a as u128;
                if q > mask(bits) as u128 {
                    trigger_de();
                    return Err(());
                }
                (q as u64, r as u64)
            };
            if bits == 8 {
                set_reg(RAX, 16, true, r << 8 | q);
            }
            else {
                set_reg(RAX, bits, true, q);
                set_reg(RDX, bits, true, r);
            }
        },
    }
    Ok(())
}

unsafe fn string_op(p: &Prefixes, opcode: u8) -> OrPageFault<()> {
    let is_io = (0x6C..=0x6F).contains(&opcode);
    let bits = if opcode & 1 == 0 {
        8
    }
    else if is_io && p.opsize() == 64 {
        32
    }
    else {
        p.opsize()
    };
    let port = (r64(RDX) & 0xFFFF) as i32;
    if is_io && !test_privileges_for_io(port, bits as i32 / 8) {
        return Err(());
    }
    let step = bits as u64 / 8;
    let abits = if p.asz32 { 32 } else { 64 };
    let df = *flags & FLAG_DIRECTION != 0;
    let delta = if df { step.wrapping_neg() } else { step };
    let seg_base = if p.seg >= 0 { seg_base(p.seg) } else { 0 };
    let compares = matches!(opcode, 0xA6 | 0xA7 | 0xAE | 0xAF);

    loop {
        if p.rep != 0 && get_reg(RCX, abits, true) == 0 {
            break;
        }
        let si = get_reg(RSI, abits, true);
        let di = get_reg(RDI, abits, true);
        match opcode {
            0xA4 | 0xA5 => {
                let v = mem_read(si.wrapping_add(seg_base), bits)?;
                mem_write(di, bits, v)?;
                set_reg(RSI, abits, true, si.wrapping_add(delta));
                set_reg(RDI, abits, true, di.wrapping_add(delta));
            },
            0xA6 | 0xA7 => {
                let a = mem_read(si.wrapping_add(seg_base), bits)?;
                let b = mem_read(di, bits)?;
                alu(7, a, b, bits);
                set_reg(RSI, abits, true, si.wrapping_add(delta));
                set_reg(RDI, abits, true, di.wrapping_add(delta));
            },
            0x6C | 0x6D => {
                let v = match bits {
                    8 => io_port_read8(port),
                    16 => io_port_read16(port),
                    _ => io_port_read32(port),
                };
                mem_write(di, bits, v as u32 as u64 & mask(bits))?;
                set_reg(RDI, abits, true, di.wrapping_add(delta));
            },
            0x6E | 0x6F => {
                let v = mem_read(si.wrapping_add(seg_base), bits)? as i32;
                match bits {
                    8 => io_port_write8(port, v),
                    16 => io_port_write16(port, v),
                    _ => io_port_write32(port, v),
                }
                set_reg(RSI, abits, true, si.wrapping_add(delta));
            },
            0xAA | 0xAB => {
                mem_write(di, bits, get_reg(RAX, bits, true))?;
                set_reg(RDI, abits, true, di.wrapping_add(delta));
            },
            0xAC | 0xAD => {
                let v = mem_read(si.wrapping_add(seg_base), bits)?;
                set_reg(RAX, bits, true, v);
                set_reg(RSI, abits, true, si.wrapping_add(delta));
            },
            _ => {
                let b = mem_read(di, bits)?;
                alu(7, get_reg(RAX, bits, true), b, bits);
                set_reg(RDI, abits, true, di.wrapping_add(delta));
            },
        }
        if p.rep == 0 {
            break;
        }
        let c = get_reg(RCX, abits, true).wrapping_sub(1);
        set_reg(RCX, abits, true, c);
        if compares {
            let zf = get_eflags() & FLAG_ZERO != 0;
            if (p.rep == 0xF3 && !zf) || (p.rep == 0xF2 && zf) {
                break;
            }
        }
    }
    Ok(())
}

unsafe fn exec_0f(p: &Prefixes) -> OrPageFault<()> {
    let opcode = fetch8()? as u8;
    let osz = p.opsize();

    match opcode {
        0x01 => {
            let m = decode_modrm(p, 0)?;
            match (m.reg & 7, m.loc) {
                (0, Loc::Mem(a)) | (1, Loc::Mem(a)) => {
                    // sgdt / sidt: 2 byte limit, 8 byte base
                    let (size, off) = if m.reg & 7 == 0 {
                        (*gdtr_size, *gdtr_offset)
                    }
                    else {
                        (*idtr_size, *idtr_offset)
                    };
                    mem_write(a, 16, size as u64 & 0xFFFF)?;
                    mem_write(a + 2, 64, off as u32 as u64)?;
                },
                (2, Loc::Mem(a)) | (3, Loc::Mem(a)) => {
                    if *cpl != 0 {
                        trigger_gp(0);
                        return Err(());
                    }
                    let size = mem_read(a, 16)? as i32;
                    let base = mem_read(a + 2, 64)?;
                    if base >> 32 != 0 {
                        dbg_log!("long mode: unsupported descriptor table base {:#x}", base);
                        trigger_gp(0);
                        return Err(());
                    }
                    if m.reg & 7 == 2 {
                        *gdtr_size = size;
                        *gdtr_offset = base as i32;
                    }
                    else {
                        *idtr_size = size;
                        *idtr_offset = base as i32;
                    }
                },
                (4, _) => {
                    let v = *cr & 0xFFFF;
                    rm_write(p, &m, if matches!(m.loc, Loc::Reg(_)) { osz } else { 16 }, v as u64)?;
                },
                (7, Loc::Mem(a)) => {
                    if *cpl != 0 {
                        trigger_gp(0);
                        return Err(());
                    }
                    invlpg64(a);
                },
                (7, Loc::Reg(0)) => {
                    // swapgs
                    if *cpl != 0 || !*is_64 {
                        return if *is_64 {
                            trigger_gp(0);
                            Err(())
                        }
                        else {
                            ud()
                        };
                    }
                    let old = *gs_base;
                    *gs_base = *kernel_gs_base;
                    *kernel_gs_base = old;
                    *segment_offsets.offset(GS as isize) = *gs_base as i32;
                },
                _ => return unimplemented_opcode(0x0F01),
            }
        },
        0x0B | 0x24 | 0x25 | 0x26 | 0x27 | 0xA7 | 0xFF => return ud(),
        0x06 => {
            if *cpl != 0 {
                trigger_gp(0);
                return Err(());
            }
            *cr &= !CR0_TS;
        },
        0x08 | 0x09 => {
            // invd / wbinvd
            if *cpl != 0 {
                trigger_gp(0);
                return Err(());
            }
        },
        0x18 | 0x19 | 0x1A | 0x1B | 0x1C | 0x1D | 0x1E | 0x1F | 0x0D => {
            // prefetch / hint nops: decode the operand and do nothing
            decode_modrm(p, 0)?;
        },
        0x20 | 0x22 => {
            if *cpl != 0 {
                trigger_gp(0);
                return Err(());
            }
            let m = decode_modrm(p, 0)?;
            let n = match m.loc {
                Loc::Reg(n) => n,
                _ => return ud(),
            };
            let creg = m.reg;
            if opcode == 0x20 {
                let v = match creg {
                    2 => (*cr2_hi as u32 as u64) << 32 | *cr.offset(2) as u32 as u64,
                    0 | 3 | 4 => *cr.offset(creg as isize) as u32 as u64,
                    8 => *cr8_tpr as u64,
                    _ => return ud(),
                };
                set_r64(n, v);
            }
            else {
                let v = r64(n);
                match creg {
                    2 => {
                        *cr.offset(2) = v as i32;
                        *cr2_hi = (v >> 32) as i32;
                    },
                    0 | 3 | 4 => {
                        if v >> 32 != 0 && creg != 3 {
                            trigger_gp(0);
                            return Err(());
                        }
                        instructions_0f::write_cr(creg as i32, v as i32);
                    },
                    8 => {
                        if v > 15 {
                            trigger_gp(0);
                            return Err(());
                        }
                        *cr8_tpr = v as i32;
                    },
                    _ => return ud(),
                }
            }
        },
        0x00 => {
            let m = decode_modrm(p, 0)?;
            if !*protected_mode {
                return ud();
            }
            let reg_size = if matches!(m.loc, Loc::Reg(_)) { osz } else { 16 };
            match m.reg & 7 {
                0 => rm_write(p, &m, reg_size, *sreg.offset(LDTR as isize) as u64)?,
                1 => rm_write(p, &m, reg_size, *sreg.offset(TR as isize) as u64)?,
                2 | 3 => {
                    if *cpl != 0 {
                        trigger_gp(0);
                        return Err(());
                    }
                    let sel = rm_read(p, &m, 16)? as i32;
                    if m.reg & 7 == 2 {
                        load_ldt(sel)?;
                    }
                    else {
                        load_tr(sel);
                    }
                },
                4 => verr(rm_read(p, &m, 16)? as i32),
                5 => verw(rm_read(p, &m, 16)? as i32),
                _ => return ud(),
            }
        },
        0xAE => {
            let m = decode_modrm(p, 0)?;
            match (m.reg & 7, m.loc) {
                (0, Loc::Mem(a)) => with_window(a, |w| instructions_0f::instr_0FAE_0_mem(w))?,
                (1, Loc::Mem(a)) => with_window(a, |w| instructions_0f::instr_0FAE_1_mem(w))?,
                (2, Loc::Mem(a)) => with_window(a, |w| instructions_0f::instr_0FAE_2_mem(w))?,
                (3, Loc::Mem(a)) => with_window(a, |w| instructions_0f::instr_0FAE_3_mem(w))?,
                (5, Loc::Reg(_)) | (6, Loc::Reg(_)) | (7, Loc::Reg(_)) => {}, // lfence mfence sfence
                (7, Loc::Mem(_)) => {},                                       // clflush
                (r @ 0..=3, Loc::Reg(n)) if p.rep == 0xF3 => {
                    // rdfsbase rdgsbase wrfsbase wrgsbase
                    if *cr.offset(4) & (1 << 16) == 0 {
                        return ud();
                    }
                    let is_fs = r & 1 == 0;
                    if r < 2 {
                        let v = if is_fs { *fs_base } else { *gs_base };
                        set_reg(n, if p.w() { 64 } else { 32 }, true, v);
                    }
                    else {
                        let v = get_reg(n, if p.w() { 64 } else { 32 }, true);
                        if !is_canonical(v) {
                            trigger_gp(0);
                            return Err(());
                        }
                        if is_fs {
                            *fs_base = v;
                            *segment_offsets.offset(FS as isize) = v as i32;
                        }
                        else {
                            *gs_base = v;
                            *segment_offsets.offset(GS as isize) = v as i32;
                        }
                    }
                },
                _ => return unimplemented_opcode(0x0FAE),
            }
        },
        0x05 => syscall(p)?,
        0x07 => sysret(p)?,
        0xC7 => {
            let m = decode_modrm(p, 0)?;
            match (m.reg & 7, m.loc) {
                (1, Loc::Mem(a)) => cmpxchg_double(p, a)?,
                _ => return unimplemented_opcode(0x0FC7),
            }
        },
        0x30 => {
            instructions_0f::instr_0F30();
        },
        0x31 => {
            instructions_0f::instr_0F31();
            *reg_hi.offset(0) = 0;
            *reg_hi.offset(2) = 0;
        },
        0x32 => {
            instructions_0f::instr_0F32();
            *reg_hi.offset(0) = 0;
            *reg_hi.offset(2) = 0;
        },
        0x40..=0x4F => {
            let m = decode_modrm(p, 0)?;
            let v = rm_read(p, &m, osz)?;
            if cond(opcode & 0xF) {
                set_reg(m.reg, osz, true, v);
            }
            else if osz == 32 {
                // the destination is zero extended even if the move is not taken
                set_reg(m.reg, 32, true, get_reg(m.reg, 32, true));
            }
        },
        0x80..=0x8F => {
            let d = sign_extend(fetch32()?, 32);
            if cond(opcode & 0xF) {
                set_rip(rip().wrapping_add(d))?;
            }
        },
        0x90..=0x9F => {
            let m = decode_modrm(p, 0)?;
            rm_write(p, &m, 8, cond(opcode & 0xF) as u64)?;
        },
        0xA2 => {
            instructions_0f::instr_0FA2();
            for i in 0..4 {
                *reg_hi.offset(i) = 0;
            }
        },
        0xA3 | 0xAB | 0xB3 | 0xBB => bit_op(p, opcode, false)?,
        0xAF => {
            let m = decode_modrm(p, 0)?;
            let a = get_reg(m.reg, osz, true);
            let b = rm_read(p, &m, osz)?;
            set_reg(m.reg, osz, true, imul(a, b, osz));
        },
        0xBA => bit_op(p, opcode, true)?,
        0xB6 | 0xB7 | 0xBE | 0xBF => {
            let src_bits = if opcode & 1 == 0 { 8 } else { 16 };
            let m = decode_modrm(p, 0)?;
            let v = rm_read(p, &m, src_bits)?;
            let v = if opcode >= 0xBE { sign_extend(v, src_bits) } else { v };
            set_reg(m.reg, osz, true, v);
        },
        0xBC | 0xBD => {
            let m = decode_modrm(p, 0)?;
            let v = rm_read(p, &m, osz)?;
            if p.rep == 0xF3 {
                // tzcnt / lzcnt
                let r = if opcode == 0xBC {
                    if v == 0 { osz } else { v.trailing_zeros() }
                }
                else if v == 0 {
                    osz
                }
                else {
                    v.leading_zeros() - (64 - osz)
                };
                set_reg(m.reg, osz, true, r as u64);
                set_flags(v == 0, false, false, r as u64, osz);
            }
            else if v == 0 {
                set_eflags_raw(get_eflags() | FLAG_ZERO);
            }
            else {
                let r = if opcode == 0xBC { v.trailing_zeros() } else { 63 - v.leading_zeros() };
                set_reg(m.reg, osz, true, r as u64);
                set_eflags_raw(get_eflags() & !FLAG_ZERO);
            }
        },
        0xB8 if p.rep == 0xF3 => {
            let m = decode_modrm(p, 0)?;
            let v = rm_read(p, &m, osz)?;
            let r = v.count_ones() as u64;
            set_reg(m.reg, osz, true, r);
            set_flags(false, false, false, r, 32);
        },
        0xC8..=0xCF => {
            let n = (opcode & 7) | p.b();
            if osz == 64 {
                set_r64(n, r64(n).swap_bytes());
            }
            else {
                set_reg(n, 32, true, (r64(n) as u32).swap_bytes() as u64);
            }
        },
        0xB0 | 0xB1 => {
            let bits = if opcode == 0xB0 { 8 } else { osz };
            let m = decode_modrm(p, 0)?;
            let dst = rm_read(p, &m, bits)?;
            let acc = get_reg(RAX, bits, true);
            alu(7, acc, dst, bits);
            if acc == dst {
                rm_write(p, &m, bits, get_reg(m.reg, bits, p.has_rex))?;
            }
            else {
                set_reg(RAX, bits, true, dst);
            }
        },
        0xC0 | 0xC1 => {
            let bits = if opcode == 0xC0 { 8 } else { osz };
            let m = decode_modrm(p, 0)?;
            let dst = rm_read(p, &m, bits)?;
            let src = get_reg(m.reg, bits, p.has_rex);
            let r = alu(0, dst, src, bits);
            set_reg(m.reg, bits, p.has_rex, dst);
            rm_write(p, &m, bits, r)?;
        },
        0xA4 | 0xA5 | 0xAC | 0xAD => {
            // shld / shrd
            let m = decode_modrm(p, if opcode & 1 == 0 { 1 } else { 0 })?;
            let count = if opcode & 1 == 0 { fetch8()? } else { r64(RCX) & 0xFF } as u32
                & if osz == 64 { 63 } else { 31 };
            if count != 0 {
                let a = rm_read(p, &m, osz)? as u128;
                let b = get_reg(m.reg, osz, true) as u128;
                let (r, cf) = if opcode < 0xA8 {
                    let wide = a << osz | b;
                    let sh = wide << count;
                    (((sh >> osz) as u64) & mask(osz), (wide >> (2 * osz - count)) & 1 != 0)
                }
                else {
                    let wide = b << osz | a;
                    (((wide >> count) as u64) & mask(osz), (a >> (count - 1)) & 1 != 0)
                };
                let of = (r >> (osz - 1) & 1) != (a as u64 >> (osz - 1) & 1);
                set_flags(cf, false, if count == 1 { of } else { false }, r, osz);
                rm_write(p, &m, osz, r)?;
            }
        },
        _ => {
            if sse_gpr(p, opcode)? {
                return Ok(());
            }
            return bridge_0f(p, opcode);
        },
    }
    Ok(())
}

unsafe fn bit_op(p: &Prefixes, opcode: u8, imm_form: bool) -> OrPageFault<()> {
    let osz = p.opsize();
    let m = decode_modrm(p, if imm_form { 1 } else { 0 })?;
    let (op, bit_index) = if imm_form {
        let imm = fetch8()?;
        ((m.reg & 7).wrapping_sub(4), imm)
    }
    else {
        (match opcode {
            0xA3 => 0,
            0xAB => 1,
            0xB3 => 2,
            _ => 3,
        }, get_reg(m.reg, osz, true))
    };
    if op > 3 {
        return ud();
    }
    // register operands wrap the bit offset, memory operands address beyond the operand
    let (loc, bit) = match m.loc {
        Loc::Reg(_) => (m.loc, bit_index & (osz as u64 - 1)),
        Loc::Mem(a) => {
            if imm_form {
                (m.loc, bit_index & (osz as u64 - 1))
            }
            else {
                let signed = sign_extend(bit_index, osz) as i64;
                let off = (signed >> 3) & !(osz as i64 / 8 - 1);
                (Loc::Mem(a.wrapping_add(off as u64)), (signed & (osz as i64 - 1)) as u64)
            }
        },
    };
    let m2 = ModRM { reg: m.reg, md: m.md, loc };
    let v = rm_read(p, &m2, osz)?;
    let cf = v >> bit & 1 != 0;
    set_eflags_raw(get_eflags() & !FLAG_CARRY | cf as i32 * FLAG_CARRY);
    let r = match op {
        1 => v | 1 << bit,
        2 => v & !(1 << bit),
        3 => v ^ 1 << bit,
        _ => return Ok(()),
    };
    rm_write(p, &m2, osz, r)
}

// ---------------------------------------------------------------------------------------------
// interrupts

/// Interrupt delivery through a 16 byte IDT gate in 64-bit mode. Only same-privilege delivery
/// without IST is implemented.
pub unsafe fn call_interrupt_vector_64(nr: i32, is_software_int: bool, error_code: Option<i32>) {
    let _ = is_software_int;
    if (nr as u32) * 16 + 15 > *idtr_size as u32 {
        panic!(
            "long mode: exception/interrupt {} (error code {:?}) outside of idt limit {:#x}, rip={:#x} cr2={:#x}",
            nr,
            error_code,
            *idtr_size,
            rip(),
            (*cr2_hi as u32 as u64) << 32 | *cr.offset(2) as u32 as u64
        );
    }
    let gate_addr = *idtr_offset + nr * 16;
    let low = match translate_address_system_read(gate_addr) {
        Ok(a) => crate::cpu::memory::read64s(a) as u64,
        Err(()) => return,
    };
    let high = match translate_address_system_read(gate_addr + 8) {
        Ok(a) => crate::cpu::memory::read64s(a) as u64,
        Err(()) => return,
    };
    let present = low >> 47 & 1 != 0;
    let gate_type = low >> 40 & 0xF;
    if !present || (gate_type != 0xE && gate_type != 0xF) {
        panic!(
            "long mode: bad idt gate for exception/interrupt {} (error code {:?}): {:#x}, rip={:#x} cr2={:#x} rsp={:#x}",
            nr,
            error_code,
            low,
            rip(),
            (*cr2_hi as u32 as u64) << 32 | *cr.offset(2) as u32 as u64,
            r64(RSP)
        );
    }
    let selector = (low >> 16 & 0xFFFF) as i32;
    let ist = low >> 32 & 7;
    let target = (low & 0xFFFF) | (low >> 48 & 0xFFFF) << 16 | (high & 0xFFFF_FFFF) << 32;
    dbg_assert!(ist == 0, "Unimplemented: IST");
    if *cpl != 0 {
        panic!("Unimplemented: interrupts from CPL > 0 in long mode");
    }

    let old_rsp = r64(RSP);
    let old_ss = *sreg.offset(SS as isize) as u64;
    let old_cs = *sreg.offset(CS as isize) as u64;
    let old_flags = get_eflags() as u32 as u64;
    let old_rip = rip();

    // the stack pointer is aligned to 16 bytes before pushing the frame
    set_r64(RSP, old_rsp & !0xF);
    let mut ok = push_val(old_ss, 64).is_ok()
        && push_val(old_rsp, 64).is_ok()
        && push_val(old_flags, 64).is_ok()
        && push_val(old_cs, 64).is_ok()
        && push_val(old_rip, 64).is_ok();
    if ok {
        if let Some(code) = error_code {
            ok = push_val(code as u32 as u64, 64).is_ok();
        }
    }
    if !ok {
        set_r64(RSP, old_rsp);
        return;
    }

    if gate_type == 0xE {
        *flags &= !FLAG_INTERRUPT;
    }
    *flags &= !(FLAG_TRAP | FLAG_NT | FLAG_RF | FLAG_VM);
    let _ = load_cs_64(selector, target);
}

unsafe fn load_cs_64(selector: i32, new_rip: u64) -> OrPageFault<()> {
    let sel = SegmentSelector::of_u16(selector as u16);
    let d = match lookup_segment_selector(sel) {
        Ok(Ok((d, _))) => d,
        Ok(Err(_)) => {
            trigger_gp(selector & !3);
            return Err(());
        },
        Err(()) => return Err(()),
    };
    update_cs_mode(&d);
    *segment_is_null.offset(CS as isize) = false;
    *segment_limits.offset(CS as isize) = d.effective_limit();
    *segment_access_bytes.offset(CS as isize) = d.access_byte();
    *segment_offsets.offset(CS as isize) = d.base();
    *sreg.offset(CS as isize) = selector as u16 & !3 | *cpl as u16;
    *instruction_pointer = d.base().wrapping_add(new_rip as i32);
    *rip_hi = (new_rip >> 32) as i32;
    update_state_flags();
    Ok(())
}

unsafe fn iretq(p: &Prefixes) -> OrPageFault<()> {
    let bits = if p.w() { 64 } else if p.osz16 { 16 } else { 32 };
    let sp = r64(RSP);
    let new_rip = mem_read(sp, bits)?;
    let new_cs = mem_read(sp + bits as u64 / 8, bits)? as i32 & 0xFFFF;
    let new_flags = mem_read(sp + 2 * (bits as u64 / 8), bits)?;
    // in 64-bit mode ss:rsp are always popped
    let new_rsp = mem_read(sp + 3 * (bits as u64 / 8), bits)?;
    let new_ss = mem_read(sp + 4 * (bits as u64 / 8), bits)? as i32 & 0xFFFF;
    if new_cs & 3 != *cpl as i32 {
        panic!("Unimplemented: iretq to a different privilege level");
    }
    load_cs_64(new_cs, new_rip)?;
    update_eflags(new_flags as i32);
    set_r64(RSP, new_rsp);
    if !switch_seg(SS, new_ss) {
        return Err(());
    }
    Ok(())
}

/// Loads a flat code segment without consulting the GDT, as syscall / sysret do
unsafe fn load_fast_cs(selector: i32, long: bool, dpl: u8) {
    *segment_is_null.offset(CS as isize) = false;
    *segment_limits.offset(CS as isize) = 0xFFFF_FFFF;
    *segment_offsets.offset(CS as isize) = 0;
    *segment_access_bytes.offset(CS as isize) = 0x80 | dpl << 5 | 0x10 | 0x08 | 0x02 | 1;
    *sreg.offset(CS as isize) = selector as u16;
    update_cs_size(!long);
    *is_64 = long;
}

unsafe fn load_fast_ss(selector: i32, dpl: u8) {
    *segment_is_null.offset(SS as isize) = false;
    *segment_limits.offset(SS as isize) = 0xFFFF_FFFF;
    *segment_offsets.offset(SS as isize) = 0;
    *segment_access_bytes.offset(SS as isize) = 0x80 | dpl << 5 | 0x10 | 0x02 | 1;
    *sreg.offset(SS as isize) = selector as u16;
    *stack_size_32 = true;
}

unsafe fn syscall(_p: &Prefixes) -> OrPageFault<()> {
    if *efer & EFER_SCE == 0 || !*is_64 {
        return ud();
    }
    let target = *msr_lstar;
    if !is_canonical(target) {
        trigger_gp(0);
        return Err(());
    }
    let flags_now = get_eflags();
    set_r64(RCX, rip());
    set_r64(11, flags_now as u32 as u64);
    let cs = (*msr_star >> 32) as i32 & 0xFFFC;
    load_fast_cs(cs, true, 0);
    load_fast_ss(cs + 8, 0);
    *cpl = 0;
    cpl_changed();
    set_eflags_raw((flags_now & !(*msr_sfmask as i32) & !FLAG_RF) | FLAGS_DEFAULT);
    update_state_flags();
    set_rip(target)
}

unsafe fn sysret(p: &Prefixes) -> OrPageFault<()> {
    if *efer & EFER_SCE == 0 || !*is_64 {
        return ud();
    }
    if *cpl != 0 {
        trigger_gp(0);
        return Err(());
    }
    let long = p.w();
    let target = if long { r64(RCX) } else { r64(RCX) & 0xFFFF_FFFF };
    if !is_canonical(target) {
        trigger_gp(0);
        return Err(());
    }
    let star_hi = (*msr_star >> 48) as i32 & 0xFFFF;
    update_eflags(r64(11) as i32);
    load_fast_cs((if long { star_hi + 16 } else { star_hi }) | 3, long, 3);
    load_fast_ss((star_hi + 8) | 3, 3);
    *cpl = 3;
    cpl_changed();
    update_state_flags();
    set_rip(target)
}

/// cmpxchg8b / cmpxchg16b (REX.W)
unsafe fn cmpxchg_double(p: &Prefixes, addr: u64) -> OrPageFault<()> {
    let (bits, half) = if p.w() { (64, 8u64) } else { (32, 4u64) };
    if p.w() && addr & 15 != 0 {
        trigger_gp(0);
        return Err(());
    }
    let lo = mem_read(addr, bits)?;
    let hi = mem_read(addr + half, bits)?;
    let equal = lo == get_reg(RAX, bits, true) && hi == get_reg(RDX, bits, true);
    if equal {
        mem_write(addr, bits, get_reg(3, bits, true))?;
        mem_write(addr + half, bits, get_reg(RCX, bits, true))?;
    }
    else {
        set_reg(RAX, bits, true, lo);
        set_reg(RDX, bits, true, hi);
    }
    set_eflags_raw(get_eflags() & !FLAG_ZERO | equal as i32 * FLAG_ZERO);
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// bridge to the generated interpreters (sse, mmx, x87)
//
// The generated 64-bit variants (gen/interpreter0f64.rs, gen/interpreter64.rs) decode modrm bytes
// by calling the functions below, which honour REX and 64-bit addressing. The instruction
// implementations behind them only support 32-bit addresses.

static mut BRIDGE_PREFIXES: Prefixes = Prefixes {
    rex: 0,
    has_rex: false,
    osz16: false,
    asz32: false,
    seg: -1,
    rep: 0,
    lock: false,
};
static mut BRIDGE_IMM_LEN: u64 = 0;

pub unsafe fn rex_b() -> i32 { (BRIDGE_PREFIXES.rex as i32 & 1) << 3 }
pub unsafe fn rex_r() -> i32 { (BRIDGE_PREFIXES.rex as i32 & 4) << 1 }

pub unsafe fn read_imm8() -> OrPageFault<i32> { Ok(fetch8()? as i32) }
pub unsafe fn read_imm8s() -> OrPageFault<i32> { Ok(sign_extend(fetch8()?, 8) as i32) }
pub unsafe fn read_imm16() -> OrPageFault<i32> { Ok(fetch16()? as i32) }
pub unsafe fn read_imm32s() -> OrPageFault<i32> { Ok(fetch32()? as i32) }

pub unsafe fn modrm_resolve64(modrm_byte: i32) -> OrPageFault<i32> {
    let p = BRIDGE_PREFIXES;
    match decode_modrm_byte(&p, modrm_byte as u8, BRIDGE_IMM_LEN)?.loc {
        Loc::Mem(a) => window(a),
        Loc::Reg(_) => unreachable!(),
    }
}

unsafe fn bridge_prefix_bits(p: &Prefixes) -> u8 {
    (if p.osz16 { prefix::PREFIX_66 } else { 0 })
        | match p.rep {
            0xF2 => prefix::PREFIX_F2,
            0xF3 => prefix::PREFIX_F3,
            _ => 0,
        }
}

/// Immediate bytes following the modrm operand, needed for rip-relative addressing
fn bridge_imm_len(opcode: u8) -> u64 {
    match opcode {
        0x70..=0x73 | 0xC2 | 0xC4 | 0xC5 | 0xC6 | 0x3A => 1,
        _ => 0,
    }
}

unsafe fn bridge_0f(p: &Prefixes, opcode: u8) -> OrPageFault<()> {
    // mmx instructions (no mandatory prefix) only have 8 registers, REX.R/B do not apply
    let xmm = p.osz16
        || p.rep != 0
        || matches!(opcode, 0x10..=0x17 | 0x28..=0x2F | 0x50..=0x5F | 0xC2 | 0xC6);
    BRIDGE_PREFIXES = *p;
    if !xmm {
        BRIDGE_PREFIXES.rex &= !5;
    }
    BRIDGE_IMM_LEN = bridge_imm_len(opcode);
    *prefixes = bridge_prefix_bits(p);
    let size_bit = if p.osz16 && !p.w() { 0 } else { 0x100 };
    gen::interpreter0f64::run(opcode as u32 | size_bit);
    *prefixes = 0;
    window_close();
    Ok(())
}

unsafe fn bridge_x87(p: &Prefixes, opcode: u8) -> OrPageFault<()> {
    BRIDGE_PREFIXES = *p;
    BRIDGE_IMM_LEN = 0;
    *prefixes = 0;
    gen::interpreter64::run(opcode as u32 | 0x100);
    window_close();
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// sse instructions with general purpose register operands (64-bit forms and r8-r15)

fn f64_to_i64(x: f64) -> u64 {
    if x.is_nan() || x >= 9.223372036854775807e18 || x < -9.223372036854775808e18 {
        i64::MIN as u64
    }
    else {
        x as i64 as u64
    }
}

unsafe fn xmm_source(m: &ModRM, bits: u32) -> OrPageFault<u64> {
    match m.loc {
        Loc::Reg(n) => Ok(read_xmm64s(n as i32) & mask(bits)),
        Loc::Mem(a) => mem_read(a, bits),
    }
}

/// Returns Ok(true) if the instruction was handled
unsafe fn sse_gpr(p: &Prefixes, opcode: u8) -> OrPageFault<bool> {
    let wide = p.w();
    match opcode {
        // movd / movq xmm, r/m
        0x6E if p.osz16 => {
            let m = decode_modrm(p, 0)?;
            let v = rm_read(p, &m, if wide { 64 } else { 32 })?;
            write_xmm128_2(m.reg as i32, v, 0);
        },
        // movd / movq r/m, xmm
        0x7E if p.osz16 => {
            let m = decode_modrm(p, 0)?;
            let v = read_xmm64s(m.reg as i32);
            rm_write(p, &m, if wide { 64 } else { 32 }, v & mask(if wide { 64 } else { 32 }))?;
        },
        // movd / movq mm, r/m
        0x6E if p.rep == 0 && wide => {
            let m = decode_modrm(p, 0)?;
            let v = rm_read(p, &m, 64)?;
            write_mmx_reg64((m.reg & 7) as i32, v);
            transition_fpu_to_mmx();
        },
        0x7E if p.rep == 0 && wide => {
            let m = decode_modrm(p, 0)?;
            let v = read_mmx64s((m.reg & 7) as i32);
            rm_write(p, &m, 64, v)?;
            transition_fpu_to_mmx();
        },
        // cvtsi2ss / cvtsi2sd xmm, r/m32/64
        0x2A if p.rep == 0xF2 || p.rep == 0xF3 => {
            let m = decode_modrm(p, 0)?;
            let bits = if wide { 64 } else { 32 };
            let v = sign_extend(rm_read(p, &m, bits)?, bits) as i64;
            if p.rep == 0xF2 {
                write_xmm_f64(m.reg as i32, v as f64);
            }
            else {
                write_xmm_f32(m.reg as i32, v as f32);
            }
        },
        // cvttss2si / cvttsd2si / cvtss2si / cvtsd2si r, xmm/m
        0x2C | 0x2D if p.rep == 0xF2 || p.rep == 0xF3 => {
            let m = decode_modrm(p, 0)?;
            let double = p.rep == 0xF2;
            let src = xmm_source(&m, if double { 64 } else { 32 })?;
            let x = if double { f64::from_bits(src) } else { f32::from_bits(src as u32) as f64 };
            let x = if opcode == 0x2D { sse_integer_round(x) } else { x };
            if wide {
                set_r64(m.reg, f64_to_i64(x));
            }
            else {
                let r = if x.is_nan() || x >= 2147483648.0 || x < -2147483648.0 {
                    i32::MIN as u32
                }
                else {
                    x as i32 as u32
                };
                set_reg(m.reg, 32, true, r as u64);
            }
        },
        // movmskps / movmskpd r32, xmm
        0x50 => {
            let m = decode_modrm(p, 0)?;
            let Loc::Reg(n) = m.loc else { return ud().map(|_| true) };
            let v = read_xmm128s(n as i32);
            let bits = if p.osz16 {
                (v.u64[0] >> 63 | (v.u64[1] >> 63) << 1) as u64
            }
            else {
                (v.u32[0] >> 31 | (v.u32[1] >> 31) << 1 | (v.u32[2] >> 31) << 2 | (v.u32[3] >> 31) << 3)
                    as u64
            };
            set_reg(m.reg, 32, true, bits);
        },
        // pmovmskb r32, xmm / mm
        0xD7 => {
            let m = decode_modrm(p, 0)?;
            let Loc::Reg(n) = m.loc else { return ud().map(|_| true) };
            let mut bits = 0u64;
            if p.osz16 {
                let v = read_xmm128s(n as i32);
                for i in 0..16 {
                    bits |= ((v.u8[i] >> 7) as u64) << i;
                }
            }
            else {
                let v = read_mmx64s((n & 7) as i32);
                for i in 0..8 {
                    bits |= (v >> (8 * i + 7) & 1) << i;
                }
            }
            set_reg(m.reg, 32, true, bits);
        },
        // pextrw r32, xmm/mm, imm8
        0xC5 => {
            let m = decode_modrm(p, 1)?;
            let Loc::Reg(n) = m.loc else { return ud().map(|_| true) };
            let imm = fetch8()?;
            let v = if p.osz16 {
                read_xmm128s(n as i32).u16[(imm & 7) as usize] as u64
            }
            else {
                read_mmx64s((n & 7) as i32) >> (16 * (imm & 3)) & 0xFFFF
            };
            set_reg(m.reg, 32, true, v);
        },
        // pinsrw xmm/mm, r32/m16, imm8
        0xC4 => {
            let m = decode_modrm(p, 1)?;
            let v = rm_read(p, &m, if matches!(m.loc, Loc::Reg(_)) { 32 } else { 16 })? as u16;
            let imm = fetch8()?;
            if p.osz16 {
                let mut x = read_xmm128s(m.reg as i32);
                x.u16[(imm & 7) as usize] = v;
                write_xmm_reg128(m.reg as i32, x);
            }
            else {
                let r = (m.reg & 7) as i32;
                let shift = 16 * (imm & 3);
                let old = read_mmx64s(r);
                write_mmx_reg64(r, old & !(0xFFFFu64 << shift) | (v as u64) << shift);
                transition_fpu_to_mmx();
            }
        },
        // movnti m32/m64, r32/r64
        0xC3 => {
            let m = decode_modrm(p, 0)?;
            let bits = if wide { 64 } else { 32 };
            let v = get_reg(m.reg, bits, true);
            rm_write(p, &m, bits, v)?;
        },
        _ => return Ok(false),
    }
    Ok(true)
}

/// Runs an instruction implementation that takes a 32-bit address on the operand at `addr`
unsafe fn with_window(addr: u64, f: impl FnOnce(i32)) -> OrPageFault<()> {
    let w = window(addr)?;
    f(w);
    window_close();
    Ok(())
}
