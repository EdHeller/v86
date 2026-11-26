use std::collections::{BTreeMap, VecDeque};
// (the jit's maps and sets hash with page::FastHasher unless they name another hasher)
type HashMap<K, V, S = crate::page::FastBuildHasher> = std::collections::HashMap<K, V, S>;
type HashSet<K, S = crate::page::FastBuildHasher> = std::collections::HashSet<K, S>;
use std::iter::FromIterator;
use std::mem::{self, MaybeUninit};
use std::ops::{Deref, DerefMut};
use std::ptr::NonNull;
use std::sync::{Mutex, MutexGuard};

use crate::analysis;
use crate::analysis::AnalysisType;
use crate::codegen;
use crate::control_flow;
use crate::control_flow::WasmStructure;
use crate::cpu::cpu;
use crate::cpu::global_pointers;
use crate::cpu::memory;
use crate::cpu_context::CpuContext;
use crate::jit_instructions;
use crate::opstats;
use crate::page::{Page, PageBuildHasher};
use crate::profiler;
use crate::profiler::stat;
use crate::state_flags::CachedStateFlags;
use crate::wasmgen::wasm_builder::{Label, WasmBuilder, WasmLocal};

#[derive(Copy, Clone, Eq, Hash, PartialEq)]
#[repr(transparent)]
pub struct WasmTableIndex(u16);
impl WasmTableIndex {
    pub fn to_u16(self) -> u16 { self.0 }
}

mod unsafe_jit {
    use super::{CachedStateFlags, WasmTableIndex};

    #[link(wasm_import_module = "env")]
    extern "C" {
        pub fn codegen_finalize(
            wasm_table_index: WasmTableIndex,
            phys_addr: u32,
            state_flags: CachedStateFlags,
            ptr: u32,
            len: u32,
        );
        pub fn jit_clear_func(wasm_table_index: WasmTableIndex);
    }
}

fn codegen_finalize(
    wasm_table_index: WasmTableIndex,
    phys_addr: u32,
    state_flags: CachedStateFlags,
    ptr: u32,
    len: u32,
) {
    unsafe { unsafe_jit::codegen_finalize(wasm_table_index, phys_addr, state_flags, ptr, len) }
}

pub fn jit_clear_func(wasm_table_index: WasmTableIndex) {
    unsafe { unsafe_jit::jit_clear_func(wasm_table_index) }
}

static mut JIT_DISABLED: bool = false;

// Maximum number of pages per wasm module. Necessary for the following reasons:
// - There is an upper limit on the size of a single function in wasm (currently ~7MB in all browsers)
//   See https://github.com/WebAssembly/design/issues/1138
// - v8 poorly handles large br_table elements and OOMs on modules much smaller than the above limit
//   See https://bugs.chromium.org/p/v8/issues/detail?id=9697 and https://bugs.chromium.org/p/v8/issues/detail?id=9141
//   Will hopefully be fixed in the near future by generating direct control flow
static mut MAX_PAGES: u32 = 3;

static mut JIT_USE_LOOP_SAFETY: bool = true;

pub static mut MAX_EXTRA_BASIC_BLOCKS: u32 = 250;

pub const JIT_THRESHOLD: u32 = 200 * 1000;
/// Indirect jumps look up their target inline (else through jit_find_cache_entry_in_page, as upstream);
/// set_jit_config(65, 0) for comparisons
pub static mut INLINE_LOOKUP: bool = true;
/// ... in a function of the module (true), or in the main function itself; set_jit_config(66, 0/1)
pub static mut LOOKUP_IN_FUNCTION: bool = true;
/// A module loads and stores only the guest registers it uses (set_jit_config(67, 0): all eight, as upstream)
pub static mut DROP_REGISTER_MOVES: bool = true;

/// The heat at which a page is compiled: JIT_THRESHOLD, lowered by tests through set_jit_config(59, ..)
static mut JIT_THRESHOLD_NOW: u32 = JIT_THRESHOLD;
/// The heat at which a page that already has compiled code is compiled again with the entry points the interpreter
/// found since (set_jit_config(68, ..)). Those pages are known to be hot: until then every entry into them through a
/// new entry point is interpreted
static mut JIT_RECOMPILE_THRESHOLD: u32 = 50 * 1000;

// less branches will generate if-else, more will generate brtable
pub const BRTABLE_CUTOFF: usize = 10;

// needs to be synced to const.js
pub const WASM_TABLE_SIZE: u32 = 12000;

pub const CHECK_JIT_STATE_INVARIANTS: bool = false;

const MAX_INSTRUCTION_LENGTH: u32 = 16;

static JIT_STATE: Mutex<MaybeUninit<JitState>> = Mutex::new(MaybeUninit::uninit());
fn get_jit_state() -> JitStateRef { JitStateRef(JIT_STATE.try_lock().unwrap()) }

struct JitStateRef(MutexGuard<'static, MaybeUninit<JitState>>);

impl Deref for JitStateRef {
    type Target = JitState;
    fn deref(&self) -> &Self::Target { unsafe { self.0.assume_init_ref() } }
}
impl DerefMut for JitStateRef {
    fn deref_mut(&mut self) -> &mut Self::Target { unsafe { self.0.assume_init_mut() } }
}

#[no_mangle]
pub fn rust_init() {
    dbg_assert!(std::mem::size_of::<[Option<NonNull<cpu::Code>>; 0x100000]>() == 0x100000 * 4);

    let _ = JIT_STATE
        .try_lock()
        .unwrap()
        .write(JitState::create_and_initialise());

    use std::panic;

    panic::set_hook(Box::new(|panic_info| {
        console_log!("{}", panic_info.to_string());
    }));
}

/// The entry points noted for a page (offsets 0 to 4095): a bitmap, as every interpreted run notes one (a hash set's
/// hashing showed in profiles)
#[derive(Clone)]
pub struct EntrySet {
    bits: [u64; 64],
}
impl Default for EntrySet {
    fn default() -> Self { EntrySet { bits: [0; 64] } }
}
impl EntrySet {
    #[inline]
    pub fn insert(&mut self, offset: u16) { self.bits[(offset >> 6 & 63) as usize] |= 1 << (offset & 63); }
    pub fn iter(&self) -> impl Iterator<Item = u16> + '_ {
        self.bits.iter().enumerate().flat_map(|(i, &w)| {
            let mut w = w;
            std::iter::from_fn(move || {
                if w == 0 {
                    return None;
                }
                let b = w.trailing_zeros();
                w &= w - 1;
                Some((i as u32 * 64 + b) as u16)
            })
        })
    }
}

struct PageInfo {
    wasm_table_index: WasmTableIndex,
    hidden_wasm_table_indices: Vec<WasmTableIndex>,
    entry_points: Vec<(u16, u16)>,
    state_flags: CachedStateFlags,
}

enum CompilingPageState {
    Compiling { pages: HashMap<Page, PageInfo, PageBuildHasher> },
    CompilingWritten,
}

pub struct JitState {
    wasm_builder: WasmBuilder,

    // as an alternative to HashSet, we could use a bitmap of 4096 bits here
    // (faster, but uses much more memory)
    // or a compressed bitmap (likely faster)
    // or HashSet<u32> rather than nested
    entry_points: HashMap<Page, (u32, EntrySet), PageBuildHasher>,
    pages: HashMap<Page, PageInfo, PageBuildHasher>,
    wasm_table_index_free_list: Vec<WasmTableIndex>,
    /// the modules being compiled (asynchronously, by the engine): at most MAX_COMPILING, no page in two of them
    compiling: Vec<(WasmTableIndex, CompilingPageState)>,
    #[cfg(debug_assertions)]
    wasm_table_index_to_page: HashMap<WasmTableIndex, HashSet<Page>>,
}

fn check_jit_state_invariants(ctx: &mut JitState) {
    if !CHECK_JIT_STATE_INVARIANTS {
        return;
    }

    for (_, state) in &ctx.compiling {
        if let CompilingPageState::Compiling { pages } = state {
            dbg_assert!(pages.keys().all(|page| ctx.entry_points.contains_key(page)));
        }
    }

    let free: HashSet<WasmTableIndex> =
        HashSet::from_iter(ctx.wasm_table_index_free_list.iter().copied());
    let used = HashSet::from_iter(ctx.pages.values().map(|info| info.wasm_table_index));
    let compiling: HashSet<WasmTableIndex> = HashSet::from_iter(ctx.compiling.iter().map(|&(index, _)| index));
    dbg_assert!(free.intersection(&used).next().is_none());
    dbg_assert!(used.intersection(&compiling).next().is_none());
    dbg_assert!(free.len() + used.len() + compiling.len() == (WASM_TABLE_SIZE - 1) as usize);

    let hidden: HashSet<WasmTableIndex> = ctx
        .pages
        .values()
        .flat_map(|info| info.hidden_wasm_table_indices.iter().copied())
        .collect();
    dbg_assert!(free.intersection(&hidden).next().is_none());
    dbg_assert!(hidden.is_subset(&used));

    #[cfg(debug_assertions)]
    for (wasm_table_index, pages) in &ctx.wasm_table_index_to_page {
        for page in pages {
            match ctx.pages.get(page) {
                Some(info) => dbg_assert!(
                    info.wasm_table_index == *wasm_table_index
                        || info.hidden_wasm_table_indices.contains(wasm_table_index)
                ),
                None => dbg_assert!(false),
            }
        }
    }

    for (_, state) in &ctx.compiling {
        if let CompilingPageState::Compiling { pages } = state {
            dbg_assert!(pages.keys().all(|page| ctx.entry_points.contains_key(page)));
        }
    }

    for i in 0..unsafe { cpu::valid_tlb_entries_count } {
        let page = unsafe { cpu::valid_tlb_entries[i as usize] };
        let entry = unsafe { cpu::tlb_data[page as usize] };
        if 0 != entry {
            let tlb_physical_page = Page::of_u32(
                (entry as u32 >> 12 ^ page as u32) - (unsafe { memory::mem8 } as u32 >> 12),
            );
            let w = match unsafe { cpu::tlb_code[page as usize] } {
                None => None,
                Some(c) => unsafe { Some(c.as_ref().wasm_table_index) },
            };
            let tlb_has_code = entry & cpu::TLB_HAS_CODE == cpu::TLB_HAS_CODE;
            let infos = ctx.pages.get(&tlb_physical_page);
            let entry_points = ctx.entry_points.get(&tlb_physical_page);
            dbg_assert!(tlb_has_code || !w.is_some());
            dbg_assert!(tlb_has_code || !infos.is_some());
            dbg_assert!(tlb_has_code || !entry_points.is_some());
            //dbg_assert!((w.is_some() || page.is_some() || entry_points.is_some()) == tlb_has_code); // XXX: check this
        }
    }
}

impl JitState {
    pub fn create_and_initialise() -> JitState {
        // don't assign 0 (XXX: Check)
        let wasm_table_indices = (1..=(WASM_TABLE_SIZE - 1) as u16).map(|x| WasmTableIndex(x));

        JitState {
            wasm_builder: WasmBuilder::new(),

            entry_points: HashMap::default(),
            pages: HashMap::default(),

            wasm_table_index_free_list: Vec::from_iter(wasm_table_indices),
            compiling: Vec::new(),

            #[cfg(debug_assertions)]
            wasm_table_index_to_page: HashMap::default(),
        }
    }
}

#[derive(PartialEq, Eq)]
pub enum BasicBlockType {
    Normal {
        next_block_addr: Option<u32>,
        jump_offset: i32,
        jump_offset_is_32: bool,
    },
    ConditionalJump {
        next_block_addr: Option<u32>,
        next_block_branch_taken_addr: Option<u32>,
        condition: u8,
        jump_offset: i32,
        jump_offset_is_32: bool,
    },
    // Set eip to an absolute value (ret, jmp r/m, call r/m)
    AbsoluteEip,
    Exit,
}

pub struct BasicBlock {
    pub addr: u32,
    pub virt_addr: i32,
    pub last_instruction_addr: u32,
    pub end_addr: u32,
    pub is_entry_block: bool,
    pub ty: BasicBlockType,
    pub has_sti: bool,
    pub number_of_instructions: u32,
    /// ends with a call (its return address is the next instruction)
    pub is_call: bool,
    /// its last instruction crosses into the next page (crossing_instruction): its bytes there are checked when it runs
    pub crossing: bool,
    /// that instruction is a relative jump: its condition (None: always) and offset
    pub crossing_jump: Option<(Option<u8>, i32)>,
    /// it ends with a rep string instruction (rep_string_instruction) and goes on to the next block when that
    /// instruction finished in its one call; otherwise (stopped at a page's end, a fault, compiled code
    /// invalidated) the module returns, as after any block boundary
    pub rep_string: Option<RepString>,
}

#[derive(Copy, Clone, PartialEq)]
pub enum RepString {
    /// lods, cmps, scas
    Reads,
    /// movs, stos: may write to pages with compiled code
    Writes,
}

/// A rep-prefixed string instruction (not ins/outs) at addr: how it touches memory
fn rep_string_instruction(addr: u32) -> Option<RepString> {
    let mut rep = false;
    for i in 0..15 {
        let a = addr + i;
        if a & 0xFFF == 0 && i > 0 {
            return None;
        }
        match memory::read8(a) as u8 {
            0xF2 | 0xF3 => rep = true,
            0x26 | 0x2E | 0x36 | 0x3E | 0x64 | 0x65 | 0x66 | 0x67 => {},
            0xA4 | 0xA5 | 0xAA | 0xAB if rep => return Some(RepString::Writes),
            0xA6 | 0xA7 | 0xAC | 0xAD | 0xAE | 0xAF if rep => return Some(RepString::Reads),
            _ => return None,
        }
    }
    None
}

/// Incremented whenever compiled code may have been freed (jit_dirty_page): a rep string instruction that writes
/// leaves the module when it changed
pub static mut CODE_INVALIDATIONS: u32 = 0;

/// set_jit_config 24: rep string instructions that finished go on in the module
pub static mut REP_STRING_CONTINUE: bool = true;

#[derive(Copy, Clone, PartialEq)]
pub struct CachedCode {
    pub wasm_table_index: WasmTableIndex,
    pub initial_state: u16,
}

impl CachedCode {
    pub const NONE: CachedCode = CachedCode {
        wasm_table_index: WasmTableIndex(0),
        initial_state: 0,
    };
}

#[derive(PartialEq)]
pub enum InstructionOperandDest {
    WasmLocal(WasmLocal),
    Other,
}
#[derive(PartialEq)]
pub enum InstructionOperand {
    WasmLocal(WasmLocal),
    Immediate(i32),
    Other,
}
impl InstructionOperand {
    pub fn is_zero(&self) -> bool {
        match self {
            InstructionOperand::Immediate(0) => true,
            _ => false,
        }
    }
}
impl Into<InstructionOperand> for InstructionOperandDest {
    fn into(self: InstructionOperandDest) -> InstructionOperand {
        match self {
            InstructionOperandDest::WasmLocal(l) => InstructionOperand::WasmLocal(l),
            InstructionOperandDest::Other => InstructionOperand::Other,
        }
    }
}
pub enum Instruction {
    Cmp {
        dest: InstructionOperandDest,
        source: InstructionOperand,
        opsize: i32,
    },
    Sub {
        dest: InstructionOperandDest,
        source: InstructionOperand,
        opsize: i32,
        is_dec: bool,
    },
    Add {
        dest: InstructionOperandDest,
        source: InstructionOperand,
        opsize: i32,
        is_inc: bool,
    },
    AdcSbb {
        dest: InstructionOperandDest,
        #[allow(dead_code)]
        source: InstructionOperand,
        opsize: i32,
    },
    NonZeroShift {
        dest: InstructionOperandDest,
        opsize: i32,
    },
    Bitwise {
        dest: InstructionOperandDest,
        opsize: i32,
    },
    /// a compare whose flags only the conditional jump after it reads: it left that jump's condition (by its low
    /// four bits) in JitContext::condition_local
    Condition {
        condition: u8,
    },
    Other,
}

