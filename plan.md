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

## State (2026-10-31)

- **x86_64:** faster than v64. Alpine x86_64 reaches the login prompt in about 40 s; v64 takes 44.5 s (Node, 4-core
  host). The busybox shell loop takes 2.4 s here against 2.9-3.3 s on v64. `docs/v64-comparison.md` says what
  changed.
- **32-bit x86:** Alpine boots to login in about 38 s.
- **Done earlier:**
  - UEFI (OVMF) and Q35 for x86, with BIOS mode kept. Tests: `tests/uefi.mjs`, `tests/q35.mjs`.
  - TPM 2.0 (`tests/tpm.mjs`).
  - USB (UHCI), HDA sound, virtio-pci devices.
  - Graphics adapters (`bochs_vga`, `vmware_svga`; WebGPU optional; `tests/graphics_adapter.mjs`).
  - Multicore, cooperative and in workers.
- **ARM core:** `build/v86-arm64.wasm`, from v64 (`docs/arm64.md`). It boots Alpine aarch64 on the `virt` board by
  direct kernel boot (`tests/a64/boot.mjs`, `examples/alpine-arm64.html`) and from the official ISO through UEFI
  (`tests/a64/uefi.mjs`, `?uefi=1`; `docs/arm64.md`, "UEFI"). Not ported yet: PCIe, networking,
  graphics, sound, snapshots, Android (`src/arm/virt/ranchu.js` is only a stub copy).
- **Demo package:** `alpine-demo.tar.gz` in the repository root (git-ignored). It has our build at `/`, v64 at
  `/v64/` and upstream v86 at `/v86/` (EdHeller/v86 master, labelled by commit and author date, e.g. "v86 315b2b0
  (Oct 30, 2026 21:24 GMT)"; 32-bit only). Packages are recorded in `docs/releases.md`.
- **Known issues:**
  - A 32-bit Alpine kernel hangs at boot when SSSE3/SSE4 are advertised, so they are advertised in long mode only
    (`instr_0FA2`).
  - V8 compiles each jit64 module lazily with Liftoff on its first call. That costs 1-3 s of the main thread per
    boot.

## Measuring

- **x86_64 boot time:** `/root/jitwork/bench3.sh 3` gives the mean time to login over 3 boots. The scripts and ISOs
  are in `/root/jitwork`; they are gone if the container was reset. Rebuild them from the notes in
  `docs/next-session.md`.
- **Tests:**
  - `make build/v86.wasm build/v86-debug.wasm`
  - `node tests/x64/run.js` (expect "88 ok")
  - `JIT64=0 node tests/x64/fuzz.js --seed 6 --count 1000`
  - `node tests/uefi.mjs`
  - `node tests/a64/boot.mjs`
- Run benchmarks one after another, never in parallel. Differences under about 3 % are noise.

## Next tasks, in order

Order set by the user on 2026-10-31: virtio-gpu with WebGPU first, then SSE3/SSE4 on x86, then the rest.

1. [ ] **virtio-gpu with an optional WebGPU renderer (top priority).**
   - Port v64's `src/graphics_adapters/virtio_gpu/` (19 files; `docs/gpu-devices.md`, `docs/display-design.md`
     there) as a plugin next to `bochs_vga` and `vmware_svga`, option `graphics_adapter: "virtio_gpu"`.
   - Step 1, 2D: resources, transfers, scanout to the existing screen adapter, the cursor, EDID. On x86 over
     virtio-pci (the x86 VirtIO core, which needs a memory BAR for its structures: today it only has I/O BARs). On
     arm64 over virtio-mmio first (no PCIe needed), and over virtio-pci once task 3 lands.
   - Step 2, 3D: virgl (GL) and Venus (Vulkan) through v64's glbridge/gfxstream path with WebGPU
     (`docs/glbridge.md`, `docs/gfxstream-plan.md`). Host-visible memory is device memory on the core's bus.
   - It must stay optional: without WebGPU, or with the option off, the device is 2D only (or the machine keeps its
     old adapter) and nothing else breaks.
   - Checks: Alpine x86_64 and aarch64 show their console on it (`tests/virtio_gpu.mjs`, `tests/a64/virtio_gpu.mjs`,
     a screenshot that has the login prompt); with WebGPU in headless Chrome, kmscube or a GL test runs on virgl.
