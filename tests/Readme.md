# Tests

Run a suite with its `make` target in the root directory. The instruction tests have a Readme per architecture,
each with a table of every opcode and the tests that cover it (`node tools/opcode_coverage.mjs` regenerates them):

| Architecture | Core | Instruction tests |
|---|---|---|
| x86 (32-bit) | `v86.wasm` | [nasm/Readme.md](nasm/Readme.md): generated tests for every encoding, run on the host CPU and in v86 |
| x86_64 | `v64.wasm` | [x64/Readme.md](x64/Readme.md): long mode, jit64, formerly stubbed instructions, several cores, a differential fuzzer |
| ARM (AArch64) | `v86-arm64.wasm` | [a64/Readme.md](a64/Readme.md): Linux and UEFI boot tests, the core's unit tests, the encoding classes |

The other suites, roughly from the most useful:

| Suite | What it tests | `make` |
|---|---|---|
| [qemu](qemu/) | QEMU's `test-i386`: many CPU features, compared with a run in QEMU | `qemutests` |
| [kvm-unit-tests](kvm-unit-tests/) | tests from the KVM project: task switches, real mode, PAT, NX | `kvm-unit-test` |
| [full](full/) | boots several OSes and checks that they come up | `tests` |
| [jit-paging](jit-paging/) | the JIT with paging and self-modifying code | `jitpagingtests` |
| [api](api/) | the V86 API | `api-tests` |
| [devices](devices/) | devices | `devices-test` |
| [expect](expect/) | the code the JIT generates: each asm file and the wasm it must produce | `expect-tests` |
| [rust](rust/) | Rust unit test helpers | `rust-test` |
| `*.mjs` here | machines and devices: UEFI (`uefi.mjs`), Q35 (`q35.mjs`), TPM (`tpm.mjs`), USB (`usb.mjs`), graphics adapters (`graphics_adapter.mjs`), virtio-gpu 2D (`virtio_gpu.mjs`) and 3D on WebGPU (`virtio_gpu_3d.mjs`) | `node tests/<file>` |

Most tests respect these environment variables:

- `TEST_RELEASE_BUILD=1`: the release build (libv86.mjs and the release cores) instead of the source files and the
  debug cores
- `MAX_PARALLEL_TESTS=n`: at most n tests at once (default: the number of host cores or less)
- `TEST_NAME="…"`: only the matching tests (expect, full, nasm, x64)