pub struct JitContext<'a> {
    pub cpu: &'a mut CpuContext,
    pub builder: &'a mut WasmBuilder,
    pub register_locals: &'a mut Vec<WasmLocal>,
    pub start_of_current_instruction: u32,
    pub exit_with_fault_label: Label,
    pub exit_label: Label,
    pub current_instruction: Instruction,
    pub previous_instruction: Instruction,
    pub instruction_counter: WasmLocal,
    /// the part of instruction_counter that is in the global counter already (nested calls, gen_nested_call)
    pub instruction_counter_flushed: WasmLocal,
    pub wasm_table_index: WasmTableIndex,
    /// the arithmetic flags the current instruction writes are overwritten before anything reads them (see
    /// flags_dead_after): its emitter may leave out storing them
    pub flags_dead: bool,
    /// the current instruction's flags are read only by the conditional jump right after it (the block's last
    /// instruction, with this condition), and are dead after that: when the jump computes its condition from locals
    /// (codegen::condition_from_locals), the emitter may leave out storing them
    pub flags_jcc_only: Option<u8>,
    /// per basic block of the module: whether the flags may be read after its end, and what its instructions do to
    /// them (see flags_live_out)
    pub flags_live_out: HashMap<u32, (bool, Vec<(bool, bool)>)>,
    /// see Instruction::Condition
    pub condition_local: WasmLocal,
    /// the address modrm::gen computed last, when it is a register plus a displacement (codegen::gen_access_anchor)
    pub address_hint: Option<AddressHint>,
    /// the registers' pages whose TLB entries were loaded in this basic block (codegen::gen_access_anchor)
    pub anchors: Vec<PageAnchor>,
}

#[derive(Copy, Clone)]
pub struct AddressHint {
    pub reg: u32,
    pub disp: i32,
    /// the register local's version (WasmBuilder::local_version) when the address was computed
    pub reg_version: u32,
    /// the body's length at the end of the computation: the store of the address into its local follows
    pub pos: u32,
    /// the address is the register's own local (no displacement: gen_modrm_resolve_with_local), not a local
    /// written at pos
    pub is_reg_local: bool,
}

pub struct PageAnchor {
    pub reg: u32,
    pub version: u32,
    pub token: (u32, usize, u32),
    pub local: WasmLocal,
}
impl<'a> JitContext<'a> {
    pub fn reg(&self, i: u32) -> WasmLocal {
        match self.register_locals.get(i as usize) {
            Some(x) => x.unsafe_clone(),
            None => {
                dbg_assert!(false);
                unsafe { std::hint::unreachable_unchecked() }
            },
        }
    }
}

pub const JIT_INSTR_BLOCK_BOUNDARY_FLAG: u32 = 1 << 0;

pub fn is_near_end_of_page(address: u32) -> bool {
    address & 0xFFF >= 0x1000 - MAX_INSTRUCTION_LENGTH
}

/// Whether the instruction at physical address `phys` ends before its page's last byte, so that it can be compiled (an
/// instruction that crosses into the next page is the interpreter's: the next page may map elsewhere). Near the
/// page's end the instruction is decoded; conservatively no when the next physical page is device memory or past
/// RAM (decoding a crossing instruction reads its bytes)
pub fn instruction_fits_in_page(phys: u32, cpu: &CpuContext) -> bool {
    if !is_near_end_of_page(phys) {
        return true;
    }
    if !unsafe { COMPILE_NEAR_END_OF_PAGE } {
        return false;
    }
    let next_page = (phys | 0xFFF) + 1;
    if next_page >= unsafe { *crate::cpu::global_pointers::memory_size } || memory::in_mapped_range(next_page) {
        return false;
    }
    let mut c = CpuContext { eip: phys, ..cpu.clone() };
    analysis::analyze_step(&mut c);
    // (strictly before the page's end: compiled code sets eip as the page's base and the low 12 bits of the next
    // instruction, which would wrap around to the page's start)
    c.eip > phys && c.eip < next_page
}

/// Modules compiled at the same time (set_jit_config(70, ..)). The engine compiles them in the background; with one at
/// a time, hot pages wait for it, interpreted, while a boot finds code faster than it is compiled
pub static mut MAX_COMPILING: u32 = 4;

/// Whether a page is part of a module that is still being compiled
fn page_is_compiling(ctx: &JitState, page: Page) -> bool {
    ctx.compiling.iter().any(|(_, state)| matches!(state, CompilingPageState::Compiling { pages } if pages.contains_key(&page)))
}

/// Compile instructions in the last 15 bytes of a page when they end within it (set_jit_config(69, ..))
pub static mut COMPILE_NEAR_END_OF_PAGE: bool = true;

/// Compile an instruction that crosses into the next page at the end of a block (set_jit_config(71, ..))
pub static mut COMPILE_CROSSING: bool = true;

/// An instruction at physical `phys` (virtual `virt`) that crosses into the next page and can end a compiled block:
/// where it ends, and for a relative jump its condition and offset. Only plain instructions (no use of their own eip:
/// an allowlist of opcodes) and relative jumps (jcc, jmp, call), in flat 32-bit
/// code, when the next virtual page maps to the next physical page now (the JIT decodes physical memory). When it
/// runs, the compiled code checks that its bytes in the next page are still those (through the TLB: a remapped or
/// rewritten page, a TLB miss, leave it to the interpreter)
fn crossing_instruction(phys: u32, virt: i32, cpu: &CpuContext) -> Option<(u32, Option<(Option<u8>, i32)>)> {
    if !unsafe { COMPILE_CROSSING } || phys & 0xFFF == 0 || !is_near_end_of_page(phys) {
        return None;
    }
    if !cpu.state_flags.is_32() || !cpu.state_flags.has_flat_segmentation() || cpu.cs_offset != 0 {
        return None;
    }
    let next_page = (phys | 0xFFF) + 1;
    if next_page + 0x1000 > unsafe { *crate::cpu::global_pointers::memory_size } || memory::in_mapped_range(next_page) {
        return None;
    }
    if cpu::translate_address_read_no_side_effects((virt | 0xFFF) + 1) != Ok(next_page) {
        return None;
    }
    if needs_atomic_interpretation(phys) {
        return None;
    }
    // the opcode (after an operand size prefix): plain instructions only
    let byte = |i: u32| memory::read8(phys + i) as u8;
    let skip = (byte(0) == 0x66) as u32;
    let (op, op2) = (byte(skip), byte(skip + 1));
    let allowed = match op {
        // cmovcc, setcc, imul, movzx, movsx; jcc rel32 (first: 0x0F is also in the next range)
        0x0F => matches!(op2, 0x40..=0x4F | 0x90..=0x9F | 0xAF | 0xB6 | 0xB7 | 0xBE | 0xBF | 0x80..=0x8F),
        0x00..=0x3F => op & 7 <= 5,
        0x68..=0x6B | 0x80..=0x8B | 0x8D | 0xA0..=0xA3 | 0xA8 | 0xA9 | 0xB0..=0xBF | 0xC0 | 0xC1 | 0xC6 | 0xC7
        | 0xD0..=0xD3 => true,
        // test, not, neg, mul, imul (not div: #DE is fine, but keep it simple)
        0xF6 | 0xF7 => matches!(op2 >> 3 & 7, 0 | 2 | 3 | 4 | 5),
        // jcc rel8, jmp rel32, jmp rel8, call rel32 (32-bit: not after an operand size prefix; the call pushes its
        // return address in the next page: codegen::gen_get_real_eip)
        0x70..=0x7F | 0xE9 | 0xEB | 0xE8 => skip == 0,
        _ => false,
    };
    if !allowed {
        return None;
    }
    let mut c = CpuContext { eip: phys, ..cpu.clone() };
    let analysis = analysis::analyze_step(&mut c);
    let jump = match analysis.ty {
        AnalysisType::Normal if !analysis.no_next_instruction => None,
        AnalysisType::Jump { offset, is_32: true, condition } if !analysis.absolute_jump => Some((condition, offset)),
        _ => return None,
    };
    if skip != 0 && jump.is_some() {
        return None;
    }
    let end = c.eip;
    if end <= next_page || end - next_page > 14 {
        return None;
    }
    Some((end, jump))
}

pub fn jit_find_cache_entry(phys_address: u32, state_flags: CachedStateFlags) -> CachedCode {
    // TODO: dedup with jit_find_cache_entry_in_page?
    // NOTE: This is currently only used for invariant/missed-entry-point checking
    let ctx = get_jit_state();

    match ctx.pages.get(&Page::page_of(phys_address)) {
        Some(PageInfo {
            wasm_table_index,
            state_flags: s,
            entry_points,
            hidden_wasm_table_indices: _,
        }) => {
            if *s == state_flags {
                let page_offset = phys_address as u16 & 0xFFF;
                if let Some(&(_, initial_state)) =
                    entry_points.iter().find(|(p, _)| p == &page_offset)
                {
                    return CachedCode {
                        wasm_table_index: *wasm_table_index,
                        initial_state,
                    };
                }
            }
        },
        None => {},
    }

    return CachedCode::NONE;
}

#[no_mangle]
pub fn jit_find_cache_entry_in_page(
    virt_address: u32,
    wasm_table_index: WasmTableIndex,
    state_flags: u32,
) -> i32 {
    // TODO: generate code for this
    profiler::stat_increment(stat::INDIRECT_JUMP);

    let state_flags = CachedStateFlags::of_u32(state_flags);

    unsafe {
        match cpu::tlb_code[(virt_address >> 12) as usize] {
            None => profiler::stat_increment(stat::INDIRECT_JUMP_NO_CODE),
            Some(c) => {
                let c = c.as_ref();
                if state_flags == c.state_flags && wasm_table_index == c.wasm_table_index {
                    let state = c.state_table[virt_address as usize & 0xFFF];
                    if state != u16::MAX {
                        return state.into();
                    }
                    profiler::stat_increment(stat::INDIRECT_JUMP_SAME_MODULE_NOT_ENTRY);
                }
                else if state_flags != c.state_flags {
                    profiler::stat_increment(stat::INDIRECT_JUMP_OTHER_STATE);
                }
                else {
                    profiler::stat_increment(stat::INDIRECT_JUMP_OTHER_MODULE);
                }
            },
        }
    }

    profiler::stat_increment(stat::INDIRECT_JUMP_NO_ENTRY);

    return -1;
}

