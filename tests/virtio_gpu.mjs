#!/usr/bin/env node
// Boots Alpine (x86_64, or 32-bit x86) with graphics_adapter: "virtio_gpu" (virtio-vga: the VGA core for the BIOS,
// virtio-gpu for Linux's virtio_gpu driver) and checks that the console is on it: the driver binds to the PCI
// function, fbcon runs on virtio_gpudrmfb, the scanout has the console's text, and text written to tty1 appears on
// the scanout. 2D: no renderer, no WebGPU. The kernel and initramfs come from the official virt ISO.
//
//   node tests/virtio_gpu.mjs [x86_64|x86]      (VGPU_ISO: the ISO, else build/alpine-virt-3.24.3-<arch>.iso, downloaded)

import fs from "node:fs";
import path from "node:path";
import url from "node:url";

const root = path.join(url.fileURLToPath(new URL(".", import.meta.url)), "..");
const arch = process.argv[2] === "x86" ? "x86" : "x86_64";
const { V86 } = await import(path.join(root, "src/main.js"));
const { iso_files } = await import(path.join(root, "src/arm/iso.js"));

const iso_path = process.env.VGPU_ISO || path.join(root, "build/alpine-virt-3.24.3-" + arch + ".iso");
if(!fs.existsSync(iso_path))
{
    const source = "https://dl-cdn.alpinelinux.org/alpine/v3.24/releases/" + arch + "/alpine-virt-3.24.3-" + arch + ".iso";
    const response = await fetch(source);
    if(!response.ok) throw new Error(source + ": " + response.status);
    fs.writeFileSync(iso_path, Buffer.from(await response.arrayBuffer()));
}
const file = p => { const b = fs.readFileSync(p); return b.buffer.slice(b.byteOffset, b.byteOffset + b.byteLength); };
const image = file(iso_path);
const files = iso_files(image);
const take = name => { const [offset, length] = files.get(name); return image.slice(offset, offset + length); };

const emulator = new V86({
    wasm_path: path.join(root, "build/v86.wasm"),
    bios: { buffer: file(path.join(root, "bios/seabios.bin")) },
    vga_bios: { buffer: file(path.join(root, "bios/vgabios.bin")) },
    bzimage: { buffer: take("BOOT/VMLINUZ_VIRT") },
    initrd: { buffer: take("BOOT/INITRAMFS_VIRT") },
    cmdline: "modules=loop,squashfs,sd-mod console=tty0 console=ttyS0,115200 tsc=reliable mitigations=off random.trust_cpu=on",
    cdrom: { buffer: image },
    cpu_type: arch === "x86_64" ? "x86_64" : undefined,
    memory_size: 512 << 20,
    graphics_adapter: "virtio_gpu",
    autostart: true,
    log_level: 0,
});

let serial = "";
emulator.add_listener("serial0-output-byte", b => { serial += String.fromCharCode(b); });
const start = Date.now();
const sleep = ms => new Promise(r => setTimeout(r, ms));
const wait_for = async (re, seconds) => {
    const from = Date.now();
    while(!re.test(serial))
    {
        if(Date.now() - from > seconds * 1000) throw new Error("timeout waiting for " + re + "\n" + serial.slice(-1000));
        await sleep(200);
    }
};
const run = async (command, expect, seconds = 30) => {
    const mark = serial.length;
    emulator.serial0_send(command + "\n");
    const re = new RegExp(expect);
    const from = Date.now();
    while(!re.test(serial.slice(mark)))
    {
        if(Date.now() - from > seconds * 1000) throw new Error("`" + command + "` did not print " + expect + ":\n" + serial.slice(mark));
        await sleep(100);
    }
    return serial.slice(mark);
};

/** scanout 0's picture (what the page shows): how many pixels differ from the background (the first one), and a hash */
function scanout()
{
    const gpu = emulator.v86.cpu.devices.graphics_adapter.device["virtio_gpu"];
    const s = gpu.scanouts[0];
    if(!s || !s.resource_id || !s.rgba) return { lit: 0, hash: 0, width: 0, height: 0 };
    const words = new Uint32Array(s.rgba.buffer, s.rgba.byteOffset, s.rgba.byteLength >> 2);
    let lit = 0, hash = 0;
    for(let i = 0; i < words.length; i++)
    {
        if((words[i] & 0xFFFFFF) !== (words[0] & 0xFFFFFF)) lit++;
        hash = Math.imul(hash ^ words[i], 16777619) >>> 0;
    }
    return { lit, hash, width: s.width, height: s.height };
}

try
{
    await wait_for(/localhost login:/, +process.env.VGPU_SECS || 300);
    console.log("login after %ss", ((Date.now() - start) / 1000).toFixed(1));
    emulator.serial0_send("root\n");
    await wait_for(/localhost:~# /, 60);
    await run("cat /sys/class/graphics/fb0/name", "virtio_gpudrmfb");
    await run("cat /sys/bus/pci/devices/0000:00:12.0/vendor /sys/bus/pci/devices/0000:00:12.0/device", "0x1af4\\s+0x1050");
    await run("readlink /sys/bus/pci/devices/0000:00:12.0/driver", "virtio-pci");
    await run("cat /sys/class/vtconsole/vtcon1/name", "frame buffer device");
    const before = scanout();
    if(before.width < 640 || before.lit < 2000)
    {
        throw new Error("the scanout shows no console: " + JSON.stringify(before));
    }
    await run("clear > /dev/tty1; for i in 1 2 3 4 5 6 7 8; do echo VIRTIO-GPU-CONSOLE-$i > /dev/tty1; done; echo ok", "\\nok");
    await sleep(2000);
    const after = scanout();
    if(after.hash === before.hash || after.lit < 500)
    {
        throw new Error("text written to tty1 did not reach the scanout: " + JSON.stringify({ before, after }));
    }
    console.log("ok: %s, virtio-gpu console %dx%d, %d lit pixels after writing to tty1", arch, after.width, after.height, after.lit);
    emulator.destroy();
    process.exit(0);
}
catch(error)
{
    console.error("FAIL", error.message);
    process.exit(1);
}
