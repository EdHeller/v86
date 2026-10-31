# Why v64's x86-64 tier is faster than ours (notes)

**Update (later the same day): ours is now faster.** Alpine x86_64 to login (Node, this host):
ours about 40 s, v64 44.5 s; shell loop (40000 iterations): ours 2.4 s, v64 2.9-3.3 s. What closed the gap:

- PAUSE on one core left `smp::YIELD` set for good: every interpreted run stopped after one instruction
  and every in-place step left its module.
- Incremental compiles: a page whose module misses entries gets another module for them (discovery stops
  at entries the page's modules serve) instead of recompiling the page; 98 MB of generated code per boot
  became 34 MB. The function table grew from 900 to 12000 entries (v64's size): it was nearly full.
- With compiles that cheap, page heat (2000 interpreted instructions compile a page) and noting entries
  after two visits pay off.
- INVLPG of a page without compiled code keeps the epochs; the global-page check walks the page tables when
  the TLB lacks the page (kernel code looked non-global, every MOV CR3 retired all kernel slots); a replaced
  module drops its chain entries instead of stepping the epochs.
- MOV CR3 visits only the TLB indices of non-global translations (the jit TLB is 4096 entries now).
- Shorter inline accesses (one compare for page and crossing; entries hold the address delta).
- SSSE3/SSE4.1/SSE4.2 advertised in long mode (the kernel then does about 11 % less work, like on v64),
  with pshufb, palignr, psrldq/pslldq, psrlq/psllq, psubd, paddq and psubq compiled.

Still open: V8 compiles each module lazily with Liftoff on its first call (about 1-3 s of the main thread
per boot; `--no-wasm-lazy-compilation` moves it to background threads in Node); a 32-bit Alpine kernel
does not boot with SSE4 advertised (so it is advertised in long mode only).

The notes below are from before these changes.

Measured on the same 4-core host, Node, Alpine x86_64 to the login prompt
(`bootmin.mjs`): v64 43.5 s, ours 55.5 s (it was 95 s before this round).
Both retire about the same guest work (5.6 G vs 5.9 G instructions).

## Where our time goes (CPU profile of one boot, 60 s)

| | ours | v64 |
|---|---|---|
| compiled code | ~30 s | small share of a profile that runs mostly elsewhere |
| interpreter (step64, exec_op, decode) | ~9 s | 1.5 s (run_long_instruction) |
| dispatcher (try_run_block, call_block, cycle_internal) | ~10 s | ~4 s (pages::execute_inner, run) |
| instructions interpreted | ~135 M (8 %) | about 1 % (v64 docs/x86-64.md) |
| generated Wasm for the boot | ~118 MB | (its XP numbers: 27-98 MB) |

## What v64 does that we do not (yet)

- **Coverage.** About 1 % of instructions run outside compiled code in
  v64, 8 % in ours. Its entry rules: a page compiles after 2000
  interpreted instructions on it (page heat), and recompiles when 64 entries
  it does not serve were reached through its published function, with a
  backoff. We count per entry (150 visits) and add the noted entries of a
  page at its next compile. Page heat as we implemented it (`PAGE_HOT`) made
  our boot slower (more, emptier compiles); it is off.
- **Smaller code.** Inline flags (`materialize`, 26 MB) and inline memory
  accesses (read 29 MB, write 22 MB) are two thirds of our 118 MB. v64
  computes flags only where a reader can observe them (page-local
  liveness), lets condition consumers compare a CMP/SUB/TEST producer's
  operands directly, and compiles cold pages with outlined access lookups
  (recompiling them inline once hot: `x64_hot_inline`). Helper-call flags in
  ours (`NOFLAGS=1`) gave smaller code but a slower boot (61 s).
- **One function per page with a dispatch loop** (`br_table` over the page
  offset); blocks continue into each other without returning. Ours chain
  modules through a hash table and return to the dispatcher on misses
  (~39 M dispatcher runs per boot).
- **In-place steps.** Instructions without a template run inside the
  function (`x64_page_step`). Ported (`INPLACE_STEP`): interpreter exits
  fell from 12 M to 2 M, the boot time did not change.
- **Access cache with epochs** (`x64::jac`): flushes are an epoch step, global
  entries survive CR3 writes. Ported in spirit: our 64-bit tlb and jit tlb
  keep global entries on MOV CR3, and slot/chain validity uses per-privilege
  epochs.
- **STI shadow in compiled code**: STI and the next instruction in one block.
  Ours: STI compiled, leaving the module with the shadow set.

## Things tried that did not help

Compile thresholds (80-2500), recompile backoff, heuristic function-start
seeds (twice the code), immediate recompiles for noted entries, early first
compiles, page heat, V8 tiering flags (`--no-liftoff` 104 s,
`--wasm-tiering-budget`, `--no-wasm-dynamic-tiering`: no change).
