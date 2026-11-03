#!/usr/bin/env node
// Boots Alpine aarch64 on the virt machine with graphics_adapter "virtio_gpu" on virtio-mmio (src/arm/graphics.js,
// the x86 build's plugin, src/graphics_adapters/virtio_gpu) and checks that the console is on it: Linux's virtio_gpu
// driver binds to the virtio-mmio device, fbcon runs on virtio_gpudrmfb, the scanout has the console's text, and text
// written to tty1 appears on the scanout. 2D: no renderer, no WebGPU. Needs build/v86-arm64.wasm and the ISO
// (A64_ISO, or downloaded to build/a64/alpine-virt-aarch64.iso).
//
//   node tests/a64/virtio_gpu.mjs          (A64_SECS: the limit for the login, default 600)

import fs from "node:fs";
import path from "node:path";
import url from "node:url";

const root = path.join(url.fileURLToPath(new URL(".", import.meta.url)), "../..");
const { A64Machine } = await import(path.join(root, "src/arm/machine.js"));
const { iso_files } = await import(path.join(root, "src/arm/iso.js"));
await import(path.join(root, "src/graphics_adapters/virtio_gpu/plugin.js"));
const descriptor = globalThis["V86GraphicsAdapters"]["virtio_gpu"];

const iso_path = process.env.A64_ISO || path.join(root, "build/a64/alpine-virt-aarch64.iso");
const URL_ISO = "https://dl-cdn.alpinelinux.org/alpine/v3.24/releases/aarch64/alpine-virt-3.24.3-aarch64.iso";
if(!fs.existsSync(iso_path))
{
    fs.mkdirSync(path.dirname(iso_path), { recursive: true });
    const response = await fetch(URL_ISO);
    if(!response.ok) throw new Error("download of " + URL_ISO + ": " + response.status);
    fs.writeFileSync(iso_path, Buffer.from(await response.arrayBuffer()));
}
const b = fs.readFileSync(iso_path);
const image = b.buffer.slice(b.byteOffset, b.byteOffset + b.byteLength);
const files = iso_files(image);
const take = name => { const [offset, length] = files.get(name); return image.slice(offset, offset + length); };

const machine = await A64Machine.create({
    wasm: fs.readFileSync(path.join(root, "build/v86-arm64.wasm")),
    kernel: take("BOOT/VMLINUZ_VIRT"), initrd: take("BOOT/INITRAMFS_VIRT"),
    cmdline: "console=tty0 console=ttyAMA0 modules=loop,squashfs,sd-mod,virtio-blk,virtio-mmio,virtio_blk,virtio_mmio,isofs",
    memory_size: 512 << 20, disks: [{ buffer: image, readonly: true }], virtio_rng: true,
    graphics_adapter: { descriptor },
});
let serial = "";
machine.add_listener("serial0-output-byte", c => { serial += String.fromCharCode(c); });
const start = Date.now();
machine.run();

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
    machine.serial0_send(command + "\n");
    const re = new RegExp(expect);
    const from = Date.now();
    while(!re.test(serial.slice(mark)))
    {
        if(Date.now() - from > seconds * 1000) throw new Error("`" + command + "` did not print " + expect + ":\n" + serial.slice(mark));
        await sleep(100);
    }
    return serial.slice(mark);
};

/** scanout 0's picture: how many pixels differ from the background (the first one), and a hash */
function scanout()
{
    const display = machine.devices.display;
    display.request_frame(false);
    const s = display.device["virtio_gpu"].scanouts[0];
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
    await wait_for(/localhost login:/, +process.env.A64_SECS || 600);
    console.log("login after %ss", ((Date.now() - start) / 1000).toFixed(1));
    machine.serial0_send("root\n");
    await wait_for(/localhost:~# /, 60);
    await run("cat /sys/class/graphics/fb0/name", "virtio_gpudrmfb");
    await run("for d in /sys/bus/virtio/devices/*; do echo $(cat $d/device) $(basename $(readlink $d/driver)); done", "0x0010 virtio_gpu");
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
    console.log("ok: aarch64, virtio-gpu (virtio-mmio) console %dx%d, %d lit pixels after writing to tty1", after.width, after.height, after.lit);
    process.exit(0);
}
catch(error)
{
    console.error("FAIL", error.message);
    process.exit(1);
}
