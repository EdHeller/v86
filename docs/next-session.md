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

## Next steps (in order)

1. Make Node 24 the default for tests; re-run the benchmark above on it and record the numbers in
   `docs/jit64-performance.md`.
2. jit64 speed (each change measured A/B on the Alpine loop): cut helper calls (`j64_flags`, `j64_alu`, memory), inline
   flags, more instruction coverage (cmpxchg/xadd, F7 mul/div, bsf/bsr, pushf/popf, cli/sti, SSE moves), lower hotness
   thresholds, a decode cache for the interpreter (it ran 48% of time in an Alpine boot). Tail-call chaining
   (-8%) and register locals (-15%) were tried and are off/reverted.
3. Browser snapshot cache (IndexedDB): measure whether restoring a cached snapshot beats a cold boot in
   `browser_bench.mjs`. The compiled-module cache is not worth it (about 5% of one background core).
4. TempleOS: the OSTestSuite (Tour part 12, 171 demo programs, test 1 = Talons) dies in our emulator, not diagnosed yet.
   Observed in the last run (`TOS_SHOTS`, `TOS_FPS=30`, JIT on): Talons (1/171) starts and the suite moves on; the screenshots
   show 3/171 DunGen ("58s" on the progress bar) at guest time 05:48:01, and the display never changes after about t=330 s
   (fps=0.0 for the remaining 40 minutes, frames identical) while the CPU keeps running at about 20 MIPS (cs=40, eip
   alternating in low addresses such as 0xaa7a / 0xcac4, jit 50%). So the guest is alive but the screen and guest clock
   are frozen: look for a wedged task/loop in the guest (interrupts/timer, a stuck Sleep or Yield, a graphics demo
   stuck in a loop) or an emulator bug (timer/PIT/HPET irq lost, a jit64 miscompile). Next: rerun with `JIT64=0` to
   compare, dump the guest state at the freeze (`TOS_*` hooks in `templeos.js`, `ALP_TRACE`-style trace ring in
   `cpu.rs`) and check whether timer interrupts are still delivered. The user also saw it die at Talons in their browser.
5. Later goals: Mac OS X Tiger, UEFI, a 68000 emulator, a fuller x64 test layer.
6. Do the gcc/g++ disk-size benchmark above.
7. GitHub email notifications for the repo must be turned off by the user (the token returns 403).
