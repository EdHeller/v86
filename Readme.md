# v86 fork: x86_64 support and a 64-bit JIT

A fork of [copy/v86](https://github.com/copy/v86) (branch `Vibe-Claude`) that adds 64-bit (x86_64) guests: long mode in the
interpreter, a 64-bit JIT (`src/rust/cpu/jit64.rs`), 64-bit SSE/x87, XSAVE, and test layers for all of it.
The original upstream readme is kept in [docs/upstream-Readme.md](docs/upstream-Readme.md).

## What runs

| Guest | State |
|---|---|
| TempleOS x86_64 (live CD, installed disk) | boots; the whole OSTestSuite ("Tour" part 12) passes in a real browser (Firefox) |
| Alpine Linux 3.24 x86_64 (virt ISO) | boots to the login prompt; about 190 s on a fast machine (32-bit takes about 27 s) |
| Alpine Linux 3.24 x86 (32-bit) | boots to login in about 20-27 s |
| Buildroot images with a native gcc/g++ (i686) | `tools/buildroot/build.sh` (see below) |

## Build, test, run

    make build/v86.wasm build/libv86.js        # release build (needs closure-compiler/compiler.jar and the wasm32 rust target)
    make build/v86-debug.wasm
    make x64tests                              # the 64-bit, stub and open-issue tests, plus a short differential fuzz run
    node tests/x64/manual/alpine.js alpine-virt-3.24.3-x86_64.iso 600     # see tests/x64/manual/alpine/README.md
    examples/templeos.html, examples/alpine64.html                          # browser pages (boot timer, serial console, GUI on/off)

Benchmarks: use `WASM=build/v86.wasm` (the runners default to the slower debug build), one run at a time, alternate A/B.

## Notes: issues encountered so far

Each entry: what went wrong, the cause, the fix, and the test that now guards it.

### Correctness

1. **TempleOS OSTestSuite froze in DunGen (test 3), JIT on only.** Not a miscompile: a verify mode that re-ran 20 M compiled blocks in the
   interpreter found no difference. The emulator polls timers/interrupts every 100,003 instructions at block boundaries; TempleOS's
   scheduler loop runs with interrupts off, and with the JIT the poll point locked onto the disabled stretch, so `jiffies` never advanced.
   Fix: a pending interrupt is taken after the instruction that enables interrupts (`sti`, `popf`, `iretq`). Test: `tests/x64/long/32_irq_after_sti.asm`
   (fails without the fix).
2. **Alpine 3.24 x86 died with a segmentation fault in early boot (32-bit JIT).** `movups`/`movupd`/`movdqu` stores were compiled by calling the
   *aligned* store (`movaps`), which gained a #GP-on-misaligned check with the SSE alignment work, so musl's `memcpy` faulted. Fix: separate unaligned store.
   Found by bisecting against upstream; the bisect pointed at the XSAVE/alignment commits and the experiment "JIT off boots" located it.
   Test: `tests/x64/issues/sse_unaligned_ok_jit.asm` (fails without the fix).
3. **`LinkError: function import requires a callable` when a guest ran `lddqu`/`addsubps`/`haddps` hot.** The JIT calls instruction handlers by
   name; 14 handlers had no `#[no_mangle]`, so they were not exported from the main wasm. Test: `tests/x64/js/jit_exports.js` (every name the jit
   mentions must be exported).
4. **Alignment gaps.** Memory forms of SSSE3/SSE4.1 (0F38/0F3A) and `unpcklps/pd` did not fault on a misaligned 16-byte operand; `insertps` read 128 bits
   for a 32-bit operand. Generated test: `tests/x64/gen_sse_alignment.py` -> `issues/sse_alignment_matrix.asm` and `long/34_...` (about 180 instruction forms,
   interpreted and compiled). Not implemented (#UD): `pcmpistri/pcmpistrm/pcmpestri/pcmpestrm`.
5. **XSAVE family.** `xrstor` was still an unimplemented stub (#UD) in 32-bit compiled code; `xcr0` was not saved in states. Tests: `stubs/xsave_*`,
   `issues/xsave_xrstor_jit.asm`, `js/state_roundtrip.js`.
6. **PAM read-only regions (copy/v86#1635).** Writes to a shadowed ROM range (i440FX PAM = 01) are now dropped; pages carry `TLB_HAS_CODE` so every write
   takes the slow path. Test: `issues/pam_write_protect_1635.asm`.
7. **Interrupt shadow (copy/v86#1631).** No interrupt may be taken after `mov ss`/`pop ss`/`sti` until the next instruction has finished, also not one raised
   from inside it (idea from jlxip/slop86). `IN_SHADOW_STEP` in `cpu.rs`.
8. **`insertps`/fuzz scratch area** needed 16-byte alignment in the guest image (`section .data align=16`); fuzz runs are part of `make x64tests`.

### Speed (64-bit JIT)

9. **Hotness table too small:** 4096 direct-mapped slots; kernel jump targets evicted each other's counters, 15 % of instructions stayed interpreted. 65536 slots: 1.5x on the Alpine shell loop.
10. **Flags** were computed by a helper call at every block exit: inline wasm instead, 12-16 %.
11. **Unsupported instructions ended a block** and the rest ran interpreted up to the next jump (35 M of 39 M interpreted instructions): compiled as an in-block call to the
    interpreter (`Insn::Interp`: pushf/popf/cli/sti/cmpxchg/xadd/bsf/bsr/rdtsc/x87/SSE moves...), 10-25 %.
12. Tried without gain: lower hotness threshold, block chaining with tail calls (-8 %), registers in wasm locals (-15 %), re-checking for a block start after every block exit,
    back-to-back dispatch (`CHAIN_BUDGET`, within noise).
13. **Open:** a cold 64-bit Alpine boot is about 7x slower than 32-bit (about 190 s vs 27 s on a fast machine): 35 % of boot instructions are still interpreted (cold code),
    about 99 % of compiled blocks are invalidated again (pages reused for different code). Next: interpreter decode cache, cheaper/batched compilation.

### Test infrastructure pitfalls

14. **The 32-bit JIT only compiles a page after about 200,000 instructions** (`JIT_THRESHOLD`). Loops of a few thousand iterations are never compiled, so a "JIT" test can pass on
    broken code. Verify a new test fails without the fix (`git stash` the fix, rebuild, run).
15. `alpine.js` and friends use the debug wasm unless `WASM=build/v86.wasm` is set (about 2x slower, and the numbers are not comparable).
16. Typing at the ISOLINUX `boot:` prompt drops keys. Boot the kernel and initramfs from the ISO directly (`bzimage` + `initrd` + `cmdline`) and use the serial console.
17. After the graphics driver loads, the text screen is a framebuffer: detect login from the serial console, not from the text buffer.
18. The Node runner can sit forever on a TempleOS test that a real browser passes (test 17 "Strut"); confirm in a browser before debugging the emulator.
19. A tiny block cache bug looks like a "freeze" long before it is a wrong result: bisect with `LO`/`HI` block-address ranges (`tests/x64/manual/templeos/tools/bisect_state.mjs`) and a
    verify mode (`VERIFY` knob, compares registers/flags/memory per block).
20. Profiling aids: `ALP_HIST=1` (interpreted opcodes, why code was interpreted, hot interpreted pages, invalidated pages), `TOS_FPS`, `TOS_SAMPLE`.

### Environment and tooling

21. GitHub is only reachable for the repo in scope (403 for others): Go sources come from `proxy.golang.org`; Buildroot hash files need the `sha256  <hash>  <file>` format.
22. Go's module cache is read-only: a fresh Buildroot build could not delete its old build folder (`rmrf` helper in `tools/buildroot/build.sh`).
23. Background jobs are stopped after 2 hours and by container restarts (and the download proxy port changes): relaunch long runs, keep state in files.
    `pkill -f <pattern>` kills the shell that contains the pattern: kill by pid.
24. `-sh: can't access tty; job control turned off` after login on the serial console is cosmetic (busybox init starts that getty without a controlling terminal; stock Alpine does the same).
25. `btc-*` nasm tests time out in the debug build on this machine (also on the commit this work started from); `pit_irq0_rate_1614` is timing sensitive.

## Size of a C compiler on the guest (see docs/gcc-disk-size.md)

C-only gcc 15 (x86_64, `-Os`, stripped) with libc, binutils and the sanitizer runtime: 14.7 MB with zstd, about 64 MB installed; with C++: 21 MB / 118 MB.
`tools/buildroot/build.sh --bash-env` / `--dev-env` build small Buildroot images with a native gcc and a shell development kit (`--bash-dev`, `--bashdb`, `--gdb`).

## More

- [docs/next-session.md](docs/next-session.md): handoff notes and what to do next
- [docs/jit64-performance.md](docs/jit64-performance.md): measurements and ideas for the 64-bit JIT
- [docs/os-targets.md](docs/os-targets.md): guests the 32-bit JIT must not break
- [tests/x64/Readme.md](tests/x64/Readme.md): the x64, stub and open-issue test layers
