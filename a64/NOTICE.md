# The ARM core

`a64/` is the AArch64 core of the [v64](https://github.com/speedyHKjournalist/v64) project (itself a fork of
v86), taken verbatim with the small amount of code it shares with its x86 core (`jitrt`, `pagegen`, `wasmgen`,
`parallel`, `extended`, ...), and built as a crate of its own (`make build/v86-arm64.wasm`). It is under the
licenses of v64 (`LICENSE`, `LICENSE.MIT`). The JavaScript side in `src/arm/` copies v64's `virt/` board code (device
tree, loader, PL011, PL031, platform) and drives the core with a smaller machine class (`machine.js`) and a virtio-mmio
block / rng implementation (`virtio_mmio.js`).