fn jit_find_basic_blocks(
    ctx: &mut JitState,
    entry_points: HashSet<i32>,
    cpu: CpuContext,
) -> Vec<BasicBlock> {
    fn follow_jump(
        virt_target: i32,
        ctx: &mut JitState,
        pages: &mut HashSet<Page>,
        page_blacklist: &mut HashSet<Page>,
        max_pages: u32,
        marked_as_entry: &mut HashSet<i32>,
        to_visit_stack: &mut Vec<i32>,
        cpu_ctx: &CpuContext,
    ) -> Option<u32> {
        let phys_target = match cpu::translate_address_read_no_side_effects(virt_target) {
            Err(()) => {
                dbg_log!("Not analysing {:x} (page not mapped)", virt_target);
                return None;
            },
            Ok(t) => t,
        };

        if !instruction_fits_in_page(phys_target, cpu_ctx) {
            return None;
        }

        let phys_page = Page::page_of(phys_target);

        if !pages.contains(&phys_page) && pages.len() as u32 == max_pages
            || page_blacklist.contains(&phys_page)
        {
            return None;
        }

        // (a page that is in a module still being compiled: not in two modules at once)
        if !pages.contains(&phys_page) && page_is_compiling(ctx, phys_page) {
            page_blacklist.insert(phys_page);
            return None;
        }

        if !pages.contains(&phys_page) {
            // page seen for the first time, handle entry points
            if let Some((hotness, entry_points)) = ctx.entry_points.get_mut(&phys_page) {
                let existing_entry_points: HashSet<u16> = match ctx.pages.get(&phys_page) {
                    Some(PageInfo { entry_points, .. }) => {
                        HashSet::from_iter(entry_points.iter().map(|x| x.0))
                    },
                    None => HashSet::default(),
                };

                if entry_points
                    .iter()
                    .all(|entry_point| existing_entry_points.contains(&entry_point))
                {
                    page_blacklist.insert(phys_page);
                    return None;
                }

                // XXX: Remove this paragraph
                //let old_length = entry_points.len();
                //entry_points.extend(existing_entry_points);
                //dbg_assert!(
                //    entry_points.union(&existing_entry_points).count() == entry_points.len()
                //);

                *hotness = 0;

                for addr_low in entry_points.iter() {
                    let addr = virt_target & !0xFFF | addr_low as i32;
                    to_visit_stack.push(addr);
                    marked_as_entry.insert(addr);
                }
            }
            else {
                // no entry points: ignore this page?
                page_blacklist.insert(phys_page);
                return None;
            }

            pages.insert(phys_page);
            dbg_assert!(pages.len() as u32 <= max_pages);
        }

        to_visit_stack.push(virt_target);
        Some(phys_target)
    }

    let mut to_visit_stack: Vec<i32> = Vec::new();
    let mut marked_as_entry: HashSet<i32> = HashSet::default();
    let mut basic_blocks: BTreeMap<u32, BasicBlock> = BTreeMap::new();
    let mut pages: HashSet<Page> = HashSet::default();
    let mut page_blacklist = HashSet::default();

    // 16-bit doesn't work correctly, most likely due to instruction pointer wrap-around
    let max_pages = if cpu.state_flags.is_32() { unsafe { MAX_PAGES } } else { 1 };

    for virt_addr in entry_points {
        let ok = follow_jump(
            virt_addr,
            ctx,
            &mut pages,
            &mut page_blacklist,
            max_pages,
            &mut marked_as_entry,
            &mut to_visit_stack,
            &cpu,
        );
        dbg_assert!(ok.is_some());
        dbg_assert!(marked_as_entry.contains(&virt_addr));
    }

    while let Some(to_visit) = to_visit_stack.pop() {
        let phys_addr = match cpu::translate_address_read_no_side_effects(to_visit) {
            Err(()) => {
                dbg_log!("Not analysing {:x} (page not mapped)", to_visit);
                continue;
            },
            Ok(phys_addr) => phys_addr,
        };

        if basic_blocks.contains_key(&phys_addr) {
            continue;
        }

        if !instruction_fits_in_page(phys_addr, &cpu) {
            // a block of only an instruction that crosses into the next page (a jump's target or a call's return)
            if let Some((end, jump)) = crossing_instruction(phys_addr, to_visit, &cpu) {
                basic_blocks.insert(phys_addr, BasicBlock {
                    addr: phys_addr,
                    virt_addr: to_visit,
                    last_instruction_addr: phys_addr,
                    end_addr: end,
                    ty: BasicBlockType::Exit,
                    is_entry_block: false,
                    is_call: false,
                    has_sti: false,
                    number_of_instructions: 1,
                    crossing: true,
                    crossing_jump: jump,
                    rep_string: None,
                });
                profiler::stat_increment(stat::COMPILE_CROSSING_INSTRUCTION);
                continue;
            }
            // Empty basic block, don't insert
            profiler::stat_increment(stat::COMPILE_CUT_OFF_AT_END_OF_PAGE);
            continue;
        }

        if needs_atomic_interpretation(phys_addr) {
            // (empty basic block: the interpreter runs the instruction)
            continue;
        }

        let mut current_address = phys_addr;
        let mut current_block = BasicBlock {
            addr: current_address,
            virt_addr: to_visit,
            last_instruction_addr: 0,
            end_addr: 0,
            ty: BasicBlockType::Exit,
            is_entry_block: false,
            is_call: false,
            has_sti: false,
            number_of_instructions: 0,
            crossing: false,
            crossing_jump: None,
            rep_string: None,
        };
        loop {
            let addr_before_instruction = current_address;
            if current_block.number_of_instructions > 0 && needs_atomic_interpretation(current_address) {
                // the block ends before the instruction, the interpreter runs it
                break;
            }
            let mut cpu = &mut CpuContext {
                eip: current_address,
                ..cpu
            };
            let analysis = analysis::analyze_step(&mut cpu);
            let has_next_instruction = !analysis.no_next_instruction;
            current_address = cpu.eip;

            dbg_assert!(Page::page_of(current_address - 1) == Page::page_of(addr_before_instruction));
            // (the next instruction is in this page and ends within it; not after an instruction that ended at the
            // page's end: its next address belongs to another page)
            let next_fits = current_address & 0xFFF != 0 && instruction_fits_in_page(current_address, cpu);
            let current_virt_addr = to_visit & !0xFFF | current_address as i32 & 0xFFF;

            if analysis.ty == AnalysisType::STI && !next_fits {
                // cut off before the STI so that it is handled by interpreted mode
                profiler::stat_increment(stat::COMPILE_CUT_OFF_AT_END_OF_PAGE);
                break;
            }

            current_block.number_of_instructions += 1;
            current_block.last_instruction_addr = addr_before_instruction;
            current_block.end_addr = current_address;

            match analysis.ty {
                AnalysisType::Normal | AnalysisType::STI => {
                    dbg_assert!(has_next_instruction);
                    dbg_assert!(!analysis.absolute_jump);

                    if current_block.has_sti {
                        // Convert next instruction after STI (i.e., the current instruction) into block boundary

                        marked_as_entry.insert(current_virt_addr);
                        to_visit_stack.push(current_virt_addr);

                        break;
                    }

                    if analysis.ty == AnalysisType::STI {
                        dbg_assert!(next_fits, "should be handled above");

                        current_block.has_sti = true;
                    }
                    else {
                        // Only split non-STI blocks (one instruction needs to run after STI before
                        // handle_irqs may be called)

                        if basic_blocks.contains_key(&current_address) {
                            dbg_assert!(next_fits);
                            current_block.ty = BasicBlockType::Normal {
                                next_block_addr: Some(current_address),
                                jump_offset: 0,
                                jump_offset_is_32: true,
                            };
                            break;
                        }
                    }
                },
                AnalysisType::Jump {
                    offset,
                    is_32,
                    condition: Some(condition),
                } => {
                    dbg_assert!(!analysis.absolute_jump);
                    // conditional jump: continue at next and continue at jump target

                    let jump_target = if is_32 {
                        current_virt_addr + offset
                    }
                    else {
                        cpu.cs_offset as i32
                            + (current_virt_addr - cpu.cs_offset as i32 + offset & 0xFFFF)
                    };

                    dbg_assert!(has_next_instruction);
                    to_visit_stack.push(current_virt_addr);

                    let next_block_addr = if !next_fits {
                        None
                    }
                    else {
                        Some(current_address)
                    };

                    current_block.ty = BasicBlockType::ConditionalJump {
                        next_block_addr,
                        next_block_branch_taken_addr: follow_jump(
                            jump_target,
                            ctx,
                            &mut pages,
                            &mut page_blacklist,
                            max_pages,
                            &mut marked_as_entry,
                            &mut to_visit_stack,
                            &*cpu,
                        ),
                        condition,
                        jump_offset: offset,
                        jump_offset_is_32: is_32,
                    };

                    break;
                },
                AnalysisType::Jump {
                    offset,
                    is_32,
                    condition: None,
                } => {
                    dbg_assert!(!analysis.absolute_jump);
                    // non-conditional jump: continue at jump target

                    let jump_target = if is_32 {
                        current_virt_addr + offset
                    }
                    else {
                        cpu.cs_offset as i32
                            + (current_virt_addr - cpu.cs_offset as i32 + offset & 0xFFFF)
                    };

                    if has_next_instruction {
                        // Execution will eventually come back to the next instruction (CALL)
                        marked_as_entry.insert(current_virt_addr);
                        to_visit_stack.push(current_virt_addr);
                        current_block.is_call = true;
                    }

                    current_block.ty = BasicBlockType::Normal {
                        next_block_addr: follow_jump(
                            jump_target,
                            ctx,
                            &mut pages,
                            &mut page_blacklist,
                            max_pages,
                            &mut marked_as_entry,
                            &mut to_visit_stack,
                            &*cpu,
                        ),
                        jump_offset: offset,
                        jump_offset_is_32: is_32,
                    };

                    break;
                },
                AnalysisType::BlockBoundary => {
                    // a block boundary but not a jump, get out

                    if has_next_instruction {
                        // block boundary, but execution will eventually come back
                        // to the next instruction. Create a new basic block
                        // starting at the next instruction and register it as an
                        // entry point
                        marked_as_entry.insert(current_virt_addr);
                        to_visit_stack.push(current_virt_addr);
                    }

                    if analysis.absolute_jump {
                        current_block.ty = BasicBlockType::AbsoluteEip;
                    }
                    else if has_next_instruction && next_fits && unsafe { REP_STRING_CONTINUE } {
                        if let Some(kind) = rep_string_instruction(addr_before_instruction) {
                            current_block.rep_string = Some(kind);
                            current_block.ty = BasicBlockType::Normal {
                                next_block_addr: Some(current_address),
                                jump_offset: 0,
                                jump_offset_is_32: true,
                            };
                        }
                    }

                    break;
                },
            }

            if !next_fits {
                // (an instruction crossing into the next page: the block's last one)
                if !current_block.has_sti {
                    if let Some((end, jump)) = crossing_instruction(current_address, current_virt_addr, cpu) {
                        current_block.number_of_instructions += 1;
                        current_block.last_instruction_addr = current_address;
                        current_block.end_addr = end;
                        current_block.crossing = true;
                        current_block.crossing_jump = jump;
                        profiler::stat_increment(stat::COMPILE_CROSSING_INSTRUCTION);
                    }
                }
                profiler::stat_increment(stat::COMPILE_CUT_OFF_AT_END_OF_PAGE);
                break;
            }
        }

        if current_block.number_of_instructions == 0 {
            // Empty basic block, don't insert (only happens when STI is found near end of page)
            continue;
        }

        let previous_block = basic_blocks
            .range(..current_block.addr)
            .next_back()
            .filter(|(_, previous_block)| !previous_block.has_sti)
            .map(|(_, previous_block)| previous_block);

        if let Some(previous_block) = previous_block {
            if current_block.addr < previous_block.end_addr {
                // If this block overlaps with the previous block, re-analyze the previous block
                to_visit_stack.push(previous_block.virt_addr);

                let addr = previous_block.addr;
                let old_block = basic_blocks.remove(&addr);
                dbg_assert!(old_block.is_some());

                // Note that this does not ensure the invariant that two consecutive blocks don't
                // overlay. For that, we also need to check the following block.
            }
        }

        dbg_assert!(current_block.addr < current_block.end_addr);
        dbg_assert!(current_block.addr <= current_block.last_instruction_addr);
        dbg_assert!(current_block.last_instruction_addr < current_block.end_addr);

        basic_blocks.insert(current_block.addr, current_block);
    }

    dbg_assert!(pages.len() as u32 <= max_pages);

    for block in basic_blocks.values_mut() {
        if marked_as_entry.contains(&block.virt_addr) {
            block.is_entry_block = true;
        }
    }

    // delete edges pointing to blocks that were dropped (currently only due to STI near the end of a page)
    let known_addresses: HashSet<u32> = basic_blocks.keys().copied().collect();
    for block in basic_blocks.values_mut() {
        match &mut block.ty {
            BasicBlockType::Normal {
                next_block_addr, ..
            } => {
                if next_block_addr.map_or(false, |a| !known_addresses.contains(&a)) {
                    *next_block_addr = None;
                }
            },
            BasicBlockType::ConditionalJump {
                next_block_addr,
                next_block_branch_taken_addr,
                ..
            } => {
                if next_block_addr.map_or(false, |a| !known_addresses.contains(&a)) {
                    *next_block_addr = None;
                }
                if next_block_branch_taken_addr.map_or(false, |a| !known_addresses.contains(&a)) {
                    *next_block_branch_taken_addr = None;
                }
            },
            BasicBlockType::Exit | BasicBlockType::AbsoluteEip => {},
        }
    }

    let basic_blocks: Vec<BasicBlock> = basic_blocks.into_iter().map(|(_, block)| block).collect();

    for i in 0..basic_blocks.len() - 1 {
        let next_block_addr = basic_blocks[i + 1].addr;
        let next_block_end_addr = basic_blocks[i + 1].end_addr;
        let next_block_is_entry = basic_blocks[i + 1].is_entry_block;
        let block = &basic_blocks[i];
        dbg_assert!(block.addr < next_block_addr);
        if next_block_addr < block.end_addr {
            dbg_log!(
                "Overlapping first=[from={:x} to={:x} is_entry={}] second=[from={:x} to={:x} is_entry={}]",
                block.addr,
                block.end_addr,
                block.is_entry_block as u8,
                next_block_addr,
                next_block_end_addr,
                next_block_is_entry as u8
            );
        }
    }

    basic_blocks
}

#[no_mangle]
#[cfg(debug_assertions)]
pub fn jit_force_generate_unsafe(virt_addr: i32) {
    dbg_assert!(
        !is_near_end_of_page(virt_addr as u32),
        "cannot force compile near end of page"
    );
    jit_increase_hotness_and_maybe_compile(
        virt_addr,
        cpu::translate_address_read(virt_addr).unwrap(),
        cpu::get_seg_cs() as u32,
        cpu::get_state_flags(),
        unsafe { JIT_THRESHOLD_NOW },
    );
    dbg_assert!(!get_jit_state().compiling.is_empty());
}

#[inline(never)]
fn jit_analyze_and_generate(
    ctx: &mut JitState,
    virt_entry_point: i32,
    phys_entry_point: u32,
    cs_offset: u32,
    state_flags: CachedStateFlags,
) {
    let page = Page::page_of(phys_entry_point);

    if page_is_compiling(ctx, page) {
        // (its module is not finished yet: compiled again when it is, with the entry points found until then)
        return;
    }

    let (_, entry_points) = match ctx.entry_points.get(&page) {
        None => return,
        Some(entry_points) => entry_points,
    };

    let existing_entry_points: HashSet<u16> = match ctx.pages.get(&page) {
        Some(PageInfo { entry_points, .. }) => HashSet::from_iter(entry_points.iter().map(|x| x.0)),
        None => HashSet::default(),
    };

    if entry_points
        .iter()
        .all(|entry_point| existing_entry_points.contains(&entry_point))
    {
        profiler::stat_increment(stat::COMPILE_SKIPPED_NO_NEW_ENTRY_POINTS);
        return;
    }

    // XXX: check and remove
    //let old_length = entry_points.len();
    //entry_points.extend(existing_entry_points);
    //dbg_log!(
    //    "{} + {} = {}",
    //    entry_points.len(),
    //    existing_entry_points.len(),
    //    entry_points.union(&existing_entry_points).count()
    //);
    //dbg_assert!(entry_points.union(&existing_entry_points).count() == entry_points.len());

    profiler::stat_increment(stat::COMPILE);

    let cpu = CpuContext {
        eip: 0,
        prefixes: 0,
        cs_offset,
        state_flags,
    };

    dbg_assert!(
        cpu::translate_address_read_no_side_effects(virt_entry_point).unwrap() == phys_entry_point
    );
    let virt_page = Page::page_of(virt_entry_point as u32);
    let entry_points: HashSet<i32> = entry_points
        .iter()
        .map(|e| virt_page.to_address() as i32 | e as i32)
        .collect();
    let basic_blocks = jit_find_basic_blocks(ctx, entry_points, cpu.clone());

    let mut pages: HashSet<Page> = HashSet::default();

    for b in basic_blocks.iter() {
        // (a crossing last instruction ends in the next page)
        dbg_assert!(b.crossing || Page::page_of(b.addr) == Page::page_of(b.end_addr - 1));
        pages.insert(Page::page_of(b.addr));
    }

    let print = false;

    for b in basic_blocks.iter() {
        if !print {
            break;
        }
        let last_instruction_opcode = memory::read32s(b.last_instruction_addr);
        let op = opstats::decode(last_instruction_opcode as u32);
        dbg_log!(
            "BB: 0x{:x} {}{:02x} {} {}",
            b.addr,
            if op.is_0f { "0f" } else { "" },
            op.opcode,
            if b.is_entry_block { "entry" } else { "noentry" },
            match &b.ty {
                BasicBlockType::ConditionalJump {
                    next_block_addr: Some(next_block_addr),
                    next_block_branch_taken_addr: Some(next_block_branch_taken_addr),
                    ..
                } => format!(
                    "0x{:x} 0x{:x}",
                    next_block_addr, next_block_branch_taken_addr
                ),
                BasicBlockType::ConditionalJump {
                    next_block_addr: None,
                    next_block_branch_taken_addr: Some(next_block_branch_taken_addr),
                    ..
                } => format!("0x{:x}", next_block_branch_taken_addr),
                BasicBlockType::ConditionalJump {
                    next_block_addr: Some(next_block_addr),
                    next_block_branch_taken_addr: None,
                    ..
                } => format!("0x{:x}", next_block_addr),
                BasicBlockType::ConditionalJump {
                    next_block_addr: None,
                    next_block_branch_taken_addr: None,
                    ..
                } => format!(""),
                BasicBlockType::Normal {
                    next_block_addr: Some(next_block_addr),
                    ..
                } => format!("0x{:x}", next_block_addr),
                BasicBlockType::Normal {
                    next_block_addr: None,
                    ..
                } => format!(""),
                BasicBlockType::Exit => format!(""),
                BasicBlockType::AbsoluteEip => format!(""),
            }
        );
    }

    let graph = control_flow::make_graph(&basic_blocks);
    let mut structure = control_flow::loopify(&graph);

    if print {
        dbg_log!("before blockify:");
        for group in &structure {
            dbg_log!("=> Group");
            group.print(0);
        }
    }

    control_flow::blockify(&mut structure, &graph);

    if cfg!(debug_assertions) {
        control_flow::assert_invariants(&structure);
    }

    if print {
        dbg_log!("after blockify:");
        for group in &structure {
            dbg_log!("=> Group");
            group.print(0);
        }
    }

    if ctx.wasm_table_index_free_list.is_empty() {
        dbg_log!("wasm_table_index_free_list empty, clearing cache");

        // When no free slots are available, delete all cached modules. We could increase the
        // size of the table, but this way the initial size acts as an upper bound for the
        // number of wasm modules that we generate, which we want anyway to avoid getting our
        // tab killed by browsers due to memory constraints.
        jit_clear_cache(ctx);

        profiler::stat_increment(stat::INVALIDATE_ALL_MODULES_NO_FREE_WASM_INDICES);

        dbg_log!(
            "after jit_clear_cache: {} free",
            ctx.wasm_table_index_free_list.len(),
        );

        // This assertion can fail if all entries are pending (not possible unless
        // WASM_TABLE_SIZE is set very low)
        dbg_assert!(!ctx.wasm_table_index_free_list.is_empty());
    }

    // allocate an index in the wasm table
    let wasm_table_index = ctx
        .wasm_table_index_free_list
        .pop()
        .expect("allocate wasm table index");
    dbg_assert!(wasm_table_index != WasmTableIndex(0));

    dbg_assert!(!pages.is_empty());
    dbg_assert!(pages.len() <= unsafe { MAX_PAGES } as usize);

    let basic_block_by_addr: HashMap<u32, BasicBlock> =
        basic_blocks.into_iter().map(|b| (b.addr, b)).collect();

    let entries = jit_generate_module(
        structure,
        &basic_block_by_addr,
        cpu,
        &mut ctx.wasm_builder,
        wasm_table_index,
        state_flags,
    );
    dbg_assert!(!entries.is_empty());

    let mut page_info = HashMap::default();
    for &p in &pages {
        page_info.entry(p).or_insert_with(|| PageInfo {
            wasm_table_index,
            state_flags,
            entry_points: Vec::new(),
            hidden_wasm_table_indices: Vec::new(),
        });
        maybe_has_code_set(p);
        ctx.entry_points
            .entry(p)
            .or_insert_with(|| (0, EntrySet::default()));
    }
    for &(addr, state) in &entries {
        let code = page_info.get_mut(&Page::page_of(addr)).unwrap();
        code.entry_points.push((addr as u16 & 0xFFF, state));
    }

    profiler::stat_increment_by(
        stat::COMPILE_WASM_TOTAL_BYTES,
        ctx.wasm_builder.get_output_len() as u64,
    );
    profiler::stat_increment_by(stat::COMPILE_PAGE, pages.len() as u64);

    cpu::tlb_set_has_code_multiple(&pages, true);

    ctx.compiling.push((
        wasm_table_index,
        CompilingPageState::Compiling { pages: page_info },
    ));

    let phys_addr = page.to_address();

    // will call codegen_finalize_finished asynchronously when finished
    codegen_finalize(
        wasm_table_index,
        phys_addr,
        state_flags,
        ctx.wasm_builder.get_output_ptr() as u32,
        ctx.wasm_builder.get_output_len(),
    );

    check_jit_state_invariants(ctx);
}

