//! JIT for 64-bit mode.
//!
//! The existing jit (src/rust/jit.rs) only knows 16 and 32 bit code. This module compiles hot
//! straight-line sequences of 64-bit instructions (optionally ending in a loop back to their own
//! start) to wasm. It is deliberately simple:
//!
//! - The registers always live in memory (the same `reg32` / `reg_hi` / `reg_ext` the interpreter
//!   uses), so exceptions, interrupts and the interpreter always see an architectural state.
//! - eflags are computed eagerly by helpers (`j64_alu`), exactly like the interpreter does.
//! - Memory accesses go through `j64_read` / `j64_write`, which use the same translation, access
//!   checks and fault delivery as the interpreter. A fault sets `FLAGS[FAULT]` after the exception
//!   has been delivered, and the block returns without further changes.
//! - Anything that is not supported ends the block in front of that instruction, and the
//!   interpreter executes it.
//!
//! Blocks are keyed by virtual rip and validated against the physical page they were compiled
//! from. Writes to such a page remove the blocks through `dirty_page`.

use std::collections::{HashMap, HashSet};

use crate::cpu::cpu::*;
use crate::cpu::global_pointers::*;
use crate::cpu::long_mem::*;
use crate::cpu::long_mode;
use crate::jit;
use crate::page::{Page, PageBuildHasher};
use crate::wasmgen::wasm_builder::{Label, WasmBuilder, WasmLocal, WasmLocalI64};

const NONE: u8 = 255;
const MAX_INSTRUCTIONS: u32 = 300;
/// instructions executed by a looping block before it returns to the dispatcher
const LOOP_LIMIT: i32 = 20000;

// ---------------------------------------------------------------------------------------------
// state

const FAULT: usize = 0;
const SMC: usize = 1;
static mut FLAGS: [i32; 2] = [0; 2];

#[derive(Clone, Copy, PartialEq)]
enum BlockState {
    Pending,
    Ready,
    Failed,
}

struct Block {
    state: BlockState,
    phys_page: u32,
    user: bool,
    index: u16,
    epoch: u32,
}

/// Direct mapped cache in front of the block map: counts executions of branch targets and holds
/// ready blocks
#[derive(Clone, Copy)]
struct Slot {
    rip: u64,
    count: u32,
    phys_page: u32,
    epoch: u32,
    index: u16,
    state: u8, // SLOT_*
    user: bool,
}
const SLOT_COUNTING: u8 = 0;
const SLOT_PENDING: u8 = 1;
const SLOT_READY: u8 = 2;
const SLOT_FAILED: u8 = 3;
const SLOT_EMPTY: Slot = Slot { rip: u64::MAX, count: 0, phys_page: 0, epoch: 0, index: 0, state: SLOT_COUNTING, user: false };
const SLOTS: usize = 4096;

fn slot_index(r: u64) -> usize { (r ^ r >> 12 ^ r >> 24) as usize & (SLOTS - 1) }

struct State {
    blocks: HashMap<u64, Block, PageBuildHasher>,
    by_index: HashMap<u16, u64, PageBuildHasher>,
    page_blocks: HashMap<u32, Vec<u64>, PageBuildHasher>,
    slots: Vec<Slot>,
    /// table indices of invalidated modules that are still being compiled by the host: the index
    /// must not be reused before the host is done with it
    zombies: HashSet<u16>,
    /// per virtual page: how often blocks of it were invalidated (code mixed with data is not
    /// worth compiling again and again)
    invalidations: HashMap<u64, u8, PageBuildHasher>,
}

static mut STATE: Option<State> = None;
static mut CHECK: bool = true;
static mut EPOCH: u32 = 1;
static mut THRESHOLD: u32 = 150;
static mut EXECUTING_PAGE: u32 = u32::MAX;
/// bit i set: instructions of kind i are not compiled (for bisecting problems)
pub static mut DISABLE_MASK: u32 = 0;
pub static mut STAT_COMPILED: u32 = 0;
pub static mut STAT_FAILED: u32 = 0;
pub static mut STAT_RUN: u32 = 0;
pub static mut STAT_INVALIDATED: u32 = 0;

pub unsafe fn set_threshold(t: u32) { THRESHOLD = t }
pub unsafe fn set_disable_mask(m: u32) { DISABLE_MASK = m }
pub unsafe fn threshold() -> u32 { THRESHOLD }

#[allow(static_mut_refs)]
unsafe fn state() -> &'static mut State {
    if STATE.is_none() {
        STATE = Some(State {
            blocks: HashMap::default(),
            by_index: HashMap::default(),
            page_blocks: HashMap::default(),
            slots: vec![SLOT_EMPTY; SLOTS],
            zombies: HashSet::new(),
            invalidations: HashMap::default(),
        });
    }
    STATE.as_mut().unwrap()
}

/// Called whenever the 64-bit tlb is flushed: the virtual to physical mapping of blocks has to be
/// revalidated
pub unsafe fn tlb_flushed() { EPOCH = EPOCH.wrapping_add(1) }

// ---------------------------------------------------------------------------------------------
// helpers called from the generated code

#[no_mangle]
pub unsafe fn j64_alu(op: i32, a: i64, b: i64, bits: i32) -> i64 {
    let bits = bits as u32;
    long_mode::jit_alu(op as u8, a as u64, b as u64, bits) as i64
}

#[no_mangle]
pub unsafe fn j64_read(addr: i64, bits: i32) -> i64 {
    match mem_read(addr as u64, bits as u32) {
        Ok(v) => v as i64,
        Err(()) => {
            FLAGS[FAULT] = 1;
            0
        },
    }
}

#[no_mangle]
pub unsafe fn j64_write(addr: i64, v: i64, bits: i32) {
    if mem_write(addr as u64, bits as u32, v as u64).is_err() {
        FLAGS[FAULT] = 1;
    }
}

#[no_mangle]
pub unsafe fn j64_set_rip(target: i64, _unused: i32) {
    if long_mode::jit_set_rip(target as u64).is_err() {
        FLAGS[FAULT] = 1;
    }
}

/// flags computed lazily by 16/32 bit code may still be pending when 64-bit code is entered
#[no_mangle]
pub unsafe fn j64_materialize_flags() {
    let f = get_eflags();
    *flags = f;
    *flags_changed = 0;
}

// ---------------------------------------------------------------------------------------------
// decoder

#[derive(Clone, Copy)]
struct Ea {
    base: u8,
    index: u8,
    scale: u8,
    disp: i64,
    seg: i32,
    asz32: bool,
    rip_rel: bool,
}

#[derive(Clone, Copy)]
enum Opd {
    Reg(u8, bool),
    Mem(Ea),
    Imm(u64),
}

#[derive(Clone, Copy)]
enum Count {
    Imm(u8),
}

