# Plan (continue from here)

## How to work (read first)

- **Don't be verbose.** Only message the user when a task on this list is done and verified. Use one short line,
  e.g. "UEFI boots Alpine aarch64 to a login (tests/a64/uefi.mjs)". Don't report waits, retries, failures or
  intermediate steps. Explain more only if the user asks.
- Repository `EdHeller/v86`, branch `Vibe-Claude`. Commit and push every change. Don't open pull requests unless
  asked. End commit messages with the attribution lines from the session reminder.
- v64, the reference project, is at `/home/user/speedyhkjournalist/v64` (clone `speedyHKjournalist/v64` if it is
  missing). Its docs cover each area: `docs/aarch64.md`, `docs/arm64-virt-android16-plan.md`,
  `docs/android-images-plan.md`, `docs/gpu-devices.md`, `docs/gfxstream-plan.md`, `docs/glbridge.md`, `docs/uefi.md`.
- When you finish an item, check it off here, add a dated line under "Done", and commit.

## State (2026-11-04)

- **32-bit x86 (`v86.wasm`):** SSE3/SSSE3/SSE4 in every mode; about 5-20 % slower than upstream v86 (task 1).
- **x86_64 (`v64.wasm`, `libv64.js`):** faster than v64: Alpine x86_64 to login in about 40 s against v64's 44.5 s.
- **ARM (`v86-arm64.wasm`, `libv86-arm.js`):** Alpine aarch64 by direct kernel boot and through UEFI; virtio-gpu 2D
  on virtio-mmio.
- **Machines and devices:** UEFI (OVMF) and Q35, TPM 2.0, USB, HDA, virtio-pci, graphics adapters (bochs_vga,
  vmware_svga, virtio_gpu with virgl 3D on WebGPU), multicore (cooperative and in workers), the JS split per
  architecture.
- **Packages** (git-ignored, in the root; recorded in `docs/releases.md`): `v86-release.tar.gz` (the website files),
  `v86-cube.tar.xz` and `v86-cube-x86.tar.xz` (kmscube on virgl). Send files to the user as attachments; links
  don't reach them. Keep each file under 500 MB (xz).

## Measuring

- **32-bit against upstream:** upstream's master (315b2b0) builds in a worktree at `/root/upstream-v86`;
  `REPO=<tree> node <scratchpad>/bench86b.mjs` boots 32-bit Alpine and prints the time to login and the best of three
  runs of a 20000-step shell loop and an 8 MiB gzip round trip. Alternate the two trees.
- **x86_64 boot time:** `/root/jitwork/bench3.sh 3` (the scripts and ISOs are in `/root/jitwork`; rebuild them from
  `docs/next-session.md` after a container reset).
- **Tests:** `node tests/x64/run.js` (89 ok), `JIT64=0 node tests/x64/fuzz.js --seed 6 --count 1000`,
  `node tests/uefi.mjs`, `node tests/a64/boot.mjs`, `node tests/a64/virtio_gpu.mjs`, `node tests/virtio_gpu.mjs`.
- Run benchmarks one at a time with nothing else running (no builds). Differences under about 3 % are noise.

## Tasks, in order

The user's order (2026-11-04): ⬜ open, ✅ done. Finished items move to "Done" below.

1. ⬜ **32-bit speed back to upstream's** (in progress). Measured 2026-11-04 (best of three, two boots each): the
   shell loop 2.2-2.3 s against upstream's 1.7-1.9 s, gzip 5.9 s against 5.2-5.8 s, login 44 s against 41-43 s.
   Bisect between 315b2b0 and HEAD on the loop time, fix the cause, check ours is as fast or faster.