#[no_mangle]
pub fn codegen_finalize_finished(
    wasm_table_index: WasmTableIndex,
    phys_addr: u32,
    state_flags: CachedStateFlags,
) {
    if state_flags.is_jit64() {
        unsafe { crate::cpu::jit64::finalize_finished(wasm_table_index.to_u16()) };
        return;
    }

    let mut ctx = get_jit_state();

    dbg_assert!(wasm_table_index != WasmTableIndex(0));

    dbg_log!(
        "Finished compiling for page at {:x}",
        Page::page_of(phys_addr).to_address()
    );

    let position = ctx.compiling.iter().position(|(index, _)| *index == wasm_table_index);
    let finished = match position {
        Some(i) => Some(ctx.compiling.remove(i)),
        None => None,
    };
    let pages = match finished {
        None => {
            dbg_assert!(false);
            return;
        },
        Some((in_progress_wasm_table_index, CompilingPageState::CompilingWritten)) => {
            dbg_assert!(wasm_table_index == in_progress_wasm_table_index);

            profiler::stat_increment(stat::INVALIDATE_MODULE_WRITTEN_WHILE_COMPILED);
            free_wasm_table_index(&mut ctx, wasm_table_index);
            check_jit_state_invariants(&mut ctx);
            return;
        },
        Some((in_progress_wasm_table_index, CompilingPageState::Compiling { pages })) => {
            dbg_assert!(wasm_table_index == in_progress_wasm_table_index);
            dbg_assert!(!pages.is_empty());
            pages
        },
    };

    let compiled_pages: HashSet<Page> = pages.keys().copied().collect();

    #[cfg(debug_assertions)]
    if CHECK_JIT_STATE_INVARIANTS {
        ctx.wasm_table_index_to_page
            .insert(wasm_table_index, pages.keys().copied().collect());
    }

    let mut check_for_unused_wasm_table_index: HashSet<WasmTableIndex> = HashSet::default();

    for (page, mut info) in pages {
        dbg_assert!(info.state_flags == state_flags);
        if let Some(old_entry) = ctx.pages.remove(&page) {
            info.hidden_wasm_table_indices
                .extend(old_entry.hidden_wasm_table_indices);
            info.hidden_wasm_table_indices
                .push(old_entry.wasm_table_index);
            check_for_unused_wasm_table_index.insert(old_entry.wasm_table_index);
        }
        ctx.pages.insert(page, info);
    }

    for i in 0..unsafe { cpu::valid_tlb_entries_count } {
        let page = unsafe { cpu::valid_tlb_entries[i as usize] };
        let entry = unsafe { cpu::tlb_data[page as usize] };
        if 0 != entry {
            let tlb_physical_page = Page::of_u32(
                (entry as u32 >> 12 ^ page as u32) - (unsafe { memory::mem8 } as u32 >> 12),
            );
            if compiled_pages.contains(&tlb_physical_page) {
                update_tlb_code_ctx(&ctx, Page::of_u32(page as u32), tlb_physical_page);
            }
        }
    }

    let unused: Vec<&WasmTableIndex> = check_for_unused_wasm_table_index
        .iter()
        .filter(|&&i| ctx.pages.values().all(|page| page.wasm_table_index != i))
        .collect();

    for &index in unused {
        for p in ctx.pages.values_mut() {
            p.hidden_wasm_table_indices.retain(|&w| w != index);
        }

        dbg_log!("unused after overwrite {}", index.to_u16());
        profiler::stat_increment(stat::INVALIDATE_MODULE_UNUSED_AFTER_OVERWRITE);
        free_wasm_table_index(&mut ctx, index);
    }

    check_jit_state_invariants(&mut ctx);
}

pub fn update_tlb_code(virt_page: Page, phys_page: Page) {
    update_tlb_code_ctx(&get_jit_state(), virt_page, phys_page)
}

fn update_tlb_code_ctx(ctx: &JitState, virt_page: Page, phys_page: Page) {
    if unsafe { cpu::tlb_data[virt_page.to_u32() as usize] } & (cpu::TLB_VALID | cpu::TLB_NO_EXEC)
        != cpu::TLB_VALID
    {
        cpu::clear_tlb_code(virt_page.to_u32() as i32);
        return;
    }

    let info = match ctx.pages.get(&phys_page) {
        Some(info) => info,
        None => {
            cpu::clear_tlb_code(virt_page.to_u32() as i32);
            return;
        },
    };

    let c = match unsafe { cpu::tlb_code[virt_page.to_u32() as usize] } {
        None => {
            let state_table = [u16::MAX; 0x1000];
            unsafe {
                let mut c = NonNull::new_unchecked(Box::into_raw(Box::new(cpu::Code {
                    wasm_table_index: info.wasm_table_index,
                    state_flags: info.state_flags,
                    state_table,
                })));
                cpu::tlb_code[virt_page.to_u32() as usize] = Some(c);
                c.as_mut()
            }
        },
        Some(mut c) => unsafe {
            let c = c.as_mut();
            c.state_table.fill(u16::MAX);
            c.state_flags = info.state_flags;
            c.wasm_table_index = info.wasm_table_index;
            c
        },
    };

    for &(addr, state) in &info.entry_points {
        dbg_assert!(state != u16::MAX);
        c.state_table[addr as usize] = state;
    }
}