#[derive(Clone, Copy)]
enum Insn {
    Alu { op: u8, bits: u32, dst: Opd, src: Opd },
    Test { bits: u32, a: Opd, b: Opd },
    Mov { bits: u32, dst: Opd, src: Opd },
    MovExt { dbits: u32, sbits: u32, signed: bool, dst: u8, src: Opd },
    Lea { bits: u32, dst: u8, ea: Ea },
    Push { bits: u32, src: Opd },
    Pop { bits: u32, dst: u8 },
    IncDec { dec: bool, bits: u32, dst: Opd },
    Shift { op: u8, bits: u32, dst: Opd, count: Count },
    Setcc { cc: u8, dst: Opd },
    Cmov { cc: u8, bits: u32, dst: u8, src: Opd },
    Jcc { cc: u8, target: u64 },
    Jmp { target: u64 },
    Call { target: u64 },
    Ret { adj: u64 },
    Leave { bits: u32 },
    Nop,
}

struct Cursor<'a> {
    code: &'a [u8],
    pos: usize,
}

fn kind(i: &Insn) -> u32 {
    match i {
        Insn::Alu { .. } => 0,
        Insn::Test { .. } => 1,
        Insn::Mov { .. } => 2,
        Insn::MovExt { .. } => 3,
        Insn::Lea { .. } => 4,
        Insn::Push { .. } => 5,
        Insn::Pop { .. } => 6,
        Insn::IncDec { .. } => 7,
        Insn::Shift { .. } => 8,
        Insn::Setcc { .. } => 9,
        Insn::Cmov { .. } => 10,
        Insn::Jcc { .. } => 11,
        Insn::Jmp { .. } => 12,
        Insn::Call { .. } => 13,
        Insn::Ret { .. } => 14,
        Insn::Leave { .. } => 15,
        Insn::Nop => 16,
    }
}

impl<'a> Cursor<'a> {
    fn u8(&mut self) -> Option<u8> {
        let b = *self.code.get(self.pos)?;
        self.pos += 1;
        Some(b)
    }
    fn u16(&mut self) -> Option<u64> { Some(self.u8()? as u64 | (self.u8()? as u64) << 8) }
    fn u32(&mut self) -> Option<u64> { Some(self.u16()? | self.u16()? << 16) }
    fn u64(&mut self) -> Option<u64> { Some(self.u32()? | self.u32()? << 32) }
    fn i8(&mut self) -> Option<i64> { Some(self.u8()? as i8 as i64) }
    fn i32(&mut self) -> Option<i64> { Some(self.u32()? as u32 as i32 as i64) }
}

fn sign_extend(v: u64, bits: u32) -> u64 {
    if bits == 64 { v } else { ((v << (64 - bits)) as i64 >> (64 - bits)) as u64 }
}

#[derive(Clone, Copy)]
struct Pfx {
    rex: u8,
    has_rex: bool,
    osz16: bool,
    asz32: bool,
    seg: i32,
}

