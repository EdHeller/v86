# Handoff notes for the next session

Fork `EdHeller/v86`, branch `Vibe-Claude` (commit and push there; no PRs unless asked). End commit messages with the
two attribution lines the session reminder gives (`Co-Authored-By: Claude ...` and `Claude-Session: ...`).

## State

* x86_64 support (interpreter `long_mode.rs`, `long_mem.rs`, JIT `jit64.rs`) works: TempleOS (live CD and installed to
  disk), Alpine x86_64 (boots to root login) and Alpine i686 (32-bit JIT).
* Details and measurements: `docs/jit64-performance.md`. Runners and README: `tests/x64/manual/` (`templeos.js`,
  `alpine.js`, `browser_bench.mjs`, `templeos/README.md`, `alpine/README.md`). Tests: `tests/x64/` (long/*.asm, fuzz.js).
* Build: `make build/v86.wasm build/libv86.js` (the generated tables come from `gen/x86_table.js`).

## Environment

* The setup script now installs Node 24 (new sessions); this session ran Node 22.22 and a Node 24.22.1 copy.
* Chromium 153 + Playwright are preinstalled (`require('playwright')`). No need to build Chrome.
* Use the Monitor tool (`until <check>; do sleep 2; done`) to wait; plain `sleep` is blocked.
* `pkill -f` can kill your own shell: use `pgrep -f "^node tests/..." | xargs -r kill`.
* Snapshots/images live in the old session's scratchpad and are gone. Recreate with
  `ALP_SAVE=<file>:<seconds> node tests/x64/manual/alpine.js <alpine-virt-3.24.3-x86_64.iso> <timeout>`
  (the Alpine ISO comes from dl-cdn.alpinelinux.org; TempleOS ISO per `templeos/README.md`).
* Benchmark rules: run A/B alternately and sequentially, never in parallel (parallel runs inverted rankings).

## Node 22 vs 24 vs Chromium (Alpine x86_64, busybox `ash` loop `i=$((i+1))`, 40000 iterations, jit64 on)

Guest `time` real, seconds (lower is better), loop run from a restored pre-login snapshot:

| Engine | run 1 | run 2 | mean |
|---|---|---|---|
| Node 22.22 (V8 12.4) | 28.2 | 31.9 | 30.1 |
| Node 24.22 (V8 13.6) | 22.5 | 26.8 | 24.6 |
| Chromium 153 (headless, `browser_bench.mjs`) | 22.9 | - | 22.9 |

Node 24 is about 1.22x faster than Node 22 on this loop; Chromium is in the Node 24 range (one run, other harness).
The spread between runs is 15%, so treat differences under about 10% as noise. Two earlier Node 24 runs with a different
key schedule gave 24.3 and 19.5 s.

Earlier estimate (other workloads): 1.5-1.9x; this loop shows less.
memory64 is only worth it for guests needing more than 4 GB; it was 2.5x slower on Node 24 for compute-bound work.

## Benchmark: how much disk gcc takes on an Alpine-like guest (to do)

Goal: a number for "what does a compiler cost on the guest disk" and ways to shrink it. The idea is that much of the
space may go to `g++` (cc1plus, libstdc++ headers, plugins), not to `gcc` for C.

Measure in the Alpine guest (needs a network or a local repo, or install the apks into an image on the host with
`apk --root`):

* installed sizes: `apk add build-base` then `apk info -s gcc g++ binutils musl-dev libstdc++ libstdc++-dev make`,
  and `du -sh /usr/libexec/gcc /usr/lib/gcc /usr/include/c++ /usr/bin/*g++* /usr/bin/ld`; the big single files are
  `cc1`, `cc1plus` (and `lto1`), each tens of MB.
* a C-only compiler: `apk add gcc musl-dev binutils` (no `g++`, `libstdc++-dev`) against the full `build-base`;
  record both numbers, plus the compressed size (`mksquashfs` with xz, or the image as gzip).
* shrink ideas to test: C only (no g++/lto1/plugins), `strip` the binaries, remove docs/man/info/locale and
  `/usr/lib/gcc/*/*/plugin`, `tcc` or `chibicc` as a tiny compiler, squashfs+xz or an overlay mounted lazily so the
  base image stays small and the toolchain is a separate, optional disk image or 9p share (the v86 filesystem
  loads files on demand).
* the Buildroot post (luplab.cs.ucdavis.edu, "Buildroot and compiler on target", 2022; read in the session):
  the **14 MiB is `rootfs.cpio.zst`, a zstd-compressed cpio of the whole root filesystem**, not an installed size,
  for a minimal Buildroot config on glibc. Their patch builds gcc for the target with `--enable-languages=c` (no C++),
  `--disable-lto --disable-plugin --disable-nls --disable-libsanitizer --disable-libgomp --disable-libquadmath
  --disable-gcov --disable-libmpx`, no bootstrap, and removes duplicate gcc links and ldscripts for other targets. So
  it is C only, with LTO, plugins and NLS off, and compressed with zstd: this fits the suspicion that `g++`
  (cc1plus, libstdc++) and the extras are what make ours big. It is not about cross compiling. To compare: build the
  same variants on Alpine (C only, no lto1/plugin/locales, stripped), then report the **installed size and the
  zstd/xz-compressed size** side by side, because the two differ several-fold.

## Test targets

`docs/os-targets.md` lists every OS on copy.sh/v86: the JIT speed work (32-bit and 64-bit) must make all of them faster
and must not break any. Use the representative set there for regression checks.

## Next steps (in order)

1. Make Node 24 the default for tests; re-run the benchmark above on it and record the numbers in
   `docs/jit64-performance.md`.
2. jit64 speed (each change measured A/B on the Alpine loop): cut helper calls (`j64_flags`, `j64_alu`, memory), inline
   flags, more instruction coverage (cmpxchg/xadd, F7 mul/div, bsf/bsr, pushf/popf, cli/sti, SSE moves), lower hotness
   thresholds, a decode cache for the interpreter (it ran 48% of time in an Alpine boot). Tail-call chaining
   (-8%) and register locals (-15%) were tried and are off/reverted.
3. Browser snapshot cache (IndexedDB): measure whether restoring a cached snapshot beats a cold boot in
   `browser_bench.mjs`. The compiled-module cache is not worth it (about 5% of one background core).
4. TempleOS OSTestSuite / DunGen freeze (jit64): see "DunGen freeze findings" below. Next: differential debugging.
5. Later goals: Mac OS X Tiger, UEFI, a 68000 emulator, a fuller x64 test layer.
6. gcc/g++ disk-size benchmark: first numbers are in `docs/gcc-disk-size.md` (Alpine 3.24 x86_64, gcc 15.2.0). The
   Buildroot 14 MiB figure is for a **RISC-V** system, so we cannot expect to reach it on x86; compare variants of our own.
7. GitHub email notifications for the repo must be turned off by the user (the token returns 403).

## DunGen freeze findings (session of 2026-10-27)

Goal of the project: x86_64 JIT fast and correct (Alpine x86_64, TempleOS Tour part 12 = OSTestSuite, 171 programs), and
a faster 32-bit JIT for every guest on https://copy.sh/v86/ (list: `docs/os-targets.md`).

Tooling added (all under `tests/x64/manual/`, committed):

* `TOS_SUITE=1 TOS_ISO=TempleOS.ISO python3 templeos/patch_iso.py out.ISO`: boots straight into the full OSTestSuite and
  prints `TS <n>` on serial per test (0 BlackDiamond, 1 Talons, 2 FlatTops, 3 DunGen ...), then `SUITE DONE`.
  `TOS_SUITE=dungen` runs only `::/Demo/Games/DunGen` with a `J <jiffies>` serial heartbeat every second (fast repro,
  freezes about 100-180 s after boot with the JIT on).
* `templeos.js`: `TOS_FPS=20` (frames/s, MIPS, jit share), `TOS_SHOTS='[sec,..]'` screenshots, `TOS_SAMPLE=20`
  (+`TOS_SAMPLE_REGS=1`) eip histogram every interval, `JIT64_MASK=<n>`, `JIT64=<threshold>`.
  **`JIT64=0` is threshold zero (compile everything), not off. JIT off is `JIT64=4294967295`.**
* `JIT64_MASK` bit k disables compiling instruction kind k (`kind()` in `src/rust/cpu/jit64.rs`: 0 Alu, 1 Test, 2 Mov,
  3 MovExt, 4 Lea, 5 Push, 6 Pop, 7 IncDec, 8 Shift, 9 Setcc, 10 Cmov, 11 Jcc, 12 Jmp, 13 Call, 14 Ret, 15 Leave, 16 Nop,
  17 Bt, 18 Str, 19 Neg/Not, 20 Imul, 21 CallInd/JmpInd, 22 Xchg, 23 Sext). Bit 30 keeps all flags live (no flag elision).
* `tests/x64/js/jit64_mask.js` (in `make x64tests`) checks every mask keeps results correct, all-ones compiles nothing,
  reserved bits do nothing and every kind bit changes something; `tests/x64/jit/09_masked_kinds.asm` covers the kinds the
  other tests miss. Learned writing it: a compiled block ends at the first unsupported opcode (`cld` 0xFC, `loop` 0xE2
  ...), and data in the same page as code invalidates blocks on every write (put data on a separate page).
* `x64tests` needs `make build/v86-debug.wasm` first; `libv86.js` needs `closure-compiler/compiler.jar` (re-download if
  maven returns 429) and `rustup target add wasm32-unknown-unknown`.

Results (TempleOS 5.03 live CD, patched ISO, Node 24):

* The small selftest passes with the JIT on (`1+2=3`, `fact10`, `sqrt2`, `sin1`).
* Full suite with JIT on: reaches TS 3 (DunGen) at about 290 s, then the screen stops (fps 0.0 forever). The guest is not
  dead: the jiffies heartbeat keeps advancing, CPU runs at about 20-25 MIPS, jit share 50%, and the eip histogram shows a
  loop over only 5 addresses in low kernel memory (`aa7a cac4 cb63 1c56f 1eaf3`; esp varies, so it is calls/returns).
  So DunGen's task (or the task it waits for) spins and never progresses.
* DunGen alone with the JIT off (`JIT64=4294967295`): draws steadily at 10-12 fps for 320 s+ (no freeze).
* JIT on, single kind masked (fps at the end of a 170 s run; 0.0 = frozen):
  kind 0 Alu 9.7, 1 Test 4.8, 2 Mov 4.7, 3 MovExt 8.6, 7 IncDec 13, 11 Jcc 9.8, **12 Jmp 24.5**, 17 Bt 4.3: **unfrozen**;
  4,5,6,8,9,10,13,14,15,16,18-23: frozen. All kinds masked: 10 fps. Flag elision off (bit 30): still frozen.
  So no single instruction kind is solely to blame and flag elision is ruled out; the eight kinds that help are the
  ones that make up most blocks, so the bug depends on block structure/shape or on a specific pattern, not one opcode.
  Masking only Jmp gives 24 fps, 2.4x the JIT-off speed: also a lead for speed (jmp-heavy blocks may be mis-handled or
  slow, look at how `Insn::Jmp` and block chaining are generated).

Suggested next steps for the freeze:

1. Differential run: save a state shortly before DunGen (`TOS_SAVE`/`TOS_SAVE_AT`, or `TOS_LOAD` to resume), then run the
   same instruction count with the JIT off and on and compare registers, memory pages, and jiffies at block exits; use the
   trace ring (`get_jit_config(11/12)`, `TRACE_BUF` in `cpu.rs`) to find the first divergence. The 5-address loop is the
   place to disassemble (TempleOS kernel `::/Kernel`, addresses are low): decode what it waits on.
2. Block shape ideas to test: cap block length, disable chaining/loop-in-block, check that back-edges inside a compiled
   block still deliver pending interrupts and update the instruction counter, check `invalidations`/stale-block reuse
   (invalidated reaches 4800+ in this workload, TempleOS mixes code and data in pages), async vs `sync_jit`.
3. After DunGen passes, rerun the whole suite (`TOS_SUITE=1`, about 2 h in emulation) and expect later tests to find more.
4. Then 32-bit JIT speed (`jit.rs`) with the A/B method on several guests from `docs/os-targets.md`.

Working notes for the next Claude:

* Keep the number of background monitors low: one Monitor per job, never re-arm loops, stop old ones with TaskStop.
  The user asked for terse messages and few tokens; do not poll with no-op commands.
* `pkill -f <pattern>` can kill your own shell; use `pgrep -f "^node tests/..." | xargs -r kill`.
* Run A/B performance comparisons sequentially, but correctness bisects (like the mask scan) can run 3 at a time.
  `/tmp/claude-0/bisect.sh`-style helper: run `templeos.js` on the dungen ISO with `JIT64_MASK=<n> TOS_FPS=20` for 170 s
  and read the last `fps=` lines.
* TempleOS image: `curl -o TempleOS.ISO https://templeos.org/Downloads/TempleOS.ISO`; Alpine ISO from dl-cdn.alpinelinux.org.