fn jit_generate_module(
    structure: Vec<WasmStructure>,
    basic_blocks: &HashMap<u32, BasicBlock>,
    mut cpu: CpuContext,
    builder: &mut WasmBuilder,
    wasm_table_index: WasmTableIndex,
    state_flags: CachedStateFlags,
) -> Vec<(u32, u16)> {
    builder.reset();

    // (the guest registers in locals; loads and stores of the ones the module does not use are removed when it is
    // complete: drop_unused_register_moves)
    let mut register_locals = (0..8)
        .map(|i| {
            let local = builder.alloc_local();
            builder.register_move(&local, false, |b| {
                b.load_fixed_i32(global_pointers::get_reg32_offset(i));
                b.set_local_untracked(&local);
            });
            local
        })
        .collect();

    builder.const_i32(0);
    let instruction_counter = builder.set_new_local();
    builder.const_i32(0);
    let instruction_counter_flushed = builder.set_new_local();
    let condition_local = builder.alloc_local();

    let exit_label = builder.block_void();
    let exit_with_fault_label = builder.block_void();
    let main_loop_label = builder.loop_void();
    if unsafe { JIT_USE_LOOP_SAFETY } {
        builder.get_local(&instruction_counter);
        // (the limit as a constant: a load on every back edge cost the tight loops; smp_set_loop_limit clears the
        // cache)
        builder.const_i32(unsafe { cpu::JIT_LOOP_LIMIT });
        builder.geu_i32();
        if cfg!(feature = "profiler") {
            builder.if_void();
            codegen::gen_debug_track_jit_exit(builder, 0);
            builder.br(exit_label);
            builder.block_end();
        }
        else {
            builder.br_if(exit_label);
        }
    }
    let brtable_default = builder.block_void();

    let flags_live_out = if unsafe { FLAG_ELISION && FLAG_ELISION_ACROSS_BLOCKS } {
        flags_live_out(&cpu, basic_blocks)
    }
    else {
        HashMap::default()
    };
    let ctx = &mut JitContext {
        cpu: &mut cpu,
        builder,
        register_locals: &mut register_locals,
        start_of_current_instruction: 0,
        exit_with_fault_label,
        exit_label,
        current_instruction: Instruction::Other,
        previous_instruction: Instruction::Other,
        instruction_counter,
        instruction_counter_flushed,
        wasm_table_index,
        flags_dead: false,
        flags_jcc_only: None,
        flags_live_out,
        condition_local,
        address_hint: None,
        anchors: Vec::new(),
    };

    let entry_blocks = {
        let mut nodes = &structure;
        let result;
        loop {
            match &nodes[0] {
                WasmStructure::Dispatcher(e) => {
                    result = e.clone();
                    break;
                },
                WasmStructure::Loop { .. } => {
                    dbg_assert!(false);
                },
                WasmStructure::BasicBlock(_) => {
                    dbg_assert!(false);
                },
                // Note: We could use these blocks as entry points, which will yield
                // more entries for free, but it requires adding those to the dispatcher
                // It's to be investigated if this yields a performance improvement
                // See also the comment at the bottom of this function when creating entry
                // points
                WasmStructure::Block(children) => {
                    nodes = children;
                },
            }
        }
        result
    };

    let mut index_for_addr: HashMap<u32, u16> = HashMap::default();
    for (i, &addr) in entry_blocks.iter().enumerate() {
        dbg_assert!(i < 0x10000);
        index_for_addr.insert(addr, i as u16);
    }
    for b in basic_blocks.values() {
        if !index_for_addr.contains_key(&b.addr) {
            let i = index_for_addr.len();
            dbg_assert!(i < 0x10000);
            index_for_addr.insert(b.addr, i as u16);
        }
    }

    let mut label_for_addr: HashMap<u32, (Label, Option<u16>)> = HashMap::default();

    enum Work {
        WasmStructure(WasmStructure),
        BlockEnd {
            label: Label,
            targets: Vec<u32>,
            olds: HashMap<u32, (Label, Option<u16>)>,
        },
        LoopEnd {
            label: Label,
            entries: Vec<u32>,
            olds: HashMap<u32, (Label, Option<u16>)>,
        },
    }
    let mut work: VecDeque<Work> = structure
        .into_iter()
        .map(|x| Work::WasmStructure(x))
        .collect();

    while let Some(block) = work.pop_front() {
        let next_addr: Option<Vec<u32>> = work.iter().find_map(|x| match x {
            Work::WasmStructure(l) => Some(l.head().collect()),
            _ => None,
        });
        let target_block = &ctx.builder.arg_local_initial_state.unsafe_clone();

        match block {
            Work::WasmStructure(WasmStructure::BasicBlock(addr)) => {
                let block = basic_blocks.get(&addr).unwrap();
                jit_generate_basic_block(ctx, block);

                if block.has_sti {
                    match block.ty {
                        BasicBlockType::ConditionalJump {
                            condition,
                            jump_offset,
                            jump_offset_is_32,
                            ..
                        } => {
                            codegen::gen_set_eip_low_bits(
                                ctx.builder,
                                block.end_addr as i32 & 0xFFF,
                            );
                            codegen::gen_condition_fn(ctx, condition);
                            ctx.builder.if_void();
                            if jump_offset_is_32 {
                                codegen::gen_relative_jump(ctx.builder, jump_offset);
                            }
                            else {
                                codegen::gen_jmp_rel16(ctx.builder, jump_offset as u16);
                            }
                            ctx.builder.block_end();
                        },
                        BasicBlockType::Normal {
                            jump_offset,
                            jump_offset_is_32,
                            ..
                        } => {
                            if jump_offset_is_32 {
                                codegen::gen_set_eip_low_bits_and_jump_rel32(
                                    ctx.builder,
                                    block.end_addr as i32 & 0xFFF,
                                    jump_offset,
                                );
                            }
                            else {
                                codegen::gen_set_eip_low_bits(
                                    ctx.builder,
                                    block.end_addr as i32 & 0xFFF,
                                );
                                codegen::gen_jmp_rel16(ctx.builder, jump_offset as u16);
                            }
                        },
                        BasicBlockType::Exit => {},
                        BasicBlockType::AbsoluteEip => {},
                    };
                    codegen::gen_debug_track_jit_exit(ctx.builder, block.last_instruction_addr);
                    codegen::gen_move_registers_from_locals_to_memory(ctx);
                    codegen::gen_fn0_const(ctx.builder, "handle_irqs");
                    codegen::gen_update_instruction_counter(ctx);
                    ctx.builder.return_();
                    continue;
                }

                match &block.ty {
                    BasicBlockType::Exit => {
                        // Exit this function
                        codegen::gen_debug_track_jit_exit(ctx.builder, block.last_instruction_addr);
                        codegen::gen_profiler_stat_increment(ctx.builder, stat::DIRECT_EXIT);
                        ctx.builder.br(ctx.exit_label);
                    },
                    BasicBlockType::AbsoluteEip => {
                        // Check if we can stay in this module, if not exit
                        if cfg!(feature = "parallel") || cfg!(feature = "profiler") || unsafe { !INLINE_LOOKUP } {
                            codegen::gen_get_eip(ctx.builder);
                            ctx.builder.const_i32(wasm_table_index.to_u16() as i32);
                            ctx.builder.const_i32(state_flags.to_u32() as i32);
                            ctx.builder.call_fn3_ret("jit_find_cache_entry_in_page");
                        }
                        else if unsafe { LOOKUP_IN_FUNCTION } {
                            codegen::gen_get_eip(ctx.builder);
                            codegen::gen_find_entry_in_page_call(
                                ctx.builder,
                                wasm_table_index.to_u16(),
                                state_flags.to_u32() as u8,
                            );
                        }
                        else {
                            codegen::gen_get_eip(ctx.builder);
                            let eip = ctx.builder.set_new_local();
                            codegen::gen_find_entry_in_page(
                                ctx.builder,
                                &eip,
                                wasm_table_index.to_u16(),
                                state_flags.to_u32() as u8,
                            );
                            ctx.builder.free_local(eip);
                        }
                        ctx.builder.tee_local(target_block);
                        ctx.builder.const_i32(0);
                        ctx.builder.ge_i32();
                        // TODO: Could make this unconditional by including exit_label in the main br_table
                        ctx.builder.br_if(main_loop_label);

                        codegen::gen_debug_track_jit_exit(ctx.builder, block.last_instruction_addr);
                        ctx.builder.br(ctx.exit_label);
                    },
                    &BasicBlockType::Normal {
                        next_block_addr: None,
                        jump_offset,
                        jump_offset_is_32,
                    } => {
                        if jump_offset_is_32 {
                            codegen::gen_set_eip_low_bits_and_jump_rel32(
                                ctx.builder,
                                block.end_addr as i32 & 0xFFF,
                                jump_offset,
                            );
                        }
                        else {
                            codegen::gen_set_eip_low_bits(
                                ctx.builder,
                                block.end_addr as i32 & 0xFFF,
                            );
                            codegen::gen_jmp_rel16(ctx.builder, jump_offset as u16);
                        }

                        if block.is_call && unsafe { NESTED_CALLS } && !cfg!(feature = "profiler") {
                            // a call out of the module: the callee's compiled code as a call, and after its return
                            // (its ret leaves its page) on in this module at the return address
                            codegen::gen_nested_call(ctx, main_loop_label, target_block, wasm_table_index, state_flags);
                        }
                        codegen::gen_debug_track_jit_exit(ctx.builder, block.last_instruction_addr);
                        codegen::gen_profiler_stat_increment(ctx.builder, stat::DIRECT_EXIT);
                        ctx.builder.br(ctx.exit_label);
                    },
                    &BasicBlockType::Normal {
                        next_block_addr: Some(next_block_addr),
                        jump_offset,
                        jump_offset_is_32,
                    } => {
                        // Unconditional jump to next basic block
                        // - All instructions that don't change eip
                        // - Unconditional jumps

                        if Page::page_of(next_block_addr) != Page::page_of(block.addr) {
                            if jump_offset_is_32 {
                                codegen::gen_set_eip_low_bits_and_jump_rel32(
                                    ctx.builder,
                                    block.end_addr as i32 & 0xFFF,
                                    jump_offset,
                                );
                            }
                            else {
                                codegen::gen_set_eip_low_bits(
                                    ctx.builder,
                                    block.end_addr as i32 & 0xFFF,
                                );
                                codegen::gen_jmp_rel16(ctx.builder, jump_offset as u16);
                            }

                            codegen::gen_profiler_stat_increment(
                                ctx.builder,
                                stat::NORMAL_PAGE_CHANGE,
                            );

                            codegen::gen_page_switch_check(
                                ctx,
                                next_block_addr,
                                block.last_instruction_addr,
                            );

                            #[cfg(debug_assertions)]
                            codegen::gen_fn2_const(
                                ctx.builder,
                                "check_page_switch",
                                block.addr,
                                next_block_addr,
                            );
                        }

                        if next_addr
                            .as_ref()
                            .map_or(false, |n| n.contains(&next_block_addr))
                        {
                            // Blocks are consecutive
                            if next_addr.unwrap().len() > 1 {
                                let target_index = *index_for_addr.get(&next_block_addr).unwrap();
                                if cfg!(feature = "profiler") {
                                    ctx.builder.const_i32(target_index.into());
                                    ctx.builder.call_fn1("debug_set_dispatcher_target");
                                }
                                ctx.builder.const_i32(target_index.into());
                                ctx.builder.set_local(target_block);
                                codegen::gen_profiler_stat_increment(
                                    ctx.builder,
                                    stat::NORMAL_FALLTHRU_WITH_TARGET_BLOCK,
                                );
                            }
                            else {
                                codegen::gen_profiler_stat_increment(
                                    ctx.builder,
                                    stat::NORMAL_FALLTHRU,
                                );
                            }
                        }
                        else {
                            let &(br, target_index) = label_for_addr.get(&next_block_addr).unwrap();
                            if let Some(target_index) = target_index {
                                if cfg!(feature = "profiler") {
                                    ctx.builder.const_i32(target_index.into());
                                    ctx.builder.call_fn1("debug_set_dispatcher_target");
                                }
                                ctx.builder.const_i32(target_index.into());
                                ctx.builder.set_local(target_block);
                                codegen::gen_profiler_stat_increment(
                                    ctx.builder,
                                    stat::NORMAL_BRANCH_WITH_TARGET_BLOCK,
                                );
                            }
                            else {
                                codegen::gen_profiler_stat_increment(
                                    ctx.builder,
                                    stat::NORMAL_BRANCH,
                                );
                            }
                            ctx.builder.br(br);
                        }
                    },
                    &BasicBlockType::ConditionalJump {
                        next_block_addr,
                        next_block_branch_taken_addr,
                        condition,
                        jump_offset,
                        jump_offset_is_32,
                    } => {
                        // Conditional jump to next basic block
                        // - jnz, jc, loop, jcxz, etc.

                        // Generate:
                        // (1) condition()
                        // (2) br_if()
                        // (3) br()
                        // Except:
                        // If we need to update eip in case (2), it's replaced by if { update_eip(); br() }
                        // If case (3) can fall through to the next basic block, the branch is eliminated
                        // Dispatcher target writes can be generated in either case
                        // Condition may be inverted if it helps generate a fallthrough instead of the second branch

                        codegen::gen_profiler_stat_increment(ctx.builder, stat::CONDITIONAL_JUMP);

                        #[derive(PartialEq)]
                        enum Case {
                            BranchTaken,
                            BranchNotTaken,
                        }

                        let mut handle_case = |case: Case, is_first| {
                            // first case generates condition and *has* to branch away,
                            // second case branches unconditionally or falls through

                            if is_first {
                                if case == Case::BranchNotTaken {
                                    codegen::gen_condition_fn_negated(ctx, condition);
                                }
                                else {
                                    codegen::gen_condition_fn(ctx, condition);
                                }
                            }

                            let next_block_addr = if case == Case::BranchTaken {
                                next_block_branch_taken_addr
                            }
                            else {
                                next_block_addr
                            };

                            if let Some(next_block_addr) = next_block_addr {
                                if Page::page_of(next_block_addr) != Page::page_of(block.addr) {
                                    dbg_assert!(case == Case::BranchTaken); // currently not possible in other case
                                    if is_first {
                                        ctx.builder.if_i32();
                                    }
                                    if jump_offset_is_32 {
                                        codegen::gen_set_eip_low_bits_and_jump_rel32(
                                            ctx.builder,
                                            block.end_addr as i32 & 0xFFF,
                                            jump_offset,
                                        );
                                    }
                                    else {
                                        codegen::gen_set_eip_low_bits(
                                            ctx.builder,
                                            block.end_addr as i32 & 0xFFF,
                                        );
                                        codegen::gen_jmp_rel16(ctx.builder, jump_offset as u16);
                                    }

                                    codegen::gen_profiler_stat_increment(
                                        ctx.builder,
                                        stat::CONDITIONAL_JUMP_PAGE_CHANGE,
                                    );
                                    codegen::gen_page_switch_check(
                                        ctx,
                                        next_block_addr,
                                        block.last_instruction_addr,
                                    );

                                    #[cfg(debug_assertions)]
                                    codegen::gen_fn2_const(
                                        ctx.builder,
                                        "check_page_switch",
                                        block.addr,
                                        next_block_addr,
                                    );

                                    if is_first {
                                        ctx.builder.const_i32(1);
                                        ctx.builder.else_();
                                        ctx.builder.const_i32(0);
                                        ctx.builder.block_end();
                                    }
                                }

                                if next_addr
                                    .as_ref()
                                    .map_or(false, |n| n.contains(&next_block_addr))
                                {
                                    // blocks are consecutive

                                    // fallthrough, has to be second
                                    dbg_assert!(!is_first);

                                    if next_addr.as_ref().unwrap().len() > 1 {
                                        let target_index =
                                            *index_for_addr.get(&next_block_addr).unwrap();
                                        if cfg!(feature = "profiler") {
                                            ctx.builder.const_i32(target_index.into());
                                            ctx.builder.call_fn1("debug_set_dispatcher_target");
                                        }
                                        ctx.builder.const_i32(target_index.into());
                                        ctx.builder.set_local(target_block);
                                        codegen::gen_profiler_stat_increment(
                                            ctx.builder,
                                            stat::CONDITIONAL_JUMP_FALLTHRU_WITH_TARGET_BLOCK,
                                        );
                                    }
                                    else {
                                        codegen::gen_profiler_stat_increment(
                                            ctx.builder,
                                            stat::CONDITIONAL_JUMP_FALLTHRU,
                                        );
                                    }
                                }
                                else {
                                    let &(br, target_index) =
                                        label_for_addr.get(&next_block_addr).unwrap();
                                    if let Some(target_index) = target_index {
                                        if cfg!(feature = "profiler") {
                                            // Note: Currently called unconditionally, even if the
                                            // br_if below doesn't branch
                                            ctx.builder.const_i32(target_index.into());
                                            ctx.builder.call_fn1("debug_set_dispatcher_target");
                                        }
                                        ctx.builder.const_i32(target_index.into());
                                        ctx.builder.set_local(target_block);
                                    }

                                    if is_first {
                                        if cfg!(feature = "profiler") {
                                            ctx.builder.if_void();
                                            codegen::gen_profiler_stat_increment(
                                                ctx.builder,
                                                if target_index.is_some() {
                                                    stat::CONDITIONAL_JUMP_BRANCH_WITH_TARGET_BLOCK
                                                }
                                                else {
                                                    stat::CONDITIONAL_JUMP_BRANCH
                                                },
                                            );
                                            ctx.builder.br(br);
                                            ctx.builder.block_end();
                                        }
                                        else {
                                            ctx.builder.br_if(br);
                                        }
                                    }
                                    else {
                                        codegen::gen_profiler_stat_increment(
                                            ctx.builder,
                                            if target_index.is_some() {
                                                stat::CONDITIONAL_JUMP_BRANCH_WITH_TARGET_BLOCK
                                            }
                                            else {
                                                stat::CONDITIONAL_JUMP_BRANCH
                                            },
                                        );
                                        ctx.builder.br(br);
                                    }
                                }
                            }
                            else {
                                // target is outside of this module, update eip and exit
                                if is_first {
                                    ctx.builder.if_void();
                                }

                                if case == Case::BranchTaken {
                                    if jump_offset_is_32 {
                                        codegen::gen_set_eip_low_bits_and_jump_rel32(
                                            ctx.builder,
                                            block.end_addr as i32 & 0xFFF,
                                            jump_offset,
                                        );
                                    }
                                    else {
                                        codegen::gen_set_eip_low_bits(
                                            ctx.builder,
                                            block.end_addr as i32 & 0xFFF,
                                        );
                                        codegen::gen_jmp_rel16(ctx.builder, jump_offset as u16);
                                    }
                                }
                                else {
                                    codegen::gen_set_eip_low_bits(
                                        ctx.builder,
                                        block.end_addr as i32 & 0xFFF,
                                    );
                                }

                                codegen::gen_debug_track_jit_exit(
                                    ctx.builder,
                                    block.last_instruction_addr,
                                );
                                codegen::gen_profiler_stat_increment(
                                    ctx.builder,
                                    stat::CONDITIONAL_JUMP_EXIT,
                                );
                                ctx.builder.br(ctx.exit_label);

                                if is_first {
                                    ctx.builder.block_end();
                                }
                            }
                        };

                        let branch_taken_is_fallthrough = next_block_branch_taken_addr
                            .map_or(false, |addr| {
                                next_addr.as_ref().map_or(false, |n| n.contains(&addr))
                            });
                        let branch_not_taken_is_fallthrough = next_block_addr
                            .map_or(false, |addr| {
                                next_addr.as_ref().map_or(false, |n| n.contains(&addr))
                            });

                        if branch_not_taken_is_fallthrough && branch_taken_is_fallthrough {
                            let next_block_addr = next_block_addr.unwrap();
                            let next_block_branch_taken_addr =
                                next_block_branch_taken_addr.unwrap();

                            dbg_log!(
                                "Conditional control flow: fallthrough in both cases, page_switch={} next_is_multi={}",
                                Page::page_of(next_block_branch_taken_addr)
                                    != Page::page_of(block.addr),
                                next_addr.as_ref().unwrap().len() > 1,
                            );

                            dbg_assert!(
                                Page::page_of(next_block_addr) == Page::page_of(block.addr)
                            ); // currently not possible

                            if Page::page_of(next_block_branch_taken_addr)
                                != Page::page_of(block.addr)
                            {
                                codegen::gen_condition_fn(ctx, condition);
                                ctx.builder.if_void();

                                if jump_offset_is_32 {
                                    codegen::gen_set_eip_low_bits_and_jump_rel32(
                                        ctx.builder,
                                        block.end_addr as i32 & 0xFFF,
                                        jump_offset,
                                    );
                                }
                                else {
                                    codegen::gen_set_eip_low_bits(
                                        ctx.builder,
                                        block.end_addr as i32 & 0xFFF,
                                    );
                                    codegen::gen_jmp_rel16(ctx.builder, jump_offset as u16);
                                }

                                codegen::gen_profiler_stat_increment(
                                    ctx.builder,
                                    stat::CONDITIONAL_JUMP_PAGE_CHANGE,
                                );
                                codegen::gen_page_switch_check(
                                    ctx,
                                    next_block_branch_taken_addr,
                                    block.last_instruction_addr,
                                );

                                #[cfg(debug_assertions)]
                                codegen::gen_fn2_const(
                                    ctx.builder,
                                    "check_page_switch",
                                    block.addr,
                                    next_block_branch_taken_addr,
                                );

                                dbg_assert!(next_addr.unwrap().len() > 1);

                                let target_index_taken =
                                    *index_for_addr.get(&next_block_branch_taken_addr).unwrap();
                                let target_index_not_taken =
                                    *index_for_addr.get(&next_block_addr).unwrap();

                                ctx.builder.const_i32(target_index_taken.into());
                                ctx.builder.set_local(target_block);

                                ctx.builder.else_();
                                ctx.builder.const_i32(target_index_not_taken.into());
                                ctx.builder.set_local(target_block);

                                ctx.builder.block_end();
                            }
                            else if next_addr.unwrap().len() > 1 {
                                let target_index_taken =
                                    *index_for_addr.get(&next_block_branch_taken_addr).unwrap();
                                let target_index_not_taken =
                                    *index_for_addr.get(&next_block_addr).unwrap();

                                codegen::gen_condition_fn(ctx, condition);
                                ctx.builder.if_i32();
                                ctx.builder.const_i32(target_index_taken.into());
                                ctx.builder.else_();
                                ctx.builder.const_i32(target_index_not_taken.into());
                                ctx.builder.block_end();
                                ctx.builder.set_local(target_block);
                            }
                        }
                        else if branch_taken_is_fallthrough {
                            handle_case(Case::BranchNotTaken, true);
                            handle_case(Case::BranchTaken, false);
                        }
                        else {
                            handle_case(Case::BranchTaken, true);
                            handle_case(Case::BranchNotTaken, false);
                        }
                    },
                }
            },
            Work::WasmStructure(WasmStructure::Dispatcher(entries)) => {
                profiler::stat_increment(stat::COMPILE_DISPATCHER);

                if cfg!(feature = "profiler") {
                    ctx.builder.get_local(target_block);
                    ctx.builder.const_i32(index_for_addr.len() as i32);
                    ctx.builder.call_fn2("check_dispatcher_target");
                }

                if entries.len() > BRTABLE_CUTOFF {
                    // generate a brtable
                    codegen::gen_profiler_stat_increment(ctx.builder, stat::DISPATCHER_LARGE);
                    let mut cases = Vec::new();
                    for &addr in &entries {
                        let &(label, target_index) = label_for_addr.get(&addr).unwrap();
                        let &index = index_for_addr.get(&addr).unwrap();
                        dbg_assert!(target_index.is_none() || target_index == Some(index));
                        while index as usize >= cases.len() {
                            cases.push(brtable_default);
                        }
                        cases[index as usize] = label;
                    }
                    ctx.builder.get_local(target_block);
                    ctx.builder.brtable(brtable_default, &mut cases.iter());
                }
                else {
                    // generate a if target == block.addr then br block.label ...
                    codegen::gen_profiler_stat_increment(ctx.builder, stat::DISPATCHER_SMALL);
                    let nexts: HashSet<u32> = next_addr
                        .as_ref()
                        .map_or(HashSet::default(), |nexts| nexts.iter().copied().collect());
                    for &addr in &entries {
                        if nexts.contains(&addr) {
                            continue;
                        }
                        let index = *index_for_addr.get(&addr).unwrap();
                        let &(label, _) = label_for_addr.get(&addr).unwrap();
                        ctx.builder.get_local(target_block);
                        ctx.builder.const_i32(index.into());
                        ctx.builder.eq_i32();
                        ctx.builder.br_if(label);
                    }
                }
            },
            Work::WasmStructure(WasmStructure::Loop(children)) => {
                profiler::stat_increment(stat::COMPILE_WASM_LOOP);

                let entries: Vec<u32> = children[0].head().collect();
                let label = ctx.builder.loop_void();
                codegen::gen_profiler_stat_increment(ctx.builder, stat::LOOP);

                if entries.len() == 1 {
                    let addr = entries[0];
                    codegen::gen_set_eip_low_bits(ctx.builder, addr as i32 & 0xFFF);
                    profiler::stat_increment(stat::COMPILE_WITH_LOOP_SAFETY);
                    codegen::gen_profiler_stat_increment(ctx.builder, stat::LOOP_SAFETY);
                    if unsafe { JIT_USE_LOOP_SAFETY } {
                        ctx.builder.get_local(&ctx.instruction_counter);
                        ctx.builder.const_i32(unsafe { cpu::JIT_LOOP_LIMIT });
                        ctx.builder.geu_i32();
                        if cfg!(feature = "profiler") {
                            ctx.builder.if_void();
                            codegen::gen_debug_track_jit_exit(ctx.builder, addr);
                            ctx.builder.br(exit_label);
                            ctx.builder.block_end();
                        }
                        else {
                            ctx.builder.br_if(exit_label);
                        }
                    }
                }

                let mut olds = HashMap::default();
                for &target in entries.iter() {
                    let index = if entries.len() == 1 {
                        None
                    }
                    else {
                        Some(*index_for_addr.get(&target).unwrap())
                    };
                    let old = label_for_addr.insert(target, (label, index));
                    if let Some(old) = old {
                        olds.insert(target, old);
                    }
                }

                work.push_front(Work::LoopEnd {
                    label,
                    entries,
                    olds,
                });
                for c in children.into_iter().rev() {
                    work.push_front(Work::WasmStructure(c));
                }
            },
            Work::LoopEnd {
                label,
                entries,
                olds,
            } => {
                for target in entries {
                    let old = label_for_addr.remove(&target);
                    dbg_assert!(old.map(|(l, _)| l) == Some(label));
                }
                for (target, old) in olds {
                    let old = label_for_addr.insert(target, old);
                    dbg_assert!(old.is_none());
                }

                ctx.builder.block_end();
            },
            Work::WasmStructure(WasmStructure::Block(children)) => {
                profiler::stat_increment(stat::COMPILE_WASM_BLOCK);

                let targets = next_addr.clone().unwrap();
                let label = ctx.builder.block_void();
                let mut olds = HashMap::default();
                for &target in targets.iter() {
                    let index = if targets.len() == 1 {
                        None
                    }
                    else {
                        Some(*index_for_addr.get(&target).unwrap())
                    };
                    let old = label_for_addr.insert(target, (label, index));
                    if let Some(old) = old {
                        olds.insert(target, old);
                    }
                }

                work.push_front(Work::BlockEnd {
                    label,
                    targets,
                    olds,
                });
                for c in children.into_iter().rev() {
                    work.push_front(Work::WasmStructure(c));
                }
            },
            Work::BlockEnd {
                label,
                targets,
                olds,
            } => {
                for target in targets {
                    let old = label_for_addr.remove(&target);
                    dbg_assert!(old.map(|(l, _)| l) == Some(label));
                }
                for (target, old) in olds {
                    let old = label_for_addr.insert(target, old);
                    dbg_assert!(old.is_none());
                }

                ctx.builder.block_end();
            },
        }
    }

    dbg_assert!(label_for_addr.is_empty());

    {
        ctx.builder.block_end(); // default case for the brtable
        ctx.builder.unreachable();
    }
    {
        ctx.builder.block_end(); // main loop
    }
    {
        // exit with exception or due to smc
        ctx.builder.block_end();
        codegen::gen_move_registers_from_locals_to_memory(ctx);
        codegen::gen_fn0_const(ctx.builder, "exit_jit");
        codegen::gen_update_instruction_counter(ctx);
        ctx.builder.return_();
    }
    {
        // exit
        ctx.builder.block_end();
        codegen::gen_move_registers_from_locals_to_memory(ctx);
        codegen::gen_update_instruction_counter(ctx);
        if unsafe { cpu::JIT_CHAINING } && !cfg!(feature = "profiler") && !cfg!(test) {
            codegen::gen_chain_to_next_block(ctx.builder);
        }
    }

    for local in ctx.register_locals.drain(..) {
        ctx.builder.free_local(local);
    }
    ctx.builder
        .free_local(ctx.instruction_counter.unsafe_clone());
    ctx.builder
        .free_local(ctx.instruction_counter_flushed.unsafe_clone());
    ctx.builder.free_local(ctx.condition_local.unsafe_clone());
    for a in ctx.anchors.drain(..) {
        ctx.builder.free_local(a.local);
    }

    if unsafe { DROP_REGISTER_MOVES } {
        ctx.builder.drop_unused_register_moves();
    }
    ctx.builder.finish();

    let entries = Vec::from_iter(entry_blocks.iter().map(|addr| {
        let block = basic_blocks.get(&addr).unwrap();
        let index = *index_for_addr.get(&addr).unwrap();

        profiler::stat_increment(stat::COMPILE_ENTRY_POINT);

        dbg_assert!(block.addr < block.end_addr);
        // Note: We also insert blocks that weren't originally marked as entries here
        //       This doesn't have any downside, besides making the hash table slightly larger

        (block.addr, index)
    }));

    for b in basic_blocks.values() {
        if b.is_entry_block {
            dbg_assert!(entries.iter().find(|(addr, _)| *addr == b.addr).is_some());
        }
    }

    return entries;
}

