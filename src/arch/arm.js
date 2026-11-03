// libv86-arm.js: the ARM virt machine for libv86.js (src/browser/architectures.js): cpu_type "arm64" (AArch64)
// or "arm32", its core build/v86-arm64.wasm. new V86() hands the machine's options to "create" and drives it
// (src/browser/starter.js, init_arm). Load it next to libv86.js:
//
//     <script src="libv86.js"></script>
//     <script src="libv86-arm.js"></script>

import { register_architecture } from "../browser/architectures.js";
import { A64Machine } from "../arm/machine.js";
import { iso_files } from "../arm/iso.js";

register_architecture("arm", {
    "name": "arm",
    "create": options => A64Machine.create(options),
    "iso_files": image => iso_files(image),
});
