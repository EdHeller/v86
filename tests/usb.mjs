// boots Alpine x86 with usb option, checks the controller and the devices through serial
import fs from "node:fs";
const repo = process.env.REPO || "/home/user/v86", tag = process.env.TAG || "x86";
const { V86 } = await import(repo + "/src/main.js");
const buf = p => { const b = fs.readFileSync(p); return b.buffer.slice(b.byteOffset, b.byteOffset + b.byteLength); };
const disk = new Uint8Array(8 * 1024 * 1024); disk.set(new TextEncoder().encode("HELLO FROM USB DISK"), 0);
const emulator = new V86({
    bios: { buffer: buf(repo + "/bios/seabios.bin") }, vga_bios: { buffer: buf(repo + "/bios/vgabios.bin") },
    bzimage: { buffer: buf(`/root/jitwork/kern/vmlinuz-${tag}`) }, initrd: { buffer: buf(`/root/jitwork/kern/initramfs-${tag}`) },
    cmdline: "modules=loop,squashfs,sd-mod,usb-storage,uhci-hcd,ehci-pci,xhci-pci,usbhid console=ttyS0,115200 tsc=reliable mitigations=off random.trust_cpu=on",
    cdrom: { buffer: buf(`/root/jitwork/alpine-virt-3.24.3-${tag}.iso`) }, memory_size: 256 * 1024 * 1024,
    autostart: true, acpi: false, wasm_path: repo + "/build/v86.wasm", log_level: +process.env.LOG || 0,
    usb: { controller: process.env.USBC || "uhci", tablet: true, mass_storage: [{ id: "stick", image: { buffer: disk.buffer } }] },
});
let ser = ""; const t0 = Date.now();
emulator.add_listener("serial0-output-byte", b => { ser += String.fromCharCode(b); });
const iv = setInterval(() => {
    if(/localhost login:/.test(ser)) {
        clearInterval(iv);
        console.log("login at " + ((Date.now() - t0) / 1000).toFixed(1) + "s");
        emulator.serial0_send("root\n");
        setTimeout(() => emulator.serial0_send("\n"), 1500);
        const cmd = process.env.CMD || `modprobe ${process.env.USBM || "uhci-hcd"}; sleep 3; echo ==; dmesg | grep -i -E 'usb|uhci|sd[a-z]' | tail -30; ls /sys/bus/usb/devices; echo ==; head -c 19 /dev/sda; echo; echo DONE`;
        setTimeout(() => { ser = ""; emulator.serial0_send(cmd + "\n"); }, 4000);
        const iv2 = setInterval(() => { if(/\nDONE/.test(ser)) { clearInterval(iv2); console.log(ser); process.exit(0); } }, 500);
    }
}, 500);
setTimeout(() => { console.log("TIMEOUT\n" + ser.slice(-800)); process.exit(1); }, 240000);
