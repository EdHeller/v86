# Plan (continue from here)

## How to work (read first)

- **Don't be verbose.** Only message the user when a task on this list is done and verified. Use one short line,
  e.g. "UEFI boots Alpine aarch64 to a login (tests/a64/uefi.mjs)". Don't report waits, retries, failures or
  intermediate steps. Explain more only if the user asks.
- Repository `EdHeller/v86`, branch `Vibe-Claude`. Commit and push every change. Don't open pull requests unless
  asked. End commit messages with the attribution lines from the session reminder.
- v64, the reference project, is at `/home/user/speedyhkjournalist/v64` (clone `speedyHKjournalist/v64` if it is
  missing). Its docs cover each area: `docs/aarch64.md`, `docs/arm64-virt-android16-plan.md`,
  `docs/android-images-plan.md`, `docs/gpu-devices.md`, `docs/gfxstream-plan.md`, `docs/glbridge.md`, `docs/uefi.md`.
- When you finish an item, check it off here, add a dated line under "Done", and commit.

## State (2026-11-05)

- **32-bit x86 (`v86.wasm`):** SSE3/SSSE3/SSE4 in every mode; as fast as upstream v86 again (`docs/releases.md`, speed note).
- **x86_64 (`v64.wasm`, `libv64.js`):** faster than v64: Alpine x86_64 to login in about 40 s against v64's 44.5 s.
- **ARM (`v86-arm64.wasm`, `libv86-arm.js`):** Alpine aarch64 by direct kernel boot and through UEFI; virtio-gpu 2D
  on virtio-mmio.
- **Machines and devices:** UEFI (OVMF) and Q35, TPM 2.0, USB, HDA, virtio-pci, graphics adapters (bochs_vga,
  vmware_svga, virtio_gpu with virgl 3D on WebGPU), multicore (cooperative and in workers), the JS split per
  architecture.
- **Packages** (git-ignored, in the root; recorded in `docs/releases.md`): `v86-release.tar.gz` (the website files),
  `v86-cube.tar.xz` and `v86-cube-x86.tar.xz` (kmscube on virgl). Send files to the user as attachments; links
  don't reach them. Keep each file under 500 MB (xz).

## Measuring

- **Profiles and build comparisons:** `tools/profile/profile.mjs` (`docs/profiling.md`): areas, functions, boot
  timeline, guest symbols; `--runs N` with several `--wasm` for A/B on a virtual guest clock. Compare only within
  one invocation (the host drifts by 10 %).

- **32-bit against upstream:** upstream's master (315b2b0) builds in a worktree at `/root/upstream-v86`;
  `REPO=<tree> node <scratchpad>/bench86b.mjs` boots 32-bit Alpine and prints the time to login and the best of three
  runs of a 20000-step shell loop and an 8 MiB gzip round trip. Alternate the two trees.
- **x86_64 boot time:** `/root/jitwork/bench3.sh 3` (the scripts and ISOs are in `/root/jitwork`; rebuild them from
  `docs/next-session.md` after a container reset).
- **Tests:** `node tests/x64/run.js` (89 ok), `JIT64=0 node tests/x64/fuzz.js --seed 6 --count 1000`,
  `node tests/uefi.mjs`, `node tests/a64/boot.mjs`, `node tests/a64/virtio_gpu.mjs`, `node tests/virtio_gpu.mjs`.
- Run benchmarks one at a time with nothing else running (no builds). Differences under about 3 % are noise.

## Tasks, in order

The user's order (2026-11-04): ⬜ open, ✅ done; the numbers are the Tasks tab's. Finished items move to "Done" below.

**Packages for the user** (benchmarks, releases): every README starts with the reminder to clear the browser cache
(Ctrl+Shift+R, or DevTools > Network > "Disable cache") and unregister old service workers before trying a new
build; a release always ships v86-fallback.wasm / v64-fallback.wasm with libv86.js (an old fallback beside a new
libv86.js fails with "Missing import").

1. ✅ **32-bit speed back to upstream's** (2026-11-04; `docs/releases.md`, speed note).

