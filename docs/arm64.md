# AArch64 (the ARM core)

`build/v86-arm64.wasm` is a second core, an AArch64 CPU for QEMU's `virt` board, taken from the
[v64](https://github.com/speedyHKjournalist/v64) project (`a64/`, see `a64/NOTICE.md`). It is built as a crate of its own
(`make build/v86-arm64.wasm`) and shares nothing with the x86 core at build time.

The core has the CPU (interpreter and page tier, AArch32 at EL0 with profile v3, `cpu_type: "arm32"`), the MMU and TLB, the GICv3,
the generic timer, PSCI, SMP and the physical address map. The devices are in JavaScript: `src/arm/virt/` (device tree,
Linux loader, PL011, PL031, platform; copied from v64) and `src/arm/virtio_mmio.js` (virtio-mmio transport, block device,
rng). `src/arm/machine.js` drives it:

```js
import { A64Machine } from "./src/arm/machine.js";
const machine = await A64Machine.create({ wasm, kernel, initrd, cmdline, memory_size: 512 << 20, cpu_cores: 2,
                                          disks: [{ buffer: iso, readonly: true }], virtio_rng: true });
machine.add_listener("serial0-output-byte", byte => ...);
machine.run();
machine.serial0_send("root\n");
```

* The page tier (the JIT) is on by default (`jit_switches: { a64_page: 0 }` turns it off; the interpreter alone needs
  ~390 s to the login prompt on a 4-core x86-64 host, the page tier 56 s).
* Cores in vCPU workers: `wasm` = `build/v86-arm64-parallel.wasm` (bytes) and
  `parallel: { worker_url: new URL("src/arm/parallel/vcpu_worker_entry.js", ...) }`. On a host with few cores the cooperative
  cores are faster (66 s with 4 cores, against 170 s in workers on a 4-core host).
* `tests/a64/boot.mjs [cores]` boots Alpine aarch64 (the official virt ISO) to a login and runs a few commands;
  `make a64-boot-tests`. `examples/alpine-arm64.html` does it in a browser (the page needs `alpine-aarch64.iso` next to it).
* Not ported from v64: networking, PCIe devices, graphics (virtio-gpu, WebGPU), sound, snapshots, the Android
  machine (`ranchu`).

## UEFI

`firmware: { code, vars }` (ArrayBuffers or Uint8Arrays) boots UEFI firmware instead of a kernel, as QEMU's
`-machine virt` with two pflash units does. The official Alpine aarch64 ISO boots only this way: the firmware finds
the ISO on virtio-blk, runs its GRUB (`EFI/BOOT/BOOTAA64.EFI`), and GRUB loads the kernel and initramfs.

```js
const machine = await A64Machine.create({ wasm, memory_size: 512 << 20, disks: [{ buffer: iso, readonly: true }],
    firmware: { code: aavmf_code, vars: new Uint8Array(aavmf_vars_template) } });
```

* **The firmware.** `bios/aavmf-code.fd` and `bios/aavmf-vars.fd` are QEMU 9.2.0's prebuilt edk2 ArmVirtQemu
  (`pc-bios/edk2-aarch64-code.fd`, `edk2-arm-vars.fd`, edk2-stable202408) without their padding: the 2 MiB firmware
  volume and the 768 KiB variable store. `bios/fetch-aavmf.sh` fetches and checks them; their licences are in
  `bios/AAVMF-LICENSES.txt`.
* **The flash** (`src/arm/virt/pflash.js`). Unit 0 at 0 holds the code: read-only guest memory on the core's bus (a
  ROM after the RAM in guest memory), so the firmware runs from it in place and the page tier compiles it; the rest
  of its 64 MiB reads as zeros. Unit 1 at 0x0400_0000 is the variable store: a CFI flash as QEMU's `pflash_cfi01`
  (bank width 4, two 16-bit devices) with the commands edk2's VirtNorFlashDxe uses (read array, status, identifiers,
  CFI query, word and buffered program, block erase, lock setup). Programs and erases write `firmware.vars` in place:
  keep the buffer to keep the variables, pass a fresh copy of the template for a new machine.
* **fw_cfg** (`src/arm/virt/fw_cfg.js`) at 0x0902_0000, the MMIO interface: the signature, revision 1 (no DMA), the
  CPU count and an empty file directory. There is no `etc/table-loader`, so the firmware gives the kernel the device
  tree, not ACPI.
* **The device tree** gets the reference's `flash@0` and `fw-cfg@9020000` nodes and goes at RAM's base
  (0x4000_0000), where ArmVirtQemu looks for it. The firmware disables the flash node before it hands the tree on, so
  Linux leaves the variable store to the firmware's runtime services. No bootargs or initrd: GRUB passes those.
* **The core** starts at 0 in EL1 (no EL2, as QEMU without `virtualization=on`); PSCI is HVC, as the tree says. A
  reset (PSCI SYSTEM_RESET) reloads the tree and starts the firmware again; the variables stay.
* No console but the serial port yet: the firmware has no GOP (no ramfb or virtio-gpu).
* `tests/a64/uefi.mjs [cores]` boots the ISO through the firmware to a login (about 60 s with the page tier on a
  4-core host; GRUB after 9 s), runs `uname -m`, reads `BootOrder` through `/sys/firmware/efi/efivars` and checks
  that the store was written; part of `make a64-boot-tests`. `examples/alpine-arm64.html?uefi=1` does the same in a
  browser.
