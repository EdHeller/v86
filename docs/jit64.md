# The 64-bit JIT (jit64)

x86-64 code (long mode) is compiled by `src/rust/cpu/jit64.rs`; the interpreter for it is `long_mode.rs`. The design
follows the page tier of v64 (see `docs/ir-design.md` there): no intermediate representation, direct templates.

## How it runs

* **One module per physical page and privilege level.** A page that has become hot (a branch target seen 150 times)
  compiles into one wasm function whose entry is a binary search over the offsets of its basic blocks, so it can be entered
  at every block start (`Block::served`). Call targets and return addresses of direct calls are block starts too;
  an entry that turns up later makes the page compile again with it as an extra seed (with a backoff after four
  compiles). Modules do not depend on where a page is mapped: the same module serves every mapping of the physical page.
* **Discovery** follows direct branches from the seeds; streams that overlap (a guess can start in the middle of an
  instruction) are dropped by an owner map. An instruction the JIT has no template for is left to the interpreter (the
  length comes from `full_len`, a length decoder for all of 64-bit mode), and so is an instruction that crosses the page end.
* **Registers live in i64 locals** (only the ones a module uses): loaded when the module is entered, stored by the
  one common epilogue, which also stores rip, counts the instructions and chains to the next module.
* **Slow accesses leave and the interpreter retries.** A memory access that cannot be done inline (no entry in the jit
  tlb, mmio, a page with compiled code, a fault) exits the module *before the instruction changed anything*, and the
  dispatcher runs exactly that instruction in the interpreter (`EXIT_KIND`: 2 interpreter-only instruction, 3 retry).
  Nothing inside a module raises an exception, so there is no previous_ip bookkeeping and no self-modifying-code check:
  stores to pages with compiled code always take the slow path.
* **The jit tlb** (`long_mem::JTLB`): four direct mapped tables (read / write, supervisor / user) with one compare per
  access; an entry exists only for accesses that can be done inline. Filled by the interpreter's translations,
  flushed with the tlb, write entries dropped when a page gets compiled code.
* **Flags are lazy:** the operands of the last flag-writing instruction stay in locals until something reads them (a
  branch reads them directly) or the block is left; `inc` / `dec` keep the carry flag from memory.
* **Chaining:** a module that leaves for an address with a validated slot tail-calls its module (`CHAIN`, a table
  the dispatcher fills), up to `CHAIN_LIMIT` instructions per dispatcher call.
* Instructions with templates: integer ALU, mov / movzx / movsx / lea, push / pop / call / ret / leave, shifts and
  rotates, bt family, cmov, setcc, bswap, mul / imul / div / idiv, cmpxchg, xadd, rdtsc, fences, simple string
  instructions, and the sse moves (movups / movaps / movdqa / movdqu / movd / movq) and logic ops (pxor / por / pand /
  pandn / xorps ...). Everything else is `Insn::Interp`.

## Coverage and dispatch

- **Global translations survive MOV CR3.** The 64-bit tlb and the jit tlb keep
  global entries (G with CR4.PGE) on a CR3 write; the dispatcher's slots and
  chain entries carry an epoch per privilege, and the supervisor epoch only
  steps on a CR3 write when supervisor code was validated through a
  non-global translation.
- **Module generations.** A slot is revalidated by its module index's
  generation (bumped when the module is replaced or dropped), not through the
  module map.
- **Hashes.** Slots (65536) and chain entries (65536) use a multiplicative
  hash: the xor of address bits put the hot entries of some kernel pages on
  the same slots, which then never reached the compile threshold.
- **Pending seeds.** An entry visited eight times in a page whose module does
  not serve it is noted; the page's next compile serves all noted entries
  (instead of one per recompile).
- **Straight-line runs enter modules.** The interpreter's run up to the next
  jump stops at the first instruction the page's module serves.
- **Compiled instead of interpreted:** cli/sti at CPL 0 (sti leaves with the
  interrupt shadow set), pause on one core, pushfq, shifts by cl without live
  flags, dword lane SSE (paddd, pslld/psrld imm, pshufd, punpckldq, shufps),
  and instructions that continue into the next page (compiled from the next
  physical page; the module is code of both pages and its slots check that
  the next virtual page maps there).

Alpine x86_64 to the login prompt (bootmin.mjs, one core, Node, this host):
about 95 s before these changes, 55-56 s after; v64 43.5 s.

## The interpreter

`rep movs` / `rep stos` go page stretch by page stretch (`string_fast`), the interpreter runs straight-line code in a
batch (`run`), and probes for a module at jumps and at page crossings.

## Measuring

* `jit64: step profile` (`step_profile_enable`): which instructions the interpreter runs; `get_jit_config(14)` counts the
  steps, `(1000 + opcode)` the instructions that ended a block's discovery.
* `tests/x64/jit/*`, `js/jit64_mask`, `tests/x64/fuzz.js`; `JIT64=0` runs every test with a threshold of 0.
* Alpine x86_64 to login (one core, Node, 4-core host, medians of several runs): v86's blocks 201 s (one module per
  entry), page modules 130 s, bswap / generic lengths / page crossings / string fast path 99 s, retry design and
  registers in locals 91 s, the jit tlb, chaining and native mul / div / sse 82 s. v64 on the same host: 52 s.
  A 100000-iteration busybox shell loop: 16.7 s -> 10.4 s (v64: ~7.9 s).
