# Architectures as separate files

A page loads the architectures it runs, next to `libv86.js`:

| File | Adds | `cpu_type` |
| --- | --- | --- |
| `libv86.js` | the PC and a 32-bit x86 CPU (as upstream v86) | `"x86"` (the default without libv64.js) |
| `libv64.js` | x86_64: the CPU reports long mode, 64-bit kernels boot | `"x86_64"` (the default when it is loaded) |
| `libv86-arm.js` | the ARM virt machine (core: `v86-arm64.wasm`) | `"arm64"`, `"arm32"` |

```html
<script src="/js/libv86.js"></script>
<script src="/js/libv64.js"></script>       <!-- only for x86_64 guests -->
<script src="/js/libv86-arm.js"></script>   <!-- only for ARM guests -->
```

x86 and x86_64 share every device and the same core (`v86.wasm`; the 64-bit CPU lives there), so libv64.js is
small: it registers x86_64, and with it `cpu_type: "x86_64"` turns long mode on in CPUID. Without it, that
cpu_type is an error that names the file. libv86-arm.js is the ARM machine (`src/arm/`):
`new V86({ cpu_type: "arm64", wasm_path, memory_size, cpu_cores, bzimage, initrd, cmdline, cdrom, hda,
firmware: { code, vars }, serial_console, autostart })` boots it, with `serial0_send`, the `serial0-output-byte`
event, `run`, `stop`, `restart` and `destroy` (`examples/arm-v86.html`; not yet: the screen, networking,
snapshots, cores in workers).

The files register themselves on `globalThis.V86Architectures` (`src/browser/architectures.js`), in any order
before `new V86()`. In the source tree (`src/main.js`) every architecture is built in: the bundles are compiled
with `ARCH_PLUGINS=true`. Build: `make build/libv86.js build/libv64.js build/libv86-arm.js` (`make architectures`
for these and their `-debug` versions).
