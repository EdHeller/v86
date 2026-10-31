#!/usr/bin/env node
// Boots the official Alpine aarch64 ISO through UEFI on the ARM core: edk2's ArmVirtQemu firmware
// (bios/aavmf-code.fd) from the flash, with its variable store (a copy of bios/aavmf-vars.fd) in the second flash
// unit, finds the ISO on virtio-blk, runs its GRUB, which loads the kernel (an EFI zboot image) and the initramfs;
// no direct kernel boot. Then a login on the serial console, `uname -m`, and checks that the kernel came up on EFI
// (the variables through its runtime services, the device tree the firmware passed on) and that the firmware wrote its variables to the flash.
// Needs build/v86-arm64.wasm and the ISO (A64_ISO, or downloaded to build/a64/alpine-virt-aarch64.iso).
//
//   node tests/a64/uefi.mjs [cores]          (A64_SECS: the limit, default 600)

import fs from "node:fs";
import path from "node:path";
import url from "node:url";

const root = path.join(url.fileURLToPath(new URL(".", import.meta.url)), "../..");
const { A64Machine } = await import(path.join(root, "src/arm/machine.js"));
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
const code = fs.readFileSync(path.join(root, "bios/aavmf-code.fd"));
const template = new Uint8Array(fs.readFileSync(path.join(root, "bios/aavmf-vars.fd")));
const vars = template.slice();
const machine = await A64Machine.create({
    wasm: fs.readFileSync(path.join(root, "build/v86-arm64.wasm")),
    firmware: { code, vars },
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
    const limit = +process.env.A64_SECS || 600;
    await wait_for(/UEFI firmware \(version edk2/, 60);
    await wait_for(/GNU GRUB/, limit);
    console.log("GRUB after %ss", ((Date.now() - start) / 1000).toFixed(1));
    await wait_for(/localhost login:/, limit);
    console.log("login after %ss", ((Date.now() - start) / 1000).toFixed(1));
    machine.serial0_send("root\n");
    await wait_for(/localhost:~# /, 60);
    await run("uname -m", "\\naarch64\r?\n");
    // (booted by the firmware: its variables through EFI's runtime services, its system table, the device tree it passed on)
    await run("ls /sys/firmware/efi/efivars/ | grep -c ^BootOrder-", "\\n1\r?\n");
    await run("dmesg | grep -c 'EFI v2'", "\\n[1-9]\r?\n");
    await run("cat /proc/device-tree/model; echo", "linux,dummy-virt");
    await run("echo $((6*7))", "\\n42\r?\n");
    if(vars.every((b, i) => b === template[i])) throw new Error("the firmware wrote nothing to its variable store");
    console.log("ok: UEFI, %d core(s), %s retired instructions", cores, machine.retired_instructions());
    process.exit(0);
}
catch(error)
{
    console.error("FAIL", error.message);
    process.exit(1);
}
