#!/usr/bin/env node
// Boots Alpine aarch64 (the official virt ISO's kernel and initramfs, the ISO on virtio-blk) on the ARM core to a
// login, logs in, and checks the machine through the serial console: the kernel, the cores, the CPU features and
// that apk runs (a program of the guest). Needs build/v86-arm64.wasm (make build/v86-arm64.wasm) and the ISO
// (A64_ISO, or downloaded to build/a64/alpine-virt-aarch64.iso).
//
//   node tests/a64/boot.mjs [cores]          (A64_SECS: the limit, default 600)

import fs from "node:fs";
import path from "node:path";
import url from "node:url";

const root = path.join(url.fileURLToPath(new URL(".", import.meta.url)), "../..");
const { A64Machine } = await import(path.join(root, "src/arm/machine.js"));
const { iso_files } = await import(path.join(root, "src/arm/iso.js"));
const cores = +process.argv[2] || 1;
const iso_path = process.env.A64_ISO || path.join(root, "build/a64/alpine-virt-aarch64.iso");
const URL_ISO = "https://dl-cdn.alpinelinux.org/alpine/v3.24/releases/aarch64/alpine-virt-3.24.3-aarch64.iso";

async function iso() {
    if(!fs.existsSync(iso_path))
    {
        fs.mkdirSync(path.dirname(iso_path), { recursive: true });
        const response = await fetch(URL_ISO);
        if(!response.ok) throw new Error("download of " + URL_ISO + ": " + response.status);
        fs.writeFileSync(iso_path, Buffer.from(await response.arrayBuffer()));
    }
    const b = fs.readFileSync(iso_path);
    return b.buffer.slice(b.byteOffset, b.byteOffset + b.byteLength);
}

const image = await iso();
const files = iso_files(image);
const take = name => {
    const [offset, length] = files.get(name);
    return image.slice(offset, offset + length);
};
const machine = await A64Machine.create({
    wasm: fs.readFileSync(path.join(root, "build/v86-arm64.wasm")),
    kernel: take("BOOT/VMLINUZ_VIRT"), initrd: take("BOOT/INITRAMFS_VIRT"),
    cmdline: "console=ttyAMA0 modules=loop,squashfs,sd-mod,virtio-blk,virtio-mmio,virtio_blk,virtio_mmio,isofs",
    memory_size: 512 << 20, cpu_cores: cores, disks: [{ buffer: image, readonly: true }], virtio_rng: true,
});
let serial = "";
machine.add_listener("serial0-output-byte", b => { serial += String.fromCharCode(b); });
const start = Date.now();
machine.run();

const wait_for = (re, seconds) => new Promise((resolve, reject) => {
    const from = Date.now(), mark = serial.length;
    const t = setInterval(() => {
        if(re.test(serial.slice(mark))) { clearInterval(t); resolve(); }
        else if(Date.now() - from > seconds * 1000) { clearInterval(t); reject(new Error("timeout waiting for " + re + "\n" + serial.slice(-800))); }
    }, 200);
});
const run = async (command, expect, seconds = 30) => {
    const mark = serial.length;
    machine.serial0_send(command + "\n");
    const re = new RegExp(expect);
    const t0 = Date.now();
    while(!re.test(serial.slice(mark)))
    {
        if(Date.now() - t0 > seconds * 1000) throw new Error("`" + command + "` did not print " + expect + ":\n" + serial.slice(mark));
        await new Promise(r => setTimeout(r, 100));
    }
};

try
{
    await wait_for(/localhost login:/, +process.env.A64_SECS || 600);
    console.log("login after %ss", ((Date.now() - start) / 1000).toFixed(1));
    machine.serial0_send("root\n");
    await wait_for(/localhost:~# /, 60);
    await run("uname -m", "aarch64\r?\n");
    await run("grep -c ^processor /proc/cpuinfo", "\\n" + cores + "\r?\n");
    await run("grep -o -m1 'aes' /proc/cpuinfo", "aes\r?\n");
    await run("apk --version", "apk-tools");
    await run("echo $((6*7))", "\\n42\r?\n");
    console.log("ok: %d core(s), %s retired instructions", cores, machine.retired_instructions());
    process.exit(0);
}
catch(error)
{
    console.error("FAIL", error.message);
    process.exit(1);
}
