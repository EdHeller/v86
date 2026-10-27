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

RESULTS_PLACEHOLDER

Earlier estimate: the same wasm32 build is 1.5-1.9x faster on Node 24 (V8 13.6) than on Node 22 (V8 12.4).
memory64 is only worth it for guests needing more than 4 GB; it was 2.5x slower on Node 24 for compute-bound work.

## Next steps (in order)

1. Make Node 24 the default for tests; re-run the benchmark above on it and record the numbers in
   `docs/jit64-performance.md`.
2. jit64 speed (each change measured A/B on the Alpine loop): cut helper calls (`j64_flags`, `j64_alu`, memory), inline
   flags, more instruction coverage (cmpxchg/xadd, F7 mul/div, bsf/bsr, pushf/popf, cli/sti, SSE moves), lower hotness
   thresholds, a decode cache for the interpreter (it ran 48% of time in an Alpine boot). Tail-call chaining
   (-8%) and register locals (-15%) were tried and are off/reverted.
3. Browser snapshot cache (IndexedDB): measure whether restoring a cached snapshot beats a cold boot in
   `browser_bench.mjs`. The compiled-module cache is not worth it (about 5% of one background core).
4. TempleOS: find why the OSTestSuite (Tour part 12, `::/Demo/...`, 171 programs, test 1 = Talons) eventually dies;
   compare with `JIT64=0`, and with the user's browser build. Check the last run's log if it still exists.
5. Later goals: Mac OS X Tiger, UEFI, a 68000 emulator, a fuller x64 test layer.
6. GitHub email notifications for the repo must be turned off by the user (the token returns 403).