2. ✅ **Full instruction coverage on x86: 16-bit, 32-bit and x86_64** (2026-11-04). The tool: `tools/x86_coverage`
   (`reference.py`: every encoding GNU objdump decodes, per mode; `probe.mjs`: one flat binary that runs each form from
   patterned registers and memory and records the exception, registers, flags, xmm and memory; `run.mjs 32 all`:
   runs it in v86 and in QEMU (TCG, `-cpu max`) and writes `coverage-32.md`).
   - ✅ 32-bit protected mode (2026-11-04): added the SSE4.2 string compares, AES-NI, SHA, ADX, lzcnt, rdtscp
     (IA32_TSC_AUX), rdseed, monitor/mwait, clac/stac, fbld, 16-bit fsave/frstor, 16-bit movbe, hint NOPs (0F 1A/1B);
     fixed the x87 indefinite's sign, fninit's selectors, SSE4a #UD; CPUID reports them. What only QEMU runs is
     AMD-only (3DNow, SSE4a) or TSX; every value difference has its reason in the report (QEMU's deviations from the
     SDM, undefined flags).
   - ✅ 64-bit long mode (2026-11-04, `run.mjs 64 all`, `coverage-64.md`, 47,632 forms): the probe runs on an IST
     stack with a shared dump routine. Fixed in v64: `enter` (missing), lahf/sahf (and their CPUID bit), push/pop
     fs/gs (8 bytes), REX.W over 66 for stack operations, lss/lfs/lgs with REX, swapgs with REX.B, movd r8d..r15d,
     mm (lost the write), cvt(t)ps2pi with REX.R (wrote past mm7 and crashed the core), mmx memory forms with REX.B
     (addressed [rax] for [r8]), 66 for fnstenv/fldenv/fnsave/frstor, fxsave64's 64-bit pointers, #UD for movnti
     and prefetch register forms and xgetbv with a prefix; both cores: ptwrite/clrssbsy #UD, clwb. Every remaining
     difference has its reason in the report (AMD-only, TSX, QEMU's REX-on-mmx bug, undefined flags).
   - ✅ 16-bit code (`run.mjs 16 all`, `coverage-16.md`): the probes in 16-bit code segments of protected mode
     (16-bit operand and address sizes, [bx+si] memory forms). v86 matches; QEMU runs crc32, popcnt and movbe with
     32-bit operands in 16-bit code. Known: a bt/btc/btr/bts bit offset beyond the segment does not wrap the 16-bit
     effective address at 64 KiB (rare).
   - ✅ The JIT (`run.mjs <bits> jit`, `jit-<bits>.md`): every probe interpreted and compiled (many passes, the JIT's
     threshold lowered through `set_jit_config(59, ..)`), no differences. Fixed: compiled `bound` with a 16-bit operand
     (read 32-bit bounds: the handler asked the prefixes at run time), compiled `fbld` (#UD), `fpu_fbstp` not
     exported (a LinkError when a compiled block used fbstp; `tests/x64/js/jit_exports.js` now checks every function
     name the code generators call), jit64's push/pop/leave with 66 and REX.W.
   - Left for later: real mode specifics (segment loads without descriptors) are covered by the hand-written tests.
3. ⬜ **Fewer switches between JavaScript and WebAssembly** (copy/v86#924: "Port pic+apic to Rust. Allows running
   more of the main loop in Rust (fewer switches between Rust/JS)"; the PIC, APIC and I/O APIC are in Rust already).
   Every pass of the main loop still calls JavaScript: `run_hardware_timers` (the PIT, RTC, HPET, ACPI timer and the
   devices' timers), `microtick` for the clock, and port I/O to devices written in JavaScript. Move the PIT, RTC, HPET
   and ACPI PM timer to Rust, read the clock without a call per check (`performance.now` once per slice), and run the
   timer pass in Rust, so a slice leaves WebAssembly only for devices that need JavaScript (disks, network, screen).
   Measure with the micro benchmarks (task 6's harness) and `tools/v86-bench`.
   - ✅ Measured (2026-11-04, a profile of Alpine's boot and the four workloads): JavaScript is **1.2 %** of the time
     (the main loop calls it twice per 100,000 instructions), so moving the timers to Rust gains at most about 1 %.
     The switch that costs is between the main module and compiled code: 406 M entries into compiled blocks (about 40
     instructions each), 89 % of the exits leave for another page, and `main_loop` itself is 11 % of the time (about
     30 ns per round trip: an indirect call into another instance, registers loaded and stored at each end). Calls
     from compiled code into the main module (helpers) cost the same way: see task 6's SSE work.
   - ✅ Block chaining re-measured on the user's machine (bench7, `SCALE=4`): slower (loop +7.8 % against +6.2 %
     without it, against upstream); stays off. A page change costs about 8 ns here (micro `page_hop`: the same loop
     on one page takes a quarter of the time); upstream pays the same.
   - ✅ The generated code against upstream's (`DUMP_WASM`, wasm2wat): the same apart from the inline indirect-jump
     lookup and the loop limit, which was loaded from memory on every back edge (`a8ada50f`: a constant now; a tight
     loop +10.5 % -> +2.8 %).
   - ⬜ The CPUID features we report send the guest down other paths: 70,000 monitor/mwait (mwait is a no-op, so
     Linux's idle loop spins), 1.7 M stac/clac (SMAP; each ends a compiled block), 25 M SHA-NI instructions
     (sha256sum; each a call into the interpreter). `set_jit_config(62, bits)` hides groups; bench8 sent to measure
     them. Then: compile the 0F 38 / 0F 3A instructions (SHA, pshufb, palignr, pclmulqdq) instead of interpreting
     them, mwait as hlt when interrupts are on, the rdtsc clock read and the JavaScript devices' port I/O.
   - ✅ The indirect-jump lookup in a function of the module (`8c2cfe12`): in the main function it slowed every
     loop of the module on the user's CPU (code16 +96 %, segment +13 %); through upstream's call it is slow itself.
   - ✅ Modules move only the registers they use (`9727e77e`): page_hop -11 %, call_pages -13 % against upstream here.
   - ✅ bench11 on the user's machine (2026-11-05, `REPEAT=50`, 5 rounds, medians against upstream): nothing slower
     beyond noise (worst: spinlock +4.1 %, smc +3.2 %, bitops +3.2 %). Faster: unaligned -56.5 %, pagewalk -42.7 %,
     page_hop -35.9 %, tlb4k -19.9 %, cpuid -18.9 %, indirect -16.4 %, pagefault -16.2 %, scalar_fp -15.9 %, sysenter
     -12.3 %, call_pages -9.9 %, call_near -9.8 %, call_deep -9.3 %, sse -8.9 %, call_far -8.3 %, pit -7.1 %.
     `ours11-allregs` (every register moved at each module boundary) loses most of it: page_hop +1.1 %, call_pages
     -6.8 %, spinlock +14.2 %, interrupt +5.3 %; moving only the registers a module uses stays.
   - ✅ The dispatcher's own share (2026-11-05): its hot path is about 15 loads and the call; nothing left to cut
     there. Fewer round trips need bigger modules, and those cost compile time: `set_jit_config(1, 4)` and `(1, 8)`
     halve page_hop but log in 3 % and 18 % slower.
   - ✅ 0F 38 / 0F 3A (SSSE3, SSE4, SHA, AES-NI, crc32, movbe) no longer end a compiled block: the module calls the
     interpreter's function and leaves only on an exception (`jit_instr_0F38`). `run.mjs 32 jit`: 0 forms differ;
     Alpine boots. ⬜ Measure with programs that use them (glibc's SSSE3/SSE4 string functions, sha256 with SHA-NI).
4. ✅ **The lazy-compile cost** (measured 2026-11-04; Alpine x86_64 boot to login and the four workloads,
   `ARCH=x86_64 JIT64_STATS=1 node tools/v86-bench/run.mjs --child <build>`, `--trace-wasm-compilation-times`).
   - The main thread's Liftoff compiles: 3,633 functions, 45 MB of WebAssembly, **1.0 s** in a 95 s boot (about 1 %,
     the main module's functions included), not 1-3 s. TurboFan tiers the hot ones up in background threads: 2,371
     functions, **52 s of CPU** (no main-thread time; costs on machines with few cores).
   - jit64 generates 48 MB per boot: memory accesses 19 MB (inline TLB checks), flags 6 MB; 3,207 modules, a third
     of them invalidated (pages freed and reused). Smaller code costs speed: memory accesses out of line in the
     module (`set_jit_config(63, 2)`) 39.6 MB but login 103 s instead of 75 s; a shared retry exit (`64`) only 3 %
     smaller. A page cannot ask V8 for eager compilation (only `--no-wasm-lazy-compilation` does). Closed: nothing
     to gain worth the speed; revisit if compiled code gets much larger.
5. ⬜ **v64 parity on x86.** See "Plan: v64 parity" (devices, firmware, memory above 4 GiB, disk formats and
   persistence, the VM manager). The ARM half of v64 parity is task 8.
6. ⬜ **Speed ideas from copy/v86#924** (copy's comment of 2023-09-19). Profile first as copy suggests: the Firefox
   profiler (https://profiler.firefox.com/docs/), `make debug-with-profiler` (stats at the bottom of debug.html), the
   generated wasm and x86 code (`docs/profiling.md`).
   - ✅ a. Dead flags in the 32-bit JIT (2026-11-16): a pass over each basic block (`flags_dead_after`, `flag_effect`)
     finds add/sub/cmp/and/or/xor/test whose flags the next such instruction overwrites before a read; the 32-bit
     add/sub/bitwise emitters and cmp/test of any size then store nothing. Unknown instructions, block ends and other
     memory instructions count as reads; plain loads, stores, push and pop do not (`set_jit_config(19, 0)` makes them
     reads: the frame of an exception they raise would hold older arithmetic flags, as jit64 already accepts).
     Alpine x86 boot: 334 M of 1,305 M flag stores left out; login -2.5 % (median of 4 rounds, -5.5 % to +0.1 %);
     with movzx/movsx and mov to/from a fixed address as neutral: 356 M, login -3.0 % (faster in every round, -5.9 %
     to -0.8 %);
     registers only (19 = 0): 245 M left out, no measurable change. `set_jit_config(18, 0)` turns it off.
     Tests: `tests/x64/jit/19_flag_elision_32.asm`, `run.mjs 32 jit` 0 forms differ. cmp+jcc fusing was there.
     Across the basic blocks of a module (`flags_live_out`, a fixpoint over the blocks; `set_jit_config(20, 0)` turns
     it off): flags at a block's end are dead when every successor in the module overwrites them first; 395 M of
     1,305 M left out. Test: `tests/x64/jit/20_flag_elision_blocks.asm`. Login -1.1 % against block-local elision
     (median of 6 rounds, -4.8 % to +4.7 %: within the noise).
     A cmp/test/bitwise/add/sub right before a block's jcc, its flags dead after the jump, stores nothing when the
     jump computes its condition from registers (`codegen::condition_from_locals`; `set_jit_config(23, 0)` turns it
     off): 456 M of 1,305 M left out; login -0.9 % (5 rounds, -2.7 % to +1.2 %). Test: `21_flag_elision_jcc.asm`.
   - ✅ jit64's dispatcher measured (2026-11-17): an Alpine x86_64 boot calls try_run_block 18 M times; 13 M run a
     ready slot, 7.6 M of those after the slot's epoch changed (a MOV CR3 steps the user epoch, about 33 K times), so
     the slot checks its page again (a page walk: probe_fetch does not fill the tlb). A per-epoch cache of those
     probes did not change the boot (+1.0 %, within the noise); reverted. The hot path is cheap: 10 M calls and
     returns across pages take 0.35 s (`tests/x64/manual/bench_dispatch.asm`). Counters: tools/profile for x86_64
     ("dispatch: ..."). Then (option 2): per call run about 22 ns, try_run_block 74 ns, call_block 126 ns (18 M, 18 M
     and 12.6 M calls), against 17 ns per entry in the micro benchmark: the boot's slots (65536 x 48 bytes, hashed
     apart) and 3,126 modules miss the caches. The rest is small: revalidations 0.19 s, first visits 0.9 s (refill_slot
     0.5 s), counting slots 1.6 s with the interpreter they run. jit64's chaining (tail calls between modules,
     `set_jit_config(56, 0)` off) is what keeps the entries few: off, 197 M entries instead of 12.6 M and login
     +5.3 % (every round), unlike the 32-bit JIT's chaining (+6.7 %). Possible later: a smaller slot table (less
     than 1 %).
   - ✅ 2026-11-17, 32-bit JIT on buildroot (i686) workloads (`tools/profile`, rounds against the build before):
     rol/ror inline (sha256 -9 to -11 %); the last 15 bytes of a page compiled when the instruction ends before
     the page's last byte (boot -4.3 %, shell loop -5.7 %, sha256 -7.2 %; Alpine -0.9 %); pages with code
     recompiled after 50 K heat instead of 200 K (boot -1.6 %, loop -2.1 %); up to 4 modules compiled at once
     (boot -6.3 %, loop -5.3 %); more flag elision (inc/dec, shifts; memory-operand ALU kills flags: noise).
     Against `Vibe-Claude` on the host clock (`bench.mjs`, 3 rounds): sha256 -48 %, md5 -20 %, gzip -7 %, boot,
     shell loop and forks about -3 %. Also: the guest sampler of `--guest-map` was mostly measuring IRQ stubs
     (fixed: eip from the core's timer pass). Not better: a lower first-compile threshold (100 K, 50 K), 8
     modules at once.
   - ✅ 2026-11-18 instructions crossing a page compiled at the end of their block (plain ones from an allowlist,
     jcc/jmp rel; their bytes in the next page compared through the TLB before they run): gzip -4.4 %, shell loop
     -1.7 % (noise).
   - ✅ 2026-11-18 interpreter: read_imm inlined, flat segment bases without loads, modrm_resolve inlined, the TLB
     fast path of compiled code for safe_read/safe_write/safe_read_write 8/16/32. JIT off (the interpreter alone):
     buildroot boot -24 %, faster in every round. With the JIT: within noise (Alpine +1.7 %, buildroot -3.5 %).
     Not better: going on interpreting across jumps in a page instead of returning to the dispatcher; the 24 modrm
     arms as one compact function (noise; inlined everywhere +2 MB of wasm). Next for the interpreter: decoded
     instructions cached per page (a larger change).
   - ✅ 2026-11-18 calls that cross (`call rel32`: the pushed return address is in the next page,
     `codegen::gen_get_real_eip`) and blocks that start at a crossing instruction (a jump's target, a call's return).
     Buildroot shell loop -2 % (noise). Test `tests/x64/jit/28_crossing_call`.
   - ✅ 2026-11-18 dispatch (12 % of the shell loop) looked at again: its hot path is the tlb_code lookup, the state
     check and the indirect call; the profiler-only branches compile out. Nothing cheap left; fewer round trips
     need the measured-and-lost options (chaining +6.7 %, nested calls +5.9 %, bigger modules). Removed a leftover
     IRQ debug log from the interrupt path.
   - ✅ 2026-11-18 interpreter phase 2: decoded instructions cached per physical page (`cpu/decoded.rs`, the
     handlers generated from the interpreter's arms by `gen/generate_decoded.js`: 354 opcodes); a page is decoded
     after 32 interpreted runs (`set_jit_config(73, n)`; 72 turns it off); writes reach it through jit_dirty_page.
     JIT off: buildroot boot -19 %; with the JIT: -2 to -4 % (noise). Lost: handlers as function pointers (an
     indirect call per instruction, +13 %); decoding on the first run (+6 % with the JIT: cold code). Test
     `tests/x64/jit/27_decoded_cache_smc`; nasm tests 15,659/15,659 with every page decoded.
   - ⬜ For later (2026-11-17): measure the experiments that lost in V8 in Firefox (SpiderMonkey): chaining with tail
     calls (`set_jit_config(17, 1)`, +6.7 % in V8), nested calls (`(21, 1)`, +5.9 %), bigger modules
     (`(1, 4)`, `(1, 8)`). Their cost in V8 was the call between generated modules; another engine may make it
     cheaper. tools/profile runs in Node only: needs a page-based harness (or the browser tests) for Firefox.
   - ✅ The JIT's per-page entry points in a bitmap instead of a hash set (2026-11-17): every interpreted run notes
     one, and SipHash cost 0.5 s of a boot (hotness bookkeeping 2.0 s -> 1.5 s inclusive in the profile).
   - ✅ b. call/ret as wasm calls, measured (2026-11-17), off by default: a call out of a module (32-bit JIT) calls
     the callee's module (`codegen::gen_nested_call`, `set_jit_config(21, 1)`; depth at most 16; the callee's
     instructions count for the caller's loop limit) and goes on at the return address when the callee returns.
     Alpine boot: 58 M nested calls, the caller goes on after 43 M of them (each two passes through the dispatcher
     saved), and still +5.9 % (+4.5 % with the code in the main function): in V8 a call from one generated module into
     another costs about 85 ns, more than the dispatcher's round trips. As chaining (+6.7 %). Counters:
     `set_jit_config(22, 1)`, `get_jit_config(32)` / `(33)`, shown by tools/profile for x86. Worth measuring in other
     engines (Firefox).
     (was: ⬜ b. Function boundaries instead of one big br_table; call/ret as wasm calls. jit64 has one function per page;)
     call/ret still return to Rust; the 32-bit JIT is as upstream.
   - ⬜ c. VGA memory writes through the memory fast path (copy/v86#301).
   - ⬜ d. The boot CPU in a Web Worker (copy/v86#534, #219); the cores after the first already run in workers.
   - ⬜ e. Stack accesses: translate ESP to physical once per block when the access size is known.
   - ✅ f. Micro benchmarks (`tools/v86-bench/micro`, 33 benchmarks, no OS) and whole-system runs (`tools/v86-bench`);
     packages with one improvement taken out per build. The user's machine (low noise), 2026-11-04, ours against
     upstream: unaligned -56 %, pagewalk -43 %, tlb4k -17 %, indirect -13 %, call_deep -10 %, sysenter -10 %, call_far
     -9 %; slower: sse +34 %, spinlock +28 %, scalar_fp +19 % (before its fma fix), rep_small +8.5 %, interrupt +6 %.
     Whole system: shell loop +5.8 %, login +2.5 %. Taken out: the dispatcher fixes cost +42 % on the loop without
     them, the inline lookup -10 points on indirect, the SSE fast paths -41 points on scalar_fp.
     - ✅ Inline indirect-jump lookup (`20b72004`); ✅ dispatcher inlined again (`6721d285`, `7d89c947`); ✅ SSE fast
       paths (`684914a5`, `2fe2b6d6`); ✅ chaining with tail calls: slower, off; ✅ bigger modules: no gain.
     - ✅ SSE inline with wasm SIMD (`2e5fec9b`): the bitwise and integer packed instructions are one SIMD
       instruction; addps/subps/mulps and the scalar add/sub/mul the operation and a cheap bit check, the helper only
       when a lane needs it; cvtsi2sd, cvttsd2si inline. The call from compiled code into the main module was the
       cost (a trivial helper like xorps took as long as upstream's whole addsd). Here: sse +45 % -> +8 %,
       scalar_fp +127 % -> +48 % (the micro benchmarks' values stay exact, so the inexact test runs every time;
       with the flag set, as in real programs: sse -4 %, scalar_fp +5 %). `tests/x64/js/sse_fast_paths.js` checks
       45 forms against the host CPU bit for bit and found three old flag bugs (sums with far apart exponents,
       underflowing double products, conversions without flags), fixed.
     - ✅ bench11 (task 3): sse -8.9 %, scalar_fp -15.9 %, rep_small -4.9 %, interrupt +0.3 %, spinlock +4.1 %
       against upstream (were +34 %, +19 %, +8.5 %, +6 %, +28 %).
     - ✅ jit64: imul, adc and sbb whose flags are dead compile inline (2026-11-16); they were 40 M of the 108 M helper
       calls of an Alpine x86_64 boot (`jit64_alu_ops_addr`: the helper's counts per operation). Micro
       (`tests/x64/manual/bench_alu.asm`, 30 M iterations): 1.44 s -> 0.34-0.45 s; Alpine x86_64 login -3.3 % (median of
       4 rounds, within the noise). Test: `tests/x64/jit/18_dead_flag_alu.asm`. Tried and dropped: a cheap hasher for the
       32-bit JIT's entry sets (no change), chaining (+6.7 %) and a lower JIT threshold (+3.8 % at 50K) re-measured.
       Known deviation (old, by design: `FLAGS_ACROSS_ACCESSES`): an ALU instruction with a memory operand that faults
       in compiled code shows the flags from before the previous flag-setting instruction in its exception frame
       (that one's flags were dead); the retried instruction overwrites them all, so only a handler that reads the
       frame's arithmetic flags sees it. Adding memory -> also memory destinations with live flags inline.
     - ⬜ Next: spinlock, smc and bitops (+3 to 4 %, near noise); more SSE inline (movaps/movdqa, comparisons, shuffles, the remaining conversions, divs/sqrt).
7. ⬜ **virtio-gpu, the rest.**
   - ⬜ a. The ARM machine through `new V86({ cpu_type: "arm64", graphics_adapter: "virtio_gpu" })` (today only
     `examples/alpine-arm64.html?gpu=1`).
   - ⬜ b. virgl 3D on arm64 (host visible memory as device memory on the ARM bus).
   - ⬜ c. Venus (Vulkan) through v64's gfxstream path (`docs/gfxstream-plan.md` there).
   - It stays optional: without WebGPU the device is 2D, and without the option nothing changes.
8. ⬜ **ARM: PCIe, then Android.** Wire `src/arm/virt/pcie.js` in (`virtio_transport: "pci"`), boot Alpine on
   virtio-blk-pci by direct kernel boot and through UEFI (`make a64-boot-tests`), virtio-gpu-pci; then the rest of
   v64's ARM devices and Android, see "Plan: Android".
9. ⬜ **Apple: Darwin up through Mac OS X 10.14 Mojave on x86; the iPhone 4 on ARM.** See "Plan: Apple".
10. ⬜ **Buildroot: an x86_64 build and multicore** (`tools/buildroot/build.sh`): a 64-bit kernel and userland next to
   the 32-bit one; ACPI, SMP with the local and I/O APIC, HPET and TSC; the matching v86 options in the run
   instructions (`acpi`, `cpu_cores`, `cpu_type: "x86_64"`, the worker build). Check: `nproc` shows the cores.
11. ⬜ **The JavaScript devices in Rust** (added 2026-11-05). About 98,000 lines of device and machine code in
   `src/` (`cpu.js`, IDE, floppy, VGA, NE2000, virtio, SB16, HDA, USB, PS/2, the graphics adapters) become Rust in
   the wasm module; `src/browser/` (13,000 lines: DOM, canvas, WebGPU, Web Audio, fetch, WebSocket, files) stays
   JavaScript, since only JavaScript reaches those APIs. Expect: task 3 measured JavaScript at 1.2 % of the time,
   so the speed comes from the hot paths (port I/O and MMIO from compiled code, timers, interrupts), not the line
   count; the rest is for one language and fewer boundaries (snapshots, workers).
   - a. The boundary first: one Rust device trait (port and MMIO handlers, a timer callback, save/restore state),
     a table of devices in Rust that the I/O dispatch calls without leaving wasm, and a small import set for what
     must stay in JavaScript (a frame ready, audio samples, a network packet, a disk block request). Every device
     moves behind this one at a time, each build passing the tests before the next.
   - b. Timers and the clock (task 3's item): PIT, RTC/CMOS, HPET, ACPI PM timer, `run_hardware_timers` in Rust;
     `performance.now` once per slice.
   - c. The hot devices: PS/2, the serial UART, DMA, the PCI configuration space, IDE/ATAPI's register file (the
     data comes asynchronously from JavaScript buffers), NE2000 and virtio's rings (packets handed to JavaScript).
   - d. VGA and the graphics adapters' register and memory paths in Rust (task 6c's VGA memory writes), the drawing
     to canvas and WebGPU stays JavaScript and reads the framebuffer from wasm memory.
   - e. Sound (SB16, HDA, the AC97 family) and USB (UHCI/EHCI/xHCI): emulation in Rust, sample output stays in
     JavaScript.
   - f. The large protocol tables (`venus_protocol.js`, `gfxstream_protocol.js`, `svga_constants.js`): generated
     code, move only with the device that uses them; the WebGPU side stays JavaScript.
   - g. `cpu.js` last: machine setup, memory map, snapshots; `starter.js`'s API (`new V86({...})`) unchanged.
   - Check per step: `make all`, the boot tests (`tests/full/run.js`, `tests/x64/run.js`, `tests/uefi.mjs`,
     `tests/a64/boot.mjs`, `tests/virtio_gpu.mjs`), snapshots from the step before restore, and the micro benchmarks
     (`port`, `pit`, `interrupt`, `rdtsc`) against the JavaScript version. Same rule for ARM (`src/arm/`) after x86.

## Fixed 2026-11-18: hang at "Run /init" (the user's buildroot no-GUI image, learncode.nekoweb.org)

A block of only an instruction that crosses into the next page (allowed since the crossing-call change) whose
check of the next page's bytes failed (that page not in the TLB) returned without running anything; the
dispatcher called it again at the same eip, forever: the tab froze (Firefox and Chrome). Now the dispatcher runs
the instruction interpreted when a compiled run made no progress (`cycle_internal`). The user's image boots to its
prompt (5 of 5 on v86.wasm, 2 of 2 on v86-fallback.wasm; `set_jit_config(71, 0)` had made it boot too). No small
test reproduces it (the block, its entry and a TLB miss of only the next page together); the image is the check.

## Profile (2026-11-18, rust c3368cc): where the time goes

Buildroot boot + the kit's five workloads, host clock (`--clock real`; the virtual clock hangs on this sequence),
22.3 s CPU, 11.25 G guest instructions (99.3 % run compiled, 84 % in user space).

| phase | CPU s | compiled code | dispatch (main loop) | interpreter + rest of core | JIT codegen |
|---|---|---|---|---|---|
| boot | 3.16 | 1.16 | 0.36 | 0.97 | 0.27 |
| shell loop | 5.54 | 3.90 | 0.79 | 0.41 | 0.13 |
| md5 | 1.37 | 0.84 | 0.15 | 0.13 | 0.05 |
| sha256 | 2.84 | 2.26 | 0.31 | 0.10 | 0.01 |
| gzip | 6.13 | 5.68 | 0.10 | 0.14 | 0.02 |
| fork | 3.28 | 2.12 | 0.45 | 0.39 | 0.09 |
| all | 22.33 | 15.96 (71.5 %) | 2.15 (9.6 %) | 2.14 (9.6 %) | 0.56 (2.5 %) |

Per 1000 guest instructions (profiler counters): 460 memory accesses with the inline TLB check (307 reads, 136
writes, 17 read-modify-writes; slow paths ~0), 282 of them base register + offset (the stack); flags stored 128
(elided 216); conditions read from stored flags 23 (from locals 89); loop-safety checks 35; module entries 12.2
(an entry runs ~82 instructions), of which exits to another page 8.4; indirect jumps 9.1, 4.6 of them leaving the
module (no entry for the target). Guest side: the kernel's ChaCha (/dev/urandom, the gzip input) is 6 % of all
instructions; the boot's decompressor and initramfs unpacking 4.5 %.

## Round 3 (2026-11-26)

- ✅ #28 sub/add/and/or/xor (32-bit) whose flags only the next jcc reads compute its condition into a local
  (Instruction::Condition, as cmp/test): sub any condition but o/p, logic ops all but p, add e/ne/s/ns; memory and
  register destinations. Test `jit/31_alu_condition` (210 cases; each path broken in turn fails its own range).
  Measured neutral: kit within ±2 %; flags stored 127.7 -> 127.1 per 1000 instructions, jumps reading stored flags
  22.7 -> 22.7: the remaining stores come from other patterns (inc/dec count as flag readers: they keep CF).
- 2026-11-26 #27 exits back to the same page (3.8 per 1000 instructions, ~10 M over the kit): by the instruction
  that ended the run (profiler opstats, `jit_exits` in the report), mostly rep string instructions (rep movsb 3.5 M,
  rep stosb 0.9 M, rep movsd 0.9 M, repne scasb 0.7 M), then segment loads, faulting loads, lfence. A rep string
  instruction ended its block unconditionally; now (`rep_string`, set_jit_config 24) the block goes on when it
  finished in its call (eip still after it) and, for movs/stos, no compiled code was freed (CODE_INVALIDATIONS).
  Test `jit/32_rep_string` (fails with the invalidation check off). Measured: same core, on vs off, 6 rounds:
  sha256 -2.3 %, md5 -2.6 %, boot -1.7 % (all within noise); the kit against the build before shows sha256 +8 %
  in every round, which the on/off comparison puts on the build's code layout, not on the change.
  Note: tests/x64 run on v64 (rebuild v64-debug); a short loop may never reach the JIT threshold: check with
  `make build/v64-prof.wasm` and PROFILER_STATS=1 X64_TEST_WASM=build/v86-prof.wasm node tests/x64/worker.js <bin>
  (29-31 are compiled: 4-6 modules, 0.8 M compiled runs each; 32_rep_string needed N=200000).
- 2026-11-26 #26 page anchors (codegen::gen_access_anchor, set_jit_config 25). The generated code of gzip's
  hottest loop (`--dump-modules dir@mark` in tools/profile, wasm2wat from npm's wabt) is half inline TLB checks
  (968 in one module), gcc reloading `[esp]`, `[esp+4]` with esp unchanged. Now, per basic block, the first access at
  reg+disp (|disp| < 128, flat segment; also [reg]) keeps the TLB entry its check loaded as the register's anchor
  (0 when the register is within 128 bytes of its page's edge); later accesses through the unchanged register test
  only that entry (7 wasm instructions instead of 18, no load), the full check if it fails. Register writes are
  versioned in the wasm builder, the structure (blocks, else, loops, internal functions) by tokens; anchors drop
  at every basic block. In the gzip run 984 M of 2.84 G accesses test an anchor, 619 M create one. Test
  `jit/33_page_anchor` (paging, two pages at unrelated physical pages; fails with the version check or the edge
  window removed). Same core on vs off, 6 rounds, the whole kit sequence: -3.0 % (5 of 6 rounds faster; per
  workload loop -3.2 %, md5 -4.7 %, sha256 -6.7 %, gzip -4.5 %, fork -3.0 %, each within its rounds' noise);
  wasm +6.6 %.
- 2026-11-26 #29 loop safety: the check itself is a compare on a local and a br_if; each iteration also stored eip's
  low bits (load, and, or, store) for that exit only. Moved into the exit (set_jit_config 26): measured on vs off, 6
  rounds, +1.5 % to +3.8 % per workload, all within noise: no gain in V8 (the store is cheap; the if around the exit
  may cost as much). Off by default, the code kept for Firefox.
- 2026-11-26 #32 the profiler's virtual-clock hang on chained workloads no longer reproduces: the whole kit sequence
  (boot, loop, md5, sha256, gzip, fork) runs to its end on the virtual clock, profiled and timed (fork mark at
  31.95 s profiled). Probably fixed with the no-progress fallback of 2026-11-18 (a crossing-only block whose guard
  failed was called forever; the same hang as the user's image at "Run /init"). The virtual clock is usable again
  for A/B runs of the kit's sequence.
- 2026-11-26 #30 the boot's interpreter time (jit_run_interpreted 0.79 s of a 3.6 s profiled boot, 57 M of 636 M
  instructions interpreted): by address (profiler `interp_addrs`, now sorted by steps; `--watch-page`, `--dump-mem` in
  tools/profile), the largest are the kernel's inflate_fast unpacking the initramfs (c126fbec, 7 M steps in 1577
  runs), the decompressor before paging (1ed7xxx, 100 k-step runs) and cold init code. The hot pages are compiled
  early but then wait: while the kernel's init runs, all 4 compile slots (MAX_COMPILING) stay full for up to ~10 M
  instructions at a time, the hot page's heat climbing to 700 k+ meanwhile. Not better: 16 compile slots (boot
  +6.5 %, noise), threshold 100 k instead of 200 k (+5.0 %, noise; 117 modules instead of 92). The engine's compile
  throughput is the limit; nothing cheap left here.
- 2026-11-26 #31 nested calls in Firefox: Firefox 161 headless runs the kit (tools/bench-kit: bench.html with
  `builds=new,new%2321=1`, `auto`, `report=/log`; serve.mjs collects the lines). 4 rounds, the same core:
  nested calls (21=1): boot -0.9 %, shell loop -8.2 % (every round), md5 -9.5 %, sha256 +7.3 %, gzip -2.2 %,
  fork +2.7 % (every round): about -4 % of the whole sequence. Page anchors off (25=0) in Firefox: sha256 +12.4 %
  (every round), the rest within noise: anchors help there too. In V8 nested calls make 8.7 M calls on sha256 +
  fork, 71 % of which the caller goes on after; fork +5.6 % (every round). The depth limit (new set_jit_config 27:
  1, 4, 16) changes nothing measurable: the fork loss is not deep nesting. Left off by default (V8 loses); a page
  could turn it on for Firefox (`set_jit_config(21, 1)` when navigator.userAgent has "Firefox").
- 2026-11-26 #35 a priority lane for hot pages (heat >= 4x the threshold: 2 compile slots beyond the 4): the boot
  interprets 42.5-52.6 M instructions either way (3 runs each, profiler counter; wall time +2.0 %, noise of
  +-20 %). Reverted: the engine's compile throughput is the limit, as for 16 slots.
- 2026-11-26 #33 page anchors for push/pop (set_jit_config 29): esp's anchor follows push, pop and stack adjustments
  with the offset esp moved by. Test `jit/33_page_anchor` (40 pushes across a page, checked at the physical pages;
  fails with the offset not kept; the test needed values that change per iteration and its data off the code page).
  6 rounds on vs off: +3.6 % to +8.1 % per workload (each within noise, all slower). Off by default.
- 2026-11-26 #34 nested calls' fork loss: every nested call the caller could not go on after was one whose callee
  left its page other than by returning (new counter: none needed the dispatcher). A per-site skip after 4 misses
  (set_jit_config 30, on when nested calls are) helps the shell loop but not fork. Firefox, 4 rounds against
  nested calls off: with the skip shell loop -17.3 % (every round), md5 -3.3 %, sha256 +2.7 %, gzip +0.1 %,
  fork +8.4 %; without it shell loop -12.4 %, fork +8.1 %. V8, 5 rounds: both slower everywhere (skip: loop +5.5 %,
  sha256 +5.4 %, fork +4.6 %). The fork loss is not the failed attempts; nested calls stay off by default, and are
  worth turning on for Firefox (set_jit_config(21, 1)) when the guest runs shell-style code.

## Next round of speedups (2026-11-18, branch `Vibe-Claude-rust`)

Numbers from the last profile (buildroot boot and shell loop) and the kit (rust against original: boot -7 %, shell
loop -11 %, md5 -23 %, sha256 -66 %, gzip -11 %, fork +2 %).

1. ✅ 2026-11-18 What the shell loop interprets: in steady state under 1 % of its instructions (profiler export
   `interp_addrs`: the interpreted entry addresses; the top one a crossing instruction that is a jump target, run
   once per iteration, its page not hot enough to recompile). The ~20 % of the profile is the first run's warm-up.
   Not better: heat for every interpreted run on top of its instructions (+10 per run: loop +2.5 %, +40: +9 %).
2. ✅ 2026-11-18 measured, not extended (profiler counters in `interp_opcodes`, entries 1024-1027): buildroot's
   boot interprets 48 M instructions, 89 % from the decoded cache, 8 % on pages not decoded yet, 1.9 % not
   cacheable (prefixes), 0.8 % 16-bit; FreeDOS's 6 M are 86 % 16-bit but under 0.1 s. Prefixed or 16-bit decoding
   would gain at most a few percent of the interpreter's share.
3. ✅ 2026-11-18 JIT code generation: the analysis's and code generator's maps and sets hash with
   `page::FastHasher` (FxHash's step) instead of SipHash with a random seed: generating code 0.21-0.28 s ->
   0.20-0.21 s of a buildroot boot, the core 30 KB smaller. What is left is spread thin (no function above 1 %).
4. ✅ 2026-11-18 The fork workload is not slower: on the virtual clock (4 rounds) 500 forks take 3.45 s against
   3.55 s on `Vibe-Claude` (-3 %, noisy; the kit's +2 % was noise). Its time: compiled code 54 %, interpreter 18 %
   (cold kernel paths of exec), dispatch 13 %; nothing particular to fork stands out.
5. ✅ 2026-11-18 Compiled code of gzip's hot loop (longest_match: byte loads and `cmp m8, r8; jne`): a cmp whose
   flags only the next jcc reads computes the jump's condition into a local at the cmp (`Instruction::Condition`;
   any size, memory operands), instead of storing the lazy flags and reloading them. Kit, 5 rounds against the
   build before: md5 -3.9 %, gzip -2.5 %, sha256 -0.6 %, fork faster in 4 of 5 rounds. Test `jit/29_cmp_condition`.
   The same for `test` (CF and OF clear: every condition from the result's sign and zero; test `jit/30_test_condition`):
   within noise on the kit (md5 -4.1 %, gzip -1.8 %, sha256 +4.7 %).
6. ⬜ The experiments lost in V8 (chaining, nested calls, bigger modules) measured in Firefox: no Firefox in this
   environment; the kit's `bench.html` runs the workloads in any browser with switches for the three (checked in
   headless Chromium: nested calls on, shell loop +6.5 %, fork +27 %, as in Node). The user's Firefox 160 (Windows,
   3 rounds, no experiments), rust against original, faster in every round: boot -10.2 %, shell loop -8.9 %, md5
   -28.3 %, sha256 -58.3 %, gzip -12.4 %, fork -7.4 %. One experiment at a time (rust, medians of 3; two baseline runs:
   shell loop 9.00/9.02, md5 0.59/0.61, sha256 1.54/1.28 (it flips between ~1.3 and ~1.55 s: one mode per boot),
   gzip 2.86/2.88, fork 1.51/1.54): chaining neutral (9.10, 0.61, 1.31, 2.88, 1.54); bigger modules neutral (8.89,
   0.63, 1.28, 2.89, 1.50); nested calls shell loop 8.40 (-7 %, every round) but fork 1.61 (+5 %), the rest the
   same. In Chrome nested calls lost on both (+6.5 %, +27 %). Not switched on: next, find what nested calls cost
   in fork (kernel paths, deep call chains) before a Firefox-only default.
7. ✅ 2026-11-18 Kit refreshed (rust at c18db79), 5 rounds against original: boot -13.6 %, shell loop -8.1 %, md5
   -25.4 %, sha256 -65.0 %, gzip -12.2 %, fork -4.8 %.

## Plan: the Rust port (branch `Vibe-Claude-rust`)

**Views of the module's memory**: the core's heap grows the WebAssembly memory after the devices are made (the
JIT's hash maps), and growing detaches every typed array JavaScript holds on the old buffer. A device in Rust is
reached from JavaScript through views made at each use, or kept through `view()` (src/lib.js), which makes a
fresh one on every access. (PCI's first version kept plain arrays: SeaBIOS saw no host bridge.)

Task 11, device by device. Each device moves to `src/rust/cpu/<name>.rs` with its state in a `#[repr(C)]` struct;
the JavaScript file shrinks to a shim (snapshot format, bus messages, the ports when they are reached through
JavaScript: 16-bit accesses, machines with more cores). The core's port dispatch (`io_port_read8` / `write8` in
`cpu.rs`) asks the Rust devices first; `pit::run_hardware_timers` runs their timers and calls JavaScript's only
when due. Check per device: Alpine boots on both cores, a serial login, `tools/profile` A/B against `Vibe-Claude`
(`--wasm main=...@lib --wasm rust=...`), a snapshot taken mid-boot restores, `tests/x64/run.js`.

| Device | JavaScript | Rust | State |
|---|---|---|---|
| PIT (8254) | `pit.js` 359 -> 119 | `pit.rs` | ✅ (on `Vibe-Claude` too) |
| RTC / CMOS | `rtc.js` 424 -> 160 | `rtc.rs` | ✅ 2026-11-14 |
| Serial ports (16550) | `uart.js` 457 -> 150 | `uart.rs` | ✅ 2026-11-14 (output buffered in the core, taken after each slice) |
| ACPI PM block, GPE, PM timer | `acpi.js` 273 -> 176 | `acpi.rs` | ✅ 2026-11-14 (the PCI function and the hotplug registers stay in the shim); the timer pass of the main loop calls JavaScript only for plugin timers |
| PS/2 controller, keyboard, mouse | `ps2.js` 901 -> 205 | `ps2.rs` | ✅ 2026-11-14 |
| DMA registers | `dma.js` 430 -> 235 | `dma.rs` | ✅ 2026-11-14 (transfers stay with JavaScript's buffers) |
| PCI configuration spaces, 0xCF8/0xCFC | `pci.js` 724 (kept: BARs, ROM, PAM, hooks, IRQ routing, Q35) | `pci.rs` | ✅ 2026-11-15; the guest's device list matches `Vibe-Claude` on i440FX and Q35 |
| IDE task file (status, error, count, LBA, device, features, drive selection), device control, bus master registers | `ide.js` (accessors) | `ide.rs` | ✅ 2026-11-15; data reads and writes in blocks (`io_port_read_block`, `io_port_write_block`) |
| IDE drives: every ATA command, ATAPI packet commands, the data port, bus master DMA, identify data | `ide.js` 3044 -> 920 (images through `ide_read`/`ide_write`, CMOS geometry, insert/eject, snapshots) | `ide.rs` | ✅ 2026-11-17; cdrom-insert-eject, iso9660, state, reset; Alpine x86 from CD; buildroot DMA write/read 3 MB |
| VGA planar memory and host interface (0xA0000-0xBFFFF: read modes, latches, write modes, chain 4, odd/even, plane masks) | `vga.js` (views, accessors, `sync_from_core`) | `vga_legacy.rs` | ✅ 2026-11-15; text cells and graphics ranges marked in the core, drawn after each slice. The register ports (0x3C0-0x3DF, VBE) and drawing stay in `vga.js` |
| VGA drawing (planes to palette indices, indices to RGBA, the 256 colour VBE fill) | `vga.js` (`screen_fill_buffer` calls it) | `vga_legacy.rs` | ✅ 2026-11-15; 0.52 ms a frame against 2.03 ms (TempleOS 640x480x16); TempleOS with 60 frames a second -16.3 % against Vibe-Claude, faster in every round. Every pixel as the JavaScript reference (`tools/profile/vga_reference.mjs`, `--check-draw`) in modes 13h, 04h, 06h, 0Dh, 12h, VBE 0x101 (`tests/devices/vga_modes.asm`) and TempleOS |
| VGA register ports (attribute controller, sequencer, graphics controller, DAC and palette, CRT controller, status) | `vga.js` (accessors; it keeps the writes that change size, layout, mode or font) | `vga_legacy.rs` | ✅ 2026-11-15; redraws and cursor moves are events for `sync_from_core`. Planes, palettes and registers as Vibe-Claude in every mode of `tests/devices/vga_modes.asm`; Alpine's text screen at ten points; TempleOS pixel check; calls into JavaScript per boot 31,569 -> 20,453 |
| NE2000 (first card: registers, buffer memory, remote DMA) | `ne2k.js` (accessors) | `ne2k.rs` | ✅ 2026-11-15; i440FX interrupt routing in `pci.rs`; on Q35 the card runs in JavaScript. 2026-11-17: transmit and receive (filter, ring, header) too; fetch_network passes |
| Floppy controller (registers, all commands, drive geometry and heads) | `floppy.js` 1515 -> 670 (images, snapshots) | `floppy.rs` | ✅ 2026-11-15; 2026-11-17 the DMA transfers too (`floppy_io`); FreeDOS boots, lists and writes the floppy as the original build. 2026-11-18: the READ/WRITE transfer starts in the command's port write again (waiting for the timer pass left the halted guest asleep until the next timer interrupt: 24 M instructions and 12 s of guest time to the prompt, now 10 M as `Vibe-Claude`) |
| Virtio split virtqueues (descriptor chains with indirect tables, available and used rings, interrupt decision) | `virtio.js` (queue state, transport capabilities, devices) | `virtio.rs` | ✅ 2026-11-15; one call per queue operation instead of five per descriptor. The PCI transport's capability registers and the device models (net, blk, rng, console, balloon, 9p, input, snd, gpu) stay JavaScript |
| USB UHCI controller (registers, root ports, the frame schedule of queue heads and TDs) | `usb/uhci.js` (devices, attach, snapshots; Q35 and EHCI's companions run it in JavaScript) | `uhci.rs` | ✅ 2026-11-15; one call per transaction to the JavaScript device (`usb_transaction`). tests/usb.mjs: tablet and stick enumerate, 4 MiB through the stick written and read back |
| USB xHCI controller (registers, ports, command and event rings, slots and contexts, transfer rings, the 1 ms retry timer) | `usb/xhci.js` (devices, attach, PCI function, snapshots in the old array format; Q35 runs it in JavaScript) | `xhci.rs` | ✅ 2026-11-15; the same `usb_transaction` call (port bit, data at a pointer into the module's memory). tests/usb.mjs xhci: tablet and stick, 4 MiB written and read back, snapshot save and restore mid-session |
| USB HID tablet with the USB device layer (address, configuration, endpoint 0's control transfers, standard requests, halts) | `usb/hid_tablet.js` CoreTablet (pointer, snapshots in the old format, transactions from controllers JavaScript runs) | `usb_tablet.rs` | ✅ 2026-11-15; the core's UHCI and xHCI ask it directly. Idle with the tablet open: JavaScript device calls per 10 s 1,250 → 0 (UHCI), 8,762 → 0 (xHCI); reports checked in the guest (/dev/hidraw0), snapshots on both |
| USB mass storage: the Bulk-Only data stage (read queue, the host's data for the image) | `usb/mass_storage.js` (control, CBW, SCSI, CSW, the async block backend, snapshots) | `usb_storage.rs` | ✅ 2026-11-15; the core's UHCI and xHCI move the data packets; JavaScript once per command and per 64 KiB piece. 4 MiB written and read back: JavaScript device calls 131,344 → 400 (UHCI), 8,464 → 400 (xHCI); snapshots on both |
| Graphics adapter bochs_vga on i440FX | `graphics_adapters/vga_core.js` stays for Q35 and inside vmware_svga / virtio-vga | the built-in VGA (`vga_legacy.rs`) | ✅ 2026-11-15; `graphics_adapter: "bochs_vga"` (vram_size kept) is v86's own VGA, the same device: Alpine boot 110,500 → 16,859 calls into JavaScript; bochs-drm binds, 1024x768x32. Snapshots of the plugin's VGA do not restore into it |
| virtio PCI transport (common configuration, notifications, ISR, queue state) | `virtio.js` (accessors; capability-BAR adapters and Q35 keep its own) | `virtio_pci.rs` | ✅ 2026-11-17; buildroot 9p, virtio_9p/console/balloon tests, snapshots |
| virtio device models: rng, blk, net, console, balloon, input, snd | their .js files keep the backends (image, network adapter, bus, speaker), the configuration and the API | `virtio_dev.rs`, `virtio_{rng,blk,net,console,balloon,input,snd}.rs` | ✅ 2026-11-17; fixes on the way: virtio-input's events never reached the driver (a missing `present()`), virtio-snd's timer was never registered. 9p's server and filesystem (`lib/9p.js`, `lib/filesystem.js`) are the host's storage and stay JavaScript, as virtio-gpu (a renderer bridge) |
| SB16 (DSP, mixer, DMA playback, MPU reset, FM ports) | `sb16.js` 1895 -> 196 (bus, unmask listener, snapshots) | `sb16.rs` | ✅ 2026-11-17; `tests/devices/sb16.js` same on old and new |
| HD Audio controller and codec (i440FX) | `hd_audio/controller.js` (PCI function, snapshots; Q35 runs its own) | `hda.rs` | ✅ 2026-11-17; `tests/devices/hd_audio.js` same on old and new; snapshot mid-stream |
| fw_cfg data port (0x511) | `cpu.js` makes the selected item | `fw_cfg.rs` | ✅ 2026-11-17 |
| VMware backdoor pointer (GETVERSION, absolute pointer, packet queue) | `vmware.js` (mouse events, clipboard, GETTIME) | `vmware.rs` | ✅ 2026-11-17; `tests/devices/vmware.js` same on old and new |
| vmware_svga, virtio-gpu (shared with ARM) | the GPU protocols | | ⬜ renderer bridges: stay JavaScript |
| Port table: which accesses a JavaScript device answers (`ports.rs`); port 0x92 | `io.js` keeps it in step | `ports.rs` | ✅ 2026-11-15: accesses nobody answers return the empty port's value in the core |
| Firmware MMIO: the VGA option ROM at 0xFEB00000, the BIOS alias at 0xFFF00000 | `cpu.js` load_bios (hands them over; registers the ranges) | `firmware_rom.rs` | ✅ 2026-11-18; calls into JavaScript per buildroot boot 10,052 -> 967 (port I/O 437, the timer pass 530) |
| Port and MMIO handler tables (`io.js`), machine setup and snapshots (`cpu.js`, `state.js`) | 3,800 | | stays: the tables hold the closures of the devices that stay JavaScript (renderers, 9p, network, images); setup builds those objects. What reaches JavaScript at run time is now firmware-time setup only |
| `src/browser/` (DOM, canvas, audio, network, files) | 13,000 | | stays JavaScript |

**Still JavaScript (2026-11-17)**, and why. Boot crossings into JavaScript on i686 buildroot to a shell: about 10,000,
all at firmware time (SeaBIOS reading the VGA option ROM through MMIO 9,089, PCI configuration writes 195, the rest
VGA and PS/2 setup): about 1 ms a boot, so none of this is on a hot path.
- PCI (`pci.js`): BAR moves, the option ROM, PAM, IRQ routing on Q35 and hooks call into JavaScript device objects
  (io.js port remapping, mmap ranges). Movable once those tables are the core's (next item).
- Port and MMIO handler tables (`io.js`) and machine setup (`cpu.js`, `state.js`): the tables hold JavaScript
  closures of the devices still in JavaScript; setup builds the JavaScript device objects.
- VGA: the register writes that change size, layout, mode or font (they resize the page's screen).
- The disk, CD, floppy and USB images (`buffer.js`): asynchronous (network, files): the devices ask through imports.
- USB: EHCI (refused by Linux on both branches: fix first), the mass storage SCSI commands (once per command, the
  async block backend), UHCI/xHCI/storage on Q35 (JavaScript's interrupt routing).
- NE2000 cards after the first and on Q35; HD Audio on Q35; virtio PCI capability adapters on Q35.
- 9p server and filesystem, virtio-gpu, vmware_svga, the graphics adapters' renderers, TPM (libtpms), ACPI tables
  (generated once), the network adapters, the speaker: the host's side.

**Tests per step** (2026-11-15): `images/` from https://i.copy.sh/ (linux4.iso, linux.iso, buildroot-bzimage68.bin,
buildroot-bzimage.bin, msdos622.img, freedos722.img; TinyCore-11.0.iso for tests/api/floppy.js is missing); `/root/jitwork`
with the Alpine 3.24.3 ISOs and their kernels for tests/usb.mjs and tests/q35.mjs. Results on this branch: tests/devices
virtio_9p (4 allowed failures, as Vibe-Claude), virtio_console, virtio_balloon pass; tests/api 2g-mem, cdrom-insert-eject,
clean-shutdown, iso9660, parallel, reboot, reset, serial, state pass (reboot-buildroot times out on Vibe-Claude too;
test.js runs FreeDOS from floppy forever, the same screens as Vibe-Claude); tests/usb.mjs uhci and xhci read the stick
(ehci is refused on both branches). Alpine's virt kernel finds no sound card for HDA or SB16: they need another guest to test.

Calls from the core into JavaScript (`tools/profile --count-crossings`, Alpine x86 to a shell): 4,149,603 on
`Vibe-Claude`; 4,147,693 with PIT, RTC, UART, ACPI, PS/2 and DMA in Rust; **135,428** with `rep insw/insd` read in
blocks (`io_port_read_block`, the IDE data port: 4.01 M of the calls were reads of port 0x170). x86_64: 144,140.
Boot time: not separable from noise (+0.7 %, rounds -5.0 % to +4.5 %): a call cost about 100 ns, 0.4 s of 41 s.
With PCI and the IDE task file in Rust: **108,975**, of which port I/O 16,800 and MMIO 86,500 (VGA memory writes).
With VGA memory in Rust: **31,569** (port I/O 16,800, the timer pass 5,600, MMIO reads 9,100).
With the VGA register ports in Rust: **20,453** (the most: IDE data writes and commands, ISA PnP 0x279, 0x92, NE2000 on 0xC0xx).

Measured so far (Alpine x86 to a serial login and a command, 3 rounds): RTC and UART in Rust -1.0 % CPU time
(-1.1 % to -0.5 %, faster in every round).

## Plan: Android

Follows v64's `docs/arm64-virt-android16-plan.md` and `docs/android-images-plan.md`. v64 runs Android 16 on the
`virt` board with Cuttlefish's arm64-only phone images, so its path, not `ranchu`, is the one to port first.

- A1. Device groundwork: PCIe and virtio-pci (task 8), virtio-console with several ports, virtio-input (keyboard,
  tablet, multi-touch), virtio-vsock (adbd), more than 2 GiB of RAM (the core has `extended.rs`; the machine
  refuses more than 2 GiB today).
- A2. The GKI kernel (android16-6.12) with Alpine's initramfs on our `virt` board: the kernel's own features
  (39-bit VA, SW_TTBR0_PAN, pseudo-NMIs, KASLR) and the virtio modules load. This is v64's P10.6.
- A3. Images. Port v64's `src/android/` (boot.img, vendor_boot, bootconfig, AVB, GPT, super/lp, sparse, erofs,
  ext4) and `tools/a64_android.mjs`, which fetches the pinned public Cuttlefish build (BP4A.251205.006) from
  ci.android.com. If that download is blocked here, ask the user once for the images or a URL.
- A4. Boot to `sys.boot_completed=1` with adb over vsock, with SwiftShader (software) graphics.
- A5. Display and input: virtio-gpu 2D scanout, touch through virtio-input; then GPU acceleration through Venus on
  WebGPU (task 7c).
- A6. Sound, Wi-Fi over virtio-net, snapshots, a reboot. Check: v64's A9 scenario (launcher, an APK, input,
  reboot).
- Later: the `ranchu` board (goldfish devices, the Android Emulator's images) as a second Android machine.

## Plan: Apple

What exists for which CPU decides where each step runs:

- Mac OS X 10.0-10.3 ran on PowerPC only, 10.4-10.5 on PowerPC and Intel, and 10.6-10.14 Mojave on Intel only. No
  release up to Mojave ever ran on ARM; macOS on ARM starts with 11 Big Sur on Apple silicon, which is not feasible
  here (arm64e pointer authentication, Apple's private CPU extensions, the SEP, the AGX GPU, no public
  documentation).
- So the Darwin and Mac OS X ladder runs on the **x86 target**, our fastest core. The ARM side of Apple is the
  iPhone/iPod (ARMv7, Darwin's ARM kernel), which v64 has planned (`docs/iphone4-plan.md`: iPhone 4, A4/S5L8930,
  iOS 6.1.3, on the AArch32 core; v64 has started it with Mach-O, the Apple device tree, IMG3, IPSW and HFS+
  tooling).
- Legal: v86 ships no Apple files. The user supplies installers or firmware. Apple's licence allows macOS virtual
  machines only on Apple hardware; the docs will say so. The SMC's OSK key cannot be shipped: the user supplies it,
  or the guest uses an open replacement (VirtualSMC).

Shared groundwork (x86), before the ladder:

- G1. (SSE3/SSSE3/SSE4 in every mode: done 2026-11-03.) POPCNT, and CPUID leaves that Darwin reads (cache info, TSC
  frequency or a matching FSB in SMBIOS).
- G2. Port from v64: AHCI/SATA (`src/ahci.js`; later Mac OS X drops PIIX IDE), e1000 (Mac OS X has the 8254x driver),
  SMBIOS (`src/smbios.js`), HPET (check ours against v64's), Q35 extras (ICH9 LPC details, PCIe root ports).
- G3. An AppleSMC device (ports 0x300-0x31F, as QEMU's `applesmc`), keys supplied by the user.
- G4. Firmware: BIOS with Darwin's own boot-132 loader for the early steps; UEFI with OVMF and the open-source
  OpenCore bootloader (it supplies the Apple protocols that boot.efi needs) for 10.6 and later.

The ladder (each step: the installer boots, it installs to an AHCI disk, the installed system boots to its
login or desktop, network works; one test per step under `tests/apple/`; each step only starts when the one before
passes):

| Step | Release | Kernel | Needs |
| --- | --- | --- | --- |
| D1 | Darwin 8 (open source xnu-792, PureDarwin) | 32-bit | boot-132, IDE or AHCI, SSE2/SSE3; text console |
| D2 | Darwin 9-17 (PureDarwin 17 = xnu of 10.13) | 32/64-bit | the same, then UEFI + OpenCore |
| M4 | Mac OS X 10.4 Tiger (Intel) | 32-bit | SSE3, AppleSMC, boot-132 or OpenCore |
| M5 | 10.5 Leopard | 32-bit | the same, a VESA/GOP framebuffer |
| M6 | 10.6 Snow Leopard | 32/64-bit | UEFI + OpenCore, x86_64 |
| M7 | 10.7 Lion | 64-bit | SSSE3 |
| M8 | 10.8 Mountain Lion | 64-bit only | 64-bit EFI, HPET, the TSC calibrated |
| M9-M11 | 10.9 Mavericks, 10.10 Yosemite, 10.11 El Capitan | 64-bit | AHCI, e1000, SMBIOS model matched to the release |
| M12 | 10.12 Sierra | 64-bit | SSE4.1/4.2, POPCNT |
| M13 | 10.13 High Sierra | 64-bit | APFS boot (OpenCore's driver) |
| M14 | 10.14 Mojave | 64-bit | a framebuffer without Metal (software rendering) |

Expect each later release to be much slower than Linux: they are large and the desktop renders in software. A
WebGPU-backed display for Mac OS X would be a separate, later project.

ARM track (after Android): v64's iPhone 4 plan on our AArch32 core, as a new `s5l8930` machine, iOS 4 up through
6.1.3, with firmware and keys from the user.

## Plan: v64 parity

What v64 has and we don't yet (2026-10-31, by comparing the two trees and v64's docs):

- ARM (task 8): PCIe/virtio-pci; virtio-gpu 3D and Venus on ARM (task 7); virtio-console, -input, -vsock, -snd;
  snapshots; RAM above 2 GiB; Android (Cuttlefish images and tooling, `ranchu`); the iPhone 4 machine (v64 has it
  only as a plan and tooling).
- x86 devices: AHCI/SATA, e1000, SMBIOS, PCIe root ports, ICH9 TCO/SMBus; virtio-gpu and virtio-vsock.
- x86 firmware: the UEFI variable store as CFI flash with write-back, and Secure Boot (ours keeps variables in RAM for
  the session).
- x86 memory: RAM above 4 GiB (v64's extended memory).
- Graphics: v64's GLES proxy and gfxstream (`src/gles/`), and the GeForce 6800 plan.
- Storage and app: disk image formats (qcow2, VMDK, VHD/VHDX, VDI, OVA), disk persistence (OPFS, write-back to a
  local file), and the VM manager web app (`src/app/`).
- Performance: x86 is already faster than v64; ARM boot speed against v64 has not been compared on the same host.

## Done

- ✅ 2026-11-18 Multi-core cores rebuilt with the new JS (`v86-parallel.wasm`, `v64-parallel.wasm`, `vcpu-worker.js`);
  `tests/parallel_bench.mjs` passes every workload (its io workload read the PM timer at 0x608, where nothing is
  without firmware: the block stays at 0xB000; it failed on `Vibe-Claude` too). Workers, 2 / 4 cores, against
  `Vibe-Claude`: compute 0.12 / 0.13 s against 0.44 / 0.34 s; lock about the same.
- ✅ 2026-11-18 Marking a page as holding code scanned the x86_64 TLB tables (empty outside long mode) and, without
  paging, every TLB entry: 66 % of a FreeDOS boot. Now skipped / one entry: FreeDOS from floppy to its prompt in a
  quarter of the CPU time of `Vibe-Claude` (-73 %). The floppy transfer starts at the command again (24 M -> 10 M
  instructions to the prompt).
- ✅ 2026-11-14 `tools/profile` (profiler and A/B timing on a virtual guest clock, `docs/profiling.md`); the CPU
  reports no speculation bugs (`IA32_ARCH_CAPABILITIES`): Alpine to login -11 % on x86, -20 % on x86_64; the PIT
  and the main loop's timer pass in Rust (task 3/11b's first step: JavaScript device time halved in the profile,
  0.64 s to 0.31 s of 50 s, the boot time unchanged within noise). jit64 compiles SHA-NI: x86_64 login -3.1 %.
  Then swapgs, rdtscp,
  shld/shrd, tzcnt/lzcnt/popcnt, punpckhqdq, pcmpgtd: interpreted inside compiled blocks 45.4 M -> 4.3 M, login -4.1 %.
  Comparisons print CPU time round by round and machine-independent counters (`docs/profiling.md`).
- ✅ 2026-11-04 32-bit speed back to upstream's: the multicore loop kept the dispatcher of compiled code from being
  inlined (`docs/releases.md`, speed note).
- ✅ 2026-11-03 virtio-gpu 2D on arm64 over virtio-mmio (`tests/a64/virtio_gpu.mjs`, `alpine-arm64.html?gpu=1`).
- ✅ 2026-11-03 virgl 3D on x86: kmscube on 32- and 64-bit Alpine with WebGPU (`tests/virtio_gpu_3d.mjs`,
  `tools/cube_disk.mjs`); the CPU wakes when the GPU answers.
- ✅ 2026-11-03 SSE3/SSSE3/SSE4 in every x86 mode: the 32-bit JIT passes 0F 38 / 0F 3A's prefixes
  (`tests/x64/jit/16_sse4_prefix_x86.asm`).
- ✅ 2026-11-03 The JS split per architecture (libv86.js, libv64.js, libv86-arm.js; `docs/architectures.md`); one
  core without cross-origin isolation; coi-serviceworker for static servers.
- ✅ 2026-11-03 Buildroot extras: neofetch, tput, curl, OpenSSL and CA certificates, sudo, the `student` user,
  TERM=xterm-256color on the serial port.
- ✅ 2026-10-31 virtio-gpu 2D on x86 (`tests/virtio_gpu.mjs`).
- ✅ 2026-10-31 jit64 faster than v64.
- ✅ 2026-10-31 UEFI on ARM (edk2 ArmVirtQemu; `tests/a64/uefi.mjs`).
- ✅ Earlier: UEFI and Q35 on x86, TPM 2.0, USB, HDA, virtio-pci, graphics adapters, multicore.
