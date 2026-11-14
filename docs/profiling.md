# Profiling and timing

## tools/profile: where a boot's time goes, and how long it takes

`tools/profile/profile.mjs` boots a guest in Node (24 or later) on `build/v86.wasm` (32-bit) or `build/v64.wasm`
(x86_64) and reports:

    node tools/profile/profile.mjs --cdrom alpine-virt-3.24.3-x86.iso
    node tools/profile/profile.mjs --arch x86_64 --cdrom alpine-virt-3.24.3-x86_64.iso
    node tools/profile/profile.mjs --help

- **Seconds by area** (V8's sampling profiler): compiled guest code, core functions called from it, the main loop
  and dispatch, the interpreter, the JIT generating code, the engine compiling modules, devices in JavaScript,
  garbage collection, idle; per phase when there are several marks (`--mark name=regex`, found in the serial output).
- **Self and inclusive time by function** of the core (Rust names demangled) and of JavaScript.
- **Boot timeline**: the console lines (serial and text screen) that were followed by the most time: what the
  guest was doing while the user waited.
- **Guest instructions by function** with `--guest-map System.map` (boot the kernel with `nokaslr`, e.g.
  `--bzimage vmlinuz --initrd initramfs --cmdline "... nokaslr"`): the guest's instruction pointer is sampled about
  every 100,000 instructions, before interrupts are delivered.
- `--step-profile` (x86_64): the instructions jit64 does not compile, by opcode: those the interpreter ran from
  inside a compiled block and those a block ended at.
- The `.cpuprofile` in `build/profile/` opens in Chrome DevTools (Performance > Load profile) and speedscope.app;
  `node tools/profile/analyze.mjs file.cpuprofile` prints the tables for any profile of v86, also one saved from a
  browser. The profiled run is about 20 % slower than a plain one.

**Timing and comparing builds**: `--runs N` adds N boots without the profiler; with several `--wasm name=file[@lib]`
the builds take turns and the summary gives median, fastest and spread per mark:

    node tools/profile/profile.mjs --cdrom x.iso --no-profile --runs 5 \
        --wasm before=/tmp/old/v86.wasm@/tmp/old/libv86.mjs --wasm after=build/v86.wasm --lib build/libv86.mjs

Use `--lib build/libv86.mjs` (and `--plugin build/libv64.js` for x86_64) to measure the release JavaScript; the
default, `src/main.js`, needs no build but runs with DEBUG on.

**The virtual clock** (the default). The guest's time advances with the instructions it executes (`--mips`, 50
million per guest second) and jumps ahead while it is halted; random numbers are seeded. Every run of a build then
does the same work: the instruction counts of two boots agree to 0.1 %. On the host's clock (`--clock real`, what a
browser does, and what `tools/v86-bench/run.mjs` measures) timer interrupts land elsewhere on every run and waiting
is part of the time: three boots of 32-bit Alpine were 12 % apart, against 3 % on the virtual clock. What remains
is the host: on a shared machine the same build drifts by 10 % within an hour, so compare builds only within one
invocation (they take turns), and take a difference smaller than the printed spread for noise.

Findings so far (Alpine 3.24 to login, 5.3 billion guest instructions on either core):

- 58 % of the time is compiled guest code, 17 % the interpreter, 10 to 13 % the main loop and dispatch, 2 % the
  JIT. JavaScript devices are 1.3 % (0.6 s of 50 s).
- The guest took the emulated CPU for hardware with speculation bugs and ran page table isolation, retpolines,
  RSB filling and VERW. `IA32_ARCH_CAPABILITIES` now says it has none: login 11 % sooner on x86 (46.4 s to 41.1 s),
  20 % sooner on x86_64 (57.4 s to 45.6 s).
- The PIT in Rust (`src/rust/cpu/pit.rs`; the main loop's timer pass no longer calls JavaScript on every pass) and
  the hex strings the IDE data port built in release builds: JavaScript device time 0.64 s to 0.31 s in the
  profile; no difference in the boot time beyond noise (x86 +2.4 % with 7 % spread, x86_64 -3.9 % with 11 %). Moving
  the remaining JavaScript devices to Rust can gain at most the 0.6 % that is left of them.

- After the PIT, the emulator's own JavaScript on this boot is about 0.1 s of 50 s (the loop's `do_tick`, VGA text
  redraws, IDE data reads): there is nothing left for a Rust port of the devices to win here. The time is in
  compiled code, dispatch and the interpreter, so that is where the work goes.
- The 32-bit JIT's threshold (`set_jit_config(59, n)`, 200,000): 100k +5.9 %, 50k +7.7 %, 20k +9.7 %, 400k +5.4 %,
  800k +13.3 %. It stays.
- x86_64, `--step-profile`: 92 M instructions ran in the interpreter from inside compiled blocks (7 % of the
  time), 35 M of them SHA-NI (modloop and module checks). jit64 compiles them now (`SseLane` 20 to 26): login
  -3.1 % (47.3 s to 45.8 s, spread 3 to 4 %). Next in that list: `0F 01 /7` with a register form (swapgs, 4 M),
  punpckhqdq and pcmpgtd (66 0F 6D / 66, 3.5 M), shrd, tzcnt, stac/clac.

## The counters of the profiler build

v86 has a built-in profiler, which instruments generated code to count certain
events and types of instructions. It can be used by building with `make
debug-with-profiler` and opening debug.html; `tools/profile/profile.mjs --wasm` on a `make with-profiler` build
prints the counters by name.

For debugging networking, packet logging is available in the UI in both debug
and release builds. The resulting `traffic.hex` file can be loaded in Wireshark
using file -> import from hex -> tick direction indication, timestamp %s.%f.
