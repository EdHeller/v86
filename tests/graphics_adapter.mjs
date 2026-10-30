// boots Alpine x86 with graphics_adapter, checks the driver and a mode set
import fs from "node:fs";
const repo = "/home/user/v86", tag = "x86";
const { V86 } = await import(repo + "/src/main.js");
const buf = p => { const b = fs.readFileSync(p); return b.buffer.slice(b.byteOffset, b.byteOffset + b.byteLength); };
const emulator = new V86({
    bios: { buffer: buf(repo + "/bios/seabios.bin") }, vga_bios: { buffer: buf(repo + "/bios/vgabios.bin") },
    bzimage: { buffer: buf(`/root/jitwork/kern/vmlinuz-${tag}`) }, initrd: { buffer: buf(`/root/jitwork/kern/initramfs-${tag}`) },
    cmdline: "modules=loop,squashfs,sd-mod console=ttyS0,115200 tsc=reliable mitigations=off random.trust_cpu=on",
    cdrom: { buffer: buf(`/root/jitwork/alpine-virt-3.24.3-${tag}.iso`) }, memory_size: 256 * 1024 * 1024,
    autostart: true, wasm_path: repo + "/build/v86.wasm", log_level: 0, graphics_adapter: process.env.GA || "vmware_svga",
});
let ser = ""; const t0 = Date.now(); let sizes = [];
emulator.add_listener("serial0-output-byte", b => { ser += String.fromCharCode(b); });
emulator.add_listener("screen-set-size", s => sizes.push(s.join("x")));
const iv = setInterval(() => {
    if(/localhost login:/.test(ser)) {
        clearInterval(iv);
        console.log("login at " + ((Date.now() - t0) / 1000).toFixed(1) + "s, screen sizes " + sizes.slice(-4).join(" "));
        emulator.serial0_send("root\n");
        setTimeout(() => emulator.serial0_send("\n"), 1500);
        const cmd = process.env.CMD || "modprobe vmwgfx; sleep 5; dmesg | grep -i -E 'vmwgfx|drm|fb' | tail -12; cat /sys/class/graphics/fb0/virtual_size; echo DONE";
        setTimeout(() => { ser = ""; emulator.serial0_send(cmd + "\n"); }, 4000);
        const iv2 = setInterval(() => { if(/\nDONE/.test(ser)) { clearInterval(iv2); console.log(ser); console.log("sizes " + sizes.slice(-4).join(" ")); process.exit(0); } }, 500);
    }
}, 500);
setTimeout(() => { console.log("TIMEOUT\n" + ser.slice(-800)); process.exit(1); }, 200000);