impl Pfx {
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

struct Modrm {
    reg: u8,
    rm: Opd,
}

fn modrm(c: &mut Cursor, p: &Pfx) -> Option<Modrm> {
    let m = c.u8()?;
    let md = m >> 6;
    let reg = (m >> 3 & 7) | p.r();
    let rm = m & 7;
    if md == 3 {
        return Some(Modrm { reg, rm: Opd::Reg(rm | p.b(), p.has_rex) });
    }
    let mut ea = Ea { base: NONE, index: NONE, scale: 0, disp: 0, seg: p.seg, asz32: p.asz32, rip_rel: false };
    if rm == 4 {
        let sib = c.u8()?;
        let index = (sib >> 3 & 7) | p.x();
        ea.scale = sib >> 6;
        if sib & 7 == 5 && md == 0 {
            ea.disp = c.i32()?;
        }
        else {
            ea.base = (sib & 7) | p.b();
        }
        if index != 4 {
            ea.index = index;
        }
    }
    else if rm == 5 && md == 0 {
        ea.disp = c.i32()?;
        ea.rip_rel = true;
    }
    else {
        ea.base = rm | p.b();
    }
    match md {
        1 => ea.disp = ea.disp.wrapping_add(c.i8()?),
        2 => ea.disp = ea.disp.wrapping_add(c.i32()?),
        _ => {},
    }
    Some(Modrm { reg, rm: Opd::Mem(ea) })
}

/// Decodes one instruction at `rip` (the code slice starts at the instruction). Returns the
/// instruction and its length, or None if it is not supported.
fn decode(code: &[u8], rip: u64) -> Option<(Insn, usize)> {
    let mut c = Cursor { code, pos: 0 };
    let mut p = Pfx { rex: 0, has_rex: false, osz16: false, asz32: false, seg: -1 };
    let opcode = loop {
        let b = c.u8()?;
        match b {
            0x66 => p.osz16 = true,
            0x67 => p.asz32 = true,
            0xF0 => {},
            0x64 => p.seg = FS,
            0x65 => p.seg = GS,
            0x26 | 0x2E | 0x36 | 0x3E => {},
            0x40..=0x4F => {
                p.rex = b;
                p.has_rex = true;
                break c.u8()?;
            },
            // rep prefixes change the meaning of too many instructions
            0xF2 | 0xF3 => return None,
            _ => break b,
        }
    };
    let osz = p.opsize();

    let insn = match opcode {
        0x00..=0x3F if opcode & 7 < 6 => {
            let op = opcode >> 3;
            match opcode & 7 {
                0 | 1 => {
                    let bits = if opcode & 1 == 0 { 8 } else { osz };
                    let m = modrm(&mut c, &p)?;
                    Insn::Alu { op, bits, dst: m.rm, src: Opd::Reg(m.reg, p.has_rex) }
                },
                2 | 3 => {
                    let bits = if opcode & 1 == 0 { 8 } else { osz };
                    let m = modrm(&mut c, &p)?;
                    Insn::Alu { op, bits, dst: Opd::Reg(m.reg, p.has_rex), src: m.rm }
                },
                4 => {
                    let b = c.u8()? as u64;
                    Insn::Alu { op, bits: 8, dst: Opd::Reg(0, true), src: Opd::Imm(b) }
                },
                _ => {
                    let b = if osz == 16 { c.u16()? } else { sign_extend(c.u32()?, 32) };
                    Insn::Alu { op, bits: osz, dst: Opd::Reg(0, true), src: Opd::Imm(b) }
                },
            }
        },
        0x50..=0x57 => Insn::Push {
            bits: if p.osz16 { 16 } else { 64 },
            src: Opd::Reg((opcode & 7) | p.b(), true),
        },
        0x58..=0x5F => Insn::Pop { bits: if p.osz16 { 16 } else { 64 }, dst: (opcode & 7) | p.b() },
        0x63 => {
            let m = modrm(&mut c, &p)?;
            if !p.w() {
                return None;
            }
            Insn::MovExt { dbits: 64, sbits: 32, signed: true, dst: m.reg, src: m.rm }
        },
        0x68 => {
            let v = if p.osz16 { c.u16()? } else { sign_extend(c.u32()?, 32) };
            Insn::Push { bits: if p.osz16 { 16 } else { 64 }, src: Opd::Imm(v) }
        },
        0x6A => {
            let v = c.i8()? as u64;
            Insn::Push { bits: if p.osz16 { 16 } else { 64 }, src: Opd::Imm(v) }
        },
        0x70..=0x7F => {
            let d = c.i8()?;
            let target = rip.wrapping_add(c.pos as u64).wrapping_add(d as u64);
            Insn::Jcc { cc: opcode & 0xF, target }
        },
        0x80 | 0x81 | 0x83 => {
            let bits = if opcode == 0x80 { 8 } else { osz };
            let m = modrm(&mut c, &p)?;
            let imm = match opcode {
                0x80 => c.u8()? as u64,
                0x81 => {
                    if osz == 16 { c.u16()? } else { sign_extend(c.u32()?, 32) }
                },
                _ => sign_extend(c.u8()? as u64, 8),
            };
            Insn::Alu { op: m.reg & 7, bits, dst: m.rm, src: Opd::Imm(imm) }
        },
        0x84 | 0x85 => {
            let bits = if opcode == 0x84 { 8 } else { osz };
            let m = modrm(&mut c, &p)?;
            Insn::Test { bits, a: m.rm, b: Opd::Reg(m.reg, p.has_rex) }
        },
        0x88 | 0x89 => {
            let bits = if opcode == 0x88 { 8 } else { osz };
            let m = modrm(&mut c, &p)?;
            Insn::Mov { bits, dst: m.rm, src: Opd::Reg(m.reg, p.has_rex) }
        },
        0x8A | 0x8B => {
            let bits = if opcode == 0x8A { 8 } else { osz };
            let m = modrm(&mut c, &p)?;
            Insn::Mov { bits, dst: Opd::Reg(m.reg, p.has_rex), src: m.rm }
        },
        0x8D => {
            let m = modrm(&mut c, &p)?;
            match m.rm {
                Opd::Mem(mut ea) => {
                    ea.seg = -1;
                    Insn::Lea { bits: osz, dst: m.reg, ea }
                },
                _ => return None,
            }
        },
        0x90 => {
            if p.b() != 0 {
                return None;
            }
            Insn::Nop
        },
        0xA8 => {
            let b = c.u8()? as u64;
            Insn::Test { bits: 8, a: Opd::Reg(0, true), b: Opd::Imm(b) }
        },
        0xA9 => {
            let b = if osz == 16 { c.u16()? } else { sign_extend(c.u32()?, 32) };
            Insn::Test { bits: osz, a: Opd::Reg(0, true), b: Opd::Imm(b) }
        },
        0xB0..=0xB7 => {
            let v = c.u8()? as u64;
            Insn::Mov { bits: 8, dst: Opd::Reg((opcode & 7) | p.b(), p.has_rex), src: Opd::Imm(v) }
        },
        0xB8..=0xBF => {
            let v = match osz {
                64 => c.u64()?,
                16 => c.u16()?,
                _ => c.u32()?,
            };
            Insn::Mov { bits: osz, dst: Opd::Reg((opcode & 7) | p.b(), true), src: Opd::Imm(v) }
        },
        0xC0 | 0xC1 | 0xD0 | 0xD1 => {
            let bits = if opcode & 1 == 0 { 8 } else { osz };
            let m = modrm(&mut c, &p)?;
            let count = if opcode < 0xD0 { c.u8()? } else { 1 };
            // count 0 has special cases (no write, flags unchanged): leave it to the interpreter
            if count & if bits == 64 { 63 } else { 31 } == 0 {
                return None;
            }
            Insn::Shift { op: m.reg & 7, bits, dst: m.rm, count: Count::Imm(count) }
        },
        0xC2 | 0xC3 => {
            let adj = if opcode == 0xC2 { c.u16()? } else { 0 };
            Insn::Ret { adj }
        },
        0xC6 | 0xC7 => {
            let bits = if opcode == 0xC6 { 8 } else { osz };
            let m = modrm(&mut c, &p)?;
            if m.reg & 7 != 0 {
                return None;
            }
            let v = match bits {
                8 => c.u8()? as u64,
                16 => c.u16()?,
                _ => sign_extend(c.u32()?, 32),
            };
            Insn::Mov { bits, dst: m.rm, src: Opd::Imm(v) }
        },
        0xC9 => Insn::Leave { bits: if p.osz16 { 16 } else { 64 } },
        0xE8 => {
            let d = c.i32()?;
            let target = rip.wrapping_add(c.pos as u64).wrapping_add(d as u64);
            Insn::Call { target }
        },
        0xE9 => {
            let d = c.i32()?;
            let target = rip.wrapping_add(c.pos as u64).wrapping_add(d as u64);
            Insn::Jmp { target }
        },
        0xEB => {
            let d = c.i8()?;
            let target = rip.wrapping_add(c.pos as u64).wrapping_add(d as u64);
            Insn::Jmp { target }
        },
        0xFE | 0xFF => {
            let bits = if opcode == 0xFE { 8 } else { osz };
            let m = modrm(&mut c, &p)?;
            match m.reg & 7 {
                0 | 1 => Insn::IncDec { dec: m.reg & 7 == 1, bits, dst: m.rm },
                _ => return None,
            }
        },
        0x0F => {
            let op2 = c.u8()?;
            match op2 {
                0x1F => {
                    modrm(&mut c, &p)?;
                    Insn::Nop
                },
                0x40..=0x4F => {
                    let m = modrm(&mut c, &p)?;
                    Insn::Cmov { cc: op2 & 0xF, bits: osz, dst: m.reg, src: m.rm }
                },
                0x80..=0x8F => {
                    let d = c.i32()?;
                    let target = rip.wrapping_add(c.pos as u64).wrapping_add(d as u64);
                    Insn::Jcc { cc: op2 & 0xF, target }
                },
                0x90..=0x9F => {
                    let m = modrm(&mut c, &p)?;
                    Insn::Setcc { cc: op2 & 0xF, dst: m.rm }
                },
                0xB6 | 0xB7 | 0xBE | 0xBF => {
                    let sbits = if op2 & 1 == 0 { 8 } else { 16 };
                    let m = modrm(&mut c, &p)?;
                    Insn::MovExt { dbits: osz, sbits, signed: op2 >= 0xBE, dst: m.reg, src: m.rm }
                },
                _ => return None,
            }
        },
        _ => return None,
    };

    let len = c.pos;
    // rip-relative operands are relative to the end of the instruction
    let end = rip.wrapping_add(len as u64);
    let fix = |o: Opd| match o {
        Opd::Mem(mut ea) if ea.rip_rel => {
            ea.disp = ea.disp.wrapping_add(end as i64);
            ea.rip_rel = false;
            Opd::Mem(ea)
        },
        o => o,
    };
    let insn = match insn {
        Insn::Alu { op, bits, dst, src } => Insn::Alu { op, bits, dst: fix(dst), src: fix(src) },
        Insn::Test { bits, a, b } => Insn::Test { bits, a: fix(a), b: fix(b) },
        Insn::Mov { bits, dst, src } => Insn::Mov { bits, dst: fix(dst), src: fix(src) },
        Insn::MovExt { dbits, sbits, signed, dst, src } => Insn::MovExt { dbits, sbits, signed, dst, src: fix(src) },
        Insn::Lea { bits, dst, mut ea } => {
            if ea.rip_rel {
                ea.disp = ea.disp.wrapping_add(end as i64);
                ea.rip_rel = false;
            }
            Insn::Lea { bits, dst, ea }
        },
        Insn::IncDec { dec, bits, dst } => Insn::IncDec { dec, bits, dst: fix(dst) },
        Insn::Shift { op, bits, dst, count } => Insn::Shift { op, bits, dst: fix(dst), count },
        Insn::Setcc { cc, dst } => Insn::Setcc { cc, dst: fix(dst) },
        Insn::Cmov { cc, bits, dst, src } => Insn::Cmov { cc, bits, dst, src: fix(src) },
        i => i,
    };
    if unsafe { DISABLE_MASK } >> kind(&insn) & 1 != 0 {
        return None;
    }
    Some((insn, len))
}

// ---------------------------------------------------------------------------------------------
// code generation

fn lo_addr(n: u8) -> u32 {
    if n < 8 { reg32 as u32 + 4 * n as u32 } else { reg_ext as u32 + 4 * (n as u32 - 8) }
}
fn hi_addr(n: u8) -> u32 { reg_hi as u32 + 4 * n as u32 }

struct Gen<'a> {
    b: &'a mut WasmBuilder,
    start: u64,
    loop_label: Label,
    /// instructions emitted so far in the current basic block (including the current one)
    n: i32,
    /// instructions executed by completed basic blocks
    n_local: WasmLocal,
    /// index of the basic block to run next (dispatch loop)
    pc: WasmLocal,
    t_ea: WasmLocalI64,
    t_v: WasmLocalI64,
    t_a: WasmLocalI64,
    cur: u64,
    next: u64,
    /// offset of a basic block (from the start of the region) -> its index
    block_index: HashMap<usize, usize>,
}

