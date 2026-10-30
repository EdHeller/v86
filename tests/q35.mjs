#!/usr/bin/env node
// The Q35 machine (machine_type: "q35") with SeaBIOS, or OVMF with UEFI=1:
// Alpine x86_64 boots to the login prompt, Linux uses the ECAM window and
// the ACPI tables, and a virtio-rng request is answered through its PIRQ's
// IOAPIC input (16 to 23).
import fs from "node:fs";
import url from "node:url";
const ROOT = url.fileURLToPath(new URL("../", import.meta.url));
const { V86 } = await import(ROOT + "src/main.js");
const buf = p => { const b = fs.readFileSync(p); return b.buffer.slice(b.byteOffset, b.byteOffset + b.byteLength); };
const env = (name, fallback) => process.env[name] || fallback;
const uefi = !!process.env.UEFI;
const emulator = new V86({
    ...(uefi ? { firmware: { code: { buffer: buf(ROOT + "bios/ovmf-code.fd") }, vars: { buffer: buf(ROOT + "bios/ovmf-vars.fd") } } } :
        { bios: { buffer: buf(ROOT + "bios/seabios.bin") }, vga_bios: { buffer: buf(ROOT + "bios/vgabios.bin") } }),
    machine_type: "q35", acpi: true, memory_size: 512 << 20, autostart: true, wasm_path: ROOT + "build/v86.wasm", log_level: 0,
    bzimage: { buffer: buf(env("KERNEL", "/root/jitwork/kern/vmlinuz-x86_64")) },
    initrd: { buffer: buf(env("INITRD", "/root/jitwork/kern/initramfs-x86_64")) },
    cdrom: { buffer: buf(env("ISO", "/root/jitwork/alpine-virt-3.24.3-x86_64.iso")) },
    cmdline: "modules=loop,squashfs,sd-mod console=ttyS0,115200",
    virtio_rng: true,
});
let serial = "";
const t0 = Date.now();
emulator.add_listener("serial0-output-byte", b => { serial += String.fromCharCode(b); });
const login = setInterval(() => {
    if(!/localhost login:/.test(serial)) return;
    clearInterval(login);
    const seconds = ((Date.now() - t0) / 1000).toFixed(0);
    emulator.serial0_send("root\n");
    setTimeout(() => { serial = ""; emulator.serial0_send("modprobe virtio-rng; head -c 64 /dev/hwrng | wc -c; dmesg | grep -c 'PCI: ECAM'; grep virtio /proc/interrupts; echo DONE\n"); }, 4000);
    const done = setInterval(() => {
        if(!/\nDONE/.test(serial)) return;
        clearInterval(done);
        const ok = /\n64\r?\n/.test(serial) && /IO-APIC\s+(1[6-9]|2[0-3])-fasteoi\s+virtio/.test(serial);
        console.log((ok ? "ok" : "FAIL") + ": Q35" + (uefi ? " with UEFI" : "") + ", login prompt after " + seconds + " s" + (ok ? "" : "\n" + serial));
        process.exit(ok ? 0 : 1);
    }, 500);
}, 500);
setTimeout(() => { console.log("FAIL: no login prompt\n" + serial.slice(-2000)); process.exit(1); }, 900000);
