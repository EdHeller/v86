# Architectures as separate files

A page loads the architectures it runs, next to `libv86.js`:

| File | Adds | `cpu_type` |
| --- | --- | --- |
| `libv86.js` | the PC and a 32-bit x86 CPU (as upstream v86); core `v86.wasm` | `"x86"` (the default without libv64.js) |
| `libv64.js` | x86_64; core `v64.wasm` | `"x86_64"` (the default when it is loaded) |
| `libv86-arm.js` | the ARM virt machine (core: `v86-arm64.wasm`) | `"arm64"`, `"arm32"` |

```html
<script src="/js/libv86.js"></script>
<script src="/js/libv64.js"></script>       <!-- only for x86_64 guests -->
<script src="/js/libv86-arm.js"></script>   <!-- only for ARM guests -->
```

x86 and x86_64 share every device. The CPUs are two builds of the Rust core: `v86.wasm` is a 32-bit x86 (no
long mode: EFER.LME is reserved, CPUID does not report it, and the 64-bit interpreter and JIT are not in the
module, 3.7 MB), `v64.wasm` is the x86_64 one (cargo feature `x86_64`, 4.2 MB); likewise `v86-parallel*.wasm` and
`v64-parallel*.wasm`, `-debug` and `-fallback`. libv64.js is small: it registers x86_64. With cpu_type "x86_64"
v86 loads the 64-bit core beside the one `wasm_path` names (`/v86.wasm` -> `/v64.wasm`,
`v86-parallel.wasm` -> `v64-parallel.wasm`), and refuses a core without long mode (the export
`core_has_long_mode`). cpu_type "x86_64" without libv64.js is an error that names the file. Build:
`make build/v86.wasm build/v64.wasm` (`make cores` for both and their parallel builds). libv86-arm.js is the ARM machine (`src/arm/`):
`new V86({ cpu_type: "arm64", wasm_path, memory_size, cpu_cores, bzimage, initrd, cmdline, cdrom, hda,
firmware: { code, vars }, serial_console, autostart })` boots it, with `serial0_send`, the `serial0-output-byte`
event, `run`, `stop`, `restart` and `destroy` (`examples/arm-v86.html`; not yet: the screen, networking,
snapshots, cores in workers).

The files register themselves on `globalThis.V86Architectures` (`src/browser/architectures.js`), in any order
before `new V86()`. In the source tree (`src/main.js`) every architecture is built in: the bundles are compiled
with `ARCH_PLUGINS=true`. Build: `make build/libv86.js build/libv64.js build/libv86-arm.js` (`make architectures`
for these and their `-debug` versions).