impl<'a> Gen<'a> {
    fn push_counter_update(&mut self, extra: i32) {
        let addr = instruction_counter as u32;
        self.b.const_i32(addr as i32);
        self.b.load_fixed_i32(addr);
        self.b.get_local(&self.n_local);
        self.b.add_i32();
        self.b.const_i32(extra);
        self.b.add_i32();
        self.b.store_aligned_i32(0);
    }

    /// leave the block, continuing at `target`
    fn exit_const(&mut self, target: u64, extra: i32) {
        self.b.const_i32(instruction_pointer as i32);
        self.b.const_i32(target as i32);
        self.b.store_aligned_i32(0);
        if (target >> 32) as u32 != (self.start >> 32) as u32 {
            self.b.const_i32(rip_hi as i32);
            self.b.const_i32((target >> 32) as i32);
            self.b.store_aligned_i32(0);
        }
        self.push_counter_update(extra);
        self.b.return_();
    }

    /// leave the block without touching rip (an exception has redirected it already)
    fn exit_keep_rip(&mut self) {
        self.push_counter_update(self.n);
        self.b.return_();
    }

    /// leave the function; rip is the start of the basic block selected by `pc`
    fn exit_dynamic_pc(&mut self, block_starts: &[usize]) {
        // compare-chain over the block starts (only taken when the loop budget is used up)
        for (i, &off) in block_starts.iter().enumerate() {
            self.b.get_local(&self.pc);
            self.b.const_i32(i as i32);
            self.b.eq_i32();
            self.b.if_void();
            let target = self.start.wrapping_add(off as u64);
            self.exit_const(target, 0);
            self.b.block_end();
        }
        self.b.unreachable();
    }

    fn set_previous_ip(&mut self) {
        self.b.const_i32(previous_ip as i32);
        self.b.const_i32(self.cur as i32);
        self.b.store_aligned_i32(0);
    }

    fn fault_check(&mut self) {
        self.b.load_fixed_i32(unsafe { &raw const FLAGS[FAULT] } as u32);
        self.b.if_void();
        self.exit_keep_rip();
        self.b.block_end();
    }

    /// after a write: if the write changed code of this block, continue in the interpreter
    fn smc_check(&mut self) {
        self.b.load_fixed_i32(unsafe { &raw const FLAGS[SMC] } as u32);
        self.b.if_void();
        let next = self.next;
        self.exit_const(next, self.n);
        self.b.block_end();
    }

    // registers --------------------------------------------------------------------------

    fn load_lo(&mut self, n: u8) { self.b.load_fixed_i32(lo_addr(n)); }

    fn get_reg(&mut self, n: u8, bits: u32, rex: bool) {
        match bits {
            64 => {
                self.load_lo(n);
                self.b.extend_unsigned_i32_to_i64();
                self.b.load_fixed_i32(hi_addr(n));
                self.b.extend_unsigned_i32_to_i64();
                self.b.const_i64(32);
                self.b.shl_i64();
                self.b.or_i64();
            },
            32 => {
                self.load_lo(n);
                self.b.extend_unsigned_i32_to_i64();
            },
            16 => {
                self.load_lo(n);
                self.b.const_i32(0xFFFF);
                self.b.and_i32();
                self.b.extend_unsigned_i32_to_i64();
            },
            _ => {
                if !rex && (4..8).contains(&n) {
                    self.load_lo(n - 4);
                    self.b.const_i32(8);
                    self.b.shr_u_i32();
                }
                else {
                    self.load_lo(n);
                }
                self.b.const_i32(0xFF);
                self.b.and_i32();
                self.b.extend_unsigned_i32_to_i64();
            },
        }
    }

