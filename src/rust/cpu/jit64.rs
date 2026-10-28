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
const SLOTS: usize = 65536;

fn slot_index(r: u64) -> usize { (r ^ r >> 12 ^ r >> 24) as usize & (SLOTS - 1) }

struct State {
    blocks: HashMap<u64, Block, PageBuildHasher>,
    by_index: HashMap<u16, u64, PageBuildHasher>,
    page_blocks: HashMap<u32, Vec<u64>, PageBuildHasher>,
    slots: Vec<Slot>,
    /// table indices of invalidated modules that are still being compiled by the host: the index
    /// must not be reused before the host is done with it
    zombies: HashSet<u16>,
    /// debugging (VERIFY): modules whose blocks modify memory they read
    rmw: HashSet<u16>,
    /// per virtual page: how often blocks of it were invalidated (code mixed with data is not
    /// worth compiling again and again)
    invalidations: HashMap<u64, u8, PageBuildHasher>,
}

static mut STATE: Option<State> = None;
static mut CHECK: bool = true;
static mut EPOCH: u32 = 1;
static mut THRESHOLD: u32 = 150;
static mut EXECUTING_PAGE: u32 = u32::MAX;
/// bit i set: instructions of kind i are not compiled (for bisecting problems); bit 30: flags are never elided
pub static mut DISABLE_MASK: u32 = 0;
pub static mut STAT_COMPILED: u32 = 0;
pub static mut STAT_FAILED: u32 = 0;
/// Debug histogram of the opcodes that ended block discovery: index = opcode, 256 + byte after 0F
pub static mut UNSUPPORTED: [u32; 512] = [0; 512];

/// Debug histogram of the opcodes the interpreter executed (same indexing), when HIST_ON
pub static mut EXECUTED: [u32; 512] = [0; 512];
pub static mut HIST_ON: bool = false;
/// interpreted instructions by why they were interpreted: 0 continuation after a non-jump (no block lookup), 1 slot counting (not hot yet), 2 failed block, 3 other/pending
pub static mut INTERP_WHY: [u32; 8] = [0; 8];
/// debugging: after every compiled block run, redo it in the interpreter from the saved state and
/// record the first differences (read through get_jit_config(5000 + i)); the interpreter's result is kept
pub static mut VERIFY: bool = false;
/// the block generate() produced last contains an instruction that modifies memory it reads
static mut GEN_RMW: bool = false;
/// VERIFY: memory writes of the compiled block just run: (address, bits, old value, new value)
static mut UNDO_ON: bool = false;
static mut UNDO: Vec<(u64, u32, u64)> = Vec::new();
/// 0: verified blocks, 1: mismatches, then per mismatch (up to 4): block rip lo/hi, insns, what (reg index or 100 flags / 101 rip), jit lo/hi, interp lo/hi
pub static mut VERIFY_LOG: [u32; 64] = [0; 64];
/// bisecting: only blocks that start in [RANGE_LO, RANGE_HI) are compiled (default: all)
pub static mut RANGE_LO: u64 = 0;
pub static mut RANGE_HI: u64 = u64::MAX;
/// start addresses (low 32 bits) of the first compiled blocks, in order
pub static mut COMPILED_RIPS: [u32; 64] = [0; 64];
/// instructions of compiled blocks run back to back without leaving run() (experiment knob, 0 = off)
pub static mut CHAIN_BUDGET: u32 = 0;
/// compute pending flags inline in the generated code instead of calling j64_flags (experiment knob)
pub static mut INLINE_FLAGS: bool = true;
/// Debug: interpreted instruction addresses, hashed: (rip, count); read through get_jit_config(3000 + 2 * i / + 1)
pub static mut HIST_RIPS: [(u64, u32); 4096] = [(0, 0); 4096];

fn count_unsupported(code: &[u8]) {
    let mut i = 0;
    while i < code.len() && matches!(code[i], 0x66 | 0x67 | 0xF0 | 0xF2 | 0xF3 | 0x26 | 0x2E | 0x36 | 0x3E | 0x64 | 0x65 | 0x40..=0x4F) {
        i += 1;
    }
    let Some(&op) = code.get(i) else { return };
    let key = if op == 0x0F { 256 + *code.get(i + 1).unwrap_or(&0) as usize } else { op as usize };
    unsafe { UNSUPPORTED[key] = UNSUPPORTED[key].saturating_add(1) }
}
/// inline tlb lookups in generated code (set false to call the helpers for every access)
static mut INLINE_TLB_OFF: bool = false;
pub static mut STAT_RUN: u32 = 0;
/// instructions retired inside compiled blocks (wraps)
pub static mut STAT_JIT_INSNS: u32 = 0;
pub static mut STAT_INVALIDATED: u32 = 0;

pub unsafe fn set_threshold(t: u32) { THRESHOLD = t }
pub unsafe fn set_disable_mask(m: u32) { DISABLE_MASK = m }
pub unsafe fn threshold() -> u32 { THRESHOLD }
pub unsafe fn set_inline_tlb_off(v: bool) { INLINE_TLB_OFF = v }

