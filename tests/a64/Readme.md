# ARM (AArch64) instruction tests

The tests of the ARM core (`build/v86-arm64.wasm`, the crate in [`a64/`](../../a64/), from the v64 project;
`cpu_type: "arm64"` with `libv86-arm.js`, see [docs/arm64.md](../../docs/arm64.md)).

| Test | What it checks | Run |
|---|---|---|
| `boot.mjs [cores]` | Alpine aarch64 (the official virt ISO's kernel and initramfs, the ISO on virtio-blk) to a login, then `uname -m`, the core count, the CPU features (`aes`) and `apk`: the whole instruction set as Linux and its userland use it, interpreted and with the page tier (the JIT) | `make a64-boot-tests` |
| `boot.mjs` with `A64_PARALLEL=1` | the same with cores in vCPU workers (`v86-arm64-parallel.wasm`) | `make a64-boot-tests` |
| `uefi.mjs [cores]` | the same ISO booted through UEFI (edk2 in the flash) and GRUB: the firmware's code at EL1, then Linux | `make a64-boot-tests` |
| `net.mjs` | virtio-net on the virt machine | `make a64-boot-tests` |
| `cargo test` in `a64/` | unit tests of the core: the decoder's bit masks and words, the AES tables, the bus, the JIT runtime | `cd a64 && cargo test` |
| `examples/arm-v86.html` | the machine through the V86 API in a browser (`?uefi=1`) | by hand |

Needs `build/v86-arm64.wasm` (`make build/v86-arm64.wasm`) and the ISO (`A64_ISO`, else downloaded to
`build/a64/alpine-virt-aarch64.iso`).

## Instruction-level tests: not ported yet

There are no per-instruction tests for ARM in this repository yet. v64 checks the same core instruction by
instruction against references, and these suites are the next to port (each needs a cross toolchain and QEMU,
or llvm-mc, on the host):

| v64 suite | Reference | Covers |
|---|---|---|
| `decode_check.mjs` | llvm-mc | every listed encoding class: decoded or UNDEFINED as llvm-mc says |
| `integer_oracle.mjs` | QEMU | integer, branch, load/store, exclusive instructions (2,400 random cases) |
| `simd_oracle.mjs`, `fp_oracle.mjs`, `float_ref.mjs` | QEMU, an Apple M1 Pro | AdvSIMD and floating point, every FPCR rounding / flush / default-NaN mode |
| `crypto_oracle.mjs` | QEMU | AES, PMULL, SHA-1, SHA-2 |
| `native_oracle.mjs`, `id_registers` | real hardware | ID registers, implementation-defined choices |
| `aarch32_oracle.mjs` | QEMU | AArch32 at EL0 (32-bit processes) |

## Every encoding class and instruction

Regenerate these tables with `node tools/opcode_coverage.mjs`.

<!-- opcodes:begin (tools/opcode_coverage.mjs) -->
### Encoding classes

The 49 classes of the Arm ARM's A64 encoding index (profile v86-aarch64-v1, DDI 0487 L.b C4.1), as v64's `tests/a64/encodings.json` records them for this core. Status: **implemented**: decoded and executed; **undef**: UNDEFINED (EC 0x00): unallocated, or a feature profile v1 does not declare; **hint**: a HINT the profile does not implement: executes as NOP; **id-space**: an unallocated ID register: reads 0 at EL1, UNDEF at EL0. "v64 tests" are the suites that check the class in the v64 project; they are not in this repository yet (below).

| Group | Class | Status | Feature | v64 tests |
|---|---|---|---|---|
| Reserved | UDF and the rest of op0 = 0000 | undef |  | `decode_check` |
| SME, SVE | op0 = 1xxx 000x, 0010 | undef | FEAT_SME, FEAT_SVE | `decode_check` |
| Data Processing -- Immediate | PC-rel. addressing | implemented |  | `decode_check`, `integer_oracle` |
| Data Processing -- Immediate | Add/subtract (immediate) | implemented |  | `decode_check`, `integer_oracle` |
| Data Processing -- Immediate | Add/subtract (immediate, with tags) | undef | FEAT_MTE | `decode_check` |
| Data Processing -- Immediate | Min/max (immediate) | undef | FEAT_CSSC | `decode_check` |
| Data Processing -- Immediate | Logical (immediate) | implemented |  | `decode_check`, `integer_oracle` |
| Data Processing -- Immediate | Move wide (immediate) | implemented |  | `decode_check`, `integer_oracle` |
| Data Processing -- Immediate | Bitfield | implemented |  | `decode_check`, `integer_oracle` |
| Data Processing -- Immediate | Extract | implemented |  | `decode_check`, `integer_oracle` |
| Branches, Exception Generating and System | Conditional branch (immediate) | implemented |  | `decode_check`, `integer_oracle` |
| Branches, Exception Generating and System | Exception generation | implemented |  | `decode_check`, `platform_probe` |
| Branches, Exception Generating and System | System instructions with register argument | undef | FEAT_WFxT | `decode_check` |
| Branches, Exception Generating and System | Hints | implemented |  | `decode_check` |
| Branches, Exception Generating and System | Barriers | implemented |  | `decode_check` |
| Branches, Exception Generating and System | PSTATE | implemented |  | `decode_check`, `integer_oracle` |
| Branches, Exception Generating and System | System instructions | implemented |  | `decode_check`, `kvm-unit-tests` |
| Branches, Exception Generating and System | System register move | implemented |  | `decode_check`, `integer_oracle`, `id_registers` |
| Branches, Exception Generating and System | System register move (ID space, op0=3 op1=0 CRn=0 CRm=1-7 unallocated) | id-space |  | `id_registers`, `cpu_contract` |
| Branches, Exception Generating and System | System pair instructions, 128-bit system register move | undef | FEAT_SYSINSTR128, FEAT_SYSREG128 | `decode_check` |
| Branches, Exception Generating and System | Unconditional branch (register) | implemented |  | `decode_check`, `integer_oracle` |
| Branches, Exception Generating and System | Unconditional branch (immediate) | implemented |  | `decode_check`, `integer_oracle` |
| Branches, Exception Generating and System | Compare and branch (immediate) | implemented |  | `decode_check`, `integer_oracle` |
| Branches, Exception Generating and System | Test and branch (immediate) | implemented |  | `decode_check`, `integer_oracle` |
| Loads and Stores | Compare and swap pair, LSE atomics, LDAPR, LDAPUR/STLUR, RCPC3, LSE128, GCS, MOPS, LD64B | undef | FEAT_LSE, FEAT_LRCPC, FEAT_LRCPC2, FEAT_LRCPC3, FEAT_LSE128, FEAT_GCS, FEAT_MOPS, FEAT_LS64 | `decode_check` |
| Loads and Stores | Load/store exclusive pair, register; load-acquire/store-release | implemented |  | `decode_check`, `integer_oracle` |
| Loads and Stores | Load register (literal) | implemented |  | `decode_check`, `integer_oracle`, `simd_oracle` |
| Loads and Stores | Load/store no-allocate pair (offset) | implemented |  | `decode_check`, `integer_oracle`, `simd_oracle` |
| Loads and Stores | Load/store register pair (post-indexed, offset, pre-indexed) | implemented |  | `decode_check`, `integer_oracle`, `simd_oracle` |
| Loads and Stores | Load/store register (unscaled immediate, immediate post-indexed, unprivileged, immediate pre-indexed, register offset, unsigned immediate) | implemented |  | `decode_check`, `integer_oracle`, `simd_oracle` |
| Loads and Stores | Atomic memory operations, load/store register (pac) | undef | FEAT_LSE, FEAT_PAuth | `decode_check` |
| Loads and Stores | AdvSIMD load/store multiple structures (and post-indexed) | implemented |  | `decode_check`, `simd_oracle`, `native_oracle` |
| Loads and Stores | AdvSIMD load/store single structure (and post-indexed) | implemented |  | `decode_check`, `simd_oracle`, `native_oracle` |
| Loads and Stores | Load/store memory tags | undef | FEAT_MTE | `decode_check` |
| Data Processing -- Register | Data-processing (2 source) | implemented |  | `decode_check`, `integer_oracle` |
| Data Processing -- Register | Data-processing (1 source) | implemented |  | `decode_check`, `integer_oracle` |
| Data Processing -- Register | Logical (shifted register) | implemented |  | `decode_check`, `integer_oracle` |
| Data Processing -- Register | Add/subtract (shifted register), (extended register), (with carry) | implemented |  | `decode_check`, `integer_oracle` |
| Data Processing -- Register | Conditional compare (register), (immediate); conditional select | implemented |  | `decode_check`, `integer_oracle` |
| Data Processing -- Register | Data-processing (3 source) | implemented |  | `decode_check`, `integer_oracle` |
| Data Processing -- Scalar FP and AdvSIMD | Cryptographic AES | implemented | FEAT_AES | `decode_check`, `crypto_oracle`, `native_oracle` |
| Data Processing -- Scalar FP and AdvSIMD | Cryptographic three-register SHA, two-register SHA | implemented | FEAT_SHA1, FEAT_SHA256 | `decode_check`, `crypto_oracle`, `native_oracle` |
| Data Processing -- Scalar FP and AdvSIMD | Cryptographic three-register imm2, three-register SHA512, four-register, two-register SHA512, XAR | undef | FEAT_SM3, FEAT_SHA512, FEAT_SHA3 | `decode_check` |
| Data Processing -- Scalar FP and AdvSIMD | AdvSIMD scalar copy, scalar pairwise, scalar three different, scalar three same, scalar two-register miscellaneous, scalar shift by immediate, scalar x indexed element | implemented |  | `decode_check`, `simd_oracle`, `native_oracle` |
| Data Processing -- Scalar FP and AdvSIMD | AdvSIMD table lookup, permute, extract, copy, three different, three same, two-register miscellaneous, across lanes, modified immediate, shift by immediate, vector x indexed element | implemented |  | `decode_check`, `simd_oracle`, `native_oracle` |
| Data Processing -- Scalar FP and AdvSIMD | AdvSIMD (scalar) three same FP16, two-register miscellaneous FP16, three same extra, three-register extension; FMOV (vector, half); FP16 forms of the FP classes | undef | FEAT_FP16, FEAT_RDM, FEAT_DotProd, FEAT_FCMA, FEAT_FHM, FEAT_I8MM, FEAT_BF16 | `decode_check` |
| Data Processing -- Scalar FP and AdvSIMD | Conversion between floating-point and fixed-point, and integer | implemented |  | `decode_check`, `fp_oracle`, `native_oracle`, `float_ref` |
| Data Processing -- Scalar FP and AdvSIMD | Floating-point data-processing (1 source) | implemented |  | `decode_check`, `fp_oracle`, `native_oracle`, `float_ref` |
| Data Processing -- Scalar FP and AdvSIMD | Floating-point compare, immediate, conditional compare, data-processing (2 source), conditional select, data-processing (3 source) | implemented |  | `decode_check`, `fp_oracle`, `native_oracle`, `float_ref` |

### The decoder's instructions

The 61 instruction kinds of `a64/src/aarch64/decode.rs` (`enum Op`; the FP, SIMD and AArch32 instructions are decoded in `simd_decode.rs`, `fp.rs` and `a32*.rs`).

| Op | Group | Forms |
|---|---|---|
| `Undef` |  |  |
| `Adr` | data processing (immediate) |  |
| `Adrp` | data processing (immediate) |  |
| `AddSubImm` | data processing (immediate) | ADD/ADDS/SUB/SUBS (immediate): sub = 1 for SUB, s = 1 sets flags |
| `LogicalImm` | data processing (immediate) | AND/ORR/EOR/ANDS (immediate): sub = opc, imm = the bitmask |
| `MoveWide` | data processing (immediate) | MOVN (sub 0), MOVZ (2), MOVK (3): imm = imm16, amount = hw * 16 |
| `Bitfield` | data processing (immediate) | SBFM (sub 0), BFM (1), UBFM (2): amount = immr, imm = imms |
| `Extr` | data processing (immediate) | EXTR: amount = lsb |
| `BCond` | branches, exceptions, system | B.cond: cond, imm = offset |
| `Branch` | branches, exceptions, system | B (sub 0) or BL (sub 1): imm = offset |
| `BranchReg` | branches, exceptions, system | BR (sub 0), BLR (1), RET (2): rn |
| `CompareBranch` | branches, exceptions, system | CBZ (sub 0), CBNZ (1): rd = Rt, imm = offset |
| `TestBranch` | branches, exceptions, system | TBZ (sub 0), TBNZ (1): rd = Rt, amount = bit, imm = offset |
| `ExceptionGen` | branches, exceptions, system | SVC, HVC, SMC, BRK, HLT: sub = 1..5, imm = imm16 |
| `Eret` | branches, exceptions, system |  |
| `Hint` | branches, exceptions, system | a hint: imm = CRm:op2 (0 NOP, 1 YIELD, 2 WFE, 3 WFI, 4 SEV, 5 SEVL, others NOP) |
| `Clrex` | branches, exceptions, system |  |
| `Barrier` | branches, exceptions, system | DSB (sub 0), DMB (1), ISB (2) |
| `MsrImm` | branches, exceptions, system | MSR (immediate) to PSTATE: sub 0 SPSel, 1 DAIFSet, 2 DAIFClr, 3 PAN; imm = CRm |
| `Sys` | branches, exceptions, system | SYS (cache and TLB maintenance, AT): imm = op1:CRn:CRm:op2 (14 bits), rd = Rt |
| `Mrs` | branches, exceptions, system | MRS: imm = the system register's op0:op1:CRn:CRm:op2 (16 bits), rd = Rt |
| `Msr` | branches, exceptions, system | MSR (register): as Mrs |
| `LoadLiteral` | loads and stores | LDR (literal): sub 0 W, 1 X, 2 LDRSW, 3 PRFM; imm = offset; rd = Rt |
| `LoadStore` | loads and stores | a single register: see LdSt |
| `LoadStorePair` | loads and stores | a pair: see LdSt |
| `Exclusive` | loads and stores | LDXR/STXR/LDAXR/STLXR/LDXP/STXP/LDAXP/STLXP: rd = Rt, ra = Rt2, rm = Rs, size; sub bit 0 load, bit 1 pair, bit 2 acquire/release |
| `LoadAcquireStoreRelease` | loads and stores | LDAR/STLR (and B, H): rd = Rt, size; sub = 1 for loads |
| `LogicalShifted` | data processing (register) | AND/BIC/ORR/ORN/EOR/EON/ANDS/BICS (shifted register): sub = opc << 1 \| N, shift, amount |
| `AddSubShifted` | data processing (register) | ADD/ADDS/SUB/SUBS (shifted register): sub = 1 for SUB, s, shift, amount |
| `AddSubExtended` | data processing (register) | ADD/ADDS/SUB/SUBS (extended register): sub, s, shift = option, amount = imm3 |
| `AddSubCarry` | data processing (register) | ADC/ADCS/SBC/SBCS: sub = 1 for SBC, s |
| `CondCompare` | data processing (register) | CCMN (sub 0) / CCMP (1): cond, imm = nzcv; amount = 1 for the immediate form (rm = imm5) |
| `CondSelect` | data processing (register) | CSEL (sub 0), CSINC (1), CSINV (2), CSNEG (3): cond |
| `DataProc3` | data processing (register) | MADD/MSUB/SMADDL/SMSUBL/SMULH/UMADDL/UMSUBL/UMULH: sub = op31 << 1 \| o0 |
| `DataProc2` | data processing (register) | UDIV, SDIV, LSLV, LSRV, ASRV, RORV, CRC32*: sub = opcode |
| `DataProc1` | data processing (register) | RBIT, REV16, REV32, REV, CLZ, CLS: sub = opcode |
| `FpDp1` | the word tells them apart) |  |
| `FpDp2` | the word tells them apart) |  |
| `FpDp3` | the word tells them apart) |  |
| `FpCompare` | the word tells them apart) |  |
| `FpCondCompare` | the word tells them apart) |  |
| `FpCondSelect` | the word tells them apart) |  |
| `FpImm` | the word tells them apart) |  |
| `FpFixed` | the word tells them apart) |  |
| `FpInt` | the word tells them apart) |  |
| `SimdThreeSame` | the word tells them apart) |  |
| `SimdThreeDiff` | the word tells them apart) |  |
| `SimdTwoMisc` | the word tells them apart) |  |
| `SimdAcross` | the word tells them apart) |  |
| `SimdPairwise` | the word tells them apart) |  |
| `SimdCopy` | the word tells them apart) |  |
| `SimdModImm` | the word tells them apart) |  |
| `SimdShiftImm` | the word tells them apart) |  |
| `SimdIndexed` | the word tells them apart) |  |
| `SimdTable` | the word tells them apart) |  |
| `SimdPermute` | the word tells them apart) |  |
| `SimdExtract` | the word tells them apart) |  |
| `CryptoAes` | the word tells them apart) |  |
| `CryptoSha3` | the word tells them apart) |  |
| `CryptoSha2` | the word tells them apart) |  |
| `SimdStructure` | the word tells them apart) | LD1-LD4/ST1-ST4 and LD1R-LD4R (simd_decode::structure): imm = the word |
<!-- opcodes:end -->
