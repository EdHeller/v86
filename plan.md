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

1. [x] **UEFI on the ARM target, with a check.**
   - Port v64's arm64 UEFI support: AAVMF/edk2 `ArmVirtQemu` firmware in pflash, fw_cfg (MMIO), the device tree
     handed to the firmware, and a ramfb or virtio-gpu console if the firmware needs one. Put the firmware and its
     licence in `bios/`.
   - The official Alpine aarch64 ISO boots only through UEFI. The check is a new `tests/a64/uefi.mjs`: boot that
     ISO with the firmware (no direct kernel) to `localhost login:`, log in, and run `uname -m` (expect `aarch64`).
     Add it to `make a64-boot-tests`.
   - Also add it to `examples/alpine-arm64.html` as a `?uefi=1` option.
2. [ ] **PCIe on the arm64 `virt` board.** This means ECAM, the host bridge in the device tree, and the virtio-pci
   devices the x86 side already has. Graphics and Android both need it.
3. [ ] **An optional WebGPU graphics device, like v64's.**
   - virtio-gpu, 2D first: scanout to the existing screen adapter. Then 3D through v64's gfxstream/glbridge path
     with WebGPU.
   - It must stay optional: without WebGPU (or with the option off) the machine uses the 2D framebuffer and nothing
     breaks.
   - It should work on x86 (virtio-gpu over PCI) and on arm64. Test that Alpine shows a console on it.
4. [ ] **Android (`ranchu`).**
   - Port v64's Android machine: goldfish devices, virtio-gpu/gfxstream, and the boot flow in
     `arm64-virt-android16-plan.md`.
   - Blocked on images: this needs AOSP-built images. v64's `android-images-plan.md` says how to get or build them.
     If they can't be obtained here, ask the user once for them or for a download URL.
5. [ ] **The "Mac" ARM target.** The user named it next to Android but didn't define it. Ask the user once, in one
   line, before starting. One likely meaning: a machine and build tuned for Apple-silicon hosts, i.e. the seqcst
   parallel build and Safari compatibility. A macOS guest is not realistic.
6. [ ] **x86 follow-ups (low priority).**
   - Find why the 32-bit kernel hangs with SSE4 advertised, then advertise it in all modes.
   - Cut the lazy-compile cost, for example with smaller modules or by compiling off the main thread.

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