    /// stores the i64 on the stack into a register
    fn set_reg(&mut self, n: u8, bits: u32, rex: bool) {
        let t = self.b.set_new_local_i64();
        match bits {
            64 => {
                self.b.const_i32(lo_addr(n) as i32);
                self.b.get_local_i64(&t);
                self.b.wrap_i64_to_i32();
                self.b.store_aligned_i32(0);
                self.b.const_i32(hi_addr(n) as i32);
                self.b.get_local_i64(&t);
                self.b.const_i64(32);
                self.b.shr_u_i64();
                self.b.wrap_i64_to_i32();
                self.b.store_aligned_i32(0);
            },
            32 => {
                self.b.const_i32(lo_addr(n) as i32);
                self.b.get_local_i64(&t);
                self.b.wrap_i64_to_i32();
                self.b.store_aligned_i32(0);
                self.b.const_i32(hi_addr(n) as i32);
                self.b.const_i32(0);
                self.b.store_aligned_i32(0);
            },
            16 => {
                self.b.const_i32(lo_addr(n) as i32);
                self.load_lo(n);
                self.b.const_i32(-0x10000);
                self.b.and_i32();
                self.b.get_local_i64(&t);
                self.b.wrap_i64_to_i32();
                self.b.const_i32(0xFFFF);
                self.b.and_i32();
                self.b.or_i32();
                self.b.store_aligned_i32(0);
            },
            _ => {
                if !rex && (4..8).contains(&n) {
                    let n = n - 4;
                    self.b.const_i32(lo_addr(n) as i32);
                    self.load_lo(n);
                    self.b.const_i32(!0xFF00);
                    self.b.and_i32();
                    self.b.get_local_i64(&t);
                    self.b.wrap_i64_to_i32();
                    self.b.const_i32(0xFF);
                    self.b.and_i32();
                    self.b.const_i32(8);
                    self.b.shl_i32();
                    self.b.or_i32();
                    self.b.store_aligned_i32(0);
                }
                else {
                    self.b.const_i32(lo_addr(n) as i32);
                    self.load_lo(n);
                    self.b.const_i32(!0xFF);
                    self.b.and_i32();
                    self.b.get_local_i64(&t);
                    self.b.wrap_i64_to_i32();
                    self.b.const_i32(0xFF);
                    self.b.and_i32();
                    self.b.or_i32();
                    self.b.store_aligned_i32(0);
                }
            },
        }
        self.b.free_local_i64(t);
    }

    // memory -----------------------------------------------------------------------------

    /// pushes the effective address as i64
    fn ea(&mut self, ea: &Ea, with_segment: bool) {
        self.b.const_i64(ea.disp);
        if ea.base != NONE {
            self.get_reg(ea.base, 64, true);
            self.b.add_i64();
        }
        if ea.index != NONE {
            self.get_reg(ea.index, 64, true);
            if ea.scale != 0 {
                self.b.const_i64(ea.scale as i64);
                self.b.shl_i64();
            }
            self.b.add_i64();
        }
        if ea.asz32 {
            self.b.const_i64(0xFFFF_FFFF);
            self.b.and_i64();
        }
        if with_segment && ea.seg >= 0 {
            let addr = if ea.seg == FS { fs_base as u32 } else { gs_base as u32 };
            self.b.load_fixed_i64(addr);
            self.b.add_i64();
        }
    }

    /// [addr on stack] -> value
    fn read_mem(&mut self, bits: u32) {
        self.b.const_i32(bits as i32);
        self.b.call_fn2_i64_i32_ret_i64("j64_read");
    }

    /// reads the operand (address is computed anew)
    fn read_opd(&mut self, o: &Opd, bits: u32) {
        match *o {
            Opd::Reg(n, rex) => self.get_reg(n, bits, rex),
            Opd::Imm(v) => self.b.const_i64(v as i64),
            Opd::Mem(ea) => {
                self.ea(&ea, true);
                self.read_mem(bits);
                self.fault_check();
            },
        }
    }

    /// computes the address of a memory operand into t_ea (for read-modify-write)
    fn ea_to_local(&mut self, o: &Opd) {
        if let Opd::Mem(ea) = *o {
            self.ea(&ea, true);
            self.b.set_local_i64(&self.t_ea);
        }
    }

    /// reads the operand, using the address saved in t_ea for memory operands
    fn read_opd_saved(&mut self, o: &Opd, bits: u32) {
        match *o {
            Opd::Mem(_) => {
                self.b.get_local_i64(&self.t_ea);
                self.read_mem(bits);
                self.fault_check();
            },
            _ => self.read_opd(o, bits),
        }
    }

    /// writes the i64 on the stack to the operand (memory address from t_ea)
    fn write_opd_saved(&mut self, o: &Opd, bits: u32) {
        match *o {
            Opd::Reg(n, rex) => self.set_reg(n, bits, rex),
            Opd::Mem(_) => {
                self.b.set_local_i64(&self.t_v);
                self.b.get_local_i64(&self.t_ea);
                self.b.get_local_i64(&self.t_v);
                self.b.const_i32(bits as i32);
                self.b.call_fn3_i64_i64_i32("j64_write");
                self.fault_check();
                self.smc_check();
            },
            Opd::Imm(_) => unreachable!(),
        }
    }

    /// i32 on the stack: whether condition cc holds
    fn cond(&mut self, cc: u8) {
        let f = |g: &mut Gen| g.b.load_fixed_i32(flags as u32);
        match cc >> 1 {
            0 | 1 | 2 | 4 | 5 => {
                let mask = match cc >> 1 {
                    0 => FLAG_OVERFLOW,
                    1 => FLAG_CARRY,
                    2 => FLAG_ZERO,
                    4 => FLAG_SIGN,
                    _ => FLAG_PARITY,
                };
                f(self);
                self.b.const_i32(mask);
                self.b.and_i32();
            },
            3 => {
                f(self);
                self.b.const_i32(FLAG_CARRY | FLAG_ZERO);
                self.b.and_i32();
            },
            _ => {
                // sf != of
                f(self);
                self.b.const_i32(7);
                self.b.shr_u_i32();
                f(self);
                self.b.const_i32(11);
                self.b.shr_u_i32();
                self.b.xor_i32();
                self.b.const_i32(1);
                self.b.and_i32();
                if cc >> 1 == 7 {
                    f(self);
                    self.b.const_i32(FLAG_ZERO);
                    self.b.and_i32();
                    self.b.or_i32();
                }
            },
        }
        // the value is non-zero when the condition (without the invert bit) holds
        if cc & 1 == 0 {
            self.b.const_i32(0);
            self.b.ne_i32();
        }
        else {
            self.b.eqz_i32();
        }
    }

