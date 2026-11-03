# x86_64 instruction tests

The instruction tests of the x86_64 CPU (`v64.wasm`, `cpu_type: "x86_64"`, with `libv64.js`): long mode, the
64-bit interpreter and the 64-bit JIT (jit64), and the instructions that were missing or stubbed in v86 (SSE3,
SSSE3, SSE4.1/4.2, XSAVE, ...), in 64-bit and in the older modes. They also cover open issues of upstream v86
(`issues/`) and several cores (`smp/`). Linux x86_64 and TempleOS boot on this CPU (`manual/`).

    make x64tests            # everything in v86 (v64-debug.wasm), and the fuzzer
    make x64tests-validate   # the tests themselves, on the host CPU and in QEMU

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

## Differential fuzzing

    node tests/x64/fuzz.js [--seed n] [--count n] [--family regex]

`fuzz.js` generates random instances of integer, SSE and SSSE3/SSE4.1 instructions (random
register contents and flags), runs them on the host cpu and in v86 long mode, and compares all
registers, flags, a scratch memory area and xmm0-15 (undefined flags are masked). It needs
nasm, ld and an x86-64 Linux host. `rcpps`/`rsqrtps` (implementation specific approximations)
and x87 are not covered. `JIT64=0` runs the cases compiled.

## Booting real systems

Not part of the automated run, because they need images that are not in the repository:

* `manual/alpine.js`: Alpine x86_64 (the official virt ISO) to its login.
* `manual/linux64.js`, `manual/linux64-direct.js`: an x86_64 Linux kernel, as a bzImage or as a `vmlinux` entered
  straight in 64-bit mode (`manual/boot64.asm` is that real-mode to long-mode trampoline).
* `manual/templeos.js <TempleOS.ISO>`: TempleOS to its desktop.

## Layout

| Directory | Mode | Content |
|---|---|---|
| `long/` | 64-bit (after a real -> long mode switch, `inc/long.inc`) | REX/r8-r15, 64-bit arithmetic, shifts, strings, cmpxchg16b, SSE with r64, RIP-relative addressing, #UD opcodes of 64-bit mode, canonical-address faults, MSRs (EFER, STAR, ...), FS/GS base + swapgs, syscall/sysret, compatibility mode, IDT/iretq, CR8/GDT/IDT/TR, 4-level paging (NX, WP), CPUID long mode bits |
| `stubs/` | real mode (`inc/real.inc`) | BOUND, INT1, far call offset limit, INVD/PREFETCHW, SSSE3, SSE3, SSE4.1/4.2, CRC32, MXCSR rounding/FTZ/DAZ/#XM, XSAVE/XRSTOR |
| `jit/` | 64-bit | jit64: loops, flags, faults, self-modifying code, page boundaries (below) |
| `smp/` | protected / 64-bit, several cores | IPIs, cores taking turns, a core in a Web Worker, LOCKed instructions and code changes across cores |
| `issues/` | real / protected mode / node | tests for individual open issues, see `issues/INDEX.md` |

The instructions that were stubs in upstream v86 (`unimplemented_sse`, `undefined_instruction`,
`dbg_assert!(false)` in `src/rust/cpu/`), now implemented, and the test that covers each:

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

`make x64tests-validate` checks the expected values themselves against known-good implementations, so that a
wrong expectation is not mistaken for a v86 bug:

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

## The 64-bit JIT (`src/rust/cpu/jit64.rs`)

Hot 64-bit code is compiled to wasm: basic blocks of supported instructions (mov, alu, push/pop,
lea, movzx/movsx, shifts by an immediate, inc/dec, setcc, cmov, jcc/jmp/call/ret, leave) are
joined by a small dispatch loop. Registers always stay in memory and flags are computed by the
same code as the interpreter, so exceptions and interrupts see an exact architectural state.
Anything else ends the compiled region and is executed by the interpreter.

Tests live in `jit/`. They set `V86: {"jit64_threshold": N, "sync_jit": true}` so that code is
compiled after N executions and modules are usable immediately. Useful environment variables:

- `JIT64=0 node tests/x64/run.js` compiles every instruction sequence on its first execution
  (the whole suite and `fuzz.js` then run mostly compiled code); `JIT64=4294967295` disables it
