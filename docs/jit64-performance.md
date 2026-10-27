# Making the 64-bit JIT (jit64) faster

Notes from profiling TempleOS (x86_64) and Alpine Linux (x86_64) on the Node runners in `tests/x64/manual/`.

## Where we are

| workload | measurement |
| --- | --- |
| Alpine x86_64, busybox `ash` loop `i=$((i+1))` (40000 iterations) | 54 s with jit64 off, 27 s with it on (2x) |
| Alpine i686 (32-bit), same loop (30000 iterations), existing 32-bit JIT | 22.7 s off, 5.8 s on (3.9x) |
| per iteration: 32-bit JIT vs jit64 | about 5200 vs 1500 iterations per second: **jit64 is 3.5x behind the 32-bit JIT** |
| TempleOS demos (`TOS_FPS=5`) | 23-32 real frames/s, 70-100 guest MIPS, 96-98 % of instructions in compiled blocks; the demos cap themselves at about 30 fps |
| inline tlb fast path (done) | +25 % instructions per second on TempleOS (80 -> 100 MIPS) |

Profile of a compiled-code workload (Titanium, before the inline tlb): about 30 % of host time in the memory helpers
(`mem_read`, `j64_read`, `read_phys`, `mem_write`, `write32`), 9 % in `j64_flags` / `j64_alu`, 12 % idle (guest `hlt`),
and only a few percent in the generated code itself. In Alpine's process-heavy boot about 28 % of host time is idle and
48 % is the interpreter plus helpers, since only about half of the instructions run in compiled blocks.

## Ideas, most valuable first

1. **Keep guest registers in wasm locals** across the instructions (and basic blocks) of a module, as the 32-bit JIT does
   (`gen_move_registers_from_locals_to_memory`). jit64 loads and stores `rax..r15` from memory for every operand.
2. **Lazy flags like the 32-bit JIT** (store last operands / result / size and evaluate a condition inline at the `jcc`)
   instead of the `j64_flags` helper call for every flag writing instruction whose flags are live. jit64 already drops
   dead flags (`flag_liveness`), the live ones still need the call.
3. **Link blocks without returning to Rust.** Every block exit goes through `jit64::run` (slot lookup, epoch check,
   `call_jit_table`). Within a page the br_table dispatcher already keeps control in the module; across pages and for
   `call` / `ret` / indirect jumps use wasm **tail calls** (`return_call_indirect` through the function table).
4. **Inline the remaining helpers:** shifts / rotates by cl (`D3`), `imul`, `mul` / `div`, `bsf` / `bsr`, `cmpxchg`,
   `xadd` (the last two show up as the top blocks stoppers in the Alpine boot, see `ALP_UNSUPPORTED=1`), `pushf`/`popf`,
   `cli` / `sti`, `endbr64` (a nop), SSE moves and logic ops. Today an unsupported instruction ends discovery and the
   block falls back to the interpreter for it.
5. **String instructions with `memory.copy` / `memory.fill`** (bulk memory is already enabled) when source and destination
   stay inside one tlb-mapped ram page.
6. **Faults without flag polling.** After every memory access the generated code checks `FLAGS[FAULT]` / `FLAGS[SMC]`.
   With wasm **exception handling** the slow path helper can `throw` and the block entry can `try`/`catch`, so the common
   path has no check at all.
7. **Cache the decode** in the interpreter (`decode_modrm` and `exec` are 5-20 % of the profile) keyed by page and offset,
   since a lot of kernel and process start-up code runs once or twice and never reaches the jit threshold.
8. **Persist compiled modules** (IndexedDB / OPFS) keyed by a hash of the code bytes and the generator version: hot
   kernel paths would not be compiled again on the next start. Likewise persist a booted snapshot
   (`ALP_SAVE` / `save_state`) so that Alpine or TempleOS start instantly after the first run.
9. **SIMD:** compile SSE integer instructions to wasm `simd128` (the build already uses `+simd128`).
10. Process-heavy guests (fork / exec) spend time in the kernel's cold paths: a lower threshold for the kernel,
    or compiling a whole page when its first block gets hot, may pay off. Measure with `JIT64=<threshold>`.

## WebAssembly 3.0

What the build uses today: wasm32, `bulk-memory`, `multivalue`, `simd128` (Makefile `CARGO_FLAGS`), i.e. 2.0 features.
Nothing from 3.0 is used by the emulator or by the generated JIT modules. The Node 22 runtime we test with accepts
tail calls, multiple memories, memory64 (behind a flag in the JS API) and exception tags.

- **Tail calls**: yes, for block chaining (idea 3). Needs Chrome 112+ / Firefox 121+ / Safari 18.2+.
- **Exception handling**: yes, for fault paths (idea 6).
- **Relaxed SIMD**: maybe, for the x87 / SSE float paths.
- **memory64**: not for speed. V8 implements 64-bit memories with explicit bounds checks instead of guard pages, so
  every access gets slower. It only helps guests with more than 4 GB of ram.
- **Multiple memories**: no speed gain by itself (could separate tlb / register file from guest ram).
- **GC / typed function references**: not useful for this design.
Feature support should be checked on every browser before relying on it (feature detection with `WebAssembly.validate`
and a fallback module, as the existing `v86-fallback.wasm` does for older engines).

## How to measure

`tests/x64/manual/templeos.js` (`TOS_FPS=5`, `TOS_SLOW=1`, `node --cpu-prof`), `tests/x64/manual/alpine.js`
(`ALP_UNSUPPORTED=1`, `ALP_LOAD` / `ALP_SAVE`, `JIT64=<threshold>`; `4294967295` switches jit64 off) and
`get_jit_config` (5-8, 13 = instructions retired in compiled blocks, 20-27 = inline tlb misses).

## 32-bit JIT notes (existing JIT, Alpine i686)

Profiler build (`make with-profiler`, `ALP_STATS=1`) on the 32-bit shell loop: 27.9 M compiled block runs retire 509 M
instructions (18 per run), 97 % of the exits leave through a different page, 5 % of the instructions are interpreted.

Experiment: **block chaining** (the shared exit of a module looks up the next compiled block itself and does a wasm
`return_call_indirect`, checks for halt / interrupt shadow / single stepping / same state flags, bounded by a chain
budget). It works (`jit_chaining: true`, `CHAIN=1`) but is about **8 % slower** (60000 iterations: 9.65 s vs 8.93 s,
measured sequentially; parallel runs of the two variants gave the opposite answer, always measure one at a time):
the check sequence on every exit and the tail call through the table cost more than the dispatch in Rust.
Off by default. Ideas that would make chaining pay off: do the lookup only at exits whose target is known to be in
another compiled page (direct `return_call` to a known table index patched in later), skip it for exits that are
followed by the interpreter, and make the budget a local instead of memory.
