// libv64.js: x86_64 for libv86.js (src/browser/architectures.js). The 64-bit CPU itself (long mode, the
// 64-bit JIT) is in v86.wasm; with this file loaded, cpu_type "x86_64" (the default then) has the CPU report
// long mode, and 64-bit kernels boot. Load it next to libv86.js:
//
//     <script src="libv86.js"></script>
//     <script src="libv64.js"></script>

import { register_architecture } from "../browser/architectures.js";

register_architecture("x86_64", {
    "name": "x86_64",
    // what the CPU reports (CPUID 0x80000001: long mode, SYSCALL)
    "long_mode": true,
});