2. [ ] **SSE3, SSSE3 and SSE4 on x86, in every mode.**
   - Today SSSE3/SSE4.1/SSE4.2 are advertised only in long mode (`instr_0FA2`), because a 32-bit Alpine kernel hangs
     at boot when they are advertised. Find which instruction is wrong (most likely one that only the 32-bit
     kernel's code paths use, e.g. in the raid6/crc32c/memcpy alternatives or a 32-bit operand-size encoding), fix it,
     and advertise them in all modes.
   - Check every SSE3/SSSE3/SSE4.1/SSE4.2 instruction (and POPCNT) against a reference: extend
     `tests/x64/fuzz.js` / the nasm tests to all of them in 32-bit and 64-bit modes, interpreted and compiled.
   - Then: 32-bit Alpine boots with SSE4 advertised; `node tests/x64/run.js` and the fuzzers stay clean.
   - The Apple targets below need this: Mac OS X needs SSE3 from 10.4, SSSE3 from 10.7, and later releases
     use SSE4.1/4.2.
3. [ ] **PCIe on the arm64 `virt` board.** `src/arm/virt/pcie.js` has the host (ECAM, windows, INTx) and a
   virtio-pci transport. Still to do: wire it into the machine (`virtio_transport: "pci"`), boot Alpine with
   virtio-blk-pci both by direct kernel boot and through UEFI, and add that to `make a64-boot-tests`. Android and
   virtio-gpu-pci need it.
4. [ ] **Android on ARM.** See "Plan: Android" below.
5. [ ] **Apple targets: Darwin up through Mac OS X 10.14 Mojave.** See "Plan: Apple" below.
6. [ ] **The rest of v64 parity.** See "Plan: v64 parity" below.
7. [ ] **x86 follow-up (low priority).** Cut the lazy-compile cost, for example with smaller modules or by
   compiling off the main thread.

## Plan: Android

Follows v64's `docs/arm64-virt-android16-plan.md` and `docs/android-images-plan.md`. v64 runs Android 16 on the
`virt` board with Cuttlefish's arm64-only phone images, so its path, not `ranchu`, is the one to port first.

- A1. Device groundwork: PCIe and virtio-pci (task 3), virtio-console with several ports, virtio-input (keyboard,
  tablet, multi-touch), virtio-vsock (adbd), more than 2 GiB of RAM (the core has `extended.rs`; the machine
  refuses more than 2 GiB today).
- A2. The GKI kernel (android16-6.12) with Alpine's initramfs on our `virt` board: the kernel's own features
  (39-bit VA, SW_TTBR0_PAN, pseudo-NMIs, KASLR) and the virtio modules load. This is v64's P10.6.
- A3. Images. Port v64's `src/android/` (boot.img, vendor_boot, bootconfig, AVB, GPT, super/lp, sparse, erofs,
  ext4) and `tools/a64_android.mjs`, which fetches the pinned public Cuttlefish build (BP4A.251205.006) from
  ci.android.com. If that download is blocked here, ask the user once for the images or a URL.
- A4. Boot to `sys.boot_completed=1` with adb over vsock, with SwiftShader (software) graphics.
- A5. Display and input: virtio-gpu 2D scanout, touch through virtio-input; then GPU acceleration through Venus on
  WebGPU (task 1, step 2).
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

- G1. SSE3/SSSE3/SSE4 in every mode (task 2), POPCNT, and CPUID leaves that Darwin reads (cache info, TSC
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

- ARM: PCIe/virtio-pci (task 3); virtio-gpu 2D/virgl/Venus (task 1); virtio-console, -input, -vsock, -snd;
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

- 2026-10-31:
  - jit64 is faster than v64 (incremental compiles, fixes to the YIELD bug, the epochs and the TLB, and others).
  - Demo page fixes: `console=tty1` instead of `tty0`, which removed the "-sh: can't access tty; job control turned
    off" shell and the keys split between two gettys; the serial login now ends with CR, so `^M` no longer shows.
  - New `alpine-demo.tar.gz`.
  - UEFI on the ARM target: edk2 ArmVirtQemu (QEMU 9.2.0's build, `bios/aavmf-*.fd`) from pflash, the variable
    store as CFI flash, fw_cfg over MMIO. The official Alpine aarch64 ISO boots through it and GRUB to a login in
    about 60 s (`tests/a64/uefi.mjs`, in `make a64-boot-tests`; `examples/alpine-arm64.html?uefi=1`). v64 has no
    arm64 UEFI, so this was written new. Serial console only: no GOP until there is a ramfb or virtio-gpu.
  - Upstream v86 (master 315b2b0, Oct 30, 2026 21:24 GMT) added to the demo package at `/v86/` as a third comparison
    target next to v64: 32-bit Alpine to login in about 50 s in headless Chromium.