#[allow(static_mut_refs)]
unsafe fn state() -> &'static mut State {
    if STATE.is_none() {
        STATE = Some(State {
            blocks: HashMap::default(),
            by_index: HashMap::default(),
            page_blocks: HashMap::default(),
            slots: vec![SLOT_EMPTY; SLOTS],
            zombies: HashSet::new(),
            rmw: HashSet::new(),
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
    crate::cpu::long_mem::count_slow_access(addr as u64, bits as u32, false, *cpl == 3);
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
    if UNDO_ON {
        if let Ok(old) = mem_read(addr as u64, bits as u32) {
            #[allow(static_mut_refs)]
            UNDO.push((addr as u64, bits as u32, old));
        }
    }
    crate::cpu::long_mem::count_slow_access(addr as u64, bits as u32, true, *cpl == 3);
    if mem_write(addr as u64, bits as u32, v as u64).is_err() {
        FLAGS[FAULT] = 1;
    }
}

/// Runs the one instruction at rip in the interpreter (an Insn::Interp of a block) and tells
/// whether the block has to be left: 1 when rip is not `next` (exception, jump, mode switch), when
/// interrupts were just enabled, the cpu halted or the mode changed.
#[no_mangle]
pub unsafe fn j64_interp1(next: i64, _unused: i32) -> i32 {
    // the block counts this instruction itself
    let counter = *instruction_counter;
    long_mode::step64();
    *instruction_counter = counter;
    let stay = rip() == next as u64 && !interrupt_shadow && !*in_hlt && *is_64 && *flags & FLAG_TRAP == 0;
    !stay as i32
}

#[no_mangle]
pub unsafe fn j64_set_rip(target: i64, _unused: i32) {
    if long_mode::jit_set_rip(target as u64).is_err() {
        FLAGS[FAULT] = 1;
    }
}

/// Computes the flags of an add (kind 0), logic operation (kind 1) or sub / cmp (kind 2) from the
/// operands and the result (all masked to `bits`)
#[no_mangle]
pub unsafe fn j64_flags(kind_bits: i32, a: i64, b: i64, r: i64) {
    let bits = (kind_bits & 0xFF) as u32;
    let kind = kind_bits >> 8;
    let (a, b, r) = (a as u64, b as u64, r as u64);
    let msb = bits - 1;
    if kind == 3 {
        // carry flag only
        *flags = *flags & !FLAG_CARRY | (r as i32 & 1) * FLAG_CARRY;
        *flags_changed = 0;
        return;
    }
    let (cf, of) = match kind {
        0 => (r < a, ((a ^ r) & (b ^ r)) >> msb & 1 != 0),
        1 => (false, false),
        _ => (a < b, ((a ^ b) & (a ^ r)) >> msb & 1 != 0),
    };
    let af = kind != 1 && (a ^ b ^ r) & 0x10 != 0;
    long_mode::jit_set_flags(cf, af, of, r, bits);
}

/// string instructions that are not compiled inline (rep prefix, compares)
#[no_mangle]
pub unsafe fn j64_str(code: i32, info: i32) {
    let opcode = (code & 0xFF) as u8;
    let rep = (code >> 8 & 0xFF) as u8;
    let asz32 = code >> 16 & 1 != 0;
    let bits = (info & 0xFF) as u32;
    let seg = (info >> 8) - 1;
    if long_mode::jit_string_op(opcode, rep, bits, asz32, seg).is_err() {
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
    /// the count is in cl
    Cl,
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
    /// bt (0), bts (1), btr (2), btc (3)
    Bt { op: u8, bits: u32, dst: Opd, idx: Opd },
    /// movs, cmps, stos, lods, scas (with or without rep)
    Str { opcode: u8, rep: u8, bits: u32, asz32: bool, seg: i32 },
    Neg { bits: u32, dst: Opd },
    Not { bits: u32, dst: Opd },
    Imul { bits: u32, dst: u8, a: Opd, b: Opd },
    CallInd { src: Opd },
    JmpInd { src: Opd },
    Xchg { bits: u32, a: Opd, b: Opd },
    /// cbw / cwde / cdqe (0x98) and cwd / cdq / cqo (0x99)
    Sext { opcode: u8, bits: u32 },
    /// an instruction the jit cannot compile: the interpreter runs just this one from inside the block
    Interp,
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
        Insn::Bt { .. } => 17,
        Insn::Str { .. } => 18,
        Insn::Neg { .. } | Insn::Not { .. } => 19,
        Insn::Imul { .. } => 20,
        Insn::CallInd { .. } | Insn::JmpInd { .. } => 21,
        Insn::Xchg { .. } => 22,
        Insn::Sext { .. } => 23,
        Insn::Interp => 24,
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
    let mut rep = 0u8;
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
            // rep prefixes change the meaning of many instructions: only string instructions
            // accept them (checked below)
            0xF2 | 0xF3 => rep = b,
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
            if rep != 0 {
                return None;
            }
            if p.b() != 0 {
                // xchg r8, rax
                Insn::Xchg { bits: osz, a: Opd::Reg(0, true), b: Opd::Reg(8, true) }
            }
            else {
                Insn::Nop
            }
        },
        0x91..=0x97 => Insn::Xchg { bits: osz, a: Opd::Reg(0, true), b: Opd::Reg((opcode & 7) | p.b(), true) },
        0x86 | 0x87 => {
            let bits = if opcode == 0x86 { 8 } else { osz };
            let m = modrm(&mut c, &p)?;
            Insn::Xchg { bits, a: m.rm, b: Opd::Reg(m.reg, p.has_rex) }
        },
        0x98 | 0x99 => Insn::Sext { opcode, bits: osz },
        0x69 | 0x6B => {
            let m = modrm(&mut c, &p)?;
            let imm = if opcode == 0x6B {
                sign_extend(c.u8()? as u64, 8)
            }
            else if osz == 16 {
                c.u16()?
            }
            else {
                sign_extend(c.u32()?, 32)
            };
            Insn::Imul { bits: osz, dst: m.reg, a: m.rm, b: Opd::Imm(imm) }
        },
        0xA4 | 0xA5 | 0xAA | 0xAB | 0xAC | 0xAD | 0xA6 | 0xA7 | 0xAE | 0xAF => {
            let bits = if opcode & 1 == 0 { 8 } else { osz };
            Insn::Str { opcode, rep, bits, asz32: p.asz32, seg: p.seg }
        },
        0xF6 | 0xF7 => {
            let bits = if opcode == 0xF6 { 8 } else { osz };
            let m = modrm(&mut c, &p)?;
            match m.reg & 7 {
                0 | 1 => {
                    let imm = match bits {
                        8 => c.u8()? as u64,
                        16 => c.u16()?,
                        _ => sign_extend(c.u32()?, 32),
                    };
                    Insn::Test { bits, a: m.rm, b: Opd::Imm(imm) }
                },
                2 => Insn::Not { bits, dst: m.rm },
                3 => Insn::Neg { bits, dst: m.rm },
                _ => return None,
            }
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
        0xD2 | 0xD3 => {
            let bits = if opcode & 1 == 0 { 8 } else { osz };
            let m = modrm(&mut c, &p)?;
            Insn::Shift { op: m.reg & 7, bits, dst: m.rm, count: Count::Cl }
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
                // call, jmp and push r/m (operand size 64, or 16 for push)
                2 if opcode == 0xFF => Insn::CallInd { src: m.rm },
                4 if opcode == 0xFF => Insn::JmpInd { src: m.rm },
                6 if opcode == 0xFF => Insn::Push { bits: if p.osz16 { 16 } else { 64 }, src: m.rm },
                _ => return None,
            }
        },
        0x0F => {
            let op2 = c.u8()?;
            match op2 {
                // multi-byte nop and the hint nops (endbr64 is f3 0f 1e fa)
                0x1E | 0x1F => {
                    modrm(&mut c, &p)?;
                    Insn::Nop
                },
                0xAF => {
                    let m = modrm(&mut c, &p)?;
                    Insn::Imul { bits: osz, dst: m.reg, a: Opd::Reg(m.reg, true), b: m.rm }
                },
                0xA3 | 0xAB | 0xB3 | 0xBB => {
                    let m = modrm(&mut c, &p)?;
                    let op = match op2 {
                        0xA3 => 0,
                        0xAB => 1,
                        0xB3 => 2,
                        _ => 3,
                    };
                    Insn::Bt { op, bits: osz, dst: m.rm, idx: Opd::Reg(m.reg, true) }
                },
                0xBA => {
                    let m = modrm(&mut c, &p)?;
                    let imm = c.u8()? as u64;
                    if m.reg & 7 < 4 {
                        return None;
                    }
                    Insn::Bt { op: (m.reg & 7) - 4, bits: osz, dst: m.rm, idx: Opd::Imm(imm) }
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

    if rep != 0 && !matches!(insn, Insn::Str { .. } | Insn::Nop) {
        return None;
    }
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
        Insn::Bt { op, bits, dst, idx } => Insn::Bt { op, bits, dst: fix(dst), idx },
        Insn::Neg { bits, dst } => Insn::Neg { bits, dst: fix(dst) },
        Insn::Not { bits, dst } => Insn::Not { bits, dst: fix(dst) },
        Insn::Imul { bits, dst, a, b } => Insn::Imul { bits, dst, a: fix(a), b: fix(b) },
        Insn::CallInd { src } => Insn::CallInd { src: fix(src) },
        Insn::JmpInd { src } => Insn::JmpInd { src: fix(src) },
        Insn::Xchg { bits, a, b } => Insn::Xchg { bits, a: fix(a), b: fix(b) },
        Insn::Push { bits, src } => Insn::Push { bits, src: fix(src) },
        i => i,
    };
    if unsafe { DISABLE_MASK } >> kind(&insn) & 1 != 0 {
        return None;
    }
    Some((insn, len))
}

// ---------------------------------------------------------------------------------------------
// flag liveness

fn has_mem(o: &Opd) -> bool { matches!(o, Opd::Mem(_)) }

struct FlagFx {
    /// reads the arithmetic flags
    reads: bool,
    /// overwrites all of them
    writes_all: bool,
    /// can raise an exception (which needs the flags of the previous instructions in memory)
    fault: bool,
}

fn flag_fx(i: &Insn) -> FlagFx {
    let (reads, writes_all, fault) = match *i {
        Insn::Alu { op, dst, src, .. } => (op == 2 || op == 3, true, has_mem(&dst) || has_mem(&src)),
        Insn::Test { a, b, .. } => (false, true, has_mem(&a) || has_mem(&b)),
        Insn::Mov { dst, src, .. } => (false, false, has_mem(&dst) || has_mem(&src)),
        Insn::MovExt { src, .. } => (false, false, has_mem(&src)),
        Insn::Lea { .. } | Insn::Nop | Insn::Jmp { .. } => (false, false, false),
        Insn::Push { .. } | Insn::Pop { .. } | Insn::Leave { .. } | Insn::Call { .. } | Insn::Ret { .. } => (false, false, true),
        Insn::IncDec { dst, .. } => (false, false, has_mem(&dst)),
        // by cl the flags stay as they were when the count is zero: always reads them
        Insn::Shift { op, dst, count, .. } => (op == 2 || op == 3 || matches!(count, Count::Cl), false, has_mem(&dst)),
        Insn::Setcc { dst, .. } => (true, false, has_mem(&dst)),
        Insn::Cmov { src, .. } => (true, false, has_mem(&src)),
        Insn::Jcc { .. } => (true, false, false),
        // only the carry flag changes: the rest stays as it was
        Insn::Bt { dst, .. } => (false, false, has_mem(&dst)),
        Insn::Str { .. } => (false, false, true),
        Insn::Neg { dst, .. } => (false, true, has_mem(&dst)),
        Insn::Not { dst, .. } => (false, false, has_mem(&dst)),
        Insn::Imul { a, b, .. } => (false, true, has_mem(&a) || has_mem(&b)),
        Insn::CallInd { .. } => (false, false, true),
        Insn::JmpInd { src } => (false, false, has_mem(&src)),
        Insn::Xchg { a, b, .. } => (false, false, has_mem(&a) || has_mem(&b)),
        Insn::Sext { .. } => (false, false, false),
        Insn::Interp => (true, false, true),
    };
    FlagFx { reads, writes_all, fault }
}

/// writes memory in a way the VERIFY undo log does not see (string instructions run through the interpreter)
/// Length of an instruction the jit cannot compile but can hand to the interpreter one at a time
/// (system and flag instructions, cmpxchg / xadd / bsf / bsr, the SSE moves and logic ops)
fn interp_len(code: &[u8]) -> Option<usize> {
    let mut c = Cursor { code, pos: 0 };
    let mut p = Pfx { rex: 0, has_rex: false, osz16: false, asz32: false, seg: -1 };
    let mut rep = false;
    let opcode = loop {
        let b = c.u8()?;
        match b {
            0x66 => p.osz16 = true,
            0x67 => p.asz32 = true,
            0xF2 | 0xF3 => rep = true,
            0xF0 | 0x26 | 0x2E | 0x36 | 0x3E => {},
            0x64 => p.seg = FS,
            0x65 => p.seg = GS,
            0x40..=0x4F => {
                p.rex = b;
                p.has_rex = true;
                break c.u8()?;
            },
            _ => break b,
        }
    };
    match opcode {
        // pushf popf cli sti hlt clc..std pause iretq in/out dx
        0x9C | 0x9D | 0xFA | 0xFB | 0xF4 | 0xF8..=0xFD | 0xCF | 0xEC..=0xEF => {},
        // pause (a plain 0x90 is a nop that decode knows)
        0x90 if rep => {},
        0xE4..=0xE7 => { c.u8()?; },
        0xF6 | 0xF7 => {
            // mul imul div idiv (test, not and neg are decoded)
            let m = modrm(&mut c, &p)?;
            if m.reg & 7 < 4 {
                return None;
            }
        },
        0x0F => {
            let op2 = c.u8()?;
            match op2 {
                // syscall sysret ud2 wrmsr rdtsc rdmsr sysenter sysexit cpuid
                0x05 | 0x07 | 0x0B | 0x30 | 0x31 | 0x32 | 0x34 | 0x35 | 0xA2 => {},
                0x01 | 0x20..=0x23 | 0x6E | 0x6F | 0x7E | 0x7F | 0xD6 | 0xE7 | 0xEF | 0xDB | 0xDF | 0xEB
                | 0xF8..=0xFE | 0xD4 | 0x10..=0x17 | 0x28 | 0x29 | 0x2B | 0x54..=0x5F | 0x60..=0x6D | 0x74..=0x76 | 0xD0..=0xD5
                | 0xD8..=0xDA | 0xDC | 0xDD | 0xE0..=0xE6 | 0xE8..=0xEA | 0xEC..=0xEE | 0xF1..=0xF7 | 0xAE | 0xB0 | 0xB1
                | 0xBC | 0xBD | 0xC0 | 0xC1 | 0xC3 => { modrm(&mut c, &p)?; },
                0x70..=0x73 | 0xC2 | 0xC4..=0xC6 => { modrm(&mut c, &p)?; c.u8()?; },
                _ => return None,
            }
        },
        _ => return None,
    }
    Some(c.pos)
}

fn is_rmw(i: &Insn) -> bool {
    match *i {
        Insn::Str { .. } | Insn::Interp => true,
        _ => false,
    }
}

/// For every instruction: are the flags it leaves behind (possibly) read afterwards?
fn flag_liveness(
    decoded: &std::collections::BTreeMap<usize, (Insn, usize)>,
    block_starts: &[usize],
    rip: u64,
    code_len: usize,
) -> (HashMap<usize, bool>, HashMap<usize, bool>) {
    let internal = |target: u64| -> Option<usize> {
        let off = target.wrapping_sub(rip);
        if target >= rip && (off as usize) < code_len { Some(off as usize) } else { None }
    };
    // instructions of each block
    let mut blocks: Vec<Vec<usize>> = vec![];
    for (i, &start) in block_starts.iter().enumerate() {
        let end = block_starts.get(i + 1).copied().unwrap_or(usize::MAX);
        blocks.push(decoded.range(start..end).map(|(&o, _)| o).collect());
    }
    let mut live_in_block: HashMap<usize, bool> = block_starts.iter().map(|&o| (o, false)).collect();
    let mut live_out: HashMap<usize, bool> = HashMap::new();
    loop {
        let mut changed = false;
        for (bi, offs) in blocks.iter().enumerate().rev() {
            let &last = offs.last().unwrap();
            let (insn, len) = decoded[&last];
            let succ_live = |t: usize| -> bool {
                if decoded.contains_key(&t) { live_in_block.get(&t).copied().unwrap_or(true) } else { true }
            };
            let mut live = match insn {
                Insn::Jcc { target, .. } => {
                    let taken = match internal(target) { Some(t) => succ_live(t), None => true };
                    taken || succ_live(last + len)
                },
                Insn::Jmp { target } => match internal(target) { Some(t) => succ_live(t), None => true },
                Insn::Call { .. } | Insn::Ret { .. } | Insn::CallInd { .. } | Insn::JmpInd { .. } => true,
                _ => succ_live(last + len),
            };
            for &o in offs.iter().rev() {
                let (insn, _) = decoded[&o];
                live_out.insert(o, live);
                let fx = flag_fx(&insn);
                live = fx.reads || fx.fault || (live && !fx.writes_all);
            }
            if live_in_block[&block_starts[bi]] != live {
                live_in_block.insert(block_starts[bi], live);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    // bit 30 of the disable mask: assume all flags live everywhere (bisects flag-elision bugs)
    if unsafe { DISABLE_MASK } >> 30 & 1 != 0 {
        for v in live_out.values_mut() { *v = true }
        for v in live_in_block.values_mut() { *v = true }
    }
    (live_out, live_in_block)
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
    /// inline tlb lookups: the address, the value to store, the page number and the tlb entry address
    t_fa: WasmLocalI64,
    t_fv: WasmLocalI64,
    t_fp: WasmLocalI64,
    t_fe: WasmLocal,
    cur: u64,
    next: u64,
    /// offset of a basic block (from the start of the region) -> its index
    block_index: HashMap<usize, usize>,
    /// the flags of the last flag-writing instruction have not been stored yet: their operands
    /// are in fl_a / fl_b / fl_r
    pending: Option<Pending>,
    fl_a: WasmLocalI64,
    fl_b: WasmLocalI64,
    fl_r: WasmLocalI64,
    /// are the flags left by the instruction at an offset read afterwards? / by a block start
    live_out: HashMap<usize, bool>,
    live_in: HashMap<usize, bool>,
    /// flags are needed after the current instruction
    cur_live_out: bool,
    smc_needed: bool,
}

#[derive(Clone, Copy)]
struct Pending {
    /// 0 add, 1 logic operation, 2 sub / cmp
    kind: u8,
    bits: u32,
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
        self.materialize();
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

    /// a write happened: the check for changed code runs at the end of the instruction (the
    /// instruction has to complete first)
    fn smc_check(&mut self) { self.smc_needed = true; }

    fn finish_insn(&mut self) {
        if self.smc_needed {
            self.smc_needed = false;
            self.smc_exit();
        }
    }

    /// if a write changed code of this block, continue in the interpreter
    fn smc_exit(&mut self) {
        self.b.load_fixed_i32(unsafe { &raw const FLAGS[SMC] } as u32);
        self.b.if_void();
        let next = self.next;
        // the exit stores pending flags, the code that follows still has them pending
        let saved = self.pending;
        self.exit_const(next, self.n);
        self.pending = saved;
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

    /// Leaves 1 on the stack when the tlb has an entry for the address in t_fa that allows an access of
    /// `bits` bits inside the page without calling back (see `long_mem::fast_flags`), and sets t_fe
    /// to the address of the entry
    fn tlb_hit(&mut self, bits: u32, write: bool) {
        use crate::cpu::long_mem::*;
        // page number and entry address
        self.b.get_local_i64(&self.t_fa);
        self.b.const_i64(12);
        self.b.shr_u_i64();
        self.b.set_local_i64(&self.t_fp);
        self.b.get_local_i64(&self.t_fp);
        self.b.wrap_i64_to_i32();
        self.b.const_i32(TLB_MASK as i32);
        self.b.and_i32();
        self.b.const_i32(TLB_ENTRY_SHIFT as i32);
        self.b.shl_i32();
        self.b.set_local(&self.t_fe);
        // the entry is for this page (entries only exist for canonical addresses)
        let base = tlb_address();
        self.b.get_local(&self.t_fe);
        self.b.load_unaligned_i64(base);
        self.b.get_local_i64(&self.t_fp);
        self.b.eq_i64();
        // the access is allowed for the current privilege level
        self.b.get_local(&self.t_fe);
        self.b.load_u8(base + TLB_ENTRY_FAST);
        // mask = (supervisor bit) << (cpl == 3): the user bit is the next one
        self.b.const_i32(if write { FAST_WRITE_SUPERVISOR } else { FAST_READ_SUPERVISOR } as i32);
        self.b.load_fixed_u8(cpl as u32);
        self.b.const_i32(3);
        self.b.eq_i32();
        self.b.shl_i32();
        self.b.and_i32();
        self.b.eqz_i32();
        self.b.eqz_i32();
        self.b.and_i32();
        if bits > 8 {
            // does not cross the page
            self.b.get_local_i64(&self.t_fa);
            self.b.wrap_i64_to_i32();
            self.b.const_i32(0xFFF);
            self.b.and_i32();
            self.b.const_i32(0x1000 - bits as i32 / 8);
            self.b.leu_i32();
            self.b.and_i32();
        }
    }

    /// pushes the address in ram of the byte addressed by t_fa (after tlb_hit)
    fn tlb_ram_address(&mut self) {
        use crate::cpu::long_mem::*;
        self.b.get_local(&self.t_fe);
        self.b.load_unaligned_i32(tlb_address() + TLB_ENTRY_PHYS);
        self.b.get_local_i64(&self.t_fa);
        self.b.wrap_i64_to_i32();
        self.b.const_i32(0xFFF);
        self.b.and_i32();
        self.b.or_i32();
    }

    /// [addr on stack] -> value
    fn read_mem(&mut self, bits: u32) {
        if unsafe { INLINE_TLB_OFF } {
            self.b.const_i32(bits as i32);
            self.b.call_fn2_i64_i32_ret_i64("j64_read");
            return;
        }
        let mem = unsafe { crate::cpu::memory::mem8 } as u32;
        self.b.set_local_i64(&self.t_fa);
        self.tlb_hit(bits, false);
        self.b.if_i64();
        {
            self.tlb_ram_address();
            match bits {
                8 => { self.b.load_u8(mem); self.b.extend_unsigned_i32_to_i64(); },
                16 => { self.b.load_unaligned_u16(mem); self.b.extend_unsigned_i32_to_i64(); },
                32 => { self.b.load_unaligned_i32(mem); self.b.extend_unsigned_i32_to_i64(); },
                _ => self.b.load_unaligned_i64(mem),
            }
        }
        self.b.else_();
        {
            self.b.get_local_i64(&self.t_fa);
            self.b.const_i32(bits as i32);
            self.b.call_fn2_i64_i32_ret_i64("j64_read");
        }
        self.b.block_end();
    }

    /// [addr, value on stack] -> (stores the value, faults are reported through the fault flag)
    fn write_mem(&mut self, bits: u32) {
        if unsafe { INLINE_TLB_OFF } {
            self.b.const_i32(bits as i32);
            self.b.call_fn3_i64_i64_i32("j64_write");
            return;
        }
        let mem = unsafe { crate::cpu::memory::mem8 } as u32;
        self.b.set_local_i64(&self.t_fv);
        self.b.set_local_i64(&self.t_fa);
        self.tlb_hit(bits, true);
        self.b.if_void();
        {
            self.tlb_ram_address();
            self.b.get_local_i64(&self.t_fv);
            match bits {
                8 => { self.b.wrap_i64_to_i32(); self.b.store_u8(mem); },
                16 => { self.b.wrap_i64_to_i32(); self.b.store_unaligned_u16(mem); },
                32 => { self.b.wrap_i64_to_i32(); self.b.store_unaligned_i32(mem); },
                _ => self.b.store_unaligned_i64(mem),
            }
        }
        self.b.else_();
        {
            self.b.get_local_i64(&self.t_fa);
            self.b.get_local_i64(&self.t_fv);
            self.b.const_i32(bits as i32);
            self.b.call_fn3_i64_i64_i32("j64_write");
        }
        self.b.block_end();
    }

    /// reads the operand (address is computed anew)
    fn read_opd(&mut self, o: &Opd, bits: u32) {
        match *o {
            Opd::Reg(n, rex) => self.get_reg(n, bits, rex),
            Opd::Imm(v) => {
                let v = if bits < 64 { v & ((1u64 << bits) - 1) } else { v };
                self.b.const_i64(v as i64);
            },
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
                self.write_mem(bits);
                self.fault_check();
                self.smc_check();
            },
            Opd::Imm(_) => unreachable!(),
        }
    }

    /// the same as j64_flags, in wasm: flags = flags & !ARITH | cf | pf | af | zf | sf | of
    fn inline_flags(&mut self, p: Pending) {
        let msb = p.bits as i64 - 1;
        let fl = flags as u32;
        self.b.const_i32(fl as i32);
        self.b.load_fixed_i32(fl);
        if p.kind == 3 {
            // carry flag only: r holds the new carry in bit 0
            self.b.const_i32(!FLAG_CARRY);
            self.b.and_i32();
            self.b.get_local_i64(&self.fl_r);
            self.b.const_i64(1);
            self.b.and_i64();
            self.b.wrap_i64_to_i32();
            self.b.or_i32();
            self.b.store_aligned_i32(0);
        }
        else {
            self.b.const_i32(!FLAGS_ALL);
            self.b.and_i32();
            // pf
            self.b.get_local_i64(&self.fl_r);
            self.b.wrap_i64_to_i32();
            self.b.const_i32(0xFF);
            self.b.and_i32();
            self.b.load_u8((&raw const long_mode::PARITY) as u32);
            self.b.const_i32(2);
            self.b.shl_i32();
            self.b.or_i32();
            // zf
            self.b.get_local_i64(&self.fl_r);
            self.b.eqz_i64();
            self.b.const_i32(6);
            self.b.shl_i32();
            self.b.or_i32();
            // sf
            self.b.get_local_i64(&self.fl_r);
            self.b.const_i64(msb);
            self.b.shr_u_i64();
            self.b.const_i64(1);
            self.b.and_i64();
            self.b.wrap_i64_to_i32();
            self.b.const_i32(7);
            self.b.shl_i32();
            self.b.or_i32();
            if p.kind != 1 {
                // cf
                if p.kind == 0 {
                    self.b.get_local_i64(&self.fl_r);
                    self.b.get_local_i64(&self.fl_a);
                }
                else {
                    // sub / cmp: a < b
                    self.b.get_local_i64(&self.fl_a);
                    self.b.get_local_i64(&self.fl_b);
                }
                self.b.ltu_i64();
                self.b.or_i32();
                // af
                self.b.get_local_i64(&self.fl_a);
                self.b.get_local_i64(&self.fl_b);
                self.b.xor_i64();
                self.b.get_local_i64(&self.fl_r);
                self.b.xor_i64();
                self.b.const_i64(0x10);
                self.b.and_i64();
                self.b.wrap_i64_to_i32();
                self.b.or_i32();
                // of
                if p.kind == 0 {
                    self.b.get_local_i64(&self.fl_a);
                    self.b.get_local_i64(&self.fl_r);
                    self.b.xor_i64();
                    self.b.get_local_i64(&self.fl_b);
                    self.b.get_local_i64(&self.fl_r);
                    self.b.xor_i64();
                }
                else {
                    self.b.get_local_i64(&self.fl_a);
                    self.b.get_local_i64(&self.fl_b);
                    self.b.xor_i64();
                    self.b.get_local_i64(&self.fl_a);
                    self.b.get_local_i64(&self.fl_r);
                    self.b.xor_i64();
                }
                self.b.and_i64();
                self.b.const_i64(msb);
                self.b.shr_u_i64();
                self.b.const_i64(1);
                self.b.and_i64();
                self.b.wrap_i64_to_i32();
                self.b.const_i32(11);
                self.b.shl_i32();
                self.b.or_i32();
            }
            self.b.store_aligned_i32(0);
        }
        self.b.const_i32(flags_changed as i32);
        self.b.const_i32(0);
        self.b.store_aligned_i32(0);
    }

    /// stores the pending flags into memory
    fn materialize(&mut self) {
        if let Some(p) = self.pending.take() {
            if unsafe { INLINE_FLAGS } {
                self.inline_flags(p);
                return;
            }
            self.b.const_i32((p.kind as i32) << 8 | p.bits as i32);
            self.b.get_local_i64(&self.fl_a);
            self.b.get_local_i64(&self.fl_b);
            self.b.get_local_i64(&self.fl_r);
            self.b.call_fn4_i32_i64_i64_i64("j64_flags");
        }
    }

    /// pushes a local, sign extended from `bits`
    fn get_signed(&mut self, which: u8, bits: u32) {
        match which {
            0 => self.b.get_local_i64(&self.fl_a),
            1 => self.b.get_local_i64(&self.fl_b),
            _ => self.b.get_local_i64(&self.fl_r),
        }
        if bits < 64 {
            self.b.const_i64(64 - bits as i64);
            self.b.shl_i64();
            self.b.const_i64(64 - bits as i64);
            self.b.shr_s_i64();
        }
    }

    /// Emits condition `cc` straight from the pending flags if that is possible (pushes an i32)
    fn cond_pending(&mut self, cc: u8) -> bool {
        let Some(p) = self.pending else { return false };
        let bits = p.bits;
        let base = cc >> 1;
        let invert = cc & 1 != 0;
        // each case leaves the i32 for the non-inverted condition
        match (p.kind, base) {
            // carry flag only (bt family): the other flags are in memory
            (3, 1) => {
                self.b.get_local_i64(&self.fl_r);
                self.b.wrap_i64_to_i32();
            },
            (3, _) => return false,
            // zero / sign
            (_, 2) => {
                self.b.get_local_i64(&self.fl_r);
                self.b.eqz_i64();
            },
            (_, 4) => {
                self.get_signed(2, bits);
                self.b.const_i64(0);
                self.b.lt_i64();
            },
            // sub / cmp
            (2, 1) => {
                self.b.get_local_i64(&self.fl_a);
                self.b.get_local_i64(&self.fl_b);
                self.b.ltu_i64();
            },
            (2, 3) => {
                self.b.get_local_i64(&self.fl_a);
                self.b.get_local_i64(&self.fl_b);
                self.b.leu_i64();
            },
            (2, 6) => {
                self.get_signed(0, bits);
                self.get_signed(1, bits);
                self.b.lt_i64();
            },
            (2, 7) => {
                self.get_signed(0, bits);
                self.get_signed(1, bits);
                self.b.le_i64();
            },
            // logic operations: carry and overflow are cleared
            (1, 0) => self.b.const_i32(0),
            (1, 1) => self.b.const_i32(0),
            (1, 3) => {
                self.b.get_local_i64(&self.fl_r);
                self.b.eqz_i64();
            },
            (1, 6) => {
                self.get_signed(2, bits);
                self.b.const_i64(0);
                self.b.lt_i64();
            },
            (1, 7) => {
                self.get_signed(2, bits);
                self.b.const_i64(0);
                self.b.le_i64();
            },
            // add: carry out of the operand size
            (0, 1) => {
                self.b.get_local_i64(&self.fl_r);
                self.b.get_local_i64(&self.fl_a);
                self.b.ltu_i64();
            },
            (0, 3) => {
                self.b.get_local_i64(&self.fl_r);
                self.b.get_local_i64(&self.fl_a);
                self.b.ltu_i64();
                self.b.get_local_i64(&self.fl_r);
                self.b.eqz_i64();
                self.b.or_i32();
            },
            _ => return false,
        }
        if invert {
            self.b.eqz_i32();
        }
        true
    }

    /// i32 on the stack: whether condition cc holds
    fn cond(&mut self, cc: u8) {
        if self.cond_pending(cc) {
            return;
        }
        self.materialize();
        self.cond_memory(cc);
    }

    /// i32 on the stack: whether condition cc holds (flags in memory)
    fn cond_memory(&mut self, cc: u8) {
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

    fn mask_result(&mut self, bits: u32) {
        if bits < 64 {
            self.b.const_i64(((1u64 << bits) - 1) as i64);
            self.b.and_i64();
        }
    }

    /// [a, b] -> a op b, masked to the operand size
    fn alu_value(&mut self, op: u8, bits: u32) {
        match op {
            0 => self.b.add_i64(),
            1 => self.b.or_i64(),
            4 => self.b.and_i64(),
            5 | 7 => self.b.sub_i64(),
            _ => self.b.xor_i64(),
        }
        if matches!(op, 0 | 5 | 7) {
            self.mask_result(bits);
        }
    }

    /// [a] -> a shifted / rotated by a constant count (flags are not computed)
    fn shift_value(&mut self, op: u8, bits: u32, count: u8) {
        let c = count as i64 & if bits == 64 { 63 } else { 31 };
        match op {
            4 | 6 => {
                self.b.const_i64(c);
                self.b.shl_i64();
                self.mask_result(bits);
            },
            5 => {
                self.b.const_i64(c);
                self.b.shr_u_i64();
            },
            7 => {
                if bits < 64 {
                    self.b.const_i64(64 - bits as i64);
                    self.b.shl_i64();
                    self.b.const_i64(64 - bits as i64);
                    self.b.shr_s_i64();
                }
                self.b.const_i64(c.min(63));
                self.b.shr_s_i64();
                self.mask_result(bits);
            },
            _ => {
                // rol (0) / ror (1)
                let r = c % bits as i64;
                if r != 0 {
                    let t = self.b.set_new_local_i64();
                    let (left, right) = if op == 0 { (r, bits as i64 - r) } else { (bits as i64 - r, r) };
                    self.b.get_local_i64(&t);
                    self.b.const_i64(left);
                    self.b.shl_i64();
                    self.b.get_local_i64(&t);
                    self.b.const_i64(right);
                    self.b.shr_u_i64();
                    self.b.or_i64();
                    self.mask_result(bits);
                    self.b.free_local_i64(t);
                }
            },
        }
    }

    /// add, or, and, sub, xor, cmp, test (`write`: whether the result is stored)
    fn emit_alu(&mut self, op: u8, bits: u32, dst: Opd, src: Opd, write: bool) {
        let write = write && op != 7;
        self.set_previous_ip();
        let flags_live = self.cur_live_out;
        let simple = matches!(op, 0 | 1 | 4 | 5 | 6 | 7);
        if !simple || (flags_live && write && has_mem(&dst)) {
            // adc / sbb read the carry flag; a memory destination is written after the flags are
            // computed (like the interpreter does): use the helper
            self.materialize();
            self.ea_to_local(&dst);
            self.ea_to_local(&src);
            self.b.const_i32(op as i32);
            self.read_opd_saved(&dst, bits);
            self.read_opd_saved(&src, bits);
            self.b.const_i32(bits as i32);
            self.b.call_fn4_i32_i64_i64_i32_ret_i64("j64_alu");
            if write {
                self.write_opd_saved(&dst, bits);
            }
            else {
                self.b.drop_();
            }
            return;
        }
        // this instruction overwrites all flags, the ones that were pending are not needed
        self.pending = None;
        self.ea_to_local(&dst);
        self.ea_to_local(&src);
        if !flags_live {
            self.read_opd_saved(&dst, bits);
            self.read_opd_saved(&src, bits);
            if write {
                self.alu_value(op, bits);
                self.write_opd_saved(&dst, bits);
            }
            else {
                self.b.drop_();
                self.b.drop_();
            }
        }
        else {
            self.read_opd_saved(&dst, bits);
            self.b.set_local_i64(&self.fl_a);
            self.read_opd_saved(&src, bits);
            self.b.set_local_i64(&self.fl_b);
            self.b.get_local_i64(&self.fl_a);
            self.b.get_local_i64(&self.fl_b);
            self.alu_value(op, bits);
            self.b.set_local_i64(&self.fl_r);
            if write {
                self.b.get_local_i64(&self.fl_r);
                self.write_opd_saved(&dst, bits);
            }
            let kind = match op {
                0 => 0,
                1 | 4 | 6 => 1,
                _ => 2,
            };
            self.pending = Some(Pending { kind, bits });
        }
    }

    fn emit(&mut self, insn: &Insn) {
        if flag_fx(insn).fault {
            // exceptions push the flags: the pending ones have to be in memory
            self.materialize();
        }
        match *insn {
            Insn::Nop => {},
            Insn::Interp => {
                self.materialize();
                self.b.const_i32(instruction_pointer as i32);
                self.b.const_i32(self.cur as i32);
                self.b.store_aligned_i32(0);
                self.b.const_i32(rip_hi as i32);
                self.b.const_i32((self.cur >> 32) as i32);
                self.b.store_aligned_i32(0);
                self.b.const_i64(self.next as i64);
                self.b.const_i32(0);
                self.b.call_fn2_i64_i32_ret("j64_interp1");
                self.b.if_void();
                self.exit_keep_rip();
                self.b.block_end();
                self.smc_check();
            },
            Insn::Alu { op, bits, dst, src } => self.emit_alu(op, bits, dst, src, true),
            Insn::Test { bits, a, b } => self.emit_alu(4, bits, a, b, false),
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
                self.write_mem(bits);
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
                if self.cur_live_out {
                    // the carry flag is preserved: the old flags have to be in memory
                    self.materialize();
                    self.b.const_i32(8 + dec as i32);
                    self.read_opd_saved(&dst, bits);
                    self.b.const_i64(0);
                    self.b.const_i32(bits as i32);
                    self.b.call_fn4_i32_i64_i64_i32_ret_i64("j64_alu");
                }
                else {
                    self.read_opd_saved(&dst, bits);
                    self.b.const_i64(1);
                    if dec {
                        self.b.sub_i64();
                    }
                    else {
                        self.b.add_i64();
                    }
                    self.mask_result(bits);
                }
                self.write_opd_saved(&dst, bits);
            },
            Insn::Shift { op, bits, dst, count } => {
                self.set_previous_ip();
                self.ea_to_local(&dst);
                let Count::Imm(c) = count else {
                    // count in cl: flags and result come from the helper (a zero count changes nothing)
                    self.materialize();
                    self.b.const_i32(16 + op as i32);
                    self.read_opd_saved(&dst, bits);
                    self.get_reg(1, 8, true);
                    self.b.const_i32(bits as i32);
                    self.b.call_fn4_i32_i64_i64_i32_ret_i64("j64_alu");
                    self.write_opd_saved(&dst, bits);
                    return;
                };
                if self.cur_live_out || matches!(op, 2 | 3) {
                    self.materialize();
                    self.b.const_i32(16 + op as i32);
                    self.read_opd_saved(&dst, bits);
                    self.b.const_i64(c as i64);
                    self.b.const_i32(bits as i32);
                    self.b.call_fn4_i32_i64_i64_i32_ret_i64("j64_alu");
                }
                else {
                    self.read_opd_saved(&dst, bits);
                    self.shift_value(op, bits, c);
                }
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
            Insn::Neg { bits, dst } => {
                self.set_previous_ip();
                self.ea_to_local(&dst);
                if self.cur_live_out && has_mem(&dst) {
                    self.materialize();
                    self.b.const_i32(5);
                    self.b.const_i64(0);
                    self.read_opd_saved(&dst, bits);
                    self.b.const_i32(bits as i32);
                    self.b.call_fn4_i32_i64_i64_i32_ret_i64("j64_alu");
                }
                else if !self.cur_live_out {
                    self.pending = None;
                    self.b.const_i64(0);
                    self.read_opd_saved(&dst, bits);
                    self.b.sub_i64();
                    self.mask_result(bits);
                }
                else {
                    self.pending = None;
                    self.read_opd_saved(&dst, bits);
                    self.b.set_local_i64(&self.fl_b);
                    self.b.const_i64(0);
                    self.b.set_local_i64(&self.fl_a);
                    self.b.const_i64(0);
                    self.b.get_local_i64(&self.fl_b);
                    self.b.sub_i64();
                    self.mask_result(bits);
                    self.b.set_local_i64(&self.fl_r);
                    self.b.get_local_i64(&self.fl_r);
                    self.pending = Some(Pending { kind: 2, bits });
                }
                self.write_opd_saved(&dst, bits);
            },
            Insn::Not { bits, dst } => {
                self.set_previous_ip();
                self.ea_to_local(&dst);
                self.read_opd_saved(&dst, bits);
                self.b.const_i64(-1);
                self.b.xor_i64();
                self.mask_result(bits);
                self.write_opd_saved(&dst, bits);
            },
            Insn::Imul { bits, dst, a, b } => {
                self.set_previous_ip();
                self.pending = None;
                self.ea_to_local(&a);
                self.ea_to_local(&b);
                self.b.const_i32(24);
                self.read_opd_saved(&a, bits);
                self.read_opd_saved(&b, bits);
                self.b.const_i32(bits as i32);
                self.b.call_fn4_i32_i64_i64_i32_ret_i64("j64_alu");
                self.set_reg(dst, bits, true);
            },
            Insn::Xchg { bits, a, b } => {
                self.set_previous_ip();
                self.ea_to_local(&a);
                self.ea_to_local(&b);
                let ta = self.b.new_local_i64();
                let tb = self.b.new_local_i64();
                self.read_opd_saved(&a, bits);
                self.b.set_local_i64(&ta);
                self.read_opd_saved(&b, bits);
                self.b.set_local_i64(&tb);
                self.b.get_local_i64(&tb);
                self.write_opd_saved(&a, bits);
                self.b.get_local_i64(&ta);
                self.write_opd_saved(&b, bits);
                self.b.free_local_i64(ta);
                self.b.free_local_i64(tb);
            },
            Insn::Sext { opcode, bits } => {
                if opcode == 0x98 {
                    let half = bits / 2;
                    self.get_reg(0, half, true);
                    self.b.const_i64(64 - half as i64);
                    self.b.shl_i64();
                    self.b.const_i64(64 - half as i64);
                    self.b.shr_s_i64();
                    self.set_reg(0, bits, true);
                }
                else {
                    self.b.const_i64(0);
                    self.get_reg(0, bits, true);
                    self.b.const_i64(bits as i64 - 1);
                    self.b.shr_u_i64();
                    self.b.const_i64(1);
                    self.b.and_i64();
                    self.b.sub_i64();
                    self.set_reg(2, bits, true);
                }
            },
            Insn::Bt { op, bits, dst, idx } => {
                self.set_previous_ip();
                let live = self.cur_live_out;
                if live {
                    // only the carry flag changes: the rest has to be in memory
                    self.materialize();
                }
                // bit index -> t_a, address (memory operands) -> t_ea
                match (dst, idx) {
                    (Opd::Mem(ea), Opd::Reg(m, rex)) => {
                        // the bit offset may address beyond the operand
                        self.get_reg(m, bits, rex);
                        if bits < 64 {
                            self.b.const_i64(64 - bits as i64);
                            self.b.shl_i64();
                            self.b.const_i64(64 - bits as i64);
                            self.b.shr_s_i64();
                        }
                        self.b.set_local_i64(&self.t_a);
                        self.ea(&ea, true);
                        self.b.get_local_i64(&self.t_a);
                        self.b.const_i64(3);
                        self.b.shr_s_i64();
                        self.b.const_i64(!(bits as i64 / 8 - 1));
                        self.b.and_i64();
                        self.b.add_i64();
                        self.b.set_local_i64(&self.t_ea);
                        self.b.get_local_i64(&self.t_a);
                        self.b.const_i64(bits as i64 - 1);
                        self.b.and_i64();
                        self.b.set_local_i64(&self.t_a);
                    },
                    (Opd::Mem(ea), Opd::Imm(i)) => {
                        self.ea(&ea, true);
                        self.b.set_local_i64(&self.t_ea);
                        self.b.const_i64(i as i64 & (bits as i64 - 1));
                        self.b.set_local_i64(&self.t_a);
                    },
                    (_, Opd::Reg(m, rex)) => {
                        self.get_reg(m, bits, rex);
                        self.b.const_i64(bits as i64 - 1);
                        self.b.and_i64();
                        self.b.set_local_i64(&self.t_a);
                    },
                    (_, Opd::Imm(i)) => {
                        self.b.const_i64(i as i64 & (bits as i64 - 1));
                        self.b.set_local_i64(&self.t_a);
                    },
                    _ => unreachable!(),
                }
                self.read_opd_saved(&dst, bits);
                self.b.set_local_i64(&self.t_v);
                if live {
                    self.b.get_local_i64(&self.t_v);
                    self.b.get_local_i64(&self.t_a);
                    self.b.shr_u_i64();
                    self.b.const_i64(1);
                    self.b.and_i64();
                    self.b.set_local_i64(&self.fl_r);
                }
                if live {
                    // set before the write: when the write changes this code the exit stores it
                    self.pending = Some(Pending { kind: 3, bits });
                }
                if op != 0 {
                    self.b.get_local_i64(&self.t_v);
                    self.b.const_i64(1);
                    self.b.get_local_i64(&self.t_a);
                    self.b.shl_i64();
                    match op {
                        1 => self.b.or_i64(),
                        2 => {
                            self.b.const_i64(-1);
                            self.b.xor_i64();
                            self.b.and_i64();
                        },
                        _ => self.b.xor_i64(),
                    }
                    self.mask_result(bits);
                    self.write_opd_saved(&dst, bits);
                }
            },
            Insn::Str { opcode, rep, bits, asz32, seg } => {
                self.set_previous_ip();
                let simple = rep == 0 && !asz32 && seg < 0 && matches!(opcode, 0xA4 | 0xA5 | 0xAA | 0xAB | 0xAC | 0xAD);
                if !simple {
                    self.b.const_i32(opcode as i32 | (rep as i32) << 8 | (asz32 as i32) << 16);
                    self.b.const_i32(bits as i32 | (seg + 1) << 8);
                    self.b.call_fn2("j64_str");
                    self.fault_check();
                    self.smc_check();
                    return;
                }
                let step = bits as i64 / 8;
                // delta: +step, or -step when the direction flag is set
                let delta = |g: &mut Gen| {
                    g.b.const_i64(-step);
                    g.b.const_i64(step);
                    g.b.load_fixed_i32(flags as u32);
                    g.b.const_i32(FLAG_DIRECTION);
                    g.b.and_i32();
                    g.b.select();
                };
                match opcode {
                    0xAC | 0xAD => {
                        self.get_reg(6, 64, true);
                        self.read_mem(bits);
                        self.fault_check();
                        self.set_reg(0, bits, true);
                        self.get_reg(6, 64, true);
                        delta(self);
                        self.b.add_i64();
                        self.set_reg(6, 64, true);
                    },
                    0xAA | 0xAB => {
                        self.get_reg(7, 64, true);
                        self.get_reg(0, bits, true);
                        self.b.set_local_i64(&self.t_v);
                        self.b.set_local_i64(&self.t_ea);
                        self.b.get_local_i64(&self.t_ea);
                        self.b.get_local_i64(&self.t_v);
                        self.write_mem(bits);
                        self.fault_check();
                        self.smc_check();
                        self.get_reg(7, 64, true);
                        delta(self);
                        self.b.add_i64();
                        self.set_reg(7, 64, true);
                    },
                    _ => {
                        self.get_reg(6, 64, true);
                        self.read_mem(bits);
                        self.fault_check();
                        self.b.set_local_i64(&self.t_v);
                        self.get_reg(7, 64, true);
                        self.b.set_local_i64(&self.t_ea);
                        self.b.get_local_i64(&self.t_ea);
                        self.b.get_local_i64(&self.t_v);
                        self.write_mem(bits);
                        self.fault_check();
                        self.smc_check();
                        self.get_reg(6, 64, true);
                        delta(self);
                        self.b.add_i64();
                        self.set_reg(6, 64, true);
                        self.get_reg(7, 64, true);
                        delta(self);
                        self.b.add_i64();
                        self.set_reg(7, 64, true);
                    },
                }
            },
            Insn::Jcc { .. } | Insn::Jmp { .. } | Insn::Call { .. } | Insn::Ret { .. }
            | Insn::CallInd { .. } | Insn::JmpInd { .. } => {
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
                if self.live_in.get(&(off as usize)).copied().unwrap_or(true) {
                    self.materialize();
                }
                self.pending = None;
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
            let decoded_insn = decode(&code[off..], rip.wrapping_add(off as u64)).or_else(|| {
                if DISABLE_MASK >> 24 & 1 != 0 { return None }
                interp_len(&code[off..]).map(|l| (Insn::Interp, l))
            });
            let Some((insn, len)) = decoded_insn else {
                count_unsupported(&code[off..]);
                break;
            };
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
                Insn::Call { .. } | Insn::Ret { .. } | Insn::CallInd { .. } | Insn::JmpInd { .. } => break,
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

    GEN_RMW = decoded.values().any(|(i, _)| is_rmw(i));
    let (live_out, live_in) = flag_liveness(&decoded, &block_starts, rip, code.len());

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
    let t_fa = b.new_local_i64();
    let t_fv = b.new_local_i64();
    let t_fp = b.new_local_i64();
    let t_fe = b.new_local();
    let fl_a = b.new_local_i64();
    let fl_b = b.new_local_i64();
    let fl_r = b.new_local_i64();
    let loop_label = b.loop_void();

    let mut g = Gen {
        b, start: rip, loop_label, n: 0, n_local, pc, t_ea, t_v, t_a, t_fa, t_fv, t_fp, t_fe, cur: rip, next: rip, block_index,
        pending: None, fl_a, fl_b, fl_r, live_out, live_in, cur_live_out: true, smc_needed: false,
    };

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
            g.pending = None;
            ended = false;
        }
        g.cur_live_out = g.live_out.get(&off).copied().unwrap_or(true);
        let cur = rip.wrapping_add(off as u64);
        let next = cur.wrapping_add(len as u64);
        g.cur = cur;
        g.next = next;
        g.n += 1;
        total += 1;

        match insn {
            Insn::Jcc { cc, target } => {
                g.cond(cc);
                // the taken path may store the flags (its own copy of the compile time state)
                let after_cond = g.pending;
                g.b.if_void();
                g.transfer(target);
                g.b.block_end();
                g.pending = after_cond;
            },
            Insn::Jmp { target } => {
                g.transfer(target);
                ended = true;
            },
            Insn::Call { target } => {
                g.materialize();
                g.set_previous_ip();
                g.get_reg(4, 64, true);
                g.b.const_i64(8);
                g.b.sub_i64();
                g.b.set_local_i64(&g.t_ea);
                g.b.get_local_i64(&g.t_ea);
                g.b.const_i64(next as i64);
                g.write_mem(64);
                g.fault_check();
                g.smc_check();
                g.b.get_local_i64(&g.t_ea);
                g.set_reg(4, 64, true);
                let n = g.n;
                g.exit_const(target, n);
                ended = true;
            },
            Insn::CallInd { src } => {
                g.materialize();
                g.set_previous_ip();
                let target = g.b.new_local_i64();
                g.read_opd(&src, 64);
                g.b.set_local_i64(&target);
                g.get_reg(4, 64, true);
                g.b.const_i64(8);
                g.b.sub_i64();
                g.b.set_local_i64(&g.t_ea);
                g.b.get_local_i64(&g.t_ea);
                g.b.const_i64(next as i64);
                g.write_mem(64);
                g.fault_check();
                g.smc_check();
                g.b.get_local_i64(&g.t_ea);
                g.set_reg(4, 64, true);
                g.b.get_local_i64(&target);
                g.b.const_i32(0);
                g.b.call_fn2_i64_i32("j64_set_rip");
                g.fault_check();
                g.exit_keep_rip();
                g.b.free_local_i64(target);
                ended = true;
            },
            Insn::JmpInd { src } => {
                g.materialize();
                g.set_previous_ip();
                g.read_opd(&src, 64);
                g.b.const_i32(0);
                g.b.call_fn2_i64_i32("j64_set_rip");
                g.fault_check();
                g.exit_keep_rip();
                ended = true;
            },
            Insn::Ret { adj } => {
                g.materialize();
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
            _ => {
                g.emit(&insn);
                g.finish_insn();
            },
        }
        // a block that is left right away does not need the check
        g.smc_needed = false;

        // what follows this instruction?
        let follows = off + len;
        let terminator = matches!(insn, Insn::Jmp { .. } | Insn::Call { .. } | Insn::Ret { .. } | Insn::CallInd { .. } | Insn::JmpInd { .. });
        if !terminator {
            if !decoded.contains_key(&follows) {
                // the path ends here: continue in the interpreter
                let n = g.n;
                g.exit_const(next, n);
                ended = true;
            }
            else if g.block_index.contains_key(&follows) {
                // falls through into the next basic block (its code is emitted right after)
                if g.live_in.get(&follows).copied().unwrap_or(true) {
                    g.materialize();
                }
                g.pending = None;
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

    let Gen { b, n_local, pc, t_ea, t_v, t_a, t_fa, t_fv, t_fp, t_fe, fl_a, fl_b, fl_r, .. } = g;
    b.block_end(); // loop
    b.free_local(n_local);
    b.free_local(pc);
    b.free_local_i64(t_ea);
    b.free_local_i64(t_v);
    b.free_local_i64(t_a);
    b.free_local_i64(t_fa);
    b.free_local_i64(t_fv);
    b.free_local_i64(t_fp);
    b.free_local(t_fe);
    b.free_local_i64(fl_a);
    b.free_local_i64(fl_b);
    b.free_local_i64(fl_r);
    b.finish();
    Some(total)
}

// ---------------------------------------------------------------------------------------------
// block cache and dispatcher

/// Runs one step of 64-bit code: a compiled block if there is one at rip, otherwise one
/// interpreted instruction
pub unsafe fn run() {
    let checked = CHECK;
    if CHECK {
        CHECK = false;
        if try_run_block() {
            if CHAIN_BUDGET == 0 {
                CHECK = true;
                return;
            }
            // compiled blocks cannot change the interrupt shadow, the trap flag or the mode, so the
            // next blocks can follow without the checks of cycle_internal; stop after a budget so
            // that timers and interrupts are still serviced by the caller
            let start = *instruction_counter;
            loop {
                if (*instruction_counter).wrapping_sub(start) >= CHAIN_BUDGET {
                    CHECK = true;
                    return;
                }
                if !try_run_block() {
                    break;
                }
            }
        }
    }
    let before = rip();
    if HIST_ON {
        let why = if !checked { 0 } else {
            let st = state();
            let s = &st.slots[slot_index(before)];
            if s.rip != before { 1 } else { match s.state { SLOT_COUNTING => 1, SLOT_FAILED => 2, _ => 3 } }
        };
        INTERP_WHY[why] = INTERP_WHY[why].wrapping_add(1);
        let e = &mut HIST_RIPS[slot_index(before) & 4095];
        if e.0 != before { *e = (before, 0) }
        e.1 = e.1.wrapping_add(1);
    }
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
    let counter_before = *instruction_counter;
    if VERIFY {
        verified_call(index);
        EXECUTING_PAGE = u32::MAX;
        FLAGS[FAULT] = 0;
        return;
    }
    call_jit_table(index);
    STAT_JIT_INSNS = STAT_JIT_INSNS.wrapping_add((*instruction_counter).wrapping_sub(counter_before) as u32);
    EXECUTING_PAGE = u32::MAX;
    FLAGS[FAULT] = 0;
}

unsafe fn save_regs() -> [i32; 46] {
    let mut s = [0i32; 46];
    for i in 0..8 { s[i] = *reg32.add(i); s[8 + i] = *reg_ext.add(i) }
    for i in 0..16 { s[16 + i] = *reg_hi.add(i) }
    s[32] = *instruction_pointer; s[33] = *rip_hi; s[34] = *flags; s[35] = *flags_changed; s[36] = *previous_ip;
    s[37] = *instruction_counter as i32;
    s
}
unsafe fn restore_regs(s: &[i32; 46]) {
    for i in 0..8 { *reg32.add(i) = s[i]; *reg_ext.add(i) = s[8 + i] }
    for i in 0..16 { *reg_hi.add(i) = s[16 + i] }
    *instruction_pointer = s[32]; *rip_hi = s[33]; *flags = s[34]; *flags_changed = s[35]; *previous_ip = s[36];
    *instruction_counter = s[37] as u32;
}

/// debugging aid (VERIFY): runs the block, then the same number of instructions in the interpreter
/// from the saved state, and records the first differences. Memory written by the block stays
/// written (an instruction that modifies memory runs twice), so only register state is compared.
unsafe fn verified_call(index: u16) {
    if state().rmw.contains(&index) {
        call_jit_table(index);
        VERIFY_LOG[63] += 1;
        return;
    }
    let start_rip = rip();
    let before = save_regs();
    #[allow(static_mut_refs)]
    UNDO.clear();
    UNDO_ON = true;
    call_jit_table(index);
    UNDO_ON = false;
    // the flags of the compiled code may still be lazy in memory: materialize like the interpreter does
    let after_flags = get_eflags();
    let mut jit = save_regs();
    jit[34] = after_flags;
    let n = (*instruction_counter).wrapping_sub(before[37] as u32);
    // what the compiled block left in memory, then undo its writes
    #[allow(static_mut_refs)]
    let log = UNDO.clone();
    let mut jit_mem = Vec::with_capacity(log.len());
    for &(addr, bits, _) in &log { jit_mem.push(mem_read(addr, bits).ok()) }
    for &(addr, bits, old) in log.iter().rev() { let _ = mem_write(addr, bits, old); }
    restore_regs(&before);
    let mut done = 0;
    while done < n {
        long_mode::step64();
        done += 1;
    }
    let mut interp = save_regs();
    interp[34] = get_eflags();
    VERIFY_LOG[0] += 1;
    let mut diff = None;
    for i in 0..33 {
        let (a, b) = (jit[i], interp[i]);
        if a != b { diff = Some((i, a, b)); break }
    }
    if diff.is_none() && (jit[34] ^ interp[34]) & 0x8D5 != 0 { diff = Some((34, jit[34], interp[34])) }
    if diff.is_none() {
        for (k, &(addr, bits, _)) in log.iter().enumerate() {
            let now = mem_read(addr, bits).ok();
            if now != jit_mem[k] {
                diff = Some((100, addr as i32, now.unwrap_or(0) as i32));
                VERIFY_LOG[60] = (addr >> 32) as u32; VERIFY_LOG[61] = jit_mem[k].unwrap_or(0) as u32; VERIFY_LOG[62] = (jit_mem[k].unwrap_or(0) >> 32) as u32;
                break
            }
        }
    }
    if let Some((i, a, b)) = diff {
        let k = VERIFY_LOG[1] as usize;
        VERIFY_LOG[1] += 1;
        if k < 4 {
            let o = 2 + k * 8;
            VERIFY_LOG[o] = start_rip as u32; VERIFY_LOG[o + 1] = (start_rip >> 32) as u32;
            VERIFY_LOG[o + 2] = n; VERIFY_LOG[o + 3] = i as u32;
            VERIFY_LOG[o + 4] = a as u32; VERIFY_LOG[o + 5] = b as u32;
            VERIFY_LOG[o + 6] = jit[34] as u32; VERIFY_LOG[o + 7] = interp[34] as u32;
        }
    }
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
    let out_of_range = r < RANGE_LO || r >= RANGE_HI;
    let registered = if out_of_range { None } else { jit::jit64_compile(phys, |builder| generate(builder, r, code), |index| {
        // registered before the module is handed to the host, which may finish synchronously
        if (STAT_COMPILED as usize) < 64 { COMPILED_RIPS[STAT_COMPILED as usize] = r as u32 }
        STAT_COMPILED += 1;
        if GEN_RMW { st.rmw.insert(index); } else { st.rmw.remove(&index); }
        st.blocks.insert(r, Block { state: BlockState::Pending, phys_page: page, user, index, epoch: EPOCH });
        st.by_index.insert(index, r);
        st.page_blocks.entry(page).or_default().push(r);
        jit::maybe_has_code_set(Page::of_u32(page));
        crate::cpu::cpu::tlb_set_has_code(Page::of_u32(page), true);
    }) };
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
