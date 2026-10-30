import fs from "node:fs";
const repo = new URL("../", import.meta.url).pathname.replace(/\/$/, "");
const { V86 } = await import(repo + "/src/main.js");
const buf = p => { const b = fs.readFileSync(p); return b.buffer.slice(b.byteOffset, b.byteOffset + b.byteLength); };
const uefi = !!process.env.UEFI;
const e = new V86({
    ...(uefi ? { firmware: { code: { buffer: buf(repo + "/bios/ovmf-code.fd") }, vars: { buffer: buf(repo + "/bios/ovmf-vars.fd") } } } :
        { bios: { buffer: buf(repo + "/bios/seabios.bin") }, vga_bios: { buffer: buf(repo + "/bios/vgabios.bin") } }),
    machine_type: process.env.MT || "q35", acpi: true, memory_size: 512 << 20, autostart: true, wasm_path: repo + "/build/v86.wasm", log_level: 0,
    bzimage: { buffer: buf("/root/jitwork/kern/vmlinuz-x86_64") }, initrd: { buffer: buf("/root/jitwork/kern/initramfs-x86_64") },
    cdrom: { buffer: buf("/root/jitwork/alpine-virt-3.24.3-x86_64.iso") },
    cmdline: "modules=loop,squashfs,sd-mod console=ttyS0,115200",
    tpm: { wasm: { buffer: buf(repo + "/bios/libtpms.wasm") } },
});
let ser = ""; const t0 = Date.now(); let dbg = ""; e.add_listener("debug-console-output-byte", b => { dbg += String.fromCharCode(b); });
e.add_listener("serial0-output-byte", b => { ser += String.fromCharCode(b); });
const iv = setInterval(() => {
    if(/localhost login:/.test(ser)) {
        clearInterval(iv);
        console.log("LOGIN at " + ((Date.now() - t0) / 1000).toFixed(0) + "s");
        e.serial0_send("root\n");
        setTimeout(() => { ser = ""; e.serial0_send("find /lib/modules -name 'tpm*' | head; modprobe tpm_crb; sleep 2; dmesg | grep -i tpm | tail -6; ls /dev/tpm*; cat /sys/class/tpm/tpm0/tpm_version_major; echo DONE\n"); }, 4000);
        const iv2 = setInterval(() => { if(/\nDONE/.test(ser)) { clearInterval(iv2); console.log(ser); console.log("OVMF TPM lines:\n" + dbg.split("\n").filter(l => /tpm|tcg|pcr/i.test(l)).slice(0, 25).join("\n")); process.exit(0); } }, 500);
    }
}, 500);
setTimeout(() => { console.log("TIMEOUT\n" + ser.slice(-2000)); process.exit(1); }, 400000);
