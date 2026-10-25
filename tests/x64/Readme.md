# x64 / stub / open-issue tests

Tests for everything v86 currently stubs out or gets wrong, written ahead of the
planned x86_64 (long mode) support. The long-term check is booting TempleOS in v86;
these tests are the stepping stones towards that.

    make x64tests            # run everything in v86 (debug build)
    make x64tests-validate   # check the tests themselves on the host cpu and in qemu

Each test is a flat binary (`nasm -f bin`) loaded at `0x7c00` and entered in real
mode. It reports through a result block in low memory (`inc/harness.inc`) and halts;
`worker.js` runs one test in a fresh node process, so a `dbg_assert` panic in the
emulator shows up as a crash of that test only. Plain `.js` files under `issues/`
are node scripts (pass = exit code 0) for filesystem / API level issues.

## Expectations

Header comments in each test:

    ; EXPECT: pass | xfail    xfail = known to fail today
    ; ISSUE: copy/v86#1234    reported next to the result
    ; DESC: one line
    ; NATIVE: yes             unprivileged 64-bit body, can run as a Linux ELF
    ; QEMU: xfail             qemu deviates from real hardware here

Outcomes: `ok`, `xfail` (expected failure), `FAIL` (regression), `XPASS` (an xfail
started passing: flip its header to `pass`), `error` (does not assemble). The run
exits non-zero for `FAIL`, `XPASS` and `error`.

Options: `TEST_NAME=regex`, `--show-xfail` (print why each xfail fails),
`DISABLE_JIT=1`, `TEST_RELEASE_BUILD=1`, `MAX_PARALLEL_TESTS=n`.

## Layout

| Directory | Mode | Content |
|---|---|---|
| `long/` | 64-bit (after a real -> long mode switch, `inc/long.inc`) | REX/r8-r15, 64-bit arithmetic, shifts, strings, cmpxchg16b, SSE with r64, RIP-relative addressing, #UD opcodes of 64-bit mode, canonical-address faults, MSRs (EFER, STAR, ...), FS/GS base + swapgs, syscall/sysret, compatibility mode, IDT/iretq, CR8/GDT/IDT/TR, 4-level paging (NX, WP), CPUID long mode bits |
| `stubs/` | real mode (`inc/real.inc`) | BOUND, INT1, far call offset limit, INVD/PREFETCHW, SSSE3, SSE3, SSE4.1/4.2, CRC32, MXCSR rounding/FTZ/DAZ/#XM, XSAVE/XRSTOR |
| `issues/` | real / protected mode / node | tests for individual open issues, see `issues/INDEX.md` |

Stubs found in the source (`unimplemented_sse`, `undefined_instruction`, `dbg_assert!(false)`
in `src/rust/cpu/`) and the test that covers them:

| Stub | Test |
|---|---|
| `0F 38 xx`, `0F 3A xx` (SSSE3, SSE4.x) | `stubs/ssse3_*`, `stubs/sse41_sse42_basics`, `stubs/sse42_crc32_dword` |
| `0F 7C/7D`, `0F D0`, `F2 0F F0` (SSE3 horizontal / addsub / lddqu) | `stubs/sse3_horizontal_addsub` |
| `0F AE /2 reg` (rd/wr fs/gs base) | `long/23_fsgs_swapgs` |
| `0F AE /4,/5,/6 mem` (xsave, xrstor, xsaveopt) | `stubs/xsave_xrstor` |
| `0F 08` INVD, `0F 0D` PREFETCHW | `stubs/invd_prefetchw` |
| `0F 05` / `0F 07` SYSCALL / SYSRET | `long/24_syscall_sysret` |
| `62` BOUND | `stubs/bound` |
| `F1` INT1 | `stubs/icebp` |
| `9A`/`FF /3`/`FF /5` with 32-bit offsets > 0xffff in real mode | `stubs/far_call_big_offset` |
| MXCSR rounding mode, DAZ/FTZ, exception masks | `stubs/mxcsr_*` |
| `EFER` bits other than NXE (`wrmsr` panics) | `long/00_enter_long_mode` and every other `long/` test |

Not covered yet (needs ring-3 / task switch / vm86 scaffolding): CR0.AM alignment
checking, CR4.VME, the `#GP` / `#TS` handler `panic!`s in `cpu.rs`, task switches with
`CR4.OSFXSR=0`, RSM, GETSEC, RDPMC.

## Validating the tests

v86 cannot run long mode yet, so the expected values cannot be checked against v86.
Instead `make x64tests-validate` checks them against known-good implementations:

* `--native`: tests marked `NATIVE: yes` contain only unprivileged 64-bit code and are
  built as Linux ELF64 binaries and run on the host cpu.
* `--qemu`: every test runs under `qemu-system-x86_64` (TCG, `-cpu max`) with a tiny
  reset-vector "bios". Failures there are bugs in the test. The few `QEMU: xfail`
  tests cover behaviour where qemu's TCG deviates from real hardware (no canonical
  checks in some wrmsr paths, no #XM, ICEBP, ...); their expectations follow the SDM.

## Writing a test

    %include "inc/long.inc"        ; or inc/real.inc / inc/pm32.inc
    test_main:
        mov rax, 1
        CHECK_EQ 1, rax, 1          ; id, value, expected
        TRAP_BEGIN
            ud2
        TRAP_END 2, 6               ; id, expected exception vector
        PASS

Check ids identify the failing check in the report. Flag-dependent checks must use
`CHECK_CC` before any `CHECK_EQ`, because `CHECK_EQ` clobbers the flags.