fn jit_generate_basic_block(ctx: &mut JitContext, block: &BasicBlock) {
    let needs_eip_updated = match block.ty {
        BasicBlockType::Exit => true,
        _ => block.rep_string.is_some(),
    };
    let mut rep_string_check = None;

    profiler::stat_increment(stat::COMPILE_BASIC_BLOCK);

    let start_addr = block.addr;
    let last_instruction_addr = block.last_instruction_addr;
    let stop_addr = block.end_addr;

    // First iteration of do-while assumes the caller confirms this condition
    // (or the block is one crossing instruction)
    dbg_assert!(
        instruction_fits_in_page(start_addr, &ctx.cpu)
            || block.crossing && start_addr == last_instruction_addr
    );

    if cfg!(feature = "profiler") {
        ctx.builder.const_i32(start_addr as i32);
        ctx.builder.call_fn1("enter_basic_block");
    }

    ctx.builder.get_local(&ctx.instruction_counter);
    ctx.builder.const_i32(block.number_of_instructions as i32);
    ctx.builder.add_i32();
    ctx.builder.set_local(&ctx.instruction_counter);

    ctx.cpu.eip = start_addr;
    ctx.current_instruction = Instruction::Other;
    ctx.previous_instruction = Instruction::Other;
    // (a basic block is entered from several places: nothing loaded before it is known here)
    for a in ctx.anchors.drain(..) {
        ctx.builder.free_local(a.local);
    }
    ctx.builder.structure_barrier();

    let mut jcc_only = None;
    let flags_dead = if unsafe { FLAG_ELISION } {
        // (at the block's end the flags are read, unless the module's analysis knows better)
        match ctx.flags_live_out.get(&start_addr) {
            Some((live_out, effects)) => {
                // a flag writer right before the block's conditional jump, its flags dead after the jump
                if let BasicBlockType::ConditionalJump { condition, .. } = block.ty {
                    let n = effects.len();
                    if !*live_out && n >= 2 && effects[n - 2] == (false, true) && (condition & 0xF0 == 0x70 || condition & 0xF0 == 0x80)
                    {
                        jcc_only = Some((n - 2, condition));
                    }
                }
                flags_dead_after(effects, *live_out)
            },
            None => flags_dead_after(&block_flag_effects(ctx.cpu, start_addr, stop_addr), true),
        }
    }
    else {
        Vec::new()
    };
    let mut instruction_index = 0;

    loop {
        let mut instruction = 0;
        if cfg!(feature = "profiler") {
            instruction = memory::read32s(ctx.cpu.eip) as u32;
            opstats::gen_opstats(ctx.builder, instruction);
            opstats::record_opstat_compiled(instruction);
        }

        if ctx.cpu.eip == last_instruction_addr {
            // Before the last instruction:
            // - Set eip to *after* the instruction
            // - Set previous_eip to *before* the instruction
            if block.crossing {
                // eip stays in this page while it runs (exceptions are raised at its start, in this page), and goes
                // to the next page after it
                codegen::gen_set_previous_eip_offset_from_eip_with_low_bits(
                    ctx.builder,
                    last_instruction_addr as i32 & 0xFFF,
                );
                gen_crossing_guard(ctx, last_instruction_addr, stop_addr);
            }
            else if needs_eip_updated {
                codegen::gen_set_previous_eip_offset_from_eip_with_low_bits(
                    ctx.builder,
                    last_instruction_addr as i32 & 0xFFF,
                );
                codegen::gen_set_eip_low_bits(ctx.builder, stop_addr as i32 & 0xFFF);
                if let Some(kind) = block.rep_string {
                    // eip after the instruction, and the invalidation count, to compare after it
                    codegen::gen_get_eip(ctx.builder);
                    let eip = ctx.builder.set_new_local();
                    let invalidations = if kind == RepString::Writes {
                        ctx.builder.load_fixed_i32(unsafe { (&raw const CODE_INVALIDATIONS) as u32 });
                        Some(ctx.builder.set_new_local())
                    }
                    else {
                        None
                    };
                    rep_string_check = Some((eip, invalidations));
                }
            }
        }

        let wasm_length_before = ctx.builder.instruction_body_length();

        ctx.start_of_current_instruction = ctx.cpu.eip;
        ctx.address_hint = None;
        let start_eip = ctx.cpu.eip;
        let mut instruction_flags = 0;
        ctx.flags_dead = flags_dead.get(instruction_index).copied().unwrap_or(false);
        ctx.flags_jcc_only = match jcc_only {
            Some((i, condition)) if i == instruction_index && unsafe { FLAG_ELISION_JCC } => Some(condition),
            _ => None,
        };
        instruction_index += 1;
        jit_instructions::jit_instruction(ctx, &mut instruction_flags);
        ctx.flags_dead = false;
        ctx.flags_jcc_only = None;
        if start_eip == last_instruction_addr {
            if let Some((eip, invalidations)) = rep_string_check.take() {
                // finished (eip still after it: not sent back to it for the next page, no fault) and no compiled
                // code freed: on to the next block; otherwise return, as at the end of an Exit block
                codegen::gen_get_eip(ctx.builder);
                ctx.builder.get_local(&eip);
                ctx.builder.ne_i32();
                if let Some(invalidations) = &invalidations {
                    ctx.builder.load_fixed_i32(unsafe { (&raw const CODE_INVALIDATIONS) as u32 });
                    ctx.builder.get_local(invalidations);
                    ctx.builder.ne_i32();
                    ctx.builder.or_i32();
                }
                ctx.builder.if_void();
                codegen::gen_debug_track_jit_exit(ctx.builder, last_instruction_addr);
                codegen::gen_profiler_stat_increment(ctx.builder, stat::DIRECT_EXIT);
                ctx.builder.br(ctx.exit_label);
                ctx.builder.block_end();
                codegen::gen_profiler_stat_increment(ctx.builder, stat::REP_STRING_CONTINUED);
                ctx.builder.free_local(eip);
                if let Some(invalidations) = invalidations {
                    ctx.builder.free_local(invalidations);
                }
            }
        }
        let end_eip = ctx.cpu.eip;
        if block.crossing && start_eip == last_instruction_addr {
            // after the crossing instruction: eip in the next page (a jump's target: from there)
            let after = stop_addr as i32 & 0xFFF;
            match block.crossing_jump {
                None => codegen::gen_set_eip_low_bits_and_jump_rel32(ctx.builder, after, 0x1000),
                Some((None, offset)) => {
                    codegen::gen_set_eip_low_bits_and_jump_rel32(ctx.builder, after, 0x1000i32.wrapping_add(offset))
                },
                Some((Some(condition), offset)) => {
                    codegen::gen_condition_fn(ctx, condition);
                    ctx.builder.if_void();
                    codegen::gen_set_eip_low_bits_and_jump_rel32(ctx.builder, after, 0x1000i32.wrapping_add(offset));
                    ctx.builder.else_();
                    codegen::gen_set_eip_low_bits_and_jump_rel32(ctx.builder, after, 0x1000);
                    ctx.builder.block_end();
                },
            }
        }

        let instruction_length = end_eip - start_eip;
        let was_block_boundary = instruction_flags & JIT_INSTR_BLOCK_BOUNDARY_FLAG != 0;

        let wasm_length = ctx.builder.instruction_body_length() - wasm_length_before;
        opstats::record_opstat_size_wasm(instruction, wasm_length as u64);

        dbg_assert!((end_eip == stop_addr) == (start_eip == last_instruction_addr));
        dbg_assert!(instruction_length < MAX_INSTRUCTION_LENGTH);

        let end_addr = ctx.cpu.eip;

        if end_addr == stop_addr {
            // no page was crossed (but by a crossing last instruction)
            dbg_assert!(block.crossing || Page::page_of(end_addr - 1) == Page::page_of(start_addr));
            break;
        }

        if was_block_boundary || end_addr > stop_addr {
            dbg_log!(
                "Overlapping basic blocks start={:x} expected_end={:x} end={:x} was_block_boundary={} near_end_of_page={}",
                start_addr,
                stop_addr,
                end_addr,
                was_block_boundary,
                is_near_end_of_page(end_addr)
            );
            dbg_assert!(false);
            break;
        }

        ctx.previous_instruction = mem::replace(&mut ctx.current_instruction, Instruction::Other);
    }
}

/// Before a crossing instruction (crossing_instruction): its bytes in the next page as they were compiled, read through
/// the TLB entry of the next virtual page; otherwise the module returns with eip at the instruction, which the
/// interpreter then runs (not counted here)
fn gen_crossing_guard(ctx: &mut JitContext, start: u32, end: u32) {
    let next_page = (start | 0xFFF) + 1;
    let n = end - next_page;
    dbg_assert!(n >= 1 && n <= 14);
    let expected: Vec<u8> = (0..n).map(|i| memory::read8(next_page + i) as u8).collect();
    let tlb_data_base = unsafe { (&raw const cpu::tlb_data) as u32 };
    let check = cpu::TLB_VALID | cpu::TLB_NO_EXEC | if ctx.cpu.state_flags.cpl3() { cpu::TLB_NO_USER } else { 0 };

    let gen_fail = |ctx: &mut JitContext| {
        ctx.builder.get_local(&ctx.instruction_counter);
        ctx.builder.const_i32(1);
        ctx.builder.sub_i32();
        ctx.builder.set_local(&ctx.instruction_counter);
        codegen::gen_set_eip_low_bits(ctx.builder, start as i32 & 0xFFF);
        ctx.builder.br(ctx.exit_label);
    };

    // the next virtual page's TLB entry
    codegen::gen_get_eip(ctx.builder);
    ctx.builder.const_i32(12);
    ctx.builder.shr_u_i32();
    ctx.builder.const_i32(1);
    ctx.builder.add_i32();
    let vpage = ctx.builder.set_new_local();
    ctx.builder.get_local(&vpage);
    ctx.builder.const_i32(2);
    ctx.builder.shl_i32();
    ctx.builder.load_aligned_i32(tlb_data_base);
    let entry = ctx.builder.set_new_local();

    ctx.builder.get_local(&entry);
    ctx.builder.const_i32(check);
    ctx.builder.and_i32();
    ctx.builder.const_i32(cpu::TLB_VALID);
    ctx.builder.ne_i32();
    ctx.builder.if_void();
    gen_fail(ctx);
    ctx.builder.block_end();

    // its bytes there: (entry & ~0xFFF) ^ (vpage << 12) is the page's address in this module's memory
    ctx.builder.get_local(&entry);
    ctx.builder.const_i32(!0xFFF);
    ctx.builder.and_i32();
    ctx.builder.get_local(&vpage);
    ctx.builder.const_i32(12);
    ctx.builder.shl_i32();
    ctx.builder.xor_i32();
    let host = ctx.builder.set_new_local();
    let word = |bytes: &[u8]| {
        let mut v = 0u64;
        for (i, &b) in bytes.iter().enumerate() {
            v |= (b as u64) << (8 * i);
        }
        let mask = if bytes.len() >= 8 { u64::MAX } else { (1u64 << (8 * bytes.len())) - 1 };
        (v, mask)
    };
    for (k, chunk) in expected.chunks(8).enumerate() {
        let (value, mask) = word(chunk);
        ctx.builder.get_local(&host);
        ctx.builder.load_unaligned_i64(8 * k as u32);
        ctx.builder.const_i64(mask as i64);
        ctx.builder.and_i64();
        ctx.builder.const_i64(value as i64);
        ctx.builder.ne_i64();
        ctx.builder.if_void();
        gen_fail(ctx);
        ctx.builder.block_end();
    }
    ctx.builder.free_local(host);
    ctx.builder.free_local(entry);
    ctx.builder.free_local(vpage);
}

/// Flag elision (set_jit_config(18, ..)): instructions whose arithmetic flags are overwritten before they are read
/// do not store them
pub static mut FLAG_ELISION: bool = true;
/// Calls out of a module as wasm calls of the callee's module (set_jit_config(21, ..), see codegen::gen_nested_call).
/// Off: in V8 (Node 24) an Alpine boot is 5.9 % slower with them, though the caller goes on after 43 M of the 58 M
/// nested calls (each saving two passes through the dispatcher): a wasm call from one generated module into another
/// costs more than the dispatcher (as chaining with tail calls, JIT_CHAINING). Worth measuring in other engines.
pub static mut NESTED_CALLS: bool = false;
/// how deep they are nested now; at most NESTED_CALL_DEPTH
pub static mut NESTED_CALL_LEVEL: i32 = 0;
pub const NESTED_CALL_DEPTH: i32 = 16;
/// nested calls made, and those after which the caller went on (get_jit_config(32), (33))
pub static mut NESTED_CALLS_MADE: u64 = 0;
/// (set_jit_config(22, 1): count them)
pub static mut NESTED_CALL_STATS: bool = false;
pub static mut NESTED_CALLS_CONTINUED: u64 = 0;
/// and across memory accesses (set_jit_config(19, ..), see flag_effect)
pub static mut FLAG_ELISION_ACROSS_ACCESSES: bool = true;
/// and across the basic blocks of a module (set_jit_config(20, ..), see flags_live_out)
pub static mut FLAG_ELISION_ACROSS_BLOCKS: bool = true;
/// and the flags of a cmp/test/... read only by the conditional jump after it (set_jit_config(23, ..), see
/// JitContext::flags_jcc_only)
pub static mut FLAG_ELISION_JCC: bool = true;

/// What an instruction does to the arithmetic flags, from its bytes, for flags_dead_after: (reads them or may need
/// them, writes them all). Conservative: what is not recognised reads them, and so does anything that may fault
/// (an exception needs the flags of the instructions before it) or leave the block
fn flag_effect(bytes: [u8; 4]) -> (bool, bool) {
    const READS: (bool, bool) = (true, false);
    const WRITES_ALL: (bool, bool) = (false, true);
    const NONE: (bool, bool) = (false, false);
    let skip = (bytes[0] == 0x66) as usize;
    let (op, modrm) = (bytes[skip], bytes[skip + 1]);
    let across = unsafe { FLAG_ELISION_ACROSS_ACCESSES };
    let reg_form = modrm >> 6 == 3;
    // add, or, and, sub, xor, cmp (not adc, sbb): every one of them sets all of the arithmetic flags
    let group_writes_all = |g: u8| matches!(g, 0 | 1 | 4 | 5 | 6 | 7);
    match op {
        // (with a memory operand they may fault before writing the flags: as plain loads and stores, see below)
        0x00..=0x3F if op & 7 <= 3 && group_writes_all(op >> 3) => if reg_form || across { WRITES_ALL } else { READS },
        0x00..=0x3F if (op & 7 == 4 || op & 7 == 5) && group_writes_all(op >> 3) => WRITES_ALL,
        0x80 | 0x81 | 0x83 => {
            if (reg_form || across) && group_writes_all(modrm >> 3 & 7) { WRITES_ALL } else { READS }
        },
        // test
        0x84 | 0x85 => if reg_form || across { WRITES_ALL } else { READS },
        // shl, shr, sar of a register by 1 or by a non-zero immediate: every arithmetic flag set (as the JIT computes
        // them; AF and, for counts above 1, OF are undefined on x86). rol, ror, rcl, rcr: only CF and OF
        0xD1 if reg_form && matches!(modrm >> 3 & 7, 4 | 5 | 7) => WRITES_ALL,
        0xC1 if reg_form && matches!(modrm >> 3 & 7, 4 | 5 | 7) && bytes[skip + 2] & 31 != 0 => WRITES_ALL,
        0xA8 | 0xA9 => WRITES_ALL,
        // mov between registers, mov of an immediate to a register, lea (computes an address: does not fault), nop
        0x88..=0x8B if reg_form => NONE,
        // plain loads and stores, push and pop: they may fault, and an exception's frame would then hold older flags.
        // The instruction runs again after the exception and the flags are overwritten before anything reads them,
        // so only a handler that reads the frame's arithmetic flags sees it (as jit64: FLAGS_ACROSS_ACCESSES)
        0x88..=0x8B | 0xC6 | 0xC7 | 0x50..=0x5F | 0x68 | 0x6A => {
            if across && (op < 0xC6 || modrm >> 3 & 7 == 0) { NONE } else { READS }
        },
        0xB0..=0xBF | 0x90 => NONE,
        // mov between eax and a fixed address
        0xA0..=0xA3 => if across { NONE } else { READS },
        // movzx, movsx
        0x0F if matches!(modrm, 0xB6 | 0xB7 | 0xBE | 0xBF) => {
            if bytes[skip + 2] >> 6 == 3 || across { NONE } else { READS }
        },
        0x8D => if reg_form { READS } else { NONE },
        _ => READS,
    }
}

