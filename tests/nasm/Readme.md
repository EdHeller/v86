# x86 (32-bit) instruction tests

The instruction tests of the 32-bit x86 CPU (`v86.wasm`, `cpu_type: "x86"`). Every encoding in
[`gen/x86_table.js`](../../gen/x86_table.js), the table the interpreter, the JIT and these tests are generated
from, gets tests that are run on the host CPU and in v86, and the two results must be equal.

    make nasmtests            # generate, record the host's results, run in v86 (debug build)
    make nasmtests-force-jit  # the same, with every block compiled by the JIT

Needs nasm, ld, gdb and a host that runs 32-bit x86 Linux programs (an x86-64 Linux host with 32-bit support).

## How it works

1. `create_tests.js` writes one small program per test into `build/`: `header.inc`, a few instructions, `footer.inc`.
   For each encoding of the table it makes 5 tests with random register, MMX/XMM and flag contents, in register and
   memory forms and both operand sizes where they exist; the arithmetic instructions get 100 more
   (`arith_*.asm`). The hand-written `*.asm` files in this folder are added as they are.
2. `gen_fixtures.js` runs each program on the host CPU under gdb and records the registers, the flags (masked by
   the table's `mask_flags` where the result is undefined), the FPU/MMX/XMM state and the memory it wrote, as
   `build/<test>.fixture`; an exception becomes the expected exception (SIGILL -> #UD, SIGFPE -> #DE).
3. `run.js` runs each program in v86 and compares its state with the fixture.

Options: `TEST_NAME=regex` (run only matching tests), `MAX_PARALLEL_TESTS=n`, `TEST_RELEASE_BUILD=1` (the release
bundle `libv86.mjs` and `v86.wasm` instead of the sources and `v86-debug.wasm`).

## The hand-written tests

They cover what random generation cannot, or did not, find: bit tests with large offsets (`btc-*`, `btr-*`,
`bts-*`), `cmpxchg`/`cmpxchg8b`, x87 edge cases (`f2xm1`, `fdiv-zero`, `fprem`, `fptan`, `fsincos`, `fxtract`,
`fyl2x`, `fsave_frstor`, `fstenv`, `fpu_m80`, ...), division overflow (`idiv*-overflow`), the MMX and SSE
integer and packed instructions (`padd*`, `psub*`, `pcmp*`, `pack*`, `punpck*`, `psll*`/`psra*`/`psrl*`,
`pmul*`, `maskmov*`, `movnt*`, `cvt*ps*`, `shufps-edgecase`, `comiss`/`ucomiss`), stack and segment forms
(`push`, `pushf`, `pop_esp*`, `leave16`/`leave32`, `mov_sreg*`, `double-segment-prefix`) and control flow (`jump`,
`indirect-call`/`indirect-jump`, `loop*`, `jcxz`, `ret-imm`).

## Other x86 suites

| Suite | What it checks | Run |
|---|---|---|
| [`tests/qemu`](../qemu/) | QEMU's `test-i386`: many instructions with many operands, the output compared with QEMU's | `make qemutests` |
| [`tests/kvm-unit-tests`](../kvm-unit-tests/) | task switches, real mode, PAT, NX | `make kvm-unit-test` |
| [`tests/jit-paging`](../jit-paging/) | the JIT with paging and self-modifying code | `make jitpagingtests` |
| [`tests/expect`](../expect/) | the code the JIT generates for given instructions | `make expect-tests` |
| [`tests/x64`](../x64/) | x86_64, and the stubbed and newer instructions also in 32-bit modes | see its [Readme](../x64/Readme.md) |

## Every opcode

Regenerate this table with `node tools/opcode_coverage.mjs` after changing the table or the tests.

<!-- opcodes:begin (tools/opcode_coverage.mjs) -->
876 encodings: 683 with generated tests, 181 skipped by the generator (they change control flow, segments or the machine state in ways a generated test cannot compare: jumps, calls, `int`, `iret`, `hlt`, port I/O, `pop ss`, ...), 12 prefixes. The disassembly is ndisasm's for a register (or, where only memory is valid, a memory) operand and 0x01 immediates; "Hand-written" lists up to three test files that use the mnemonic.

| Opcode | Disassembly (example) | Generated | Hand-written |
|---|---|---|---|
| `00` | `add al,al` | reg + mem | `x64/issues/sse_alignment_jit_1356.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/jit/01_loops.asm` +24 |
| `01` | `add eax,eax` | reg + mem | `x64/issues/sse_alignment_jit_1356.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/jit/01_loops.asm` +24 |
| `02` | `add al,al` | reg + mem | `x64/issues/sse_alignment_jit_1356.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/jit/01_loops.asm` +24 |
| `03` | `add eax,eax` | reg + mem | `x64/issues/sse_alignment_jit_1356.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/jit/01_loops.asm` +24 |
| `04` | `add al,0x1` | yes | `x64/issues/sse_alignment_jit_1356.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/jit/01_loops.asm` +24 |
| `05` | `add eax,0x1010101` | yes | `x64/issues/sse_alignment_jit_1356.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/jit/01_loops.asm` +24 |
| `06` | `push es` | yes | `nasm/cmpxchg.asm`, `nasm/cmpxchg8b.asm`, `nasm/fdiv-zero.asm` +21 |
| `07` | `pop es` | no (skip) | `nasm/comiss.asm`, `nasm/pop_esp.asm`, `nasm/push.asm` +17 |
| `08` | `or al,al` | reg + mem | `nasm/cvtps2pi.asm`, `nasm/fstm80.asm`, `nasm/jcxz.asm` +28 |
| `09` | `or eax,eax` | reg + mem | `nasm/cvtps2pi.asm`, `nasm/fstm80.asm`, `nasm/jcxz.asm` +28 |
| `0A` | `or al,al` | reg + mem | `nasm/cvtps2pi.asm`, `nasm/fstm80.asm`, `nasm/jcxz.asm` +28 |
| `0B` | `or eax,eax` | reg + mem | `nasm/cvtps2pi.asm`, `nasm/fstm80.asm`, `nasm/jcxz.asm` +28 |
| `0C` | `or al,0x1` | yes | `nasm/cvtps2pi.asm`, `nasm/fstm80.asm`, `nasm/jcxz.asm` +28 |
| `0D` | `or eax,0x1010101` | yes | `nasm/cvtps2pi.asm`, `nasm/fstm80.asm`, `nasm/jcxz.asm` +28 |
| `0E` | `push cs` | yes | `nasm/cmpxchg.asm`, `nasm/cmpxchg8b.asm`, `nasm/fdiv-zero.asm` +21 |
| `0F` | `(#UD here: reserved, or only valid with a 66/F2/F3 prefix)` | prefix |  |
| `10` | `adc al,al` | reg + mem | `x64/jit/06_instruction_forms.asm`, `x64/jit/07_flag_liveness.asm`, `x64/jit/11_interp_in_block.asm` +1 |
| `11` | `adc eax,eax` | reg + mem | `x64/jit/06_instruction_forms.asm`, `x64/jit/07_flag_liveness.asm`, `x64/jit/11_interp_in_block.asm` +1 |
| `12` | `adc al,al` | reg + mem | `x64/jit/06_instruction_forms.asm`, `x64/jit/07_flag_liveness.asm`, `x64/jit/11_interp_in_block.asm` +1 |
| `13` | `adc eax,eax` | reg + mem | `x64/jit/06_instruction_forms.asm`, `x64/jit/07_flag_liveness.asm`, `x64/jit/11_interp_in_block.asm` +1 |
| `14` | `adc al,0x1` | yes | `x64/jit/06_instruction_forms.asm`, `x64/jit/07_flag_liveness.asm`, `x64/jit/11_interp_in_block.asm` +1 |
| `15` | `adc eax,0x1010101` | yes | `x64/jit/06_instruction_forms.asm`, `x64/jit/07_flag_liveness.asm`, `x64/jit/11_interp_in_block.asm` +1 |
| `16` | `push ss` | yes | `nasm/cmpxchg.asm`, `nasm/cmpxchg8b.asm`, `nasm/fdiv-zero.asm` +21 |
| `17` | `pop ss` | no (skip) | `nasm/comiss.asm`, `nasm/pop_esp.asm`, `nasm/push.asm` +17 |
| `18` | `sbb al,al` | reg + mem | `x64/jit/06_instruction_forms.asm`, `x64/jit/07_flag_liveness.asm`, `x64/long/03_arith64.asm` +2 |
| `19` | `sbb eax,eax` | reg + mem | `x64/jit/06_instruction_forms.asm`, `x64/jit/07_flag_liveness.asm`, `x64/long/03_arith64.asm` +2 |
| `1A` | `sbb al,al` | reg + mem | `x64/jit/06_instruction_forms.asm`, `x64/jit/07_flag_liveness.asm`, `x64/long/03_arith64.asm` +2 |
| `1B` | `sbb eax,eax` | reg + mem | `x64/jit/06_instruction_forms.asm`, `x64/jit/07_flag_liveness.asm`, `x64/long/03_arith64.asm` +2 |
| `1C` | `sbb al,0x1` | yes | `x64/jit/06_instruction_forms.asm`, `x64/jit/07_flag_liveness.asm`, `x64/long/03_arith64.asm` +2 |
| `1D` | `sbb eax,0x1010101` | yes | `x64/jit/06_instruction_forms.asm`, `x64/jit/07_flag_liveness.asm`, `x64/long/03_arith64.asm` +2 |
| `1E` | `push ds` | yes | `nasm/cmpxchg.asm`, `nasm/cmpxchg8b.asm`, `nasm/fdiv-zero.asm` +21 |
| `1F` | `pop ds` | no (skip) | `nasm/comiss.asm`, `nasm/pop_esp.asm`, `nasm/push.asm` +17 |
| `20` | `and al,al` | reg + mem | `nasm/arith-optimisations.asm`, `nasm/cmpxchg.asm`, `nasm/cmpxchg8b.asm` +31 |
| `21` | `and eax,eax` | reg + mem | `nasm/arith-optimisations.asm`, `nasm/cmpxchg.asm`, `nasm/cmpxchg8b.asm` +31 |
| `22` | `and al,al` | reg + mem | `nasm/arith-optimisations.asm`, `nasm/cmpxchg.asm`, `nasm/cmpxchg8b.asm` +31 |
| `23` | `and eax,eax` | reg + mem | `nasm/arith-optimisations.asm`, `nasm/cmpxchg.asm`, `nasm/cmpxchg8b.asm` +31 |
| `24` | `and al,0x1` | yes | `nasm/arith-optimisations.asm`, `nasm/cmpxchg.asm`, `nasm/cmpxchg8b.asm` +31 |
| `25` | `and eax,0x1010101` | yes | `nasm/arith-optimisations.asm`, `nasm/cmpxchg.asm`, `nasm/cmpxchg8b.asm` +31 |
| `26` | `es` | prefix |  |
| `27` | `daa` | yes |  |
| `28` | `sub al,al` | reg + mem | `nasm/fsave_frstor.asm`, `nasm/maskmovdqu.asm`, `nasm/maskmovq.asm` +10 |
| `29` | `sub eax,eax` | reg + mem | `nasm/fsave_frstor.asm`, `nasm/maskmovdqu.asm`, `nasm/maskmovq.asm` +10 |
| `2A` | `sub al,al` | reg + mem | `nasm/fsave_frstor.asm`, `nasm/maskmovdqu.asm`, `nasm/maskmovq.asm` +10 |
| `2B` | `sub eax,eax` | reg + mem | `nasm/fsave_frstor.asm`, `nasm/maskmovdqu.asm`, `nasm/maskmovq.asm` +10 |
| `2C` | `sub al,0x1` | yes | `nasm/fsave_frstor.asm`, `nasm/maskmovdqu.asm`, `nasm/maskmovq.asm` +10 |
| `2D` | `sub eax,0x1010101` | yes | `nasm/fsave_frstor.asm`, `nasm/maskmovdqu.asm`, `nasm/maskmovq.asm` +10 |
| `2E` | `cs` | prefix |  |
| `2F` | `das` | yes |  |
| `30` | `xor al,al` | reg + mem | `nasm/arith-optimisations.asm`, `nasm/indirect-call.asm`, `nasm/indirect-jump.asm` +47 |
| `31` | `xor eax,eax` | reg + mem | `nasm/arith-optimisations.asm`, `nasm/indirect-call.asm`, `nasm/indirect-jump.asm` +47 |
| `32` | `xor al,al` | reg + mem | `nasm/arith-optimisations.asm`, `nasm/indirect-call.asm`, `nasm/indirect-jump.asm` +47 |
| `33` | `xor eax,eax` | reg + mem | `nasm/arith-optimisations.asm`, `nasm/indirect-call.asm`, `nasm/indirect-jump.asm` +47 |
| `34` | `xor al,0x1` | yes | `nasm/arith-optimisations.asm`, `nasm/indirect-call.asm`, `nasm/indirect-jump.asm` +47 |
| `35` | `xor eax,0x1010101` | yes | `nasm/arith-optimisations.asm`, `nasm/indirect-call.asm`, `nasm/indirect-jump.asm` +47 |
| `36` | `ss` | prefix |  |
| `37` | `aaa` | yes |  |
| `38` | `cmp al,al` | reg + mem | `nasm/arith-optimisations.asm`, `nasm/cmp16-setbe.asm`, `nasm/jump.asm` +34 |
| `39` | `cmp eax,eax` | reg + mem | `nasm/arith-optimisations.asm`, `nasm/cmp16-setbe.asm`, `nasm/jump.asm` +34 |
| `3A` | `cmp al,al` | reg + mem | `nasm/arith-optimisations.asm`, `nasm/cmp16-setbe.asm`, `nasm/jump.asm` +34 |
| `3B` | `cmp eax,eax` | reg + mem | `nasm/arith-optimisations.asm`, `nasm/cmp16-setbe.asm`, `nasm/jump.asm` +34 |
| `3C` | `cmp al,0x1` | yes | `nasm/arith-optimisations.asm`, `nasm/cmp16-setbe.asm`, `nasm/jump.asm` +34 |
| `3D` | `cmp eax,0x1010101` | yes | `nasm/arith-optimisations.asm`, `nasm/cmp16-setbe.asm`, `nasm/jump.asm` +34 |
| `3E` | `ds` | prefix |  |
| `3F` | `aas` | yes |  |
| `40` | `inc eax` | yes | `nasm/jump.asm`, `nasm/loop.asm`, `nasm/loopz.asm` +31 |
| `41` | `inc ecx` | yes | `nasm/jump.asm`, `nasm/loop.asm`, `nasm/loopz.asm` +31 |
| `42` | `inc edx` | yes | `nasm/jump.asm`, `nasm/loop.asm`, `nasm/loopz.asm` +31 |
| `43` | `inc ebx` | yes | `nasm/jump.asm`, `nasm/loop.asm`, `nasm/loopz.asm` +31 |
| `44` | `inc esp` | yes | `nasm/jump.asm`, `nasm/loop.asm`, `nasm/loopz.asm` +31 |
| `45` | `inc ebp` | yes | `nasm/jump.asm`, `nasm/loop.asm`, `nasm/loopz.asm` +31 |
| `46` | `inc esi` | yes | `nasm/jump.asm`, `nasm/loop.asm`, `nasm/loopz.asm` +31 |
| `47` | `inc edi` | yes | `nasm/jump.asm`, `nasm/loop.asm`, `nasm/loopz.asm` +31 |
| `48` | `dec eax` | yes | `nasm/loopnz.asm`, `x64/issues/sse_alignment_jit_1356.asm`, `x64/issues/sse_alignment_matrix.asm` +24 |
| `49` | `dec ecx` | yes | `nasm/loopnz.asm`, `x64/issues/sse_alignment_jit_1356.asm`, `x64/issues/sse_alignment_matrix.asm` +24 |
| `4A` | `dec edx` | yes | `nasm/loopnz.asm`, `x64/issues/sse_alignment_jit_1356.asm`, `x64/issues/sse_alignment_matrix.asm` +24 |
| `4B` | `dec ebx` | yes | `nasm/loopnz.asm`, `x64/issues/sse_alignment_jit_1356.asm`, `x64/issues/sse_alignment_matrix.asm` +24 |
| `4C` | `dec esp` | yes | `nasm/loopnz.asm`, `x64/issues/sse_alignment_jit_1356.asm`, `x64/issues/sse_alignment_matrix.asm` +24 |
| `4D` | `dec ebp` | yes | `nasm/loopnz.asm`, `x64/issues/sse_alignment_jit_1356.asm`, `x64/issues/sse_alignment_matrix.asm` +24 |
| `4E` | `dec esi` | yes | `nasm/loopnz.asm`, `x64/issues/sse_alignment_jit_1356.asm`, `x64/issues/sse_alignment_matrix.asm` +24 |
| `4F` | `dec edi` | yes | `nasm/loopnz.asm`, `x64/issues/sse_alignment_jit_1356.asm`, `x64/issues/sse_alignment_matrix.asm` +24 |
| `50` | `push eax` | yes | `nasm/cmpxchg.asm`, `nasm/cmpxchg8b.asm`, `nasm/fdiv-zero.asm` +21 |
| `51` | `push ecx` | yes | `nasm/cmpxchg.asm`, `nasm/cmpxchg8b.asm`, `nasm/fdiv-zero.asm` +21 |
| `52` | `push edx` | yes | `nasm/cmpxchg.asm`, `nasm/cmpxchg8b.asm`, `nasm/fdiv-zero.asm` +21 |
| `53` | `push ebx` | yes | `nasm/cmpxchg.asm`, `nasm/cmpxchg8b.asm`, `nasm/fdiv-zero.asm` +21 |
| `54` | `push esp` | yes | `nasm/cmpxchg.asm`, `nasm/cmpxchg8b.asm`, `nasm/fdiv-zero.asm` +21 |
| `55` | `push ebp` | yes | `nasm/cmpxchg.asm`, `nasm/cmpxchg8b.asm`, `nasm/fdiv-zero.asm` +21 |
| `56` | `push esi` | yes | `nasm/cmpxchg.asm`, `nasm/cmpxchg8b.asm`, `nasm/fdiv-zero.asm` +21 |
| `57` | `push edi` | yes | `nasm/cmpxchg.asm`, `nasm/cmpxchg8b.asm`, `nasm/fdiv-zero.asm` +21 |
| `58` | `pop eax` | yes | `nasm/comiss.asm`, `nasm/pop_esp.asm`, `nasm/push.asm` +17 |
| `59` | `pop ecx` | yes | `nasm/comiss.asm`, `nasm/pop_esp.asm`, `nasm/push.asm` +17 |
| `5A` | `pop edx` | yes | `nasm/comiss.asm`, `nasm/pop_esp.asm`, `nasm/push.asm` +17 |
| `5B` | `pop ebx` | yes | `nasm/comiss.asm`, `nasm/pop_esp.asm`, `nasm/push.asm` +17 |
| `5C` | `pop esp` | yes | `nasm/comiss.asm`, `nasm/pop_esp.asm`, `nasm/push.asm` +17 |
| `5D` | `pop ebp` | yes | `nasm/comiss.asm`, `nasm/pop_esp.asm`, `nasm/push.asm` +17 |
| `5E` | `pop esi` | yes | `nasm/comiss.asm`, `nasm/pop_esp.asm`, `nasm/push.asm` +17 |
| `5F` | `pop edi` | yes | `nasm/comiss.asm`, `nasm/pop_esp.asm`, `nasm/push.asm` +17 |
| `60` | `pusha` | yes | `x64/long/25_compat_mode.asm` |
| `61` | `popa` | yes | `x64/long/25_compat_mode.asm` |
| `62` | `bound eax,[eax]` | no (skip) | `x64/stubs/bound.asm` |
| `63` | `arpl ax,ax` | reg + mem |  |
| `64` | `fs` | prefix |  |
| `65` | `gs` | prefix |  |
| `66` | `o16` | prefix |  |
| `67` | `a16` | prefix |  |
| `68` | `push dword 0x1010101` | yes | `nasm/cmpxchg.asm`, `nasm/cmpxchg8b.asm`, `nasm/fdiv-zero.asm` +21 |
| `69` | `imul eax,eax,dword 0x1010101` | reg + mem | `x64/jit/01_loops.asm`, `x64/jit/06_instruction_forms.asm`, `x64/jit/08_bit_string_misc.asm` +4 |
| `6A` | `push byte +0x1` | yes | `nasm/cmpxchg.asm`, `nasm/cmpxchg8b.asm`, `nasm/fdiv-zero.asm` +21 |
| `6B` | `imul eax,eax,byte +0x1` | reg + mem | `x64/jit/01_loops.asm`, `x64/jit/06_instruction_forms.asm`, `x64/jit/08_bit_string_misc.asm` +4 |
| `6C` | `insb` | no (skip) |  |
| `F26C` | `repne insb` | no (skip) |  |
| `F36C` | `rep insb` | no (skip) |  |
| `6D` | `insd` | no (skip) |  |
| `F26D` | `repne insd` | no (skip) |  |
| `F36D` | `rep insd` | no (skip) |  |
| `6E` | `outsb` | no (skip) |  |
| `F26E` | `repne outsb` | no (skip) |  |
| `F36E` | `rep outsb` | no (skip) |  |
| `6F` | `outsd` | no (skip) |  |
| `F26F` | `repne outsd` | no (skip) |  |
| `F36F` | `rep outsd` | no (skip) |  |
| `70` | `jo 0x3` | no (skip) |  |
| `71` | `jno 0x3` | no (skip) |  |
| `72` | `jc 0x3` | no (skip) | `x64/jit/10_scheduler_block.asm`, `x64/long/25_compat_mode.asm`, `x64/manual/templeos/chainload_mbr.asm` +3 |
| `73` | `jnc 0x3` | no (skip) | `x64/jit/08_bit_string_misc.asm`, `x64/stubs/sse41_sse42_basics.asm` |
| `74` | `jz 0x3` | no (skip) | `x64/issues/sse_alignment_jit_1356.asm`, `x64/issues/sse_unaligned_ok_jit.asm`, `x64/jit/01_loops.asm` +4 |
| `75` | `jnz 0x3` | no (skip) | `x64/issues/apic_present_247.asm`, `x64/issues/sse_alignment_jit_1356.asm`, `x64/issues/sse_alignment_matrix.asm` +30 |
| `76` | `jna 0x3` | no (skip) |  |
| `77` | `ja 0x3` | no (skip) |  |
| `78` | `js 0x3` | no (skip) |  |
| `79` | `jns 0x3` | no (skip) | `x64/jit/08_bit_string_misc.asm` |
| `7A` | `jpe 0x3` | no (skip) |  |
| `7B` | `jpo 0x3` | no (skip) |  |
| `7C` | `jl 0x3` | no (skip) | `x64/jit/07_flag_liveness.asm` |
| `7D` | `jnl 0x3` | no (skip) |  |
| `7E` | `jng 0x3` | no (skip) |  |
| `7F` | `jg 0x3` | no (skip) | `x64/jit/10_scheduler_block.asm` |
| `80 /0` | `add al,0x1` | reg + mem | `x64/issues/sse_alignment_jit_1356.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/jit/01_loops.asm` +24 |
| `80 /1` | `or al,0x1` | reg + mem | `nasm/cvtps2pi.asm`, `nasm/fstm80.asm`, `nasm/jcxz.asm` +28 |
| `80 /2` | `adc al,0x1` | reg + mem | `x64/jit/06_instruction_forms.asm`, `x64/jit/07_flag_liveness.asm`, `x64/jit/11_interp_in_block.asm` +1 |
| `80 /3` | `sbb al,0x1` | reg + mem | `x64/jit/06_instruction_forms.asm`, `x64/jit/07_flag_liveness.asm`, `x64/long/03_arith64.asm` +2 |
| `80 /4` | `and al,0x1` | reg + mem | `nasm/arith-optimisations.asm`, `nasm/cmpxchg.asm`, `nasm/cmpxchg8b.asm` +31 |
| `80 /5` | `sub al,0x1` | reg + mem | `nasm/fsave_frstor.asm`, `nasm/maskmovdqu.asm`, `nasm/maskmovq.asm` +10 |
| `80 /6` | `xor al,0x1` | reg + mem | `nasm/arith-optimisations.asm`, `nasm/indirect-call.asm`, `nasm/indirect-jump.asm` +47 |
| `80 /7` | `cmp al,0x1` | reg + mem | `nasm/arith-optimisations.asm`, `nasm/cmp16-setbe.asm`, `nasm/jump.asm` +34 |
| `81 /0` | `add eax,0x1010101` | reg + mem | `x64/issues/sse_alignment_jit_1356.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/jit/01_loops.asm` +24 |
| `81 /1` | `or eax,0x1010101` | reg + mem | `nasm/cvtps2pi.asm`, `nasm/fstm80.asm`, `nasm/jcxz.asm` +28 |
| `81 /2` | `adc eax,0x1010101` | reg + mem | `x64/jit/06_instruction_forms.asm`, `x64/jit/07_flag_liveness.asm`, `x64/jit/11_interp_in_block.asm` +1 |
| `81 /3` | `sbb eax,0x1010101` | reg + mem | `x64/jit/06_instruction_forms.asm`, `x64/jit/07_flag_liveness.asm`, `x64/long/03_arith64.asm` +2 |
| `81 /4` | `and eax,0x1010101` | reg + mem | `nasm/arith-optimisations.asm`, `nasm/cmpxchg.asm`, `nasm/cmpxchg8b.asm` +31 |
| `81 /5` | `sub eax,0x1010101` | reg + mem | `nasm/fsave_frstor.asm`, `nasm/maskmovdqu.asm`, `nasm/maskmovq.asm` +10 |
| `81 /6` | `xor eax,0x1010101` | reg + mem | `nasm/arith-optimisations.asm`, `nasm/indirect-call.asm`, `nasm/indirect-jump.asm` +47 |
| `81 /7` | `cmp eax,0x1010101` | reg + mem | `nasm/arith-optimisations.asm`, `nasm/cmp16-setbe.asm`, `nasm/jump.asm` +34 |
| `82 /0` | `add r/m8, imm8 (alias of 80 /0)` | reg + mem | `x64/issues/sse_alignment_jit_1356.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/jit/01_loops.asm` +24 |
| `82 /1` | `or r/m8, imm8 (alias of 80 /1)` | reg + mem | `nasm/cvtps2pi.asm`, `nasm/fstm80.asm`, `nasm/jcxz.asm` +28 |
| `82 /2` | `adc r/m8, imm8 (alias of 80 /2)` | reg + mem | `x64/jit/06_instruction_forms.asm`, `x64/jit/07_flag_liveness.asm`, `x64/jit/11_interp_in_block.asm` +1 |
| `82 /3` | `sbb r/m8, imm8 (alias of 80 /3)` | reg + mem | `x64/jit/06_instruction_forms.asm`, `x64/jit/07_flag_liveness.asm`, `x64/long/03_arith64.asm` +2 |
| `82 /4` | `and r/m8, imm8 (alias of 80 /4)` | reg + mem | `nasm/arith-optimisations.asm`, `nasm/cmpxchg.asm`, `nasm/cmpxchg8b.asm` +31 |
| `82 /5` | `sub r/m8, imm8 (alias of 80 /5)` | reg + mem | `nasm/fsave_frstor.asm`, `nasm/maskmovdqu.asm`, `nasm/maskmovq.asm` +10 |
| `82 /6` | `xor r/m8, imm8 (alias of 80 /6)` | reg + mem | `nasm/arith-optimisations.asm`, `nasm/indirect-call.asm`, `nasm/indirect-jump.asm` +47 |
| `82 /7` | `cmp r/m8, imm8 (alias of 80 /7)` | reg + mem | `nasm/arith-optimisations.asm`, `nasm/cmp16-setbe.asm`, `nasm/jump.asm` +34 |
| `83 /0` | `add eax,byte +0x1` | reg + mem | `x64/issues/sse_alignment_jit_1356.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/jit/01_loops.asm` +24 |
| `83 /1` | `or eax,byte +0x1` | reg + mem | `nasm/cvtps2pi.asm`, `nasm/fstm80.asm`, `nasm/jcxz.asm` +28 |
| `83 /2` | `adc eax,byte +0x1` | reg + mem | `x64/jit/06_instruction_forms.asm`, `x64/jit/07_flag_liveness.asm`, `x64/jit/11_interp_in_block.asm` +1 |
| `83 /3` | `sbb eax,byte +0x1` | reg + mem | `x64/jit/06_instruction_forms.asm`, `x64/jit/07_flag_liveness.asm`, `x64/long/03_arith64.asm` +2 |
| `83 /4` | `and eax,byte +0x1` | reg + mem | `nasm/arith-optimisations.asm`, `nasm/cmpxchg.asm`, `nasm/cmpxchg8b.asm` +31 |
| `83 /5` | `sub eax,byte +0x1` | reg + mem | `nasm/fsave_frstor.asm`, `nasm/maskmovdqu.asm`, `nasm/maskmovq.asm` +10 |
| `83 /6` | `xor eax,byte +0x1` | reg + mem | `nasm/arith-optimisations.asm`, `nasm/indirect-call.asm`, `nasm/indirect-jump.asm` +47 |
| `83 /7` | `cmp eax,byte +0x1` | reg + mem | `nasm/arith-optimisations.asm`, `nasm/cmp16-setbe.asm`, `nasm/jump.asm` +34 |
| `84` | `test al,al` | reg + mem | `nasm/arith-optimisations.asm`, `x64/issues/apic_present_247.asm`, `x64/jit/01_loops.asm` +14 |
| `85` | `test eax,eax` | reg + mem | `nasm/arith-optimisations.asm`, `x64/issues/apic_present_247.asm`, `x64/jit/01_loops.asm` +14 |
| `86` | `xchg al,al` | reg + mem | `x64/jit/08_bit_string_misc.asm`, `x64/jit/09_masked_kinds.asm`, `x64/long/06_extend_cmov_xchg.asm` +3 |
| `87` | `xchg eax,eax` | reg + mem | `x64/jit/08_bit_string_misc.asm`, `x64/jit/09_masked_kinds.asm`, `x64/long/06_extend_cmov_xchg.asm` +3 |
| `88` | `mov al,al` | reg + mem | `nasm/btc-imm.asm`, `nasm/btc-reg16.asm`, `nasm/btc-reg32.asm` +114 |
| `89` | `mov eax,eax` | reg + mem | `nasm/btc-imm.asm`, `nasm/btc-reg16.asm`, `nasm/btc-reg32.asm` +114 |
| `8A` | `mov al,al` | reg + mem | `nasm/btc-imm.asm`, `nasm/btc-reg16.asm`, `nasm/btc-reg32.asm` +114 |
| `8B` | `mov eax,eax` | reg + mem | `nasm/btc-imm.asm`, `nasm/btc-reg16.asm`, `nasm/btc-reg32.asm` +114 |
| `8C` | `mov eax,es` | no (skip) | `nasm/btc-imm.asm`, `nasm/btc-reg16.asm`, `nasm/btc-reg32.asm` +114 |
| `8D` | `lea eax,[eax]` | reg + mem | `nasm/lea-nop.asm`, `nasm/leave16.asm`, `nasm/leave32.asm` +34 |
| `8E` | `mov es,eax` | no (skip) | `nasm/btc-imm.asm`, `nasm/btc-reg16.asm`, `nasm/btc-reg32.asm` +114 |
| `8F /0` | `pop eax` | reg + mem | `nasm/comiss.asm`, `nasm/pop_esp.asm`, `nasm/push.asm` +17 |
| `90` | `nop` | yes | `x64/issues/single_step_tf_103_161.asm`, `x64/jit/09_masked_kinds.asm`, `x64/long/08_stack.asm` +1 |
| `91` | `xchg eax,ecx` | yes | `x64/jit/08_bit_string_misc.asm`, `x64/jit/09_masked_kinds.asm`, `x64/long/06_extend_cmov_xchg.asm` +3 |
| `92` | `xchg eax,edx` | yes | `x64/jit/08_bit_string_misc.asm`, `x64/jit/09_masked_kinds.asm`, `x64/long/06_extend_cmov_xchg.asm` +3 |
| `93` | `xchg eax,ebx` | yes | `x64/jit/08_bit_string_misc.asm`, `x64/jit/09_masked_kinds.asm`, `x64/long/06_extend_cmov_xchg.asm` +3 |
| `94` | `xchg eax,esp` | yes | `x64/jit/08_bit_string_misc.asm`, `x64/jit/09_masked_kinds.asm`, `x64/long/06_extend_cmov_xchg.asm` +3 |
| `95` | `xchg eax,ebp` | yes | `x64/jit/08_bit_string_misc.asm`, `x64/jit/09_masked_kinds.asm`, `x64/long/06_extend_cmov_xchg.asm` +3 |
| `96` | `xchg eax,esi` | yes | `x64/jit/08_bit_string_misc.asm`, `x64/jit/09_masked_kinds.asm`, `x64/long/06_extend_cmov_xchg.asm` +3 |
| `97` | `xchg eax,edi` | yes | `x64/jit/08_bit_string_misc.asm`, `x64/jit/09_masked_kinds.asm`, `x64/long/06_extend_cmov_xchg.asm` +3 |
| `98` | `cwde` | yes | `x64/jit/08_bit_string_misc.asm`, `x64/jit/09_masked_kinds.asm` |
| `99` | `cdq` | yes | `x64/jit/08_bit_string_misc.asm`, `x64/jit/15_muldiv_sse.asm` |
| `9A` | `call 0x101:0x1010101` | no (skip) | `nasm/indirect-call.asm`, `nasm/jump.asm`, `nasm/ret-imm.asm` +15 |
| `9B` | `wait` | no (skip) |  |
| `9C` | `pushf` | no (skip) | `nasm/arith-optimisations.asm`, `nasm/cmpxchg.asm`, `nasm/cmpxchg8b.asm` +4 |
| `9D` | `popf` | no (skip) | `x64/issues/single_step_tf_103_161.asm` |
| `9E` | `sahf` | yes |  |
| `9F` | `lahf` | yes |  |
| `A0` | `mov al,[0x1010101]` | yes | `nasm/btc-imm.asm`, `nasm/btc-reg16.asm`, `nasm/btc-reg32.asm` +114 |
| `A1` | `mov eax,[0x1010101]` | yes | `nasm/btc-imm.asm`, `nasm/btc-reg16.asm`, `nasm/btc-reg32.asm` +114 |
| `A2` | `mov [0x1010101],al` | yes | `nasm/btc-imm.asm`, `nasm/btc-reg16.asm`, `nasm/btc-reg32.asm` +114 |
| `A3` | `mov [0x1010101],eax` | yes | `nasm/btc-imm.asm`, `nasm/btc-reg16.asm`, `nasm/btc-reg32.asm` +114 |
| `A4` | `movsb` | yes | `x64/jit/08_bit_string_misc.asm`, `x64/long/35_rep_fast.asm`, `x64/smp/01_two_cores.asm` +6 |
| `F2A4` | `repne movsb` | yes | `x64/jit/08_bit_string_misc.asm`, `x64/long/35_rep_fast.asm`, `x64/smp/01_two_cores.asm` +6 |
| `F3A4` | `rep movsb` | yes | `x64/jit/08_bit_string_misc.asm`, `x64/long/35_rep_fast.asm`, `x64/smp/01_two_cores.asm` +6 |
| `A5` | `movsd` | yes | `nasm/double-segment-prefix.asm`, `x64/issues/sse_alignment_1356.asm`, `x64/issues/sse_alignment_matrix.asm` +3 |
| `F2A5` | `repne movsd` | yes | `nasm/double-segment-prefix.asm`, `x64/issues/sse_alignment_1356.asm`, `x64/issues/sse_alignment_matrix.asm` +3 |
| `F3A5` | `rep movsd` | yes | `nasm/double-segment-prefix.asm`, `x64/issues/sse_alignment_1356.asm`, `x64/issues/sse_alignment_matrix.asm` +3 |
| `A6` | `cmpsb` | yes | `x64/jit/08_bit_string_misc.asm` |
| `F2A6` | `repne cmpsb` | yes | `x64/jit/08_bit_string_misc.asm` |
| `F3A6` | `repe cmpsb` | yes | `x64/jit/08_bit_string_misc.asm` |
| `A7` | `cmpsd` | yes |  |
| `F2A7` | `repne cmpsd` | yes |  |
| `F3A7` | `repe cmpsd` | yes |  |
| `A8` | `test al,0x1` | yes | `nasm/arith-optimisations.asm`, `x64/issues/apic_present_247.asm`, `x64/jit/01_loops.asm` +14 |
| `A9` | `test eax,0x1010101` | yes | `nasm/arith-optimisations.asm`, `x64/issues/apic_present_247.asm`, `x64/jit/01_loops.asm` +14 |
| `AA` | `stosb` | yes | `x64/jit/02_blit.asm`, `x64/jit/08_bit_string_misc.asm`, `x64/long/35_rep_fast.asm` |
| `F2AA` | `repne stosb` | yes | `x64/jit/02_blit.asm`, `x64/jit/08_bit_string_misc.asm`, `x64/long/35_rep_fast.asm` |
| `F3AA` | `rep stosb` | yes | `x64/jit/02_blit.asm`, `x64/jit/08_bit_string_misc.asm`, `x64/long/35_rep_fast.asm` |
| `AB` | `stosd` | yes | `x64/jit/08_bit_string_misc.asm`, `x64/long/09_string64.asm`, `x64/long/31_restart_faults.asm` +4 |
| `F2AB` | `repne stosd` | yes | `x64/jit/08_bit_string_misc.asm`, `x64/long/09_string64.asm`, `x64/long/31_restart_faults.asm` +4 |
| `F3AB` | `rep stosd` | yes | `x64/jit/08_bit_string_misc.asm`, `x64/long/09_string64.asm`, `x64/long/31_restart_faults.asm` +4 |
| `AC` | `lodsb` | yes | `x64/jit/08_bit_string_misc.asm` |
| `F2AC` | `repne lodsb` | yes | `x64/jit/08_bit_string_misc.asm` |
| `F3AC` | `rep lodsb` | yes | `x64/jit/08_bit_string_misc.asm` |
| `AD` | `lodsd` | yes | `x64/jit/08_bit_string_misc.asm` |
| `F2AD` | `repne lodsd` | yes | `x64/jit/08_bit_string_misc.asm` |
| `F3AD` | `rep lodsd` | yes | `x64/jit/08_bit_string_misc.asm` |
| `AE` | `scasb` | yes |  |
| `F2AE` | `repne scasb` | yes |  |
| `F3AE` | `repe scasb` | yes |  |
| `AF` | `scasd` | yes |  |
| `F2AF` | `repne scasd` | yes |  |
| `F3AF` | `repe scasd` | yes |  |
| `B0` | `mov al,0x1` | yes | `nasm/btc-imm.asm`, `nasm/btc-reg16.asm`, `nasm/btc-reg32.asm` +114 |
| `B1` | `mov cl,0x1` | yes | `nasm/btc-imm.asm`, `nasm/btc-reg16.asm`, `nasm/btc-reg32.asm` +114 |
| `B2` | `mov dl,0x1` | yes | `nasm/btc-imm.asm`, `nasm/btc-reg16.asm`, `nasm/btc-reg32.asm` +114 |
| `B3` | `mov bl,0x1` | yes | `nasm/btc-imm.asm`, `nasm/btc-reg16.asm`, `nasm/btc-reg32.asm` +114 |
| `B4` | `mov ah,0x1` | yes | `nasm/btc-imm.asm`, `nasm/btc-reg16.asm`, `nasm/btc-reg32.asm` +114 |
| `B5` | `mov ch,0x1` | yes | `nasm/btc-imm.asm`, `nasm/btc-reg16.asm`, `nasm/btc-reg32.asm` +114 |
| `B6` | `mov dh,0x1` | yes | `nasm/btc-imm.asm`, `nasm/btc-reg16.asm`, `nasm/btc-reg32.asm` +114 |
| `B7` | `mov bh,0x1` | yes | `nasm/btc-imm.asm`, `nasm/btc-reg16.asm`, `nasm/btc-reg32.asm` +114 |
| `B8` | `mov eax,0x1010101` | yes | `nasm/btc-imm.asm`, `nasm/btc-reg16.asm`, `nasm/btc-reg32.asm` +114 |
| `B9` | `mov ecx,0x1010101` | yes | `nasm/btc-imm.asm`, `nasm/btc-reg16.asm`, `nasm/btc-reg32.asm` +114 |
| `BA` | `mov edx,0x1010101` | yes | `nasm/btc-imm.asm`, `nasm/btc-reg16.asm`, `nasm/btc-reg32.asm` +114 |
| `BB` | `mov ebx,0x1010101` | yes | `nasm/btc-imm.asm`, `nasm/btc-reg16.asm`, `nasm/btc-reg32.asm` +114 |
| `BC` | `mov esp,0x1010101` | yes | `nasm/btc-imm.asm`, `nasm/btc-reg16.asm`, `nasm/btc-reg32.asm` +114 |
| `BD` | `mov ebp,0x1010101` | yes | `nasm/btc-imm.asm`, `nasm/btc-reg16.asm`, `nasm/btc-reg32.asm` +114 |
| `BE` | `mov esi,0x1010101` | yes | `nasm/btc-imm.asm`, `nasm/btc-reg16.asm`, `nasm/btc-reg32.asm` +114 |
| `BF` | `mov edi,0x1010101` | yes | `nasm/btc-imm.asm`, `nasm/btc-reg16.asm`, `nasm/btc-reg32.asm` +114 |
| `C0 /0` | `rol al,byte 0x1` | reg + mem | `x64/jit/06_instruction_forms.asm`, `x64/jit/08_bit_string_misc.asm`, `x64/jit/09_masked_kinds.asm` +8 |
| `C0 /1` | `ror al,byte 0x1` | reg + mem | `x64/jit/06_instruction_forms.asm`, `x64/long/05_shift_bit64.asm` |
| `C0 /2` | `rcl al,byte 0x1` | reg + mem |  |
| `C0 /3` | `rcr al,byte 0x1` | reg + mem |  |
| `C0 /4` | `shl al,byte 0x1` | reg + mem | `x64/jit/06_instruction_forms.asm`, `x64/jit/07_flag_liveness.asm`, `x64/jit/09_masked_kinds.asm` +2 |
| `C0 /5` | `shr al,byte 0x1` | reg + mem | `x64/issues/mov_ss_interrupt_shadow_1631.asm`, `x64/issues/pit_irq0_rate_1614.asm`, `x64/jit/06_instruction_forms.asm` +15 |
| `C0 /6` | `sal (undocumented alias of /4)` | reg + mem |  |
| `C0 /7` | `sar al,byte 0x1` | reg + mem | `x64/jit/06_instruction_forms.asm`, `x64/jit/07_flag_liveness.asm`, `x64/long/05_shift_bit64.asm` |
| `C1 /0` | `rol eax,byte 0x1` | reg + mem | `x64/jit/06_instruction_forms.asm`, `x64/jit/08_bit_string_misc.asm`, `x64/jit/09_masked_kinds.asm` +8 |
| `C1 /1` | `ror eax,byte 0x1` | reg + mem | `x64/jit/06_instruction_forms.asm`, `x64/long/05_shift_bit64.asm` |
| `C1 /2` | `rcl eax,byte 0x1` | reg + mem |  |
| `C1 /3` | `rcr eax,byte 0x1` | reg + mem |  |
| `C1 /4` | `shl eax,byte 0x1` | reg + mem | `x64/jit/06_instruction_forms.asm`, `x64/jit/07_flag_liveness.asm`, `x64/jit/09_masked_kinds.asm` +2 |
| `C1 /5` | `shr eax,byte 0x1` | reg + mem | `x64/issues/mov_ss_interrupt_shadow_1631.asm`, `x64/issues/pit_irq0_rate_1614.asm`, `x64/jit/06_instruction_forms.asm` +15 |
| `C1 /6` | `sal (undocumented alias of /4)` | reg + mem |  |
| `C1 /7` | `sar eax,byte 0x1` | reg + mem | `x64/jit/06_instruction_forms.asm`, `x64/jit/07_flag_liveness.asm`, `x64/long/05_shift_bit64.asm` |
| `C2` | `ret 0x101` | no (skip) | `nasm/jump.asm`, `nasm/ret-imm.asm`, `x64/jit/01_loops.asm` +6 |
| `C3` | `ret` | no (skip) | `nasm/jump.asm`, `nasm/ret-imm.asm`, `x64/jit/01_loops.asm` +6 |
| `C4` | `les eax,[eax]` | no (skip) |  |
| `C5` | `lds eax,[eax]` | no (skip) |  |
| `C6 /0` | `mov al,0x1` | reg + mem | `nasm/btc-imm.asm`, `nasm/btc-reg16.asm`, `nasm/btc-reg32.asm` +114 |
| `C7 /0` | `mov eax,0x1010101` | reg + mem | `nasm/btc-imm.asm`, `nasm/btc-reg16.asm`, `nasm/btc-reg32.asm` +114 |
| `C8` | `enter 0x101,0x1` | yes |  |
| `C9` | `leave` | no (skip) | `nasm/leave16.asm`, `nasm/leave32.asm`, `x64/jit/01_loops.asm` +1 |
| `CA` | `retf 0x101` | no (skip) |  |
| `CB` | `retf` | no (skip) |  |
| `CC` | `int3` | no (skip) | `x64/long/26_idt_iretq.asm` |
| `CD` | `int 0x1` | no (skip) | `x64/long/26_idt_iretq.asm`, `x64/manual/templeos/chainload_mbr.asm` |
| `CE` | `into` | no (skip) |  |
| `CF` | `iret` | no (skip) | `x64/issues/single_step_tf_103_161.asm` |
| `D0 /0` | `rol al,1` | reg + mem | `x64/jit/06_instruction_forms.asm`, `x64/jit/08_bit_string_misc.asm`, `x64/jit/09_masked_kinds.asm` +8 |
| `D0 /1` | `ror al,1` | reg + mem | `x64/jit/06_instruction_forms.asm`, `x64/long/05_shift_bit64.asm` |
| `D0 /2` | `rcl al,1` | reg + mem |  |
| `D0 /3` | `rcr al,1` | reg + mem |  |
| `D0 /4` | `shl al,1` | reg + mem | `x64/jit/06_instruction_forms.asm`, `x64/jit/07_flag_liveness.asm`, `x64/jit/09_masked_kinds.asm` +2 |
| `D0 /5` | `shr al,1` | reg + mem | `x64/issues/mov_ss_interrupt_shadow_1631.asm`, `x64/issues/pit_irq0_rate_1614.asm`, `x64/jit/06_instruction_forms.asm` +15 |
| `D0 /6` | `sal (undocumented alias of /4)` | reg + mem |  |
| `D0 /7` | `sar al,1` | reg + mem | `x64/jit/06_instruction_forms.asm`, `x64/jit/07_flag_liveness.asm`, `x64/long/05_shift_bit64.asm` |
| `D1 /0` | `rol eax,1` | reg + mem | `x64/jit/06_instruction_forms.asm`, `x64/jit/08_bit_string_misc.asm`, `x64/jit/09_masked_kinds.asm` +8 |
| `D1 /1` | `ror eax,1` | reg + mem | `x64/jit/06_instruction_forms.asm`, `x64/long/05_shift_bit64.asm` |
| `D1 /2` | `rcl eax,1` | reg + mem |  |
| `D1 /3` | `rcr eax,1` | reg + mem |  |
| `D1 /4` | `shl eax,1` | reg + mem | `x64/jit/06_instruction_forms.asm`, `x64/jit/07_flag_liveness.asm`, `x64/jit/09_masked_kinds.asm` +2 |
| `D1 /5` | `shr eax,1` | reg + mem | `x64/issues/mov_ss_interrupt_shadow_1631.asm`, `x64/issues/pit_irq0_rate_1614.asm`, `x64/jit/06_instruction_forms.asm` +15 |
| `D1 /6` | `sal (undocumented alias of /4)` | reg + mem |  |
| `D1 /7` | `sar eax,1` | reg + mem | `x64/jit/06_instruction_forms.asm`, `x64/jit/07_flag_liveness.asm`, `x64/long/05_shift_bit64.asm` |
| `D2 /0` | `rol al,cl` | reg + mem | `x64/jit/06_instruction_forms.asm`, `x64/jit/08_bit_string_misc.asm`, `x64/jit/09_masked_kinds.asm` +8 |
| `D2 /1` | `ror al,cl` | reg + mem | `x64/jit/06_instruction_forms.asm`, `x64/long/05_shift_bit64.asm` |
| `D2 /2` | `rcl al,cl` | reg + mem |  |
| `D2 /3` | `rcr al,cl` | reg + mem |  |
| `D2 /4` | `shl al,cl` | reg + mem | `x64/jit/06_instruction_forms.asm`, `x64/jit/07_flag_liveness.asm`, `x64/jit/09_masked_kinds.asm` +2 |
| `D2 /5` | `shr al,cl` | reg + mem | `x64/issues/mov_ss_interrupt_shadow_1631.asm`, `x64/issues/pit_irq0_rate_1614.asm`, `x64/jit/06_instruction_forms.asm` +15 |
| `D2 /6` | `sal (undocumented alias of /4)` | reg + mem |  |
| `D2 /7` | `sar al,cl` | reg + mem | `x64/jit/06_instruction_forms.asm`, `x64/jit/07_flag_liveness.asm`, `x64/long/05_shift_bit64.asm` |
| `D3 /0` | `rol eax,cl` | reg + mem | `x64/jit/06_instruction_forms.asm`, `x64/jit/08_bit_string_misc.asm`, `x64/jit/09_masked_kinds.asm` +8 |
| `D3 /1` | `ror eax,cl` | reg + mem | `x64/jit/06_instruction_forms.asm`, `x64/long/05_shift_bit64.asm` |
| `D3 /2` | `rcl eax,cl` | reg + mem |  |
| `D3 /3` | `rcr eax,cl` | reg + mem |  |
| `D3 /4` | `shl eax,cl` | reg + mem | `x64/jit/06_instruction_forms.asm`, `x64/jit/07_flag_liveness.asm`, `x64/jit/09_masked_kinds.asm` +2 |
| `D3 /5` | `shr eax,cl` | reg + mem | `x64/issues/mov_ss_interrupt_shadow_1631.asm`, `x64/issues/pit_irq0_rate_1614.asm`, `x64/jit/06_instruction_forms.asm` +15 |
| `D3 /6` | `sal (undocumented alias of /4)` | reg + mem |  |
| `D3 /7` | `sar eax,cl` | reg + mem | `x64/jit/06_instruction_forms.asm`, `x64/jit/07_flag_liveness.asm`, `x64/long/05_shift_bit64.asm` |
| `D4` | `aam 0x1` | yes |  |
| `D5` | `aad 0x1` | yes |  |
| `D6` | `salc` | yes |  |
| `D7` | `xlatb` | no (skip) |  |
| `D8 /0` | `fadd st0` | reg + mem | `nasm/fst-st-empty-tag.asm`, `x64/long/14_x87_64.asm` |
| `D8 /1` | `fmul st0` | reg + mem | `nasm/fstm80.asm` |
| `D8 /2` | `fcom st0` | reg + mem |  |
| `D8 /3` | `fcomp st0` | reg + mem |  |
| `D8 /4` | `fsub st0` | reg + mem |  |
| `D8 /5` | `fsubr st0` | reg + mem |  |
| `D8 /6` | `fdiv st0` | reg + mem | `nasm/fdiv-zero.asm`, `nasm/fdiv.asm` |
| `D8 /7` | `fdivr st0` | reg + mem | `nasm/fdivr.asm` |
| `D9 /0` | `fld st0` | reg + mem | `nasm/fpu_m80.asm`, `x64/long/14_x87_64.asm` |
| `D9 /1` | `fxch st0` | reg + mem | `x64/long/14_x87_64.asm` |
| `D9 /2` | `fnop` | reg + mem |  |
| `D9 /3` | `fstp dword [eax]` | reg + mem | `nasm/fst-st-empty-tag.asm`, `nasm/fstm80.asm`, `x64/long/14_x87_64.asm` +2 |
| `D9 /4` | `fchs` | reg | `nasm/f2xm1.asm`, `nasm/fchs.asm` |
| `D9 /5` | `fld1` | reg + mem | `nasm/f2xm1.asm`, `nasm/fprem.asm`, `nasm/fprem1.asm` +7 |
| `D9 /6` | `f2xm1` | no (skip) | `nasm/f2xm1.asm` |
| `D9 /7` | `fprem` | mem | `nasm/fprem-distinct.asm`, `nasm/fprem.asm` |
| `DA /0` | `fcmovb st0` | reg + mem |  |
| `DA /1` | `fcmove st0` | reg + mem |  |
| `DA /2` | `fcmovbe st0` | reg + mem |  |
| `DA /3` | `fcmovu st0` | reg + mem |  |
| `DA /4` | `fisub dword [eax]` | reg + mem |  |
| `DA /5` | `fisubr dword [eax]` | reg + mem |  |
| `DA /6` | `fidiv dword [eax]` | reg + mem |  |
| `DA /7` | `fidivr dword [eax]` | reg + mem |  |
| `DB /0` | `fcmovnb st0` | reg + mem |  |
| `DB /1` | `fcmovne st0` | reg + mem |  |
| `DB /2` | `fcmovnbe st0` | reg + mem |  |
| `DB /3` | `fcmovnu st0` | reg + mem |  |
| `DB /4` | `fneni` | reg + mem |  |
| `DB /5` | `fucomi st0` | reg + mem |  |
| `DB /6` | `fcomi st0` | reg + mem |  |
| `DB /7` | `fstp tword [eax]` | reg + mem | `nasm/fst-st-empty-tag.asm`, `nasm/fstm80.asm`, `x64/long/14_x87_64.asm` +2 |
| `DC /0` | `fadd to st0` | reg + mem | `nasm/fst-st-empty-tag.asm`, `x64/long/14_x87_64.asm` |
| `DC /1` | `fmul to st0` | reg + mem | `nasm/fstm80.asm` |
| `DC /2` | `fcom qword [eax]` | reg + mem |  |
| `DC /3` | `fcomp qword [eax]` | reg + mem |  |
| `DC /4` | `fsubr to st0` | reg + mem |  |
| `DC /5` | `fsub to st0` | reg + mem |  |
| `DC /6` | `fdivr to st0` | reg + mem | `nasm/fdivr.asm` |
| `DC /7` | `fdiv to st0` | reg + mem | `nasm/fdiv-zero.asm`, `nasm/fdiv.asm` |
| `DD /0` | `ffree st0` | reg + mem |  |
| `DD /1` | `fisttp qword [eax]` | reg + mem |  |
| `DD /2` | `fst st0` | reg + mem | `nasm/fst-st-empty-tag.asm` |
| `DD /3` | `fstp st0` | reg + mem | `nasm/fst-st-empty-tag.asm`, `nasm/fstm80.asm`, `x64/long/14_x87_64.asm` +2 |
| `DD /4` | `fucom st0` | reg |  |
| `DD /5` | `fucomp st0` | reg + mem |  |
| `DD /6` | `fnsave [eax]` | reg |  |
| `DD /7` | `fnstsw [eax]` | reg | `x64/long/14_x87_64.asm` |
| `DE /0` | `faddp st0` | reg + mem | `x64/long/14_x87_64.asm`, `x64/smp/02_registers_survive_switches.asm`, `x64/smp/03_broadcast_ipis.asm` |
| `DE /1` | `fmulp st0` | reg + mem |  |
| `DE /2` | `ficom word [eax]` | reg + mem |  |
| `DE /3` | `ficomp word [eax]` | reg + mem |  |
| `DE /4` | `fsubrp st0` | reg + mem |  |
| `DE /5` | `fsubp st0` | reg + mem |  |
| `DE /6` | `fdivrp st0` | reg + mem |  |
| `DE /7` | `fdivp st0` | reg + mem | `x64/long/14_x87_64.asm` |
| `DF /0` | `ffreep st0` | reg + mem |  |
| `DF /1` | `fisttp word [eax]` | reg + mem |  |
| `DF /2` | `fist word [eax]` | reg + mem |  |
| `DF /3` | `fistp word [eax]` | reg + mem | `nasm/fst-st-empty-tag.asm`, `x64/long/14_x87_64.asm` |
| `DF /4` | `fnstsw ax` | no (skip) | `x64/long/14_x87_64.asm` |
| `DF /5` | `fucomip st0` | reg + mem |  |
| `DF /6` | `fcomip st0` | reg + mem | `x64/long/14_x87_64.asm` |
| `DF /7` | `fistp qword [eax]` | reg + mem | `nasm/fst-st-empty-tag.asm`, `x64/long/14_x87_64.asm` |
| `E0` | `loopne 0x3` | no (skip) |  |
| `E1` | `loope 0x3` | no (skip) |  |
| `E2` | `loop 0x3` | no (skip) | `nasm/loop.asm`, `x64/long/12_branches64.asm`, `x64/manual/boot64.asm` +1 |
| `E3` | `jecxz 0x3` | no (skip) | `nasm/jcxz.asm` |
| `E4` | `in al,0x1` | no (skip) | `x64/issues/pam_write_protect_1635.asm`, `x64/issues/pit_irq0_rate_1614.asm` |
| `E5` | `in eax,0x1` | no (skip) | `x64/issues/pam_write_protect_1635.asm`, `x64/issues/pit_irq0_rate_1614.asm` |
| `E6` | `out 0x1,al` | no (skip) | `x64/issues/mov_ss_interrupt_shadow_1631.asm`, `x64/issues/pam_write_protect_1635.asm`, `x64/issues/pit_irq0_rate_1614.asm` +2 |
| `E7` | `out 0x1,eax` | no (skip) | `x64/issues/mov_ss_interrupt_shadow_1631.asm`, `x64/issues/pam_write_protect_1635.asm`, `x64/issues/pit_irq0_rate_1614.asm` +2 |
| `E8` | `call 0x1010106` | no (skip) | `nasm/indirect-call.asm`, `nasm/jump.asm`, `nasm/ret-imm.asm` +15 |
| `E9` | `jmp 0x1010106` | no (skip) | `nasm/indirect-jump.asm`, `nasm/jump.asm`, `nasm/ret-imm.asm` +24 |
| `EA` | `jmp 0x101:0x1010101` | no (skip) | `nasm/indirect-jump.asm`, `nasm/jump.asm`, `nasm/ret-imm.asm` +24 |
| `EB` | `jmp short 0x3` | no (skip) | `nasm/indirect-jump.asm`, `nasm/jump.asm`, `nasm/ret-imm.asm` +24 |
| `EC` | `in al,dx` | no (skip) | `x64/issues/pam_write_protect_1635.asm`, `x64/issues/pit_irq0_rate_1614.asm` |
| `ED` | `in eax,dx` | no (skip) | `x64/issues/pam_write_protect_1635.asm`, `x64/issues/pit_irq0_rate_1614.asm` |
| `EE` | `out dx,al` | no (skip) | `x64/issues/mov_ss_interrupt_shadow_1631.asm`, `x64/issues/pam_write_protect_1635.asm`, `x64/issues/pit_irq0_rate_1614.asm` +2 |
| `EF` | `out dx,eax` | no (skip) | `x64/issues/mov_ss_interrupt_shadow_1631.asm`, `x64/issues/pam_write_protect_1635.asm`, `x64/issues/pit_irq0_rate_1614.asm` +2 |
| `F0` | `lock` | prefix |  |
| `F1` | `int1` | no (skip) |  |
| `F2` | `repne` | prefix |  |
| `F3` | `rep` | prefix |  |
| `F4` | `hlt` | no (skip) | `x64/issues/mov_ss_interrupt_shadow_1631.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` +8 |
| `F5` | `cmc` | yes | `x64/jit/11_interp_in_block.asm` |
| `F6 /0` | `test al,0x1` | reg + mem | `nasm/arith-optimisations.asm`, `x64/issues/apic_present_247.asm`, `x64/jit/01_loops.asm` +14 |
| `F6 /1` | `test r/m, imm (undocumented alias of /0)` | reg + mem | `nasm/arith-optimisations.asm`, `x64/issues/apic_present_247.asm`, `x64/jit/01_loops.asm` +14 |
| `F6 /2` | `not al` | reg + mem | `x64/jit/06_instruction_forms.asm`, `x64/jit/08_bit_string_misc.asm`, `x64/long/31_restart_faults.asm` |
| `F6 /3` | `neg al` | reg + mem | `x64/jit/06_instruction_forms.asm`, `x64/jit/07_flag_liveness.asm`, `x64/jit/08_bit_string_misc.asm` +1 |
| `F6 /4` | `mul al` | reg + mem | `x64/jit/11_interp_in_block.asm`, `x64/jit/15_muldiv_sse.asm`, `x64/long/04_muldiv64.asm` |
| `F6 /5` | `imul al` | reg + mem | `x64/jit/01_loops.asm`, `x64/jit/06_instruction_forms.asm`, `x64/jit/08_bit_string_misc.asm` +4 |
| `F6 /6` | `div al` | reg + mem | `x64/jit/11_interp_in_block.asm`, `x64/jit/15_muldiv_sse.asm`, `x64/long/04_muldiv64.asm` +1 |
| `F6 /7` | `idiv al` | reg + mem | `nasm/idiv16-overflow.asm`, `nasm/idiv32-overflow.asm`, `nasm/idiv8-overflow.asm` +2 |
| `F7 /0` | `test eax,0x1010101` | reg + mem | `nasm/arith-optimisations.asm`, `x64/issues/apic_present_247.asm`, `x64/jit/01_loops.asm` +14 |
| `F7 /1` | `test r/m, imm (undocumented alias of /0)` | reg + mem | `nasm/arith-optimisations.asm`, `x64/issues/apic_present_247.asm`, `x64/jit/01_loops.asm` +14 |
| `F7 /2` | `not eax` | reg + mem | `x64/jit/06_instruction_forms.asm`, `x64/jit/08_bit_string_misc.asm`, `x64/long/31_restart_faults.asm` |
| `F7 /3` | `neg eax` | reg + mem | `x64/jit/06_instruction_forms.asm`, `x64/jit/07_flag_liveness.asm`, `x64/jit/08_bit_string_misc.asm` +1 |
| `F7 /4` | `mul eax` | reg + mem | `x64/jit/11_interp_in_block.asm`, `x64/jit/15_muldiv_sse.asm`, `x64/long/04_muldiv64.asm` |
| `F7 /5` | `imul eax` | reg + mem | `x64/jit/01_loops.asm`, `x64/jit/06_instruction_forms.asm`, `x64/jit/08_bit_string_misc.asm` +4 |
| `F7 /6` | `div eax` | reg + mem | `x64/jit/11_interp_in_block.asm`, `x64/jit/15_muldiv_sse.asm`, `x64/long/04_muldiv64.asm` +1 |
| `F7 /7` | `idiv eax` | reg + mem | `nasm/idiv16-overflow.asm`, `nasm/idiv32-overflow.asm`, `nasm/idiv8-overflow.asm` +2 |
| `F8` | `clc` | yes | `x64/jit/11_interp_in_block.asm` |
| `F9` | `stc` | yes | `x64/jit/11_interp_in_block.asm`, `x64/long/03_arith64.asm`, `x64/smp/02_registers_survive_switches.asm` +1 |
| `FA` | `cli` | no (skip) | `x64/issues/mov_ss_interrupt_shadow_1631.asm`, `x64/issues/pit_irq0_rate_1614.asm`, `x64/long/29_cpuid_long_mode.asm` +10 |
| `FB` | `sti` | no (skip) | `x64/issues/mov_ss_interrupt_shadow_1631.asm`, `x64/issues/pit_irq0_rate_1614.asm`, `x64/long/32_irq_after_sti.asm` +7 |
| `FC` | `cld` | yes | `nasm/double-segment-prefix.asm`, `x64/jit/08_bit_string_misc.asm`, `x64/jit/09_masked_kinds.asm` +12 |
| `FD` | `std` | yes | `x64/jit/08_bit_string_misc.asm`, `x64/long/09_string64.asm` |
| `FE /0` | `inc al` | reg + mem | `nasm/jump.asm`, `nasm/loop.asm`, `nasm/loopz.asm` +31 |
| `FE /1` | `dec al` | reg + mem | `nasm/loopnz.asm`, `x64/issues/sse_alignment_jit_1356.asm`, `x64/issues/sse_alignment_matrix.asm` +24 |
| `FF /0` | `inc eax` | reg + mem | `nasm/jump.asm`, `nasm/loop.asm`, `nasm/loopz.asm` +31 |
| `FF /1` | `dec eax` | reg + mem | `nasm/loopnz.asm`, `x64/issues/sse_alignment_jit_1356.asm`, `x64/issues/sse_alignment_matrix.asm` +24 |
| `FF /2` | `call eax` | no (skip) | `nasm/indirect-call.asm`, `nasm/jump.asm`, `nasm/ret-imm.asm` +15 |
| `FF /3` | `call far [eax]` | no (skip) | `nasm/indirect-call.asm`, `nasm/jump.asm`, `nasm/ret-imm.asm` +15 |
| `FF /4` | `jmp eax` | no (skip) | `nasm/indirect-jump.asm`, `nasm/jump.asm`, `nasm/ret-imm.asm` +24 |
| `FF /5` | `jmp far [eax]` | no (skip) | `nasm/indirect-jump.asm`, `nasm/jump.asm`, `nasm/ret-imm.asm` +24 |
| `FF /6` | `push eax` | reg + mem | `nasm/cmpxchg.asm`, `nasm/cmpxchg8b.asm`, `nasm/fdiv-zero.asm` +21 |
| `0F00 /0` | `sldt eax` | no (skip) |  |
| `0F00 /1` | `str eax` | no (skip) | `x64/long/27_control_regs_tables.asm` |
| `0F00 /2` | `lldt ax` | no (skip) |  |
| `0F00 /3` | `ltr ax` | no (skip) | `x64/long/27_control_regs_tables.asm` |
| `0F00 /4` | `verr ax` | no (skip) |  |
| `0F00 /5` | `verw ax` | no (skip) |  |
| `0F01 /0` | `enclv` | no (skip) |  |
| `0F01 /1` | `monitor` | no (skip) |  |
| `0F01 /2` | `xgetbv` | no (skip) | `x64/stubs/xsave_header_rules.asm`, `x64/stubs/xsave_xrstor.asm` |
| `0F01 /3` | `vmrun` | no (skip) |  |
| `0F01 /4` | `smsw eax` | no (skip) | `x64/long/27_control_regs_tables.asm` |
| `0F01 /6` | `lmsw ax` | no (skip) |  |
| `0F01 /7` | `invlpg [eax]` | no (skip) | `x64/long/27_control_regs_tables.asm`, `x64/long/28_paging4level.asm`, `x64/long/31_restart_faults.asm` |
| `0F02` | `lar eax,ax` | no (skip) |  |
| `0F03` | `lsl eax,ax` | no (skip) |  |
| `0F04` | `(#UD here: reserved, or only valid with a 66/F2/F3 prefix)` | no (skip) |  |
| `0F05` | `syscall` | no (skip) | `x64/jit/09_masked_kinds.asm`, `x64/jit/10_scheduler_block.asm`, `x64/jit/11_interp_in_block.asm` +4 |
| `0F06` | `clts` | no (skip) | `x64/long/27_control_regs_tables.asm` |
| `0F07` | `sysret` | no (skip) | `x64/long/24_syscall_sysret.asm` |
| `0F08` | `invd` | no (skip) | `x64/stubs/invd_prefetchw.asm` |
| `0F09` | `wbinvd` | no (skip) | `x64/long/27_control_regs_tables.asm` |
| `0F0A` | `(#UD here: reserved, or only valid with a 66/F2/F3 prefix)` | no (skip) |  |
| `0F0B` | `ud2` | no (skip) | `x64/long/26_idt_iretq.asm` |
| `0F0C` | `(#UD here: reserved, or only valid with a 66/F2/F3 prefix)` | no (skip) |  |
| `0F0D` | `prefetch [eax]` | no (skip) |  |
| `0F0E` | `femms` | no (skip) |  |
| `0F0F` | `(#UD here: reserved, or only valid with a 66/F2/F3 prefix)` | no (skip) |  |
| `0F10` | `movups xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_1356.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/issues/sse_unaligned_ok_jit.asm` +5 |
| `F30F10` | `movss xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_1356.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` +3 |
| `660F10` | `movupd xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_1356.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/issues/sse_unaligned_ok_jit.asm` +1 |
| `F20F10` | `movsd xmm0,xmm0` | reg + mem | `nasm/double-segment-prefix.asm`, `x64/issues/sse_alignment_1356.asm`, `x64/issues/sse_alignment_matrix.asm` +3 |
| `0F11` | `movups xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_1356.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/issues/sse_unaligned_ok_jit.asm` +5 |
| `F30F11` | `movss xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_1356.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` +3 |
| `660F11` | `movupd xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_1356.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/issues/sse_unaligned_ok_jit.asm` +1 |
| `F20F11` | `movsd xmm0,xmm0` | reg + mem | `nasm/double-segment-prefix.asm`, `x64/issues/sse_alignment_1356.asm`, `x64/issues/sse_alignment_matrix.asm` +3 |
| `0F12` | `movhlps xmm0,xmm0` | reg + mem | `x64/jit/15_muldiv_sse.asm` |
| `660F12` | `o16` | reg + mem |  |
| `F20F12` | `movddup xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm`, `x64/stubs/sse3_horizontal_addsub.asm` |
| `F30F12` | `movsldup xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm`, `x64/stubs/sse3_horizontal_addsub.asm` |
| `0F13` | `movlps qword [eax],xmm0` | reg + mem | `x64/issues/sse_alignment_1356.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660F13` | `o16` | reg + mem |  |
| `0F14` | `unpcklps xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660F14` | `unpcklpd xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0F15` | `unpckhps xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660F15` | `unpckhpd xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0F16` | `movlhps xmm0,xmm0` | reg + mem |  |
| `660F16` | `o16` | reg + mem |  |
| `F30F16` | `movshdup xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm`, `x64/stubs/sse3_horizontal_addsub.asm` |
| `0F17` | `movhps qword [eax],xmm0` | reg + mem | `x64/issues/sse_alignment_1356.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660F17` | `o16` | reg + mem |  |
| `0F18` | `hint_nop0 eax` | reg + mem |  |
| `0F19` | `hint_nop8 eax` | reg + mem |  |
| `0F1A` | `(#UD here: reserved, or only valid with a 66/F2/F3 prefix)` | no (skip) |  |
| `0F1B` | `(#UD here: reserved, or only valid with a 66/F2/F3 prefix)` | no (skip) |  |
| `0F1C` | `hint_nop32 eax` | reg + mem |  |
| `0F1D` | `hint_nop40 eax` | reg + mem |  |
| `0F1E` | `hint_nop48 eax` | reg + mem |  |
| `0F1F` | `nop eax` | reg + mem | `x64/issues/single_step_tf_103_161.asm`, `x64/jit/09_masked_kinds.asm`, `x64/long/08_stack.asm` +1 |
| `0F20` | `mov eax,cr0` | no (skip) | `nasm/btc-imm.asm`, `nasm/btc-reg16.asm`, `nasm/btc-reg32.asm` +114 |
| `0F21` | `mov eax,dr0` | no (skip) | `nasm/btc-imm.asm`, `nasm/btc-reg16.asm`, `nasm/btc-reg32.asm` +114 |
| `0F22` | `mov cr0,eax` | no (skip) | `nasm/btc-imm.asm`, `nasm/btc-reg16.asm`, `nasm/btc-reg32.asm` +114 |
| `0F23` | `mov dr0,eax` | no (skip) | `nasm/btc-imm.asm`, `nasm/btc-reg16.asm`, `nasm/btc-reg32.asm` +114 |
| `0F24` | `(#UD here: reserved, or only valid with a 66/F2/F3 prefix)` | no (skip) |  |
| `0F25` | `(#UD here: reserved, or only valid with a 66/F2/F3 prefix)` | no (skip) |  |
| `0F26` | `(#UD here: reserved, or only valid with a 66/F2/F3 prefix)` | no (skip) |  |
| `0F27` | `(#UD here: reserved, or only valid with a 66/F2/F3 prefix)` | no (skip) |  |
| `0F28` | `movaps xmm0,xmm0` | reg + mem | `nasm/cvttps2pi.asm`, `nasm/movaps.asm`, `x64/issues/sse_alignment_1356.asm` +7 |
| `660F28` | `movapd xmm0,xmm0` | reg + mem | `nasm/movntpd.asm`, `nasm/movntps.asm`, `x64/issues/sse_alignment_1356.asm` +5 |
| `0F29` | `movaps xmm0,xmm0` | reg + mem | `nasm/cvttps2pi.asm`, `nasm/movaps.asm`, `x64/issues/sse_alignment_1356.asm` +7 |
| `660F29` | `movapd xmm0,xmm0` | reg + mem | `nasm/movntpd.asm`, `nasm/movntps.asm`, `x64/issues/sse_alignment_1356.asm` +5 |
| `0F2A` | `cvtpi2ps xmm0,mm0` | reg + mem | `nasm/cvtpi2ps.asm` |
| `660F2A` | `cvtpi2pd xmm0,mm0` | reg + mem |  |
| `F20F2A` | `cvtsi2sd xmm0,eax` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/11_sse_r64.asm`, `x64/long/13_sse_xmm8_15.asm` +1 |
| `F30F2A` | `cvtsi2ss xmm0,eax` | reg + mem | `x64/long/11_sse_r64.asm` |
| `0F2B` | `movntps oword [eax],xmm0` | reg + mem | `nasm/movntps.asm`, `x64/issues/sse_alignment_1356.asm`, `x64/issues/sse_alignment_matrix.asm` +1 |
| `660F2B` | `o16` | reg + mem |  |
| `0F2C` | `cvttps2pi mm0,xmm0` | reg + mem | `nasm/cvttps2pi.asm` |
| `660F2C` | `cvttpd2pi mm0,xmm0` | reg + mem |  |
| `F20F2C` | `cvttsd2si eax,xmm0` | reg + mem | `x64/long/11_sse_r64.asm`, `x64/long/13_sse_xmm8_15.asm` |
| `F30F2C` | `cvttss2si eax,xmm0` | reg + mem | `x64/long/11_sse_r64.asm`, `x64/stubs/mxcsr_rounding_mode.asm` |
| `0F2D` | `cvtps2pi mm0,xmm0` | reg + mem | `nasm/cvtps2pi.asm` |
| `660F2D` | `cvtpd2pi mm0,xmm0` | reg + mem |  |
| `F20F2D` | `cvtsd2si eax,xmm0` | reg + mem | `x64/stubs/mxcsr_rounding_mode.asm` |
| `F30F2D` | `cvtss2si eax,xmm0` | reg + mem | `x64/stubs/mxcsr_rounding_mode.asm` |
| `0F2E` | `ucomiss xmm0,xmm0` | reg + mem | `nasm/comiss.asm`, `nasm/ucomiss.asm` |
| `660F2E` | `ucomisd xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0F2F` | `comiss xmm0,xmm0` | reg + mem | `nasm/comiss.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660F2F` | `comisd xmm0,xmm0` | reg + mem |  |
| `0F30` | `wrmsr` | no (skip) | `x64/long/22_msrs.asm`, `x64/long/23_fsgs_swapgs.asm`, `x64/long/24_syscall_sysret.asm` +5 |
| `0F31` | `rdtsc` | no (skip) | `x64/jit/15_muldiv_sse.asm` |
| `0F32` | `rdmsr` | no (skip) | `x64/issues/apic_present_247.asm`, `x64/long/00_enter_long_mode.asm`, `x64/long/22_msrs.asm` +4 |
| `0F33` | `rdpmc` | no (skip) |  |
| `0F34` | `sysenter` | no (skip) |  |
| `0F35` | `sysexit` | no (skip) |  |
| `0F36` | `(#UD here: reserved, or only valid with a 66/F2/F3 prefix)` | no (skip) |  |
| `0F37` | `getsec` | no (skip) |  |
| `0F38` | `(three-byte escape: SSSE3, SSE4)` | no (skip) |  |
| `0F39` | `dmint` | no (skip) |  |
| `0F3A` | `(three-byte escape: SSSE3, SSE4)` | no (skip) |  |
| `0F3B` | `(#UD here: reserved, or only valid with a 66/F2/F3 prefix)` | no (skip) |  |
| `0F3C` | `cpu_write` | no (skip) |  |
| `0F3D` | `cpu_read` | no (skip) |  |
| `0F3E` | `(#UD here: reserved, or only valid with a 66/F2/F3 prefix)` | no (skip) |  |
| `0F3F` | `(#UD here: reserved, or only valid with a 66/F2/F3 prefix)` | no (skip) |  |
| `0F40` | `cmovo eax,eax` | reg + mem |  |
| `0F41` | `cmovno eax,eax` | reg + mem |  |
| `0F42` | `cmovc eax,eax` | reg + mem |  |
| `0F43` | `cmovnc eax,eax` | reg + mem |  |
| `0F44` | `cmovz eax,eax` | reg + mem |  |
| `0F45` | `cmovnz eax,eax` | reg + mem | `x64/jit/09_masked_kinds.asm` |
| `0F46` | `cmovna eax,eax` | reg + mem |  |
| `0F47` | `cmova eax,eax` | reg + mem |  |
| `0F48` | `cmovs eax,eax` | reg + mem |  |
| `0F49` | `cmovns eax,eax` | reg + mem |  |
| `0F4A` | `cmovpe eax,eax` | reg + mem |  |
| `0F4B` | `cmovpo eax,eax` | reg + mem |  |
| `0F4C` | `cmovl eax,eax` | reg + mem |  |
| `0F4D` | `cmovnl eax,eax` | reg + mem |  |
| `0F4E` | `cmovng eax,eax` | reg + mem |  |
| `0F4F` | `cmovg eax,eax` | reg + mem |  |
| `0F50` | `movmskps eax,xmm0` | reg + mem | `x64/long/13_sse_xmm8_15.asm` |
| `660F50` | `movmskpd eax,xmm0` | reg + mem |  |
| `0F51` | `sqrtps xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660F51` | `sqrtpd xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `F20F51` | `sqrtsd xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `F30F51` | `sqrtss xmm0,xmm0` | reg + mem |  |
| `0F52` | `rsqrtps xmm0,xmm0` | no (skip) | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `F30F52` | `rsqrtss xmm0,xmm0` | no (skip) |  |
| `0F53` | `rcpps xmm0,xmm0` | no (skip) | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `F30F53` | `rcpss xmm0,xmm0` | no (skip) |  |
| `0F54` | `andps xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/jit/15_muldiv_sse.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660F54` | `andpd xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0F55` | `andnps xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/jit/15_muldiv_sse.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660F55` | `andnpd xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0F56` | `orps xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/jit/15_muldiv_sse.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660F56` | `orpd xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0F57` | `xorps xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/jit/15_muldiv_sse.asm`, `x64/long/13_sse_xmm8_15.asm` +1 |
| `660F57` | `xorpd xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0F58` | `addps xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_1356.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/13_sse_xmm8_15.asm` +1 |
| `660F58` | `addpd xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `F20F58` | `addsd xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `F30F58` | `addss xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0F59` | `mulps xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660F59` | `mulpd xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/13_sse_xmm8_15.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `F20F59` | `mulsd xmm0,xmm0` | reg + mem |  |
| `F30F59` | `mulss xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm`, `x64/stubs/mxcsr_flush_denormals.asm` |
| `0F5A` | `cvtps2pd xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660F5A` | `cvtpd2ps xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `F20F5A` | `cvtsd2ss xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `F30F5A` | `cvtss2sd xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0F5B` | `cvtdq2ps xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_arith_1356.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660F5B` | `cvtps2dq xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm`, `x64/stubs/mxcsr_rounding_mode.asm` |
| `F30F5B` | `cvttps2dq xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0F5C` | `subps xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660F5C` | `subpd xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `F20F5C` | `subsd xmm0,xmm0` | reg + mem |  |
| `F30F5C` | `subss xmm0,xmm0` | reg + mem |  |
| `0F5D` | `minps xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660F5D` | `minpd xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `F20F5D` | `minsd xmm0,xmm0` | reg + mem |  |
| `F30F5D` | `minss xmm0,xmm0` | reg + mem |  |
| `0F5E` | `divps xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660F5E` | `divpd xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `F20F5E` | `divsd xmm0,xmm0` | reg + mem |  |
| `F30F5E` | `divss xmm0,xmm0` | reg + mem | `x64/stubs/mxcsr_exceptions.asm`, `x64/stubs/mxcsr_unmasked_xm.asm` |
| `0F5F` | `maxps xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660F5F` | `maxpd xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `F20F5F` | `maxsd xmm0,xmm0` | reg + mem |  |
| `F30F5F` | `maxss xmm0,xmm0` | reg + mem |  |
| `660F60` | `punpcklbw xmm0,xmm0` | reg + mem | `nasm/punpcklbw.asm`, `x64/issues/sse_alignment_arith_1356.asm`, `x64/issues/sse_alignment_matrix.asm` +1 |
| `0F60` | `punpcklbw mm0,mm0` | reg + mem | `nasm/punpcklbw.asm`, `x64/issues/sse_alignment_arith_1356.asm`, `x64/issues/sse_alignment_matrix.asm` +1 |
| `660F61` | `punpcklwd xmm0,xmm0` | reg + mem | `nasm/punpcklwd.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0F61` | `punpcklwd mm0,mm0` | reg + mem | `nasm/punpcklwd.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660F62` | `punpckldq xmm0,xmm0` | reg + mem | `nasm/punpckldq.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/13_sse_xmm8_15.asm` +1 |
| `0F62` | `punpckldq mm0,mm0` | reg + mem | `nasm/punpckldq.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/13_sse_xmm8_15.asm` +1 |
| `660F63` | `packsswb xmm0,xmm0` | reg + mem | `nasm/packsswb.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0F63` | `packsswb mm0,mm0` | reg + mem | `nasm/packsswb.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660F64` | `pcmpgtb xmm0,xmm0` | reg + mem | `nasm/pcmpgtb.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0F64` | `pcmpgtb mm0,mm0` | reg + mem | `nasm/pcmpgtb.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660F65` | `pcmpgtw xmm0,xmm0` | reg + mem | `nasm/pcmpgtw.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0F65` | `pcmpgtw mm0,mm0` | reg + mem | `nasm/pcmpgtw.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660F66` | `pcmpgtd xmm0,xmm0` | reg + mem | `nasm/pcmpgtd.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0F66` | `pcmpgtd mm0,mm0` | reg + mem | `nasm/pcmpgtd.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660F67` | `packuswb xmm0,xmm0` | reg + mem | `nasm/packuswb.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0F67` | `packuswb mm0,mm0` | reg + mem | `nasm/packuswb.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660F68` | `punpckhbw xmm0,xmm0` | reg + mem | `nasm/punpckhbw.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0F68` | `punpckhbw mm0,mm0` | reg + mem | `nasm/punpckhbw.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660F69` | `punpckhwd xmm0,xmm0` | reg + mem | `nasm/punpckhwd.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0F69` | `punpckhwd mm0,mm0` | reg + mem | `nasm/punpckhwd.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660F6A` | `punpckhdq xmm0,xmm0` | reg + mem | `nasm/punpckhdq.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0F6A` | `punpckhdq mm0,mm0` | reg + mem | `nasm/punpckhdq.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660F6B` | `packssdw xmm0,xmm0` | reg + mem | `nasm/packssdw.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0F6B` | `packssdw mm0,mm0` | reg + mem | `nasm/packssdw.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660F6C` | `punpcklqdq xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/jit/15_muldiv_sse.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0F6C` | `(#UD here: reserved, or only valid with a 66/F2/F3 prefix)` | reg + mem |  |
| `660F6D` | `punpckhqdq xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0F6D` | `(#UD here: reserved, or only valid with a 66/F2/F3 prefix)` | reg + mem |  |
| `660F6E` | `movd xmm0,eax` | reg + mem | `nasm/comiss.asm`, `nasm/movd.asm`, `nasm/pmuludq.asm` +11 |
| `0F6E` | `movd mm0,eax` | reg + mem | `nasm/comiss.asm`, `nasm/movd.asm`, `nasm/pmuludq.asm` +11 |
| `F30F6F` | `movdqu xmm0,xmm0` | reg + mem | `nasm/maskmovdqu.asm`, `nasm/shufps-edgecase.asm`, `x64/issues/sse_alignment_1356.asm` +5 |
| `660F6F` | `movdqa xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_1356.asm`, `x64/issues/sse_alignment_jit_1356.asm`, `x64/issues/sse_alignment_matrix.asm` +10 |
| `0F6F` | `movq mm0,mm0` | reg + mem | `nasm/maskmovq.asm`, `nasm/movq.asm`, `nasm/packssdw.asm` +50 |
| `0F70` | `pshufw mm0,mm0,0x1` | reg + mem | `nasm/pshufw.asm` |
| `660F70` | `pshufd xmm0,xmm0,0x1` | reg + mem | `nasm/cvtpi2ps.asm`, `nasm/pmuludq.asm`, `x64/issues/sse_alignment_arith_1356.asm` +5 |
| `F20F70` | `pshuflw xmm0,xmm0,0x1` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `F30F70` | `pshufhw xmm0,xmm0,0x1` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0F71 /2` | `psrlw mm0,0x1` | reg + mem | `nasm/psrlw.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660F71 /2` | `psrlw xmm0,0x1` | reg + mem | `nasm/psrlw.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0F71 /4` | `psraw mm0,0x1` | reg + mem | `nasm/psraw.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660F71 /4` | `psraw xmm0,0x1` | reg + mem | `nasm/psraw.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0F71 /6` | `psllw mm0,0x1` | reg + mem | `nasm/psllw.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660F71 /6` | `psllw xmm0,0x1` | reg + mem | `nasm/psllw.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0F72 /2` | `psrld mm0,0x1` | reg + mem | `nasm/psrld.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/13_sse_xmm8_15.asm` +1 |
| `660F72 /2` | `psrld xmm0,0x1` | reg + mem | `nasm/psrld.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/13_sse_xmm8_15.asm` +1 |
| `0F72 /4` | `psrad mm0,0x1` | reg + mem | `nasm/psrad.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660F72 /4` | `psrad xmm0,0x1` | reg + mem | `nasm/psrad.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0F72 /6` | `pslld mm0,0x1` | reg + mem | `nasm/pslld.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660F72 /6` | `pslld xmm0,0x1` | reg + mem | `nasm/pslld.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0F73 /2` | `psrlq mm0,0x1` | reg + mem | `nasm/psrlq.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660F73 /2` | `psrlq xmm0,0x1` | reg + mem | `nasm/psrlq.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660F73 /3` | `psrldq xmm0,0x1` | reg + mem | `x64/long/11_sse_r64.asm` |
| `0F73 /6` | `psllq mm0,0x1` | reg + mem | `nasm/psllq.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660F73 /6` | `psllq xmm0,0x1` | reg + mem | `nasm/psllq.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660F73 /7` | `pslldq xmm0,0x1` | reg + mem |  |
| `0F74` | `pcmpeqb mm0,mm0` | reg + mem | `nasm/pcmpeqb.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660F74` | `pcmpeqb xmm0,xmm0` | reg + mem | `nasm/pcmpeqb.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0F75` | `pcmpeqw mm0,mm0` | reg + mem | `nasm/pcmpeqw.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660F75` | `pcmpeqw xmm0,xmm0` | reg + mem | `nasm/pcmpeqw.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0F76` | `pcmpeqd mm0,mm0` | reg + mem | `nasm/pcmpeqd.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/11_sse_r64.asm` +3 |
| `660F76` | `pcmpeqd xmm0,xmm0` | reg + mem | `nasm/pcmpeqd.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/11_sse_r64.asm` +3 |
| `0F77` | `emms` | no (skip) | `nasm/emms.asm`, `x64/stubs/ssse3_pshufb_palignr.asm` |
| `0F78` | `(#UD here: reserved, or only valid with a 66/F2/F3 prefix)` | no (skip) |  |
| `0F79` | `(#UD here: reserved, or only valid with a 66/F2/F3 prefix)` | no (skip) |  |
| `0F7A` | `(#UD here: reserved, or only valid with a 66/F2/F3 prefix)` | no (skip) |  |
| `0F7B` | `(#UD here: reserved, or only valid with a 66/F2/F3 prefix)` | no (skip) |  |
| `660F7C` | `haddpd xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm`, `x64/stubs/sse3_horizontal_addsub.asm` |
| `F20F7C` | `haddps xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm`, `x64/stubs/sse3_horizontal_addsub.asm` |
| `0F7C` | `svts tword [eax]` | no (skip) |  |
| `660F7D` | `hsubpd xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm`, `x64/stubs/sse3_horizontal_addsub.asm` |
| `F20F7D` | `hsubps xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm`, `x64/stubs/sse3_horizontal_addsub.asm` |
| `0F7D` | `rsts tword [eax]` | no (skip) |  |
| `0F7E` | `movd eax,mm0` | reg + mem | `nasm/comiss.asm`, `nasm/movd.asm`, `nasm/pmuludq.asm` +11 |
| `660F7E` | `movd eax,xmm0` | reg + mem | `nasm/comiss.asm`, `nasm/movd.asm`, `nasm/pmuludq.asm` +11 |
| `F30F7E` | `movq xmm0,xmm0` | reg + mem | `nasm/maskmovq.asm`, `nasm/movq.asm`, `nasm/packssdw.asm` +50 |
| `0F7F` | `movq mm0,mm0` | reg + mem | `nasm/maskmovq.asm`, `nasm/movq.asm`, `nasm/packssdw.asm` +50 |
| `660F7F` | `movdqa xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_1356.asm`, `x64/issues/sse_alignment_jit_1356.asm`, `x64/issues/sse_alignment_matrix.asm` +10 |
| `F30F7F` | `movdqu xmm0,xmm0` | reg + mem | `nasm/maskmovdqu.asm`, `nasm/shufps-edgecase.asm`, `x64/issues/sse_alignment_1356.asm` +5 |
| `0F80` | `jo near 0x1010107` | no (skip) |  |
| `0F81` | `jno near 0x1010107` | no (skip) |  |
| `0F82` | `jc near 0x1010107` | no (skip) | `x64/jit/10_scheduler_block.asm`, `x64/long/25_compat_mode.asm`, `x64/manual/templeos/chainload_mbr.asm` +3 |
| `0F83` | `jnc near 0x1010107` | no (skip) | `x64/jit/08_bit_string_misc.asm`, `x64/stubs/sse41_sse42_basics.asm` |
| `0F84` | `jz near 0x1010107` | no (skip) | `x64/issues/sse_alignment_jit_1356.asm`, `x64/issues/sse_unaligned_ok_jit.asm`, `x64/jit/01_loops.asm` +4 |
| `0F85` | `jnz near 0x1010107` | no (skip) | `x64/issues/apic_present_247.asm`, `x64/issues/sse_alignment_jit_1356.asm`, `x64/issues/sse_alignment_matrix.asm` +30 |
| `0F86` | `jna near 0x1010107` | no (skip) |  |
| `0F87` | `ja near 0x1010107` | no (skip) |  |
| `0F88` | `js near 0x1010107` | no (skip) |  |
| `0F89` | `jns near 0x1010107` | no (skip) | `x64/jit/08_bit_string_misc.asm` |
| `0F8A` | `jpe near 0x1010107` | no (skip) |  |
| `0F8B` | `jpo near 0x1010107` | no (skip) |  |
| `0F8C` | `jl near 0x1010107` | no (skip) | `x64/jit/07_flag_liveness.asm` |
| `0F8D` | `jnl near 0x1010107` | no (skip) |  |
| `0F8E` | `jng near 0x1010107` | no (skip) |  |
| `0F8F` | `jg near 0x1010107` | no (skip) | `x64/jit/10_scheduler_block.asm` |
| `0F90` | `seto al` | reg + mem | `x64/jit/03_flags_conditions.asm`, `x64/jit/08_bit_string_misc.asm` |
| `0F91` | `setno al` | reg + mem | `x64/jit/03_flags_conditions.asm` |
| `0F92` | `setc al` | reg + mem | `x64/jit/07_flag_liveness.asm`, `x64/jit/08_bit_string_misc.asm`, `x64/jit/09_masked_kinds.asm` |
| `0F93` | `setnc al` | reg + mem |  |
| `0F94` | `setz al` | reg + mem | `x64/jit/07_flag_liveness.asm`, `x64/jit/08_bit_string_misc.asm` |
| `0F95` | `setnz al` | reg + mem |  |
| `0F96` | `setna al` | reg + mem |  |
| `0F97` | `seta al` | reg + mem | `x64/jit/03_flags_conditions.asm` |
| `0F98` | `sets al` | reg + mem | `x64/jit/03_flags_conditions.asm`, `x64/jit/07_flag_liveness.asm` |
| `0F99` | `setns al` | reg + mem | `x64/jit/03_flags_conditions.asm` |
| `0F9A` | `setpe al` | reg + mem |  |
| `0F9B` | `setpo al` | reg + mem |  |
| `0F9C` | `setl al` | reg + mem | `x64/jit/03_flags_conditions.asm` |
| `0F9D` | `setnl al` | reg + mem |  |
| `0F9E` | `setng al` | reg + mem |  |
| `0F9F` | `setg al` | reg + mem | `x64/jit/03_flags_conditions.asm`, `x64/jit/06_instruction_forms.asm` |
| `0FA0` | `push fs` | yes | `nasm/cmpxchg.asm`, `nasm/cmpxchg8b.asm`, `nasm/fdiv-zero.asm` +21 |
| `0FA1` | `pop fs` | no (skip) | `nasm/comiss.asm`, `nasm/pop_esp.asm`, `nasm/push.asm` +17 |
| `0FA2` | `cpuid` | no (skip) | `x64/issues/apic_present_247.asm`, `x64/long/29_cpuid_long_mode.asm`, `x64/smp/01_two_cores.asm` +2 |
| `0FA3` | `bt eax,eax` | reg | `x64/jit/08_bit_string_misc.asm`, `x64/jit/09_masked_kinds.asm`, `x64/jit/10_scheduler_block.asm` +1 |
| `0FA4` | `shld eax,eax,0x1` | reg + mem | `x64/long/05_shift_bit64.asm` |
| `0FA5` | `shld eax,eax,cl` | reg + mem | `x64/long/05_shift_bit64.asm` |
| `0FA6` | `(#UD here: reserved, or only valid with a 66/F2/F3 prefix)` | no (skip) |  |
| `0FA7` | `(#UD here: reserved, or only valid with a 66/F2/F3 prefix)` | no (skip) |  |
| `0FA8` | `push gs` | yes | `nasm/cmpxchg.asm`, `nasm/cmpxchg8b.asm`, `nasm/fdiv-zero.asm` +21 |
| `0FA9` | `pop gs` | no (skip) | `nasm/comiss.asm`, `nasm/pop_esp.asm`, `nasm/push.asm` +17 |
| `0FAA` | `rsm` | no (skip) |  |
| `0FAB` | `bts eax,eax` | reg | `nasm/bts-imm.asm`, `nasm/bts-reg16.asm`, `nasm/bts-reg32.asm` +6 |
| `0FAC` | `shrd eax,eax,0x1` | reg + mem | `x64/long/05_shift_bit64.asm` |
| `0FAD` | `shrd eax,eax,cl` | reg + mem | `x64/long/05_shift_bit64.asm` |
| `0FAE /0` | `fxsave [eax]` | no (skip) | `x64/long/14_x87_64.asm` |
| `0FAE /1` | `fxrstor [eax]` | no (skip) | `x64/long/14_x87_64.asm` |
| `0FAE /2` | `ldmxcsr dword [eax]` | no (skip) | `nasm/cvtps2pi.asm`, `x64/stubs/mxcsr_exceptions.asm`, `x64/stubs/mxcsr_flush_denormals.asm` +2 |
| `0FAE /3` | `stmxcsr dword [eax]` | no (skip) | `nasm/comiss.asm`, `nasm/cvtps2pi.asm`, `nasm/ucomiss.asm` +2 |
| `0FAE /4` | `ptwrite eax` | no (skip) |  |
| `0FAE /5` | `lfence` | no (skip) | `x64/jit/15_muldiv_sse.asm` |
| `0FAE /6` | `mfence` | no (skip) | `x64/jit/15_muldiv_sse.asm` |
| `0FAE /7` | `sfence` | no (skip) | `x64/jit/15_muldiv_sse.asm` |
| `0FAF` | `imul eax,eax` | reg + mem | `x64/jit/01_loops.asm`, `x64/jit/06_instruction_forms.asm`, `x64/jit/08_bit_string_misc.asm` +4 |
| `0FB0` | `cmpxchg al,al` | reg + mem | `nasm/cmpxchg-setbe.asm`, `nasm/cmpxchg.asm`, `x64/jit/11_interp_in_block.asm` +5 |
| `0FB1` | `cmpxchg eax,eax` | reg + mem | `nasm/cmpxchg-setbe.asm`, `nasm/cmpxchg.asm`, `x64/jit/11_interp_in_block.asm` +5 |
| `0FB2` | `lss eax,[eax]` | no (skip) |  |
| `0FB3` | `btr eax,eax` | reg | `nasm/btr-imm.asm`, `nasm/btr-reg16.asm`, `nasm/btr-reg32.asm` +3 |
| `0FB4` | `lfs eax,[eax]` | no (skip) |  |
| `0FB5` | `lgs eax,[eax]` | no (skip) |  |
| `0FB6` | `movzx eax,al` | reg + mem | `x64/jit/02_blit.asm`, `x64/jit/06_instruction_forms.asm`, `x64/jit/07_flag_liveness.asm` +7 |
| `0FB7` | `movzx eax,ax` | reg + mem | `x64/jit/02_blit.asm`, `x64/jit/06_instruction_forms.asm`, `x64/jit/07_flag_liveness.asm` +7 |
| `F30FB8` | `popcnt eax,eax` | reg + mem | `x64/long/05_shift_bit64.asm` |
| `0FB8` | `jmpe (#UD; F3 0F B8 is popcnt)` | reg + mem |  |
| `0FB9` | `(#UD here: reserved, or only valid with a 66/F2/F3 prefix)` | yes |  |
| `0FBA /4` | `bt eax,byte 0x1` | reg + mem | `x64/jit/08_bit_string_misc.asm`, `x64/jit/09_masked_kinds.asm`, `x64/jit/10_scheduler_block.asm` +1 |
| `0FBA /5` | `bts eax,byte 0x1` | reg + mem | `nasm/bts-imm.asm`, `nasm/bts-reg16.asm`, `nasm/bts-reg32.asm` +6 |
| `0FBA /6` | `btr eax,byte 0x1` | reg + mem | `nasm/btr-imm.asm`, `nasm/btr-reg16.asm`, `nasm/btr-reg32.asm` +3 |
| `0FBA /7` | `btc eax,byte 0x1` | reg + mem | `nasm/btc-imm.asm`, `nasm/btc-reg16.asm`, `nasm/btc-reg32.asm` +3 |
| `0FBB` | `btc eax,eax` | reg | `nasm/btc-imm.asm`, `nasm/btc-reg16.asm`, `nasm/btc-reg32.asm` +3 |
| `0FBC` | `bsf eax,eax` | reg + mem | `x64/jit/11_interp_in_block.asm`, `x64/long/05_shift_bit64.asm` |
| `0FBD` | `bsr eax,eax` | reg + mem | `x64/jit/11_interp_in_block.asm`, `x64/long/05_shift_bit64.asm` |
| `0FBE` | `movsx eax,al` | reg + mem | `x64/jit/06_instruction_forms.asm`, `x64/jit/15_muldiv_sse.asm`, `x64/long/06_extend_cmov_xchg.asm` |
| `0FBF` | `movsx eax,ax` | reg + mem | `x64/jit/06_instruction_forms.asm`, `x64/jit/15_muldiv_sse.asm`, `x64/long/06_extend_cmov_xchg.asm` |
| `0FC0` | `xadd al,al` | reg + mem | `x64/jit/11_interp_in_block.asm`, `x64/jit/15_muldiv_sse.asm`, `x64/long/06_extend_cmov_xchg.asm` +3 |
| `0FC1` | `xadd eax,eax` | reg + mem | `x64/jit/11_interp_in_block.asm`, `x64/jit/15_muldiv_sse.asm`, `x64/long/06_extend_cmov_xchg.asm` +3 |
| `0FC2` | `cmpltps xmm0,xmm0` | reg + mem |  |
| `660FC2` | `cmpltpd xmm0,xmm0` | reg + mem |  |
| `F20FC2` | `cmpltsd xmm0,xmm0` | reg + mem |  |
| `F30FC2` | `cmpltss xmm0,xmm0` | reg + mem |  |
| `0FC3` | `movnti [eax],eax` | reg + mem |  |
| `0FC4` | `pinsrw mm0,ax,0x1` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660FC4` | `pinsrw xmm0,ax,0x1` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0FC5` | `pextrw eax,mm0,0x1` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660FC5` | `pextrw eax,xmm0,0x1` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0FC6` | `shufps xmm0,xmm0,byte 0x1` | reg + mem | `nasm/shufps-edgecase.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660FC6` | `shufpd xmm0,xmm0,byte 0x1` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0FC7 /1` | `cmpxchg8b qword [eax]` | reg + mem | `nasm/cmpxchg8b.asm` |
| `0FC7 /6` | `rdrand eax` | no (skip) |  |
| `0FC8` | `bswap eax` | yes | `x64/jit/13_bswap.asm`, `x64/jit/14_page_straddle.asm`, `x64/long/05_shift_bit64.asm` |
| `0FC9` | `bswap ecx` | yes | `x64/jit/13_bswap.asm`, `x64/jit/14_page_straddle.asm`, `x64/long/05_shift_bit64.asm` |
| `0FCA` | `bswap edx` | yes | `x64/jit/13_bswap.asm`, `x64/jit/14_page_straddle.asm`, `x64/long/05_shift_bit64.asm` |
| `0FCB` | `bswap ebx` | yes | `x64/jit/13_bswap.asm`, `x64/jit/14_page_straddle.asm`, `x64/long/05_shift_bit64.asm` |
| `0FCC` | `bswap esp` | yes | `x64/jit/13_bswap.asm`, `x64/jit/14_page_straddle.asm`, `x64/long/05_shift_bit64.asm` |
| `0FCD` | `bswap ebp` | yes | `x64/jit/13_bswap.asm`, `x64/jit/14_page_straddle.asm`, `x64/long/05_shift_bit64.asm` |
| `0FCE` | `bswap esi` | yes | `x64/jit/13_bswap.asm`, `x64/jit/14_page_straddle.asm`, `x64/long/05_shift_bit64.asm` |
| `0FCF` | `bswap edi` | yes | `x64/jit/13_bswap.asm`, `x64/jit/14_page_straddle.asm`, `x64/long/05_shift_bit64.asm` |
| `0FD0` | `(#UD here: reserved, or only valid with a 66/F2/F3 prefix)` | no (skip) |  |
| `660FD0` | `addsubpd xmm0,xmm0` | no (skip) | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm`, `x64/stubs/sse3_horizontal_addsub.asm` |
| `F20FD0` | `addsubps xmm0,xmm0` | no (skip) | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm`, `x64/stubs/sse3_horizontal_addsub.asm` |
| `0FD1` | `psrlw mm0,mm0` | reg + mem | `nasm/psrlw.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660FD1` | `psrlw xmm0,xmm0` | reg + mem | `nasm/psrlw.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0FD2` | `psrld mm0,mm0` | reg + mem | `nasm/psrld.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/13_sse_xmm8_15.asm` +1 |
| `660FD2` | `psrld xmm0,xmm0` | reg + mem | `nasm/psrld.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/13_sse_xmm8_15.asm` +1 |
| `0FD3` | `psrlq mm0,mm0` | reg + mem | `nasm/psrlq.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660FD3` | `psrlq xmm0,xmm0` | reg + mem | `nasm/psrlq.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0FD4` | `paddq mm0,mm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660FD4` | `paddq xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0FD5` | `pmullw mm0,mm0` | reg + mem | `nasm/pmullw.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660FD5` | `pmullw xmm0,xmm0` | reg + mem | `nasm/pmullw.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660FD6` | `movq xmm0,xmm0` | reg + mem | `nasm/maskmovq.asm`, `nasm/movq.asm`, `nasm/packssdw.asm` +50 |
| `F20FD6` | `movdq2q mm0,xmm0` | reg + mem |  |
| `F30FD6` | `movq2dq xmm0,mm0` | reg + mem |  |
| `0FD6` | `(#UD here: reserved, or only valid with a 66/F2/F3 prefix)` | reg + mem |  |
| `0FD7` | `pmovmskb eax,mm0` | reg + mem |  |
| `660FD7` | `pmovmskb eax,xmm0` | reg + mem |  |
| `0FD8` | `psubusb mm0,mm0` | reg + mem | `nasm/psubusb.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660FD8` | `psubusb xmm0,xmm0` | reg + mem | `nasm/psubusb.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0FD9` | `psubusw mm0,mm0` | reg + mem | `nasm/psubusw.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660FD9` | `psubusw xmm0,xmm0` | reg + mem | `nasm/psubusw.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0FDA` | `pminub mm0,mm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660FDA` | `pminub xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0FDB` | `pand mm0,mm0` | reg + mem | `nasm/pand.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/jit/15_muldiv_sse.asm` +1 |
| `660FDB` | `pand xmm0,xmm0` | reg + mem | `nasm/pand.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/jit/15_muldiv_sse.asm` +1 |
| `0FDC` | `paddusb mm0,mm0` | reg + mem | `nasm/paddusb.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660FDC` | `paddusb xmm0,xmm0` | reg + mem | `nasm/paddusb.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0FDD` | `paddusw mm0,mm0` | reg + mem | `nasm/paddusw.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660FDD` | `paddusw xmm0,xmm0` | reg + mem | `nasm/paddusw.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0FDE` | `pmaxub mm0,mm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660FDE` | `pmaxub xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0FDF` | `pandn mm0,mm0` | reg + mem | `nasm/pandn.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/jit/15_muldiv_sse.asm` +1 |
| `660FDF` | `pandn xmm0,xmm0` | reg + mem | `nasm/pandn.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/jit/15_muldiv_sse.asm` +1 |
| `0FE0` | `pavgb mm0,mm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660FE0` | `pavgb xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0FE1` | `psraw mm0,mm0` | reg + mem | `nasm/psraw.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660FE1` | `psraw xmm0,xmm0` | reg + mem | `nasm/psraw.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0FE2` | `psrad mm0,mm0` | reg + mem | `nasm/psrad.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660FE2` | `psrad xmm0,xmm0` | reg + mem | `nasm/psrad.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0FE3` | `pavgw mm0,mm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660FE3` | `pavgw xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0FE4` | `pmulhuw mm0,mm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660FE4` | `pmulhuw xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0FE5` | `pmulhw mm0,mm0` | reg + mem | `nasm/pmulhw.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660FE5` | `pmulhw xmm0,xmm0` | reg + mem | `nasm/pmulhw.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660FE6` | `cvttpd2dq xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `F20FE6` | `cvtpd2dq xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `F30FE6` | `cvtdq2pd xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0FE6` | `(#UD here: reserved, or only valid with a 66/F2/F3 prefix)` | reg + mem |  |
| `0FE7` | `movntq [eax],mm0` | reg + mem |  |
| `660FE7` | `o16` | reg + mem |  |
| `0FE8` | `psubsb mm0,mm0` | reg + mem | `nasm/psubsb.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660FE8` | `psubsb xmm0,xmm0` | reg + mem | `nasm/psubsb.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0FE9` | `psubsw mm0,mm0` | reg + mem | `nasm/psubsw.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660FE9` | `psubsw xmm0,xmm0` | reg + mem | `nasm/psubsw.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0FEA` | `pminsw mm0,mm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660FEA` | `pminsw xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0FEB` | `por mm0,mm0` | reg + mem | `nasm/por.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/jit/15_muldiv_sse.asm` +1 |
| `660FEB` | `por xmm0,xmm0` | reg + mem | `nasm/por.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/jit/15_muldiv_sse.asm` +1 |
| `0FEC` | `paddsb mm0,mm0` | reg + mem | `nasm/paddsb.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660FEC` | `paddsb xmm0,xmm0` | reg + mem | `nasm/paddsb.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0FED` | `paddsw mm0,mm0` | reg + mem | `nasm/paddsw.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660FED` | `paddsw xmm0,xmm0` | reg + mem | `nasm/paddsw.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0FEE` | `pmaxsw mm0,mm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660FEE` | `pmaxsw xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0FEF` | `pxor mm0,mm0` | reg + mem | `nasm/pxor.asm`, `x64/issues/sse_alignment_1356.asm`, `x64/issues/sse_alignment_matrix.asm` +9 |
| `660FEF` | `pxor xmm0,xmm0` | reg + mem | `nasm/pxor.asm`, `x64/issues/sse_alignment_1356.asm`, `x64/issues/sse_alignment_matrix.asm` +9 |
| `0FF0` | `(#UD here: reserved, or only valid with a 66/F2/F3 prefix)` | no (skip) |  |
| `F20FF0` | `repne` | no (skip) |  |
| `0FF1` | `psllw mm0,mm0` | reg + mem | `nasm/psllw.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660FF1` | `psllw xmm0,xmm0` | reg + mem | `nasm/psllw.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0FF2` | `pslld mm0,mm0` | reg + mem | `nasm/pslld.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660FF2` | `pslld xmm0,xmm0` | reg + mem | `nasm/pslld.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0FF3` | `psllq mm0,mm0` | reg + mem | `nasm/psllq.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660FF3` | `psllq xmm0,xmm0` | reg + mem | `nasm/psllq.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0FF4` | `pmuludq mm0,mm0` | reg + mem | `nasm/pmuludq.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660FF4` | `pmuludq xmm0,xmm0` | reg + mem | `nasm/pmuludq.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0FF5` | `pmaddwd mm0,mm0` | reg + mem | `nasm/pmaddwd.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660FF5` | `pmaddwd xmm0,xmm0` | reg + mem | `nasm/pmaddwd.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0FF6` | `psadbw mm0,mm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660FF6` | `psadbw xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0FF7` | `maskmovq mm0,mm0` | no (skip) | `nasm/maskmovq.asm` |
| `660FF7` | `maskmovdqu xmm0,xmm0` | no (skip) | `nasm/maskmovdqu.asm` |
| `0FF8` | `psubb mm0,mm0` | reg + mem | `nasm/psubb.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660FF8` | `psubb xmm0,xmm0` | reg + mem | `nasm/psubb.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0FF9` | `psubw mm0,mm0` | reg + mem | `nasm/psubw.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660FF9` | `psubw xmm0,xmm0` | reg + mem | `nasm/psubw.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0FFA` | `psubd mm0,mm0` | reg + mem | `nasm/psubd.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660FFA` | `psubd xmm0,xmm0` | reg + mem | `nasm/psubd.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0FFB` | `psubq mm0,mm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660FFB` | `psubq xmm0,xmm0` | reg + mem | `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0FFC` | `paddb mm0,mm0` | reg + mem | `nasm/paddb.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660FFC` | `paddb xmm0,xmm0` | reg + mem | `nasm/paddb.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0FFD` | `paddw mm0,mm0` | reg + mem | `nasm/paddw.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `660FFD` | `paddw xmm0,xmm0` | reg + mem | `nasm/paddw.asm`, `x64/issues/sse_alignment_matrix.asm`, `x64/long/34_sse_alignment_matrix.asm` |
| `0FFE` | `paddd mm0,mm0` | reg + mem | `nasm/paddd.asm`, `x64/issues/sse_alignment_arith_1356.asm`, `x64/issues/sse_alignment_jit_1356.asm` +3 |
| `660FFE` | `paddd xmm0,xmm0` | reg + mem | `nasm/paddd.asm`, `x64/issues/sse_alignment_arith_1356.asm`, `x64/issues/sse_alignment_jit_1356.asm` +3 |
| `0FFF` | `ud0` | yes |  |
<!-- opcodes:end -->