    fn emit(&mut self, insn: &Insn) {
        match *insn {
            Insn::Nop => {},
            Insn::Alu { op, bits, dst, src } => {
                self.set_previous_ip();
                self.ea_to_local(&dst);
                self.ea_to_local(&src);
                self.b.const_i32(op as i32);
                self.read_opd_saved(&dst, bits);
                self.read_opd_saved(&src, bits);
                self.b.const_i32(bits as i32);
                self.b.call_fn4_i32_i64_i64_i32_ret_i64("j64_alu");
                if op != 7 {
                    self.write_opd_saved(&dst, bits);
                }
                else {
                    self.b.drop_();
                }
            },
            Insn::Test { bits, a, b } => {
                self.set_previous_ip();
                self.ea_to_local(&a);
                self.ea_to_local(&b);
                self.b.const_i32(4);
                self.read_opd_saved(&a, bits);
                self.read_opd_saved(&b, bits);
                self.b.const_i32(bits as i32);
                self.b.call_fn4_i32_i64_i64_i32_ret_i64("j64_alu");
                self.b.drop_();
            },
            Insn::Mov { bits, dst, src } => {
                self.set_previous_ip();
                self.ea_to_local(&dst);
                self.ea_to_local(&src);
                self.read_opd_saved(&src, bits);
                self.write_opd_saved(&dst, bits);
            },
            Insn::MovExt { dbits, sbits, signed, dst, src } => {
                self.set_previous_ip();
                self.read_opd(&src, sbits);
                if signed {
                    self.b.const_i64(64 - sbits as i64);
                    self.b.shl_i64();
                    self.b.const_i64(64 - sbits as i64);
                    self.b.shr_s_i64();
                }
                self.set_reg(dst, dbits, true);
            },
            Insn::Lea { bits, dst, ea } => {
                self.ea(&ea, false);
                self.set_reg(dst, bits, true);
            },
            Insn::Push { bits, src } => {
                self.set_previous_ip();
                self.read_opd(&src, 64);
                self.b.set_local_i64(&self.t_v);
                self.get_reg(4, 64, true);
                self.b.const_i64(bits as i64 / 8);
                self.b.sub_i64();
                self.b.set_local_i64(&self.t_ea);
                self.b.get_local_i64(&self.t_ea);
                self.b.get_local_i64(&self.t_v);
                self.b.const_i32(bits as i32);
                self.b.call_fn3_i64_i64_i32("j64_write");
                self.fault_check();
                self.smc_check();
                self.b.get_local_i64(&self.t_ea);
                self.set_reg(4, 64, true);
            },
            Insn::Pop { bits, dst } => {
                self.set_previous_ip();
                self.get_reg(4, 64, true);
                self.b.set_local_i64(&self.t_ea);
                self.b.get_local_i64(&self.t_ea);
                self.read_mem(bits);
                self.fault_check();
                self.b.set_local_i64(&self.t_v);
                self.b.get_local_i64(&self.t_ea);
                self.b.const_i64(bits as i64 / 8);
                self.b.add_i64();
                self.set_reg(4, 64, true);
                self.b.get_local_i64(&self.t_v);
                self.set_reg(dst, bits, true);
            },
            Insn::IncDec { dec, bits, dst } => {
                self.set_previous_ip();
                self.ea_to_local(&dst);
                self.b.const_i32(8 + dec as i32);
                self.read_opd_saved(&dst, bits);
                self.b.const_i64(0);
                self.b.const_i32(bits as i32);
                self.b.call_fn4_i32_i64_i64_i32_ret_i64("j64_alu");
                self.write_opd_saved(&dst, bits);
            },
            Insn::Shift { op, bits, dst, count } => {
                self.set_previous_ip();
                self.ea_to_local(&dst);
                let Count::Imm(c) = count;
                self.b.const_i32(16 + op as i32);
                self.read_opd_saved(&dst, bits);
                self.b.const_i64(c as i64);
                self.b.const_i32(bits as i32);
                self.b.call_fn4_i32_i64_i64_i32_ret_i64("j64_alu");
                self.write_opd_saved(&dst, bits);
            },
            Insn::Setcc { cc, dst } => {
                self.set_previous_ip();
                self.ea_to_local(&dst);
                self.cond(cc);
                self.b.extend_unsigned_i32_to_i64();
                self.write_opd_saved(&dst, 8);
            },
            Insn::Cmov { cc, bits, dst, src } => {
                self.set_previous_ip();
                self.read_opd(&src, bits);
                self.b.set_local_i64(&self.t_v);
                // the destination is zero extended for 32 bit operands even if not taken
                self.b.get_local_i64(&self.t_v);
                self.get_reg(dst, bits, true);
                self.cond(cc);
                self.b.select();
                self.set_reg(dst, bits, true);
            },
            Insn::Leave { bits } => {
                self.set_previous_ip();
                self.get_reg(5, 64, true);
                self.b.set_local_i64(&self.t_ea);
                self.b.get_local_i64(&self.t_ea);
                self.set_reg(4, 64, true);
                self.b.get_local_i64(&self.t_ea);
                self.read_mem(bits);
                self.fault_check();
                self.b.set_local_i64(&self.t_v);
                self.b.get_local_i64(&self.t_ea);
                self.b.const_i64(bits as i64 / 8);
                self.b.add_i64();
                self.set_reg(4, 64, true);
                self.b.get_local_i64(&self.t_v);
                self.set_reg(5, bits, true);
            },
            Insn::Jcc { .. } | Insn::Jmp { .. } | Insn::Call { .. } | Insn::Ret { .. } => {
                unreachable!()
            },
        }
        let _ = self.t_a.idx();
    }

    /// continue at `target`: in another basic block of this function if it has been compiled,
    /// otherwise leave the function
    fn transfer(&mut self, target: u64) {
        let off = target.wrapping_sub(self.start);
        if target >= self.start && off < (1 << 20) {
            if let Some(&idx) = self.block_index.get(&(off as usize)) {
                self.b.get_local(&self.n_local);
                self.b.const_i32(self.n);
                self.b.add_i32();
                self.b.set_local(&self.n_local);
                self.b.const_i32(idx as i32);
                self.b.set_local(&self.pc);
                self.b.br(self.loop_label);
                return;
            }
        }
        let n = self.n;
        self.exit_const(target, n);
    }
}