/// What each instruction of the basic block from `start` to `stop` does to the flags (flag_effect)
fn block_flag_effects(cpu: &CpuContext, start: u32, stop: u32) -> Vec<(bool, bool)> {
    let mut effects = Vec::new();
    let mut c = cpu.clone();
    c.eip = start;
    while c.eip < stop {
        let at = c.eip;
        let byte = |i: u32| if (at + i) & 0xFFF >= (at & 0xFFF) { memory::read8(at + i) as u8 } else { 0xFF };
        effects.push(flag_effect([byte(0), byte(1), byte(2), byte(3)]));
        analysis::analyze_step(&mut c);
        if c.eip <= at {
            break;
        }
    }
    effects
}

/// For each instruction of a basic block (its effects): whether the flags it writes are overwritten before anything
/// reads them, `live_out` telling whether they may be read after the block's end
fn flags_dead_after(effects: &[(bool, bool)], live_out: bool) -> Vec<bool> {
    let mut dead = vec![false; effects.len()];
    let mut live = live_out;
    for i in (0..effects.len()).rev() {
        dead[i] = !live;
        let (reads, writes_all) = effects[i];
        live = reads || live && !writes_all;
    }
    dead
}

/// Whether the flags may be read at the start of a block (its effects), `live_out` at its end
fn flags_live_in(effects: &[(bool, bool)], live_out: bool) -> bool {
    let mut live = live_out;
    for &(reads, writes_all) in effects.iter().rev() {
        live = reads || live && !writes_all;
    }
    live
}

/// Per basic block of a module: whether the flags may be read after it, from the blocks of the module it goes to
/// (to anything else: yes). A successor runs the same instructions whichever way it is reached (this module, the
/// dispatcher after an exit, the interpreter), so the flags it overwrites before reading need not be stored; in
/// between, only the frame of an interrupt taken on the way would see older flags (as flag_effect's memory accesses)
fn flags_live_out(
    cpu: &CpuContext,
    basic_blocks: &HashMap<u32, BasicBlock>,
) -> HashMap<u32, (bool, Vec<(bool, bool)>)> {
    let mut effects: HashMap<u32, Vec<(bool, bool)>> =
        basic_blocks.values().map(|b| (b.addr, block_flag_effects(cpu, b.addr, b.end_addr))).collect();
    let mut live_in: HashMap<u32, bool> = basic_blocks.keys().map(|&a| (a, false)).collect();
    let mut live_out: HashMap<u32, bool> = HashMap::default();
    let successor = |live_in: &HashMap<u32, bool>, addr: Option<u32>| match addr {
        Some(a) => live_in.get(&a).copied().unwrap_or(true),
        None => true,
    };
    loop {
        let mut changed = false;
        for b in basic_blocks.values() {
            let out = b.has_sti
                || match b.ty {
                    BasicBlockType::Normal { next_block_addr, .. } => successor(&live_in, next_block_addr),
                    BasicBlockType::ConditionalJump { next_block_addr, next_block_branch_taken_addr, .. } => {
                        successor(&live_in, next_block_addr) || successor(&live_in, next_block_branch_taken_addr)
                    },
                    BasicBlockType::AbsoluteEip | BasicBlockType::Exit => true,
                };
            let inn = flags_live_in(&effects[&b.addr], out);
            if live_out.insert(b.addr, out) != Some(out) {
                changed = true;
            }
            if live_in.insert(b.addr, inn) != Some(inn) {
                changed = true;
            }
        }
        if !changed {
            return live_out.into_iter().map(|(a, out)| (a, (out, effects.remove(&a).unwrap_or_default()))).collect();
        }
    }
}

pub fn jit_increase_hotness_and_maybe_compile(
    virt_address: i32,
    phys_address: u32,
    cs_offset: u32,
    state_flags: CachedStateFlags,
    heat: u32,
) {
    if unsafe { JIT_DISABLED } {
        return;
    }
    // (code in device memory, the firmware's flash: interpreted)
    if phys_address >= unsafe { *crate::cpu::global_pointers::memory_size } {
        return;
    }

    let mut ctx = get_jit_state();
    let is_compiling = ctx.compiling.len() >= unsafe { MAX_COMPILING } as usize;
    let page = Page::page_of(phys_address);
    maybe_has_code_set(page);
    let threshold = if ctx.pages.contains_key(&page) {
        unsafe { JIT_RECOMPILE_THRESHOLD.min(JIT_THRESHOLD_NOW) }
    }
    else {
        unsafe { JIT_THRESHOLD_NOW }
    };
    let (hotness, entry_points) = ctx.entry_points.entry(page).or_insert_with(|| {
        cpu::tlb_set_has_code(page, true);
        profiler::stat_increment(stat::RUN_INTERPRETED_NEW_PAGE);
        (0, EntrySet::default())
    });

    if instruction_fits_in_page(phys_address, &CpuContext { eip: phys_address, prefixes: 0, cs_offset, state_flags }) {
        entry_points.insert(phys_address as u16 & 0xFFF);
    }

    *hotness += heat;
    if *hotness >= threshold {
        if is_compiling {
            return;
        }
        // only try generating if we're in the correct address space
        if cpu::translate_address_read_no_side_effects(virt_address) == Ok(phys_address) {
            *hotness = 0;
            jit_analyze_and_generate(&mut ctx, virt_address, phys_address, cs_offset, state_flags)
        }
        else {
            profiler::stat_increment(stat::COMPILE_WRONG_ADDRESS_SPACE);
        }
    }
}

fn free_wasm_table_index(ctx: &mut JitState, wasm_table_index: WasmTableIndex) {
    if CHECK_JIT_STATE_INVARIANTS {
        dbg_assert!(!ctx.wasm_table_index_free_list.contains(&wasm_table_index));

        for (wasm_table_index_compiling, _) in &ctx.compiling {
            dbg_assert!(
                *wasm_table_index_compiling != wasm_table_index,
                "Attempt to free wasm table index that is currently being compiled"
            );
        }

        dbg_assert!(!ctx
            .pages
            .values()
            .any(|info| info.wasm_table_index == wasm_table_index));

        dbg_assert!(!ctx
            .pages
            .values()
            .any(|info| info.hidden_wasm_table_indices.contains(&wasm_table_index)));

        for i in 0..unsafe { cpu::valid_tlb_entries_count } {
            let page = unsafe { cpu::valid_tlb_entries[i as usize] };
            unsafe {
                match cpu::tlb_code[page as usize] {
                    None => {},
                    Some(c) => {
                        let c = c.as_ref();
                        dbg_assert!(c.wasm_table_index != wasm_table_index);
                    },
                }
            }
        }
    }

    #[cfg(debug_assertions)]
    ctx.wasm_table_index_to_page.remove(&wasm_table_index);

    ctx.wasm_table_index_free_list.push(wasm_table_index);

    // It is not strictly necessary to clear the function, but it will fail more predictably if we
    // accidentally use the function and may garbage collect unused modules earlier
    jit_clear_func(wasm_table_index);
}

pub fn jit_disabled() -> bool { unsafe { JIT_DISABLED } }

/// A LOCKed instruction (or xchg with memory) of a machine whose cores share the guest RAM has to be atomic: compiled code does the
/// read-modify-write with plain loads and stores, the interpreter with a compare-exchange
fn needs_atomic_interpretation(phys_addr: u32) -> bool {
    if unsafe { !memory::ATOMIC_RMW } {
        return false;
    }
    let mut a = phys_addr;
    for _ in 0..6 {
        match memory::read8(a) & 0xFF {
            0xF0 => return true,
            // operand/address size, segment and rep prefixes
            0x66 | 0x67 | 0x26 | 0x2E | 0x36 | 0x3E | 0x64 | 0x65 | 0xF2 | 0xF3 => a += 1,
            0x86 | 0x87 => return memory::read8(a + 1) & 0xC0 != 0xC0,
            _ => return false,
        }
    }
    false
}

/// Compiles a module for cpu::jit64: `f` generates the code into the builder. Returns the wasm
/// table index; the module becomes callable when `jit64::finalize_finished` is called.
pub fn jit64_compile(
    phys_addr: u32,
    f: impl FnOnce(&mut WasmBuilder) -> Option<u32>,
    register: impl FnOnce(u16),
) -> Option<u16> {
    let mut ctx = get_jit_state();
    if ctx.wasm_table_index_free_list.is_empty() {
        jit_clear_cache(&mut ctx);
        if ctx.wasm_table_index_free_list.is_empty() {
            return None;
        }
    }
    f(&mut ctx.wasm_builder)?;
    let index = ctx.wasm_table_index_free_list.pop()?;
    register(index.to_u16());
    codegen_finalize(
        index,
        phys_addr,
        CachedStateFlags::JIT64,
        ctx.wasm_builder.get_output_ptr() as u32,
        ctx.wasm_builder.get_output_len(),
    );
    Some(index.to_u16())
}

pub fn jit64_free_index(index: u16) {
    let mut ctx = get_jit_state();
    free_wasm_table_index(&mut ctx, WasmTableIndex(index));
}

pub fn jit64_free_index_ctx(ctx: &mut JitState, index: u16) {
    free_wasm_table_index(ctx, WasmTableIndex(index));
}

// One bit per 4k page of the 4 GB physical address space: set if the page may have entry points
// or compiled code. Checked on every memory write, where it replaces two hash lookups.
static mut MAYBE_HAS_CODE: [u64; 1 << 14] = [0; 1 << 14];

pub fn maybe_has_code_set(page: Page) {
    if crate::cpu::parallel::active() {
        // (cores in other instances share the guest memory: see cpu/parallel.rs)
        return unsafe { crate::cpu::parallel::set_owner(page.to_u32()) };
    }
    let i = page.to_u32() as usize;
    unsafe { MAYBE_HAS_CODE[i >> 6] |= 1 << (i & 63) };
}
fn maybe_has_code_clear(page: Page) {
    if crate::cpu::parallel::active() {
        return unsafe { crate::cpu::parallel::clear_owner(page.to_u32()) };
    }
    let i = page.to_u32() as usize;
    unsafe { MAYBE_HAS_CODE[i >> 6] &= !(1 << (i & 63)) };
}
pub fn maybe_has_code(page: Page) -> bool {
    if crate::cpu::parallel::active() {
        return unsafe { crate::cpu::parallel::any_owner(page.to_u32()) };
    }
    let i = page.to_u32() as usize;
    unsafe { MAYBE_HAS_CODE[i >> 6] >> (i & 63) & 1 != 0 }
}

/// Register a write in this page: Delete all present code
fn jit_dirty_page_ctx(ctx: &mut JitState, page: Page) {
    if !maybe_has_code(page) {
        return;
    }
    unsafe { CODE_INVALIDATIONS = CODE_INVALIDATIONS.wrapping_add(1) };
    crate::cpu::decoded::invalidate(page);
    if crate::cpu::parallel::active() {
        // the other instances that compiled the page retire their code too
        unsafe { crate::cpu::parallel::announce_write(page.to_u32()) };
    }
    let mut did_have_code = unsafe { crate::cpu::jit64::dirty_page(ctx, page) };

    if let Some(PageInfo {
        wasm_table_index,
        hidden_wasm_table_indices,
        state_flags: _,
        entry_points: _,
    }) = ctx.pages.remove(&page)
    {
        profiler::stat_increment(stat::INVALIDATE_PAGE_HAD_CODE);
        did_have_code = true;

        free(ctx, wasm_table_index);
        for wasm_table_index in hidden_wasm_table_indices {
            free(ctx, wasm_table_index);
        }

        fn free(ctx: &mut JitState, wasm_table_index: WasmTableIndex) {
            ctx.pages.retain(|_, info| {
                if info.wasm_table_index != wasm_table_index {
                    return true;
                }
                match info.hidden_wasm_table_indices.pop() {
                    Some(new_primary) => {
                        info.wasm_table_index = new_primary;
                        info.entry_points.clear();
                        true
                    },
                    None => false,
                }
            });

            for info in ctx.pages.values_mut() {
                info.hidden_wasm_table_indices
                    .retain(|&w| w != wasm_table_index)
            }

            for i in 0..unsafe { cpu::valid_tlb_entries_count } {
                let page = unsafe { cpu::valid_tlb_entries[i as usize] };
                let entry = unsafe { cpu::tlb_data[page as usize] };
                if 0 != entry {
                    let tlb_physical_page = Page::of_u32(
                        (entry as u32 >> 12 ^ page as u32) - (unsafe { memory::mem8 } as u32 >> 12),
                    );
                    match unsafe { cpu::tlb_code[page as usize] } {
                        None => {},
                        Some(c) => unsafe {
                            let w = c.as_ref().wasm_table_index;
                            if wasm_table_index == w {
                                drop(Box::from_raw(c.as_ptr()));
                                cpu::tlb_code[page as usize] = None;
                                if !ctx.entry_points.contains_key(&tlb_physical_page)
                                    && !ctx.pages.contains_key(&tlb_physical_page)
                                {
                                    cpu::tlb_data[page as usize] &= !cpu::TLB_HAS_CODE;
                                }
                            }
                        },
                    }
                }
            }

            free_wasm_table_index(ctx, wasm_table_index);
        }
    }

    match ctx.entry_points.remove(&page) {
        None => {},
        Some(_) => {
            profiler::stat_increment(stat::INVALIDATE_PAGE_HAD_ENTRY_POINTS);
            did_have_code = true;

            for (_, state) in ctx.compiling.iter_mut() {
                let written = matches!(state, CompilingPageState::Compiling { pages } if pages.contains_key(&page));
                if written {
                    *state = CompilingPageState::CompilingWritten;
                }
            }
        },
    }

    for (_, state) in &ctx.compiling {
        if let CompilingPageState::Compiling { pages } = state {
            dbg_assert!(!pages.contains_key(&page));
        }
    }

    check_jit_state_invariants(ctx);

    maybe_has_code_clear(page);

    dbg_assert!(crate::cpu::parallel::active() || !jit_page_has_code_ctx(ctx, page));

    // (another instance may still have code of the page: the TLB entries keep the mark)
    if did_have_code && !maybe_has_code(page) {
        cpu::tlb_set_has_code(page, false);
    }

    if !did_have_code {
        profiler::stat_increment(stat::DIRTY_PAGE_DID_NOT_HAVE_CODE);
    }
}

#[no_mangle]
pub fn jit_dirty_cache(start_addr: u32, end_addr: u32) {
    dbg_assert!(start_addr < end_addr);

    let start_page = Page::page_of(start_addr);
    let end_page = Page::page_of(end_addr - 1);

    for page in start_page.to_u32()..end_page.to_u32() + 1 {
        jit_dirty_page_ctx(&mut get_jit_state(), Page::page_of(page << 12));
    }
}

#[no_mangle]
pub fn jit_dirty_page(page: Page) { jit_dirty_page_ctx(&mut get_jit_state(), page) }

/// dirty pages in the range of start_addr and end_addr, which must span at most two pages
pub fn jit_dirty_cache_small(start_addr: u32, end_addr: u32) {
    dbg_assert!(start_addr < end_addr);

    let start_page = Page::page_of(start_addr);
    let end_page = Page::page_of(end_addr - 1);

    let mut ctx = get_jit_state();
    jit_dirty_page_ctx(&mut ctx, start_page);

    // Note: This can't happen when paging is enabled, as writes across
    //       boundaries are split up on two pages
    if start_page != end_page {
        dbg_assert!(start_page.to_u32() + 1 == end_page.to_u32());
        jit_dirty_page_ctx(&mut ctx, end_page);
    }
}

#[no_mangle]
pub fn jit_clear_cache_js() { jit_clear_cache(&mut get_jit_state()) }