2. ⬜ **Full instruction coverage on x86: 16-bit, 32-bit and x86_64.** List every instruction of the Intel SDM opcode
   maps (one-byte, 0F, 0F 38, 0F 3A, x87, SSE through SSE4.2, system) for real mode, 16- and 32-bit protected mode,
   vm86 and long mode; mark each implemented, stubbed or missing (`tools/opcode_coverage.mjs`, the test Readmes);
   implement the gaps; test each against the host CPU or QEMU, interpreted and compiled (the nasm tests in all three
   code sizes, the x64 fuzzer widened to SSE and x87). Done when nothing is missing and `make nasmtests`,
   `node tests/x64/run.js` and the fuzzers are clean.
3. ⬜ **v64 parity on x86.** See "Plan: v64 parity" (devices, firmware, memory above 4 GiB, disk formats and
   persistence, the VM manager). The ARM half of v64 parity is task 7.
4. ⬜ **Speed ideas from copy/v86#924** (copy's comment of 2023-09-19). Profile first as copy suggests: the Firefox
   profiler (https://profiler.firefox.com/docs/), `make debug-with-profiler` (stats at the bottom of debug.html), the
   generated wasm and x86 code (`docs/profiling.md`).
   - ⬜ a. Register and flag analysis, to omit flags and fuse cmp+jcc (copy/v86#466). jit64 drops dead flags
     (`flag_liveness`); the 32-bit JIT does not yet. Needs task 2's fuzzer.
   - ⬜ b. Function boundaries instead of one big br_table; call/ret as wasm calls. jit64 has one function per page;
     call/ret still return to Rust; the 32-bit JIT is as upstream.
   - ⬜ c. VGA memory writes through the memory fast path (copy/v86#301).
   - ⬜ d. The boot CPU in a Web Worker (copy/v86#534, #219); the cores after the first already run in workers.
   - ⬜ e. Stack accesses: translate ESP to physical once per block when the access size is known.
5. ⬜ **The lazy-compile cost:** V8 compiles each jit64 module with Liftoff on its first call, 1-3 s of the main
   thread per boot. Smaller modules, or compiling off the main thread.
6. ⬜ **virtio-gpu, the rest.**
   - ⬜ a. The ARM machine through `new V86({ cpu_type: "arm64", graphics_adapter: "virtio_gpu" })` (today only
     `examples/alpine-arm64.html?gpu=1`).
   - ⬜ b. virgl 3D on arm64 (host visible memory as device memory on the ARM bus).
   - ⬜ c. Venus (Vulkan) through v64's gfxstream path (`docs/gfxstream-plan.md` there).
   - It stays optional: without WebGPU the device is 2D, and without the option nothing changes.
7. ⬜ **ARM: PCIe, then Android.** Wire `src/arm/virt/pcie.js` in (`virtio_transport: "pci"`), boot Alpine on
   virtio-blk-pci by direct kernel boot and through UEFI (`make a64-boot-tests`), virtio-gpu-pci; then the rest of
   v64's ARM devices and Android, see "Plan: Android".
8. ⬜ **Apple: Darwin up through Mac OS X 10.14 Mojave on x86; the iPhone 4 on ARM.** See "Plan: Apple".
9. ⬜ **Buildroot: an x86_64 build and multicore** (`tools/buildroot/build.sh`): a 64-bit kernel and userland next to
   the 32-bit one; ACPI, SMP with the local and I/O APIC, HPET and TSC; the matching v86 options in the run
   instructions (`acpi`, `cpu_cores`, `cpu_type: "x86_64"`, the worker build). Check: `nproc` shows the cores.

## Plan: Android

Follows v64's `docs/arm64-virt-android16-plan.md` and `docs/android-images-plan.md`. v64 runs Android 16 on the
`virt` board with Cuttlefish's arm64-only phone images, so its path, not `ranchu`, is the one to port first.

- A1. Device groundwork: PCIe and virtio-pci (task 7), virtio-console with several ports, virtio-input (keyboard,
  tablet, multi-touch), virtio-vsock (adbd), more than 2 GiB of RAM (the core has `extended.rs`; the machine
  refuses more than 2 GiB today).
- A2. The GKI kernel (android16-6.12) with Alpine's initramfs on our `virt` board: the kernel's own features
  (39-bit VA, SW_TTBR0_PAN, pseudo-NMIs, KASLR) and the virtio modules load. This is v64's P10.6.
- A3. Images. Port v64's `src/android/` (boot.img, vendor_boot, bootconfig, AVB, GPT, super/lp, sparse, erofs,
  ext4) and `tools/a64_android.mjs`, which fetches the pinned public Cuttlefish build (BP4A.251205.006) from
  ci.android.com. If that download is blocked here, ask the user once for the images or a URL.
- A4. Boot to `sys.boot_completed=1` with adb over vsock, with SwiftShader (software) graphics.
- A5. Display and input: virtio-gpu 2D scanout, touch through virtio-input; then GPU acceleration through Venus on
  WebGPU (task 6c).
- A6. Sound, Wi-Fi over virtio-net, snapshots, a reboot. Check: v64's A9 scenario (launcher, an APK, input,
  reboot).
- Later: the `ranchu` board (goldfish devices, the Android Emulator's images) as a second Android machine.

## Plan: Apple

What exists for which CPU decides where each step runs:

- Mac OS X 10.0-10.3 ran on PowerPC only, 10.4-10.5 on PowerPC and Intel, and 10.6-10.14 Mojave on Intel only. No
  release up to Mojave ever ran on ARM; macOS on ARM starts with 11 Big Sur on Apple silicon, which is not feasible
  here (arm64e pointer authentication, Apple's private CPU extensions, the SEP, the AGX GPU, no public
  documentation).
- So the Darwin and Mac OS X ladder runs on the **x86 target**, our fastest core. The ARM side of Apple is the
  iPhone/iPod (ARMv7, Darwin's ARM kernel), which v64 has planned (`docs/iphone4-plan.md`: iPhone 4, A4/S5L8930,
  iOS 6.1.3, on the AArch32 core; v64 has started it with Mach-O, the Apple device tree, IMG3, IPSW and HFS+
  tooling).
- Legal: v86 ships no Apple files. The user supplies installers or firmware. Apple's licence allows macOS virtual
  machines only on Apple hardware; the docs will say so. The SMC's OSK key cannot be shipped: the user supplies it,
  or the guest uses an open replacement (VirtualSMC).

Shared groundwork (x86), before the ladder:

- G1. (SSE3/SSSE3/SSE4 in every mode: done 2026-11-03.) POPCNT, and CPUID leaves that Darwin reads (cache info, TSC
  frequency or a matching FSB in SMBIOS).
- G2. Port from v64: AHCI/SATA (`src/ahci.js`; later Mac OS X drops PIIX IDE), e1000 (Mac OS X has the 8254x driver),
  SMBIOS (`src/smbios.js`), HPET (check ours against v64's), Q35 extras (ICH9 LPC details, PCIe root ports).
- G3. An AppleSMC device (ports 0x300-0x31F, as QEMU's `applesmc`), keys supplied by the user.
- G4. Firmware: BIOS with Darwin's own boot-132 loader for the early steps; UEFI with OVMF and the open-source
  OpenCore bootloader (it supplies the Apple protocols that boot.efi needs) for 10.6 and later.

The ladder (each step: the installer boots, it installs to an AHCI disk, the installed system boots to its
login or desktop, network works; one test per step under `tests/apple/`; each step only starts when the one before
passes):

| Step | Release | Kernel | Needs |
| --- | --- | --- | --- |
| D1 | Darwin 8 (open source xnu-792, PureDarwin) | 32-bit | boot-132, IDE or AHCI, SSE2/SSE3; text console |
| D2 | Darwin 9-17 (PureDarwin 17 = xnu of 10.13) | 32/64-bit | the same, then UEFI + OpenCore |
| M4 | Mac OS X 10.4 Tiger (Intel) | 32-bit | SSE3, AppleSMC, boot-132 or OpenCore |
| M5 | 10.5 Leopard | 32-bit | the same, a VESA/GOP framebuffer |
| M6 | 10.6 Snow Leopard | 32/64-bit | UEFI + OpenCore, x86_64 |
| M7 | 10.7 Lion | 64-bit | SSSE3 |
| M8 | 10.8 Mountain Lion | 64-bit only | 64-bit EFI, HPET, the TSC calibrated |
| M9-M11 | 10.9 Mavericks, 10.10 Yosemite, 10.11 El Capitan | 64-bit | AHCI, e1000, SMBIOS model matched to the release |
| M12 | 10.12 Sierra | 64-bit | SSE4.1/4.2, POPCNT |
| M13 | 10.13 High Sierra | 64-bit | APFS boot (OpenCore's driver) |
| M14 | 10.14 Mojave | 64-bit | a framebuffer without Metal (software rendering) |

Expect each later release to be much slower than Linux: they are large and the desktop renders in software. A
WebGPU-backed display for Mac OS X would be a separate, later project.

ARM track (after Android): v64's iPhone 4 plan on our AArch32 core, as a new `s5l8930` machine, iOS 4 up through
6.1.3, with firmware and keys from the user.

## Plan: v64 parity

What v64 has and we don't yet (2026-10-31, by comparing the two trees and v64's docs):

- ARM (task 7): PCIe/virtio-pci; virtio-gpu 3D and Venus on ARM (task 6); virtio-console, -input, -vsock, -snd;
  snapshots; RAM above 2 GiB; Android (Cuttlefish images and tooling, `ranchu`); the iPhone 4 machine (v64 has it
  only as a plan and tooling).
- x86 devices: AHCI/SATA, e1000, SMBIOS, PCIe root ports, ICH9 TCO/SMBus; virtio-gpu and virtio-vsock.
- x86 firmware: the UEFI variable store as CFI flash with write-back, and Secure Boot (ours keeps variables in RAM for
  the session).
- x86 memory: RAM above 4 GiB (v64's extended memory).
- Graphics: v64's GLES proxy and gfxstream (`src/gles/`), and the GeForce 6800 plan.
- Storage and app: disk image formats (qcow2, VMDK, VHD/VHDX, VDI, OVA), disk persistence (OPFS, write-back to a
  local file), and the VM manager web app (`src/app/`).
- Performance: x86 is already faster than v64; ARM boot speed against v64 has not been compared on the same host.

## Done

- ✅ 2026-11-03 virtio-gpu 2D on arm64 over virtio-mmio (`tests/a64/virtio_gpu.mjs`, `alpine-arm64.html?gpu=1`).
- ✅ 2026-11-03 virgl 3D on x86: kmscube on 32- and 64-bit Alpine with WebGPU (`tests/virtio_gpu_3d.mjs`,
  `tools/cube_disk.mjs`); the CPU wakes when the GPU answers.
- ✅ 2026-11-03 SSE3/SSSE3/SSE4 in every x86 mode: the 32-bit JIT passes 0F 38 / 0F 3A's prefixes
  (`tests/x64/jit/16_sse4_prefix_x86.asm`).
- ✅ 2026-11-03 The JS split per architecture (libv86.js, libv64.js, libv86-arm.js; `docs/architectures.md`); one
  core without cross-origin isolation; coi-serviceworker for static servers.
- ✅ 2026-11-03 Buildroot extras: neofetch, tput, curl, OpenSSL and CA certificates, sudo, the `student` user,
  TERM=xterm-256color on the serial port.
- ✅ 2026-10-31 virtio-gpu 2D on x86 (`tests/virtio_gpu.mjs`).
- ✅ 2026-10-31 jit64 faster than v64.
- ✅ 2026-10-31 UEFI on ARM (edk2 ArmVirtQemu; `tests/a64/uefi.mjs`).
- ✅ Earlier: UEFI and Q35 on x86, TPM 2.0, USB, HDA, virtio-pci, graphics adapters, multicore.