/// Compiles the code starting at `rip` (the rest of its page) to one wasm function: all
/// instructions that can be reached from `rip` through supported instructions and jumps inside
/// the page become basic blocks of a small dispatch loop. Returns the number of instructions.
unsafe fn generate(b: &mut WasmBuilder, rip: u64, code: &[u8]) -> Option<u32> {
    use std::collections::{BTreeMap, BTreeSet};

    // discover the reachable instructions
    let mut decoded: BTreeMap<usize, (Insn, usize)> = BTreeMap::new();
    let mut leaders: BTreeSet<usize> = BTreeSet::new();
    leaders.insert(0);
    let mut work = vec![0usize];
    let internal = |target: u64| -> Option<usize> {
        let off = target.wrapping_sub(rip);
        if target >= rip && (off as usize) < code.len() { Some(off as usize) } else { None }
    };
    while let Some(mut off) = work.pop() {
        loop {
            if decoded.contains_key(&off) || decoded.len() >= MAX_INSTRUCTIONS as usize || off >= code.len() {
                break;
            }
            let Some((insn, len)) = decode(&code[off..], rip.wrapping_add(off as u64)) else { break };
            if off + len > code.len() {
                break;
            }
            match insn {
                Insn::Jcc { target, .. } | Insn::Jmp { target } if !is_canonical(target) => break,
                Insn::Call { target } if !is_canonical(target) => break,
                _ => {},
            }
            decoded.insert(off, (insn, len));
            match insn {
                Insn::Jcc { target, .. } => {
                    if let Some(t) = internal(target) {
                        if leaders.insert(t) {
                            work.push(t);
                        }
                    }
                    off += len;
                },
                Insn::Jmp { target } => {
                    if let Some(t) = internal(target) {
                        if leaders.insert(t) {
                            work.push(t);
                        }
                    }
                    break;
                },
                Insn::Call { .. } | Insn::Ret { .. } => break,
                _ => off += len,
            }
        }
    }
    if !decoded.contains_key(&0) {
        return None;
    }

    // basic blocks: runs of instructions that start at a leader or after a gap
    let mut block_starts: Vec<usize> = vec![];
    let mut prev_end = usize::MAX;
    for (&off, &(_, len)) in decoded.iter() {
        if leaders.contains(&off) || off != prev_end {
            block_starts.push(off);
        }
        prev_end = off + len;
    }
    let mut block_index = HashMap::new();
    for (i, &off) in block_starts.iter().enumerate() {
        block_index.insert(off, i);
    }

    b.reset();

    // flags computed lazily by 32 bit code are materialised once on entry
    b.load_fixed_i32(flags_changed as u32);
    b.if_void();
    b.call_fn0("j64_materialize_flags");
    b.block_end();

    let n_local = b.new_local();
    let pc = b.new_local();
    let t_ea = b.new_local_i64();
    let t_v = b.new_local_i64();
    let t_a = b.new_local_i64();
    let loop_label = b.loop_void();

    let mut g = Gen { b, start: rip, loop_label, n: 0, n_local, pc, t_ea, t_v, t_a, cur: rip, next: rip, block_index };

    // budget: return to the dispatcher now and then so that interrupts can be handled
    g.b.get_local(&g.n_local);
    g.b.const_i32(LOOP_LIMIT);
    g.b.geu_i32();
    g.b.if_void();
    {
        // continue at the basic block that was about to run
        g.exit_dynamic_pc(&block_starts);
    }
    g.b.block_end();

    // dispatch: nested blocks, br_table to the end of the block whose code follows
    let count = block_starts.len();
    let labels: Vec<Label> = (0..count).map(|_| g.b.block_void()).collect();
    // labels[0] was opened first (outermost); br_table index i must reach the block whose end is
    // followed by the code of basic block i, i.e. the innermost for i = 0
    let ordered: Vec<Label> = labels.iter().rev().copied().collect();
    g.b.get_local(&g.pc);
    g.b.brtable(ordered[count - 1], &mut ordered.iter());

    let mut ended = false;
    let mut total = 0u32;
    let offsets: Vec<usize> = decoded.keys().copied().collect();
    for &off in offsets.iter() {
        let (insn, len) = decoded[&off];
        if g.block_index.contains_key(&off) {
            // start of a basic block: its code follows the end of the matching wasm block
            g.b.block_end();
            g.n = 0;
            ended = false;
        }
        let cur = rip.wrapping_add(off as u64);
        let next = cur.wrapping_add(len as u64);
        g.cur = cur;
        g.next = next;
        g.n += 1;
        total += 1;

        match insn {
            Insn::Jcc { cc, target } => {
                g.set_previous_ip();
                g.cond(cc);
                g.b.if_void();
                g.transfer(target);
                g.b.block_end();
            },
            Insn::Jmp { target } => {
                g.transfer(target);
                ended = true;
            },
            Insn::Call { target } => {
                g.set_previous_ip();
                g.get_reg(4, 64, true);
                g.b.const_i64(8);
                g.b.sub_i64();
                g.b.set_local_i64(&g.t_ea);
                g.b.get_local_i64(&g.t_ea);
                g.b.const_i64(next as i64);
                g.b.const_i32(64);
                g.b.call_fn3_i64_i64_i32("j64_write");
                g.fault_check();
                g.smc_check();
                g.b.get_local_i64(&g.t_ea);
                g.set_reg(4, 64, true);
                let n = g.n;
                g.exit_const(target, n);
                ended = true;
            },
            Insn::Ret { adj } => {
                g.set_previous_ip();
                g.get_reg(4, 64, true);
                g.b.set_local_i64(&g.t_ea);
                g.b.get_local_i64(&g.t_ea);
                g.read_mem(64);
                g.fault_check();
                g.b.set_local_i64(&g.t_v);
                g.b.get_local_i64(&g.t_ea);
                g.b.const_i64(8 + adj as i64);
                g.b.add_i64();
                g.set_reg(4, 64, true);
                g.b.get_local_i64(&g.t_v);
                g.b.const_i32(0);
                g.b.call_fn2_i64_i32("j64_set_rip");
                g.fault_check();
                g.exit_keep_rip();
                ended = true;
            },
            _ => g.emit(&insn),
        }

        // what follows this instruction?
        let follows = off + len;
        let terminator = matches!(insn, Insn::Jmp { .. } | Insn::Call { .. } | Insn::Ret { .. });
        if !terminator {
            if !decoded.contains_key(&follows) {
                // the path ends here: continue in the interpreter
                let n = g.n;
                g.exit_const(next, n);
                ended = true;
            }
            else if g.block_index.contains_key(&follows) {
                // falls through into the next basic block (its code is emitted right after)
                g.b.get_local(&g.n_local);
                g.b.const_i32(g.n);
                g.b.add_i32();
                g.b.set_local(&g.n_local);
            }
        }
        let _ = ended;
    }
    // after the last instruction: the final wasm block of the dispatcher is closed by its code
    // (all `count` blocks have been closed by the loop above, one per basic block)

    let Gen { b, n_local, pc, t_ea, t_v, t_a, .. } = g;
    b.block_end(); // loop
    b.free_local(n_local);
    b.free_local(pc);
    b.free_local_i64(t_ea);
    b.free_local_i64(t_v);
    b.free_local_i64(t_a);
    b.finish();
    Some(total)
}

// ---------------------------------------------------------------------------------------------
// block cache and dispatcher

/// Runs one step of 64-bit code: a compiled block if there is one at rip, otherwise one
/// interpreted instruction
pub unsafe fn run() {
    if CHECK {
        CHECK = false;
        if try_run_block() {
            CHECK = true;
            return;
        }
    }
    let before = rip();
    long_mode::step64();
    let after = rip();
    // a jump (or an exception): rip is a potential block start. In testing mode (threshold 0)
    // every instruction is a potential block start.
    if after.wrapping_sub(before) > 15 || THRESHOLD == 0 {
        CHECK = true;
    }
}