- `JIT64_MASK=<bits>` disables instruction kinds (bit numbers: see `kind()` in jit64.rs) to bisect
- `JIT_STATS=1` makes `worker.js` print how many blocks were compiled / run / invalidated
- `jit/gen_flags.py` regenerates `jit/03_flags_conditions.asm`; native runs print the actual
  value of a failed check, which is how the expected values of `06_instruction_forms.asm` were made

## Every instruction the 64-bit tests use

Regenerate this table with `node tools/opcode_coverage.mjs`. The encodings themselves, shared with 32-bit mode,
and their generated tests are listed in [the x86 Readme](../nasm/Readme.md#every-opcode); this table is what
runs in long mode.

<!-- opcodes:begin (tools/opcode_coverage.mjs) -->
The mnemonics the 64-bit tests use, with the number of test files per directory that use each one (macros of `inc/` excluded), 422 in all. `fuzz.js` also compares these instruction families against the host CPU, in long mode with random operands: `sse_int`, `sse_imm`, `ssse3`, `sse41`, `sse41_blend`, `sse_ptest`, `sse_fp`, `sse_fp2`, `sse_move`, `high8`, `atomic`, `string`, `stack`, `alu`, `incdec`, `shift`, `muldiv`, `bit`, `bitscan`, `mov`, `shiftd`.

| Mnemonic | `long/` | `jit/` | `stubs/` | `smp/` | `issues/` |
|---|---|---|---|---|---|
| `adc` | 1 | 3 |  |  |  |
| `add` | 6 | 15 |  | 2 | 2 |
| `addpd` | 1 |  |  |  | 1 |
| `addps` | 2 |  |  |  | 2 |
| `addsd` | 1 |  |  |  | 1 |
| `addss` | 1 |  |  |  | 1 |
| `addsubpd` | 1 |  | 1 |  | 1 |
| `addsubps` | 1 |  | 1 |  | 1 |
| `and` | 2 | 10 | 4 | 4 | 5 |
| `andnpd` | 1 |  |  |  | 1 |
| `andnps` | 1 | 1 |  |  | 1 |
| `andpd` | 1 |  |  |  | 1 |
| `andps` | 1 | 1 |  |  | 1 |
| `ap` |  |  |  | 2 |  |
| `ap32` |  |  |  | 5 |  |
| `ap64` |  |  |  | 2 |  |
| `back64` | 1 |  |  |  |  |
| `blendpd` | 1 |  |  |  | 1 |
| `blendps` | 1 |  |  |  | 1 |
| `bound` |  |  | 1 |  |  |
| `bsf` | 1 | 1 |  |  |  |
| `bsr` | 1 | 1 |  |  |  |
| `bswap` | 1 | 2 |  |  |  |
| `bt` | 1 | 3 |  |  |  |
| `btc` | 1 | 2 |  |  |  |
| `btr` | 1 | 2 |  |  |  |
| `bts` | 2 | 2 |  | 2 |  |
| `callee` |  | 1 |  |  |  |
| `callee2` |  | 1 |  |  |  |
| `cbw` |  | 2 |  |  |  |
| `cdq` |  | 2 |  |  |  |
| `cdqe` | 1 | 2 |  |  |  |
| `clc` |  | 1 |  |  |  |
| `cld` | 2 | 3 |  | 7 |  |
| `cli` | 2 |  |  | 7 | 2 |
| `clts` | 1 |  |  |  |  |
| `cmc` |  | 1 |  |  |  |
| `cmovae` |  | 1 |  |  |  |
| `cmovb` | 1 | 2 |  |  |  |
| `cmovnz` |  | 1 |  |  |  |
| `cmp` | 7 | 13 |  | 7 | 7 |
| `cmppd` | 1 |  |  |  | 1 |
| `cmpps` | 1 |  |  |  | 1 |
| `cmpsb` |  | 1 |  |  |  |
| `cmpsq` | 1 |  |  |  |  |
| `cmpxchg` | 2 | 2 |  | 2 |  |
| `cmpxchg16b` | 1 |  |  |  |  |
| `comiss` | 1 |  |  |  | 1 |
| `compat` | 1 |  |  |  |  |
| `cpuid` | 1 |  | 1 | 2 | 1 |
| `cqo` | 1 | 3 |  |  |  |
| `crc32` |  |  | 2 |  |  |
| `cvtdq2pd` | 1 |  |  |  | 1 |
| `cvtdq2ps` | 1 |  |  |  | 2 |
| `cvtpd2dq` | 1 |  |  |  | 1 |
| `cvtpd2ps` | 1 |  |  |  | 1 |
| `cvtps2dq` | 1 |  | 1 |  | 1 |
| `cvtps2pd` | 1 |  |  |  | 1 |
| `cvtsd2si` |  |  | 1 |  |  |
| `cvtsd2ss` | 1 |  |  |  | 1 |
| `cvtsi2sd` | 3 |  |  |  | 1 |
| `cvtsi2ss` | 1 |  |  |  |  |
| `cvtss2sd` | 1 |  |  |  | 1 |
| `cvtss2si` |  |  | 1 |  |  |
| `cvttpd2dq` | 1 |  |  |  | 1 |
| `cvttps2dq` | 1 |  |  |  | 1 |
| `cvttsd2si` | 2 |  |  |  |  |
| `cvttss2si` | 1 |  | 1 |  |  |
| `cwde` |  | 2 |  |  |  |
| `dec` | 4 | 12 |  | 7 | 3 |
| `div` | 2 | 2 |  |  |  |
| `divpd` | 1 |  |  |  | 1 |
| `divps` | 1 |  |  |  | 1 |
| `divss` |  |  | 2 |  |  |
| `dppd` | 1 |  |  |  | 1 |
| `dpps` | 1 |  |  |  | 1 |
| `emms` |  |  | 1 |  |  |
| `extractps` | 1 |  |  |  | 1 |
| `f1` | 1 |  |  |  |  |
| `fadd` | 1 |  |  |  |  |
| `faddp` | 1 |  |  | 2 |  |
| `fcomip` | 1 |  |  |  |  |
| `fdivp` | 1 |  |  |  |  |
| `fild` | 1 |  |  |  |  |
| `fistp` | 1 |  |  |  |  |
| `fld` | 1 |  |  |  |  |
| `fld1` | 1 |  |  | 2 |  |
| `fldz` | 1 |  |  |  |  |
| `fn` |  | 2 |  | 1 |  |
| `fnstcw` | 1 |  |  |  |  |
| `fnstsw` | 1 |  |  |  |  |
| `fsqrt` | 1 |  |  |  |  |
| `fstp` | 1 |  |  | 2 |  |
| `fxch` | 1 |  |  |  |  |
| `fxrstor` | 1 |  |  |  |  |
| `fxsave` | 1 |  |  |  |  |
| `haddpd` | 1 |  | 1 |  | 1 |
| `haddps` | 1 |  | 1 |  | 1 |
| `handler` | 1 |  |  |  |  |
| `hlt` | 1 |  |  | 7 | 2 |
| `hsubpd` | 1 |  | 1 |  | 1 |
| `hsubps` | 1 |  | 1 |  | 1 |
| `idiv` | 1 | 1 |  |  |  |
| `imul` | 1 | 6 |  |  |  |
| `in` |  |  |  |  | 2 |
| `inc` | 8 | 11 |  | 6 | 6 |
| `insertps` | 1 |  |  |  | 1 |
| `int` | 1 |  |  |  |  |
| `int3` | 1 |  |  |  |  |
| `invd` |  |  | 1 |  |  |
| `invlpg` | 3 |  |  |  |  |
| `iret` |  |  |  |  | 1 |
| `iretd` |  |  |  | 3 | 2 |
| `iretq` | 3 |  |  |  |  |
| `irq0` | 1 |  |  |  | 2 |
| `jae` | 2 | 1 |  | 2 | 3 |
| `jb` |  | 12 |  | 3 | 4 |
| `jbe` | 1 | 5 |  |  | 1 |
| `jc` | 1 | 1 | 1 | 2 |  |
| `je` | 2 | 2 |  | 7 | 3 |
| `jg` |  | 1 |  |  |  |
| `jge` |  | 2 |  |  |  |
| `jl` |  | 1 |  |  |  |
| `jnc` |  | 1 | 1 |  |  |
| `jne` | 2 | 2 |  | 4 | 4 |
| `jns` |  | 1 |  |  |  |
| `jnz` | 8 | 11 | 2 | 7 | 4 |
| `jrcxz` | 1 |  |  |  |  |
| `jz` | 1 | 1 | 1 | 2 | 2 |
| `lddqu` | 1 |  | 1 |  | 3 |
| `ldmxcsr` |  |  | 4 |  |  |
| `lea` | 16 | 15 |  | 2 |  |
| `leave` | 1 | 1 |  |  |  |
| `lfence` |  | 1 |  |  |  |
| `lgdt` |  |  |  | 7 |  |
| `lidt` | 1 |  |  | 3 | 2 |
| `lodsb` |  | 1 |  |  |  |
| `lodsd` |  | 1 |  |  |  |
| `lodsq` | 1 | 2 |  |  |  |
| `lodsw` |  | 1 |  |  |  |
| `loop` | 1 |  |  |  |  |
| `ltr` | 1 |  |  |  |  |
| `lzcnt` | 1 |  |  |  |  |
| `maxpd` | 1 |  |  |  | 1 |
| `maxps` | 1 |  |  |  | 1 |
| `mfence` |  | 1 |  |  |  |
| `minpd` | 1 |  |  |  | 1 |
| `minps` | 1 |  |  |  | 1 |
| `mov` | 29 | 15 | 11 | 7 | 12 |
| `movapd` | 2 |  | 1 |  | 3 |
| `movaps` | 2 | 1 | 1 |  | 4 |
| `movd` | 3 | 1 | 2 | 2 | 2 |
| `movddup` | 1 |  | 1 |  | 1 |
| `movdqa` | 4 | 1 | 4 |  | 4 |
| `movdqu` | 2 | 1 |  |  | 3 |
| `movhlps` |  | 1 |  |  |  |
| `movhpd` | 1 |  |  |  | 1 |
| `movhps` | 1 |  |  |  | 2 |
| `movlpd` | 1 |  |  |  | 1 |
| `movlps` | 1 |  |  |  | 2 |
| `movmskps` | 1 |  |  |  |  |
| `movntdq` | 1 |  |  |  | 3 |
| `movntdqa` | 1 |  |  |  | 1 |
| `movntpd` | 1 |  |  |  | 1 |
| `movntps` | 1 |  |  |  | 2 |
| `movq` | 3 | 1 | 1 |  | 1 |
| `movsb` | 1 | 1 |  | 7 |  |
| `movsd` | 2 | 1 |  |  | 2 |
| `movshdup` | 1 |  | 1 |  | 1 |
| `movsldup` | 1 |  | 1 |  | 1 |
| `movsq` | 2 | 2 |  |  |  |
| `movss` | 1 |  | 3 |  | 2 |
| `movsw` | 1 | 1 |  |  |  |
| `movsx` | 1 | 2 |  |  |  |
| `movsxd` | 1 | 1 |  |  |  |
| `movupd` | 1 |  |  |  | 3 |
| `movups` | 1 | 1 | 2 |  | 4 |
| `movzx` | 4 | 6 |  |  |  |
| `mpsadbw` | 1 |  |  |  | 1 |
| `mul` | 1 | 2 |  |  |  |
| `mulpd` | 2 |  |  |  | 1 |
| `mulps` | 1 |  |  |  | 1 |
| `mulss` | 1 |  | 1 |  | 1 |
| `neg` | 1 | 3 |  |  |  |
| `nop` | 2 | 1 |  |  | 1 |
| `not` | 1 | 2 |  |  |  |
| `or` | 6 | 6 | 2 | 7 | 5 |
| `orpd` | 1 |  |  |  | 1 |
| `orps` | 1 | 1 |  |  | 1 |
| `out` | 1 |  |  | 1 | 3 |
| `pabsb` | 1 |  | 1 |  | 1 |
| `pabsd` | 1 |  | 1 |  | 1 |
| `pabsw` | 1 |  | 1 |  | 1 |
| `packssdw` | 1 |  |  |  | 1 |
| `packsswb` | 1 |  |  |  | 1 |
| `packusdw` | 1 |  |  |  | 1 |
| `packuswb` | 1 |  |  |  | 1 |
| `paddb` | 1 |  |  |  | 1 |
| `paddd` | 2 |  |  |  | 3 |
| `paddq` | 1 |  |  |  | 1 |
| `paddsb` | 1 |  |  |  | 1 |
| `paddsw` | 1 |  |  |  | 1 |
| `paddusb` | 1 |  |  |  | 1 |
| `paddusw` | 1 |  |  |  | 1 |
| `paddw` | 1 |  |  |  | 1 |
| `palignr` | 1 |  | 1 |  | 1 |
| `pand` | 1 | 1 |  |  | 1 |
| `pandn` | 1 | 1 |  |  | 1 |
| `pause` |  | 1 |  |  |  |
| `pavgb` | 1 |  |  |  | 1 |
| `pavgw` | 1 |  |  |  | 1 |
| `pblendw` | 1 |  | 1 |  | 1 |
| `pcmpeqb` | 1 |  |  |  | 1 |
| `pcmpeqd` | 3 |  | 1 |  | 1 |
| `pcmpeqq` | 1 |  | 1 |  | 1 |
| `pcmpeqw` | 1 |  |  |  | 1 |
| `pcmpgtb` | 1 |  |  |  | 1 |
| `pcmpgtd` | 1 |  |  |  | 1 |
| `pcmpgtq` | 1 |  |  |  | 1 |
| `pcmpgtw` | 1 |  |  |  | 1 |
| `pextrb` | 1 |  |  |  | 1 |
| `pextrd` | 1 |  | 1 |  | 1 |
| `pextrw` | 1 |  |  |  | 1 |
| `phaddd` | 1 |  | 1 |  | 1 |
| `phaddsw` | 1 |  | 1 |  | 1 |
| `phaddw` | 1 |  | 1 |  | 1 |
| `phminposuw` | 1 |  |  |  | 1 |
| `phsubd` | 1 |  | 1 |  | 1 |
| `phsubsw` | 1 |  |  |  | 1 |
| `phsubw` | 1 |  | 1 |  | 1 |
| `pinsrb` | 1 |  |  |  | 1 |
| `pinsrd` | 1 |  | 1 |  | 1 |
| `pinsrw` | 1 |  |  |  | 1 |
| `pmaddubsw` | 1 |  | 1 |  | 1 |
| `pmaddwd` | 1 |  |  |  | 1 |
| `pmaxsb` | 1 |  |  |  | 1 |
| `pmaxsd` | 1 |  | 1 |  | 1 |
| `pmaxsw` | 1 |  |  |  | 1 |
| `pmaxub` | 1 |  |  |  | 1 |
| `pmaxud` | 1 |  |  |  | 1 |
| `pmaxuw` | 1 |  |  |  | 1 |
| `pminsb` | 1 |  |  |  | 1 |
| `pminsd` | 1 |  | 1 |  | 1 |
| `pminsw` | 1 |  |  |  | 1 |
| `pminub` | 1 |  |  |  | 1 |
| `pminud` | 1 |  |  |  | 1 |
| `pminuw` | 1 |  |  |  | 1 |
| `pmovsxbd` | 1 |  |  |  | 1 |
| `pmovsxbw` | 1 |  | 1 |  | 1 |
| `pmovsxdq` | 1 |  |  |  | 1 |
| `pmovsxwd` | 1 |  |  |  | 1 |
| `pmovsxwq` | 1 |  |  |  | 1 |
| `pmovzxbq` | 1 |  |  |  | 1 |
| `pmovzxbw` | 1 |  | 1 |  | 1 |
| `pmovzxdq` | 1 |  |  |  | 1 |
| `pmovzxwd` | 1 |  |  |  | 1 |
| `pmuldq` | 1 |  |  |  | 1 |
| `pmulhrsw` | 1 |  | 1 |  | 1 |
| `pmulhuw` | 1 |  |  |  | 1 |
| `pmulhw` | 1 |  |  |  | 1 |
| `pmulld` | 1 |  | 1 |  | 1 |
| `pmullw` | 1 |  |  |  | 1 |
| `pmuludq` | 1 |  |  |  | 1 |
| `pop` | 5 | 7 |  | 1 | 3 |
| `popa` | 1 |  |  |  |  |
| `popcnt` | 1 |  |  |  |  |
| `popf` |  |  |  |  | 1 |
| `popfq` | 2 | 1 |  |  |  |
| `por` | 1 | 1 |  |  | 1 |
| `prefetchnta` |  |  | 1 |  |  |
| `prefetchw` |  |  | 1 |  |  |
| `psadbw` | 1 |  |  |  | 1 |
| `pshufb` | 1 |  | 1 |  | 1 |
| `pshufd` | 2 |  |  | 2 | 2 |
| `pshufhw` | 1 |  |  |  | 1 |
| `pshuflw` | 1 |  |  |  | 1 |
| `psignb` | 1 |  | 1 |  | 1 |
| `psignd` | 1 |  | 1 |  | 1 |
| `psignw` | 1 |  |  |  | 1 |
| `pslld` | 1 |  |  |  | 1 |
| `psllq` | 1 |  |  |  | 1 |
| `psllw` | 1 |  |  |  | 1 |
| `psrad` | 1 |  |  |  | 1 |
| `psraw` | 1 |  |  |  | 1 |
| `psrld` | 2 |  |  |  | 1 |
| `psrldq` | 1 |  |  |  |  |
| `psrlq` | 1 |  |  |  | 1 |
| `psrlw` | 1 |  |  |  | 1 |
| `psubb` | 1 |  |  |  | 1 |
| `psubd` | 1 |  |  |  | 1 |
| `psubq` | 1 |  |  |  | 1 |
| `psubsb` | 1 |  |  |  | 1 |
| `psubsw` | 1 |  |  |  | 1 |
| `psubusb` | 1 |  |  |  | 1 |
| `psubusw` | 1 |  |  |  | 1 |
| `psubw` | 1 |  |  |  | 1 |
| `ptest` | 1 |  | 1 |  | 1 |
| `punpckhbw` | 1 |  |  |  | 1 |
| `punpckhdq` | 1 |  |  |  | 1 |
| `punpckhqdq` | 1 |  |  |  | 1 |
| `punpckhwd` | 1 |  |  |  | 1 |
| `punpcklbw` | 1 |  |  |  | 2 |
| `punpckldq` | 2 |  |  |  | 1 |
| `punpcklqdq` | 1 | 1 |  |  | 1 |
| `punpcklwd` | 1 |  |  |  | 1 |
| `push` | 6 | 5 |  |  | 3 |
| `pusha` | 1 |  |  |  |  |
| `pushf` |  |  |  |  | 1 |
| `pushfd` |  |  |  | 1 |  |
| `pushfq` | 3 | 3 |  |  |  |
| `pxor` | 4 | 1 | 3 |  | 3 |
| `rcpps` | 1 |  |  |  | 1 |
| `rdfsbase` | 1 |  |  |  |  |
| `rdgsbase` | 1 |  |  |  |  |
| `rdmsr` | 3 |  |  | 2 | 1 |
| `rdtsc` |  | 1 |  |  |  |
| `retfq` | 1 |  |  |  |  |
| `rol` | 2 | 9 |  |  |  |
| `ror` | 1 | 1 |  |  |  |
| `roundpd` | 1 |  |  |  | 1 |
| `roundps` | 1 |  | 2 |  | 1 |
| `roundsd` | 1 |  |  |  | 1 |
| `roundss` | 1 |  |  |  | 1 |
| `rsqrtps` | 1 |  |  |  | 1 |
| `sar` | 1 | 2 |  |  |  |
| `sbb` | 1 | 2 |  | 2 |  |
| `scasq` | 1 | 1 |  |  |  |
| `seta` |  | 1 |  |  |  |
| `setae` | 1 | 2 |  |  |  |
| `setb` |  | 3 |  |  |  |
| `setbe` |  | 1 |  |  |  |
| `setc` |  | 3 |  |  |  |
| `sete` | 1 | 1 |  |  |  |
| `setg` |  | 2 |  |  |  |
| `setge` |  | 1 |  |  |  |
| `setl` |  | 1 |  |  |  |
| `setle` |  | 1 |  |  |  |
| `setne` |  | 2 |  |  |  |
| `setno` |  | 1 |  |  |  |
| `setnp` |  | 1 |  |  |  |
| `setns` |  | 1 |  |  |  |
| `seto` |  | 2 |  |  |  |
| `setp` |  | 1 |  |  |  |
| `sets` |  | 2 |  |  |  |
| `setz` |  | 2 |  |  |  |
| `sfence` |  | 1 |  |  |  |
| `sgdt` | 1 |  |  |  |  |
| `shl` | 1 | 4 |  |  |  |
| `shld` | 1 |  |  |  |  |
| `shr` | 6 | 6 |  | 4 | 2 |
| `shrd` | 1 |  |  |  |  |
| `shufpd` | 1 |  |  |  | 1 |
| `shufps` | 1 |  |  |  | 1 |
| `sidt` | 1 |  |  |  |  |
| `site1` |  |  |  |  | 1 |
| `smsw` | 1 |  |  |  |  |
| `sqrtpd` | 1 |  |  |  | 1 |
| `sqrtps` | 1 |  |  |  | 1 |
| `sqrtsd` | 1 |  |  |  | 1 |
| `stc` | 1 | 1 |  | 2 |  |
| `std` | 1 | 1 |  |  |  |
| `sti` | 1 |  |  | 7 | 2 |
| `stmxcsr` |  |  | 2 |  |  |
| `stosb` | 1 | 2 |  |  |  |
| `stosd` | 3 | 1 |  | 3 |  |
| `stosq` | 4 | 2 |  | 2 |  |
| `stosw` | 1 | 1 |  |  |  |
| `str` | 1 |  |  |  |  |
| `strs` |  | 1 |  |  |  |
| `sub` | 6 | 4 |  |  |  |
| `subpd` | 1 |  |  |  | 1 |
| `subps` | 1 |  |  |  | 1 |
| `swapgs` | 1 |  |  |  |  |
| `syscall` | 2 | 5 |  |  |  |
| `sysret` | 1 |  |  |  |  |
| `t0` | 1 |  |  |  |  |
| `t1` | 1 |  |  |  |  |
| `t2` | 1 |  |  |  |  |
| `tbl1` |  | 2 |  |  |  |
| `tbl10` |  | 1 |  |  |  |
| `tbl11` |  | 1 |  |  |  |
| `tbl12` |  | 1 |  |  |  |
| `tbl13` |  | 1 |  |  |  |
| `tbl14` |  | 1 |  |  |  |
| `tbl15` |  | 1 |  |  |  |
| `tbl16` |  | 1 |  |  |  |
| `tbl17` |  | 1 |  |  |  |
| `tbl18` |  | 1 |  |  |  |
| `tbl19` |  | 1 |  |  |  |
| `tbl2` |  | 2 |  |  |  |
| `tbl20` |  | 1 |  |  |  |
| `tbl21` |  | 1 |  |  |  |
| `tbl3` |  | 2 |  |  |  |
| `tbl4` |  | 2 |  |  |  |
| `tbl5` |  | 2 |  |  |  |
| `tbl6` |  | 2 |  |  |  |
| `tbl7` |  | 2 |  |  |  |
| `tbl8` |  | 2 |  |  |  |
| `tbl9` |  | 1 |  |  |  |
| `test` | 5 | 5 | 1 | 3 | 1 |
| `tzcnt` | 1 |  |  |  |  |
| `ucomisd` | 1 |  |  |  | 1 |
| `ud2` | 1 |  |  |  |  |
| `unpckhpd` | 1 |  |  |  | 1 |
| `unpckhps` | 1 |  |  |  | 1 |
| `unpcklpd` | 1 |  |  |  | 1 |
| `unpcklps` | 1 |  |  |  | 1 |
| `wbinvd` | 1 |  |  |  |  |
| `wrapped` |  |  |  |  | 1 |
| `wrfsbase` | 1 |  |  |  |  |
| `wrgsbase` | 1 |  |  |  |  |
| `wrmsr` | 5 |  |  | 2 |  |
| `xadd` | 2 | 2 |  | 2 |  |
| `xchg` | 2 | 2 |  | 2 |  |
| `xgetbv` |  |  | 2 |  |  |
| `xor` | 13 | 15 | 4 | 7 | 4 |
| `xorpd` | 1 |  |  |  | 1 |
| `xorps` | 2 | 1 |  |  | 1 |
| `xrstor` |  |  | 2 |  | 1 |
| `xsave` |  |  | 2 |  | 1 |
| `xsaveopt` |  |  | 1 |  | 1 |
| `xsetbv` |  |  | 2 |  | 1 |
<!-- opcodes:end -->