fn jit_clear_cache(ctx: &mut JitState) {
    crate::cpu::decoded::clear();
    let mut pages_with_code: HashSet<Page> = HashSet::default();

    for &p in ctx.entry_points.keys() {
        pages_with_code.insert(p);
    }
    for &p in ctx.pages.keys() {
        pages_with_code.insert(p);
    }

    for page in pages_with_code {
        jit_dirty_page_ctx(ctx, page);
    }

    unsafe { crate::cpu::jit64::clear_all(ctx) };
}

pub fn jit_page_has_code(page: Page) -> bool { jit_page_has_code_ctx(&mut get_jit_state(), page) }

fn jit_page_has_code_ctx(ctx: &mut JitState, page: Page) -> bool {
    ctx.pages.contains_key(&page) || ctx.entry_points.contains_key(&page) || maybe_has_code(page)
}

pub fn jit_page_has_wasm_table_index(page: Page, wasm_table_index: u16) -> bool {
    let ctx = get_jit_state();
    match ctx.pages.get(&page) {
        Some(info) => {
            info.wasm_table_index.to_u16() == wasm_table_index
                || info
                    .hidden_wasm_table_indices
                    .iter()
                    .any(|w| w.to_u16() == wasm_table_index)
        },
        None => false,
    }
}

#[no_mangle]
pub fn jit_get_wasm_table_index_free_list_count() -> u32 {
    if cfg!(feature = "profiler") {
        get_jit_state().wasm_table_index_free_list.len() as u32
    }
    else {
        0
    }
}
#[no_mangle]
pub fn jit_get_cache_size() -> u32 {
    if cfg!(feature = "profiler") {
        get_jit_state()
            .pages
            .values()
            .map(|p| p.entry_points.len() as u32)
            .sum()
    }
    else {
        0
    }
}

#[cfg(feature = "profiler")]
pub fn check_missed_entry_points(phys_address: u32, state_flags: CachedStateFlags) {
    let ctx = get_jit_state();

    if let Some(infos) = ctx.pages.get(&Page::page_of(phys_address)) {
        if infos.state_flags != state_flags {
            return;
        }

        #[allow(static_mut_refs)]
        let last_jump_type = unsafe { cpu::debug_last_jump.name() };
        #[allow(static_mut_refs)]
        let last_jump_addr = unsafe { cpu::debug_last_jump.phys_address() }.unwrap_or(0);
        let last_jump_opcode =
            if last_jump_addr != 0 { memory::read32s(last_jump_addr) } else { 0 };

        let opcode = memory::read32s(phys_address);
        dbg_log!(
            "Compiled exists, but no entry point, \
                 phys_addr={:x} opcode={:02x} {:02x} {:02x} {:02x}. \
                 Last jump at {:x} ({}) opcode={:02x} {:02x} {:02x} {:02x}",
            phys_address,
            opcode & 0xFF,
            opcode >> 8 & 0xFF,
            opcode >> 16 & 0xFF,
            opcode >> 16 & 0xFF,
            last_jump_addr,
            last_jump_type,
            last_jump_opcode & 0xFF,
            last_jump_opcode >> 8 & 0xFF,
            last_jump_opcode >> 16 & 0xFF,
            last_jump_opcode >> 16 & 0xFF,
        );
    }
}

#[no_mangle]
#[cfg(feature = "profiler")]
pub fn debug_set_dispatcher_target(_target_index: i32) {
    //dbg_log!("About to call dispatcher target_index={}", target_index);
}

#[no_mangle]
#[cfg(feature = "profiler")]
pub fn check_dispatcher_target(target_index: i32, max: i32) {
    //dbg_log!("Dispatcher called target={}", target_index);
    dbg_assert!(target_index >= 0);
    dbg_assert!(target_index < max);
}

#[no_mangle]
#[cfg(feature = "profiler")]
pub fn enter_basic_block(phys_eip: u32) {
    let eip =
        unsafe { cpu::translate_address_read(*global_pointers::instruction_pointer).unwrap() };
    if Page::page_of(eip) != Page::page_of(phys_eip) {
        dbg_log!(
            "enter basic block failed block=0x{:x} actual eip=0x{:x}",
            phys_eip,
            eip
        );
        panic!();
    }
}

#[no_mangle]
pub unsafe fn set_jit_config(index: u32, value: u32) {
    match index {
        0 => JIT_DISABLED = value != 0,
        1 => MAX_PAGES = value,
        2 => JIT_USE_LOOP_SAFETY = value != 0,
        3 => MAX_EXTRA_BASIC_BLOCKS = value,
        4 => crate::cpu::jit64::set_threshold(value),
        59 => JIT_THRESHOLD_NOW = value.max(1),
        68 => JIT_RECOMPILE_THRESHOLD = value.max(1),
        69 => COMPILE_NEAR_END_OF_PAGE = value != 0,
        71 => COMPILE_CROSSING = value != 0,
        72 => crate::cpu::decoded::ENABLED = value != 0,
        73 => crate::cpu::decoded::THRESHOLD = value.max(1),
        70 => MAX_COMPILING = value.max(1),
        62 => crate::cpu::instructions_0f::CPUID_HIDE = value,
        65 => INLINE_LOOKUP = value != 0,
        66 => LOOKUP_IN_FUNCTION = value != 0,
        67 => DROP_REGISTER_MOVES = value != 0,
        63 => crate::cpu::jit64::OUTLINE_MEM = value,
        64 => crate::cpu::jit64::SHARED_RETRY = value != 0,
        60 => crate::cpu::sse_fp::FAST_PATHS_OFF = value != 0,
        9 => crate::cpu::jit64::set_disable_mask(value),
        41 => crate::cpu::jit64::WATCH_PAGE = value as u64,
        53 => crate::cpu::jit64::BACKOFF_CAP = value,
        54 => crate::cpu::jit64::HEAT_LIMIT = value as i32,
        55 => crate::cpu::jit64::HEAT_MAX = value,
        56 => crate::cpu::jit64::CHAIN_LIMIT_SET = value,
        58 => crate::cpu::jit64::RECOMPILE_FREE = value,
        57 => crate::cpu::jit64::set_inline_tlb_off(value != 0),
        48 => crate::cpu::jit64::PHYS_DUMP_BASE = value,
        47 => crate::cpu::jit64::WATCH_PAGE_FULL = 0xFFFFF_0000_0000 | value as u64,
        52 => crate::cpu::jit64::WATCH_PROBE_OFF = value as u64,
        40 => crate::cpu::jit64::HIST_ON = value != 0,
        42 => crate::cpu::jit64::INLINE_FLAGS = value != 0,
        43 => crate::cpu::jit64::CHAIN_BUDGET = value,
        44 => crate::cpu::jit64::RANGE_LO = value as u64,
        46 => { crate::cpu::jit64::VERIFY = value != 0; crate::cpu::jit64::set_inline_tlb_off(value != 0) },
        45 => crate::cpu::jit64::RANGE_HI = if value == u32::MAX { u64::MAX } else { value as u64 },
        10 => crate::cpu::cpu::TRACE_ON = value != 0,
        14 => crate::cpu::long_mode::SYSCALL_TRACE = value,
        17 => crate::cpu::cpu::JIT_CHAINING = value != 0,
        18 => FLAG_ELISION = value != 0,
        19 => FLAG_ELISION_ACROSS_ACCESSES = value != 0,
        20 => FLAG_ELISION_ACROSS_BLOCKS = value != 0,
        21 => NESTED_CALLS = value != 0,
        22 => NESTED_CALL_STATS = value != 0,
        23 => FLAG_ELISION_JCC = value != 0,
        24 => REP_STRING_CONTINUE = value != 0,
        25 => codegen::PAGE_ANCHORS = value != 0,
        15 => crate::cpu::long_mem::WATCH_ADDR = (crate::cpu::long_mem::WATCH_ADDR & !0xFFFF_FFFF) | value as u64,
        16 => crate::cpu::long_mem::WATCH_ADDR = (crate::cpu::long_mem::WATCH_ADDR & 0xFFFF_FFFF) | (value as u64) << 32,
        _ => dbg_assert!(false),
    }
}

#[no_mangle]
pub unsafe fn get_jit_config(index: u32) -> u32 {
    match index {
        0 => JIT_DISABLED as u32,
        1 => MAX_PAGES as u32,
        2 => JIT_USE_LOOP_SAFETY as u32,
        3 => MAX_EXTRA_BASIC_BLOCKS as u32,
        4 => crate::cpu::jit64::threshold(),
        59 => JIT_THRESHOLD_NOW,
        68 => JIT_RECOMPILE_THRESHOLD,
        69 => COMPILE_NEAR_END_OF_PAGE as u32,
        71 => COMPILE_CROSSING as u32,
        70 => MAX_COMPILING,
        5 => crate::cpu::jit64::STAT_COMPILED,
        6 => crate::cpu::jit64::STAT_FAILED,
        7 => crate::cpu::jit64::STAT_RUN,
        8 => crate::cpu::jit64::STAT_INVALIDATED,
        11 => crate::cpu::cpu::TRACE_BUF.as_ptr() as u32,
        12 => crate::cpu::cpu::TRACE_POS,
        13 => crate::cpu::jit64::STAT_JIT_INSNS,
        14 => crate::cpu::long_mode::STAT_STEPS,
        15 => crate::cpu::jit64::STAT_DECODED,
        16 => crate::cpu::jit64::STAT_BLOCKS,
        17 => crate::cpu::jit64::STAT_BYTES,
        30 => crate::cpu::cpu::STAT_TSC_READS,
        32 => NESTED_CALLS_MADE as u32,
        34..=41 => crate::cpu::jit64::DISPATCH_STATS[(index - 34) as usize],
        33 => NESTED_CALLS_CONTINUED as u32,
        31 => crate::cpu::cpu::STAT_LOOP_CLOCK_READS,
        20..=27 => crate::cpu::long_mem::SLOW_STATS[(index - 20) as usize],
        46000..=46063 => { let b = crate::cpu::jit64::visits_bucket(((index - 46000) / 2) as usize); if index & 1 == 0 { b.0 } else { b.1 } },
        44000..=45023 => crate::cpu::jit64::INVAL_WRITER[(index - 44000) as usize] as u32,
        40000..=42047 => { let e = crate::cpu::jit64::INVAL_LOG[((index - 40000) / 2) as usize]; if index & 1 == 0 { e.0 as u32 } else { e.1 | (e.2 as u32) << 31 } },
        100000..=104095 => crate::cpu::jit64::phys_byte(crate::cpu::jit64::PHYS_DUMP_BASE + (index - 100000)),
        80000..=84095 => { let a = (crate::cpu::jit64::WATCH_PAGE_FULL << 12) + (index - 80000) as u64; crate::cpu::long_mem::mem_read(a, 8).map_or(0xFFFF, |v| v as u32 & 0xFF) },
        90000..=90031 => { const T: [u32; 16] = [1, 100, 500, 1000, 2000, 5000, 10000, 20000, 50000, 100000, 200000, 500000, 1000000, 2000000, 5000000, 10000000]; let i = (index - 90000) as usize; crate::cpu::jit64::page_dist(T[i >> 1], (i & 1) as u32) },
        91000..=91159 => crate::cpu::jit64::top_page((index - 91000) / 4, (index - 91000) % 4),
        92000 => 0,
        97010..=97017 => crate::cpu::jit64::EXIT_COUNT[(index - 97010) as usize],
        97000 => crate::cpu::jit64::TIME_MODULES as u32,
        97001 => crate::cpu::jit64::TIME_INTERP as u32,
        96000..=96007 => crate::cpu::jit64::block_stats(index - 96000),
        95000..=95015 => crate::cpu::jit64::CAT_BYTES[(index - 95000) as usize],
        94000..=94031 => crate::cpu::jit64::KIND_BYTES[(index - 94000) as usize],
        94100..=94131 => crate::cpu::jit64::KIND_COUNT[(index - 94100) as usize],
        93000..=93007 => crate::cpu::jit64::TAIL_COUNT[(index - 93000) as usize],
        74300 => crate::cpu::jit64::HEAT_COMPILES,
        74301 => crate::cpu::jit64::HEAT_LAST as u32,
        74302 => (crate::cpu::jit64::HEAT_LAST >> 32) as u32,
        74200..=74210 => crate::cpu::jit64::watch_probe(index - 74200),
        74100..=74107 => crate::cpu::jit64::WATCH_WHY[(index - 74100) as usize],
        70000..=74095 => crate::cpu::jit64::WATCH_OFF[(index - 70000) as usize],
        50000..=58191 => { let e = crate::cpu::jit64::PAGE_HIST[((index - 50000) / 2) as usize]; if index & 1 == 0 { e.0 as u32 } else { e.1 } },
        60000..=64095 => (crate::cpu::jit64::PAGE_HIST[(index - 60000) as usize].0 >> 32) as u32,
        6220..=6227 => crate::cpu::jit64::BATCH_END[(index - 6220) as usize],
        6200..=6207 => crate::cpu::jit64::BATCH_COUNT[(index - 6200) as usize],
        6210..=6217 => crate::cpu::jit64::BATCH_INSNS[(index - 6210) as usize],
        6100..=6163 => crate::cpu::jit64::ALU_OPS[(index - 6100) as usize],
        6000..=6023 => crate::cpu::jit64::INTERP_WHY[(index - 6000) as usize],
        5000..=5063 => crate::cpu::jit64::VERIFY_LOG[(index - 5000) as usize],
        4000..=4063 => crate::cpu::jit64::COMPILED_RIPS[(index - 4000) as usize],
        20000..=28191 => { let e = crate::cpu::jit64::HIST_RIPS[((index - 20000) / 2) as usize]; if index & 1 == 0 { e.0 as u32 } else { e.1 } },
        30000..=34095 => (crate::cpu::jit64::HIST_RIPS[(index - 30000) as usize].0 >> 32) as u32,
        2000..=2511 => crate::cpu::jit64::EXECUTED[(index - 2000) as usize],
        1000..=1511 => crate::cpu::jit64::UNSUPPORTED[(index - 1000) as usize],
        _ => 0,
    }
}

/// (profiler builds) what the JIT knows of a physical page: hotness, entry points waiting, whether it has compiled
/// code, its compiled entry points, whether a compile is under way
#[cfg(feature = "profiler")]
#[no_mangle]
pub fn jit_page_debug(phys_page: u32, what: u32) -> f64 {
    let ctx = get_jit_state();
    let page = Page::page_of(phys_page << 12);
    match what {
        0 => ctx.entry_points.get(&page).map_or(-1.0, |e| e.0 as f64),
        1 => ctx.entry_points.get(&page).map_or(-1.0, |e| e.1.iter().count() as f64),
        2 => ctx.pages.contains_key(&page) as u32 as f64,
        3 => ctx.pages.get(&page).map_or(0.0, |p| p.entry_points.len() as f64),
        _ => ctx.compiling.len() as f64,
    }
}

/// (profiler builds) why crossing_instruction refuses the instruction at virtual `virt` now (0: it does not)
#[cfg(feature = "profiler")]
#[no_mangle]
pub fn jit_crossing_debug(virt: i32) -> i32 {
    let phys = match cpu::translate_address_read_no_side_effects(virt) {
        Ok(p) => p,
        Err(()) => return 1,
    };
    let cpu = CpuContext {
        eip: phys,
        prefixes: 0,
        cs_offset: unsafe { cpu::get_seg_cs() } as u32,
        state_flags: cpu::get_state_flags(),
    };
    if !unsafe { COMPILE_CROSSING } || phys & 0xFFF == 0 || !is_near_end_of_page(phys) {
        return 2;
    }
    if !cpu.state_flags.is_32() || !cpu.state_flags.has_flat_segmentation() || cpu.cs_offset != 0 {
        return 3;
    }
    let next_page = (phys | 0xFFF) + 1;
    if next_page + 0x1000 > unsafe { *crate::cpu::global_pointers::memory_size } || memory::in_mapped_range(next_page) {
        return 4;
    }
    if cpu::translate_address_read_no_side_effects((virt | 0xFFF) + 1) != Ok(next_page) {
        return 5;
    }
    if needs_atomic_interpretation(phys) {
        return 6;
    }
    let byte = |i: u32| memory::read8(phys + i) as i32;
    let mut c = CpuContext { eip: phys, ..cpu.clone() };
    let analysis = analysis::analyze_step(&mut c);
    let ty = match analysis.ty {
        AnalysisType::Normal => 10,
        AnalysisType::BlockBoundary => 20,
        AnalysisType::Jump { is_32: true, .. } => 30,
        AnalysisType::Jump { is_32: false, .. } => 40,
        AnalysisType::STI => 50,
    };
    match crossing_instruction(phys, virt, &cpu) {
        Some(_) => 0,
        None => 1000 * ty + (c.eip as i32 - phys as i32) * 10000000 + (analysis.absolute_jump as i32) * 100
            + (analysis.no_next_instruction as i32) * 200 + byte(0) * 0 + 7,
    }
}