unsafe fn call_block(index: u16, phys_page: u32) {
    FLAGS[FAULT] = 0;
    FLAGS[SMC] = 0;
    STAT_RUN += 1;
    EXECUTING_PAGE = phys_page;
    call_jit_table(index);
    EXECUTING_PAGE = u32::MAX;
    FLAGS[FAULT] = 0;
}

unsafe fn try_run_block() -> bool {
    if THRESHOLD == u32::MAX || jit::jit_disabled() {
        return false;
    }
    let r = rip();
    let user = *cpl == 3;
    let st = state();
    let slot = &mut st.slots[slot_index(r)];
    if slot.rip != r {
        *slot = Slot { rip: r, ..SLOT_EMPTY };
    }
    match slot.state {
        SLOT_READY => {
            if slot.user != user {
                return false;
            }
            if slot.epoch != EPOCH {
                // mappings may have changed: make sure rip still maps to the compiled page
                match probe_fetch(r, user) {
                    Some(phys) if phys >> 12 == slot.phys_page => slot.epoch = EPOCH,
                    _ => {
                        *slot = SLOT_EMPTY;
                        remove_block(r);
                        return false;
                    },
                }
            }
            call_block(slot.index, slot.phys_page);
            true
        },
        SLOT_COUNTING => {
            slot.count += 1;
            if slot.count < THRESHOLD {
                return false;
            }
            let backoff = st.invalidations.get(&(r >> 12)).copied().unwrap_or(0) as u32;
            if backoff != 0 && slot.count < THRESHOLD.saturating_mul(1 << backoff.min(12)) {
                return false;
            }
            slot.count = 0;
            // an evicted block, or a new one
            if !st.blocks.contains_key(&r) {
                compile(r, user);
            }
            refill_slot(r);
            if THRESHOLD == 0 {
                // testing mode: run the block right away (needs synchronous compilation)
                let slot = &st.slots[slot_index(r)];
                if slot.rip == r && slot.state == SLOT_READY && slot.user == user {
                    call_block(slot.index, slot.phys_page);
                    return true;
                }
            }
            false
        },
        _ => false,
    }
}

/// copies the state of the block at `r` (if any) to its slot
unsafe fn refill_slot(r: u64) {
    let st = state();
    let Some(b) = st.blocks.get(&r) else { return };
    let slot = &mut st.slots[slot_index(r)];
    if slot.rip != r {
        return;
    }
    slot.phys_page = b.phys_page;
    slot.user = b.user;
    slot.index = b.index;
    slot.epoch = b.epoch;
    slot.state = match b.state {
        BlockState::Pending => SLOT_PENDING,
        BlockState::Ready => SLOT_READY,
        BlockState::Failed => SLOT_FAILED,
    };
}

unsafe fn probe_fetch(r: u64, user: bool) -> Option<u32> { crate::cpu::long_mem::probe_fetch(r, user) }

unsafe fn compile(r: u64, user: bool) {
    let Some(phys) = probe_fetch(r, user) else { return };
    let page = phys >> 12;
    let offset = (phys & 0xFFF) as usize;
    let code = std::slice::from_raw_parts(crate::cpu::memory::mem8.offset(phys as isize), 0x1000 - offset);
    // the block lives in a single page, rip and physical address agree on the page offset
    let st = state();
    let registered = jit::jit64_compile(phys, |builder| generate(builder, r, code), |index| {
        // registered before the module is handed to the host, which may finish synchronously
        STAT_COMPILED += 1;
        st.blocks.insert(r, Block { state: BlockState::Pending, phys_page: page, user, index, epoch: EPOCH });
        st.by_index.insert(index, r);
        st.page_blocks.entry(page).or_default().push(r);
        jit::maybe_has_code_set(Page::of_u32(page));
        crate::cpu::cpu::tlb_set_has_code(Page::of_u32(page), true);
    });
    if registered.is_none() {
        STAT_FAILED += 1;
        let st = state();
        st.blocks.insert(r, Block { state: BlockState::Failed, phys_page: page, user, index: 0, epoch: EPOCH });
        st.page_blocks.entry(page).or_default().push(r);
        jit::maybe_has_code_set(Page::of_u32(page));
    }
}

/// Called when the asynchronous compilation of a module has finished
pub unsafe fn finalize_finished(index: u16) {
    let st = state();
    if st.zombies.remove(&index) {
        // the block was invalidated while it was compiling: the module is useless
        jit::jit64_free_index(index);
        return;
    }
    if let Some(&r) = st.by_index.get(&index) {
        if let Some(b) = st.blocks.get_mut(&r) {
            if b.state == BlockState::Pending && b.index == index {
                b.state = BlockState::Ready;
                refill_slot(r);
            }
        }
    }
}

unsafe fn forget_slot(r: u64) {
    let slot = &mut state().slots[slot_index(r)];
    if slot.rip == r {
        *slot = SLOT_EMPTY;
    }
}

unsafe fn remove_block(r: u64) {
    let st = state();
    if let Some(b) = st.blocks.remove(&r) {
        match b.state {
            BlockState::Failed => {},
            BlockState::Pending => {
                st.by_index.remove(&b.index);
                st.zombies.insert(b.index);
            },
            BlockState::Ready => {
                st.by_index.remove(&b.index);
                jit::jit64_free_index(b.index);
            },
        }
    }
    forget_slot(r);
}

/// A write hit this physical page: all blocks that were compiled from it become invalid
pub unsafe fn dirty_page(ctx: &mut jit::JitState, page: Page) -> bool {
    let st = state();
    let p = page.to_u32();
    let mut had = false;
    if let Some(list) = st.page_blocks.remove(&p) {
        had = true;
        if EXECUTING_PAGE == p {
            FLAGS[SMC] = 1;
        }
        let mut counted: Vec<u64> = vec![];
        for r in list {
            forget_slot(r);
            if !counted.contains(&(r >> 12)) {
                counted.push(r >> 12);
                let n = st.invalidations.entry(r >> 12).or_insert(0);
                *n = n.saturating_add(1);
            }
            if let Some(b) = st.blocks.remove(&r) {
                STAT_INVALIDATED += 1;
                match b.state {
                    BlockState::Failed => {},
                    BlockState::Pending => {
                        st.by_index.remove(&b.index);
                        st.zombies.insert(b.index);
                    },
                    BlockState::Ready => {
                        st.by_index.remove(&b.index);
                        jit::jit64_free_index_ctx(ctx, b.index);
                    },
                }
            }
        }
    }
    had
}

/// Removes every block (the wasm table is being cleared)
pub unsafe fn clear_all(ctx: &mut jit::JitState) {
    let pages: Vec<u32> = state().page_blocks.keys().copied().collect();
    for p in pages {
        dirty_page(ctx, Page::of_u32(p));
    }
}
