#!/usr/bin/env node
// UEFI (OVMF) on the PC machine: the firmware starts the Alpine kernel it is
// handed through fw_cfg (as QEMU's -kernel), Linux finds the ACPI tables and
// the CD, and the boot reaches the login prompt on the serial console.
// KERNEL, INITRD and ISO name the Alpine x86_64 files.
import fs from "node:fs";
import url from "node:url";
const ROOT = url.fileURLToPath(new URL("../", import.meta.url));
const { V86 } = await import(ROOT + "src/main.js");
const buf = p => { const b = fs.readFileSync(p); return b.buffer.slice(b.byteOffset, b.byteOffset + b.byteLength); };
const env = (name, fallback) => process.env[name] || fallback;
const emulator = new V86({
    firmware: { code: { buffer: buf(ROOT + "bios/ovmf-code.fd") }, vars: { buffer: buf(ROOT + "bios/ovmf-vars.fd") } },
    memory_size: 512 << 20, acpi: true, autostart: true, wasm_path: ROOT + "build/v86.wasm", log_level: 0,
    bzimage: { buffer: buf(env("KERNEL", "/root/jitwork/kern/vmlinuz-x86_64")) },
    initrd: { buffer: buf(env("INITRD", "/root/jitwork/kern/initramfs-x86_64")) },
    cdrom: { buffer: buf(env("ISO", "/root/jitwork/alpine-virt-3.24.3-x86_64.iso")) },
    cmdline: "modules=loop,squashfs,sd-mod console=ttyS0,115200",
});
let serial = "";
const t0 = Date.now();
emulator.add_listener("serial0-output-byte", b => { serial += String.fromCharCode(b); });
const check = setInterval(() => {
    if(/localhost login:/.test(serial))
    {
        clearInterval(check);
        const ok = /ACPI: Interpreter enabled/.test(serial) && /sr0/.test(serial);
        console.log((ok ? "ok" : "FAIL") + ": login prompt after " + ((Date.now() - t0) / 1000).toFixed(0) + " s" + (ok ? "" : " (no ACPI or no CD)"));
        process.exit(ok ? 0 : 1);
    }
}, 500);
setTimeout(() => { console.log("FAIL: no login prompt\n" + serial.slice(-2000)); process.exit(1); }, 600000);
