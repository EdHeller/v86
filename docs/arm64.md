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
