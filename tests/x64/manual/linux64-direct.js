#!/usr/bin/env node
// Boots an x86_64 Linux kernel directly into 64-bit mode (like firecracker does), skipping the
// self-decompressing bzImage stub, which is very slow in the interpreter. Prints the serial console.
//
//   linux64-direct.js <vmlinux (ELF)> <bzImage (for its setup header)> <initrd.gz> [timeout seconds] [cmdline]
//
// vmlinux can be extracted from a bzImage by decompressing its payload (zstd/xz/gzip).

import fs from "node:fs";
import url from "node:url";
import { execFileSync } from "node:child_process";

const { V86 } = await import("../../../src/main.js");
const __dirname = url.fileURLToPath(new URL(".", import.meta.url));

const [vmlinux_path, bzimage_path, initrd_path, timeout = "300",
    cmdline = "console=ttyS0 earlyprintk=serial,ttyS0 nokaslr loglevel=7 rdinit=/init"] = process.argv.slice(2);
if(!initrd_path)
{
    console.error("usage: linux64-direct.js <vmlinux> <bzImage> <initrd.gz> [timeout seconds] [cmdline]");
    process.exit(2);
}

const MEMORY = 256 * 1024 * 1024;
const BOOT_PARAMS = 0x10000;
const CMDLINE = 0x20000;
const INITRD = 0x6000000;

const stub = (() => {
    execFileSync("nasm", ["-f", "bin", __dirname + "/boot64.asm", "-o", "/tmp/boot64.bin"]);
    return fs.readFileSync("/tmp/boot64.bin");
})();
// `params` and `entry` are the last 16 bytes of the stub
const PARAMS_OFFSET = stub.length - 16;

const vmlinux = fs.readFileSync(vmlinux_path);
const bzimage = fs.readFileSync(bzimage_path);
const initrd = fs.readFileSync(initrd_path);

const emulator = new V86({
    bios: { buffer: new Uint8Array(fs.readFileSync(__dirname + "/../../../bios/seabios.bin")).buffer },
    memory_size: MEMORY,
    autostart: false,
    disable_jit: +process.env.DISABLE_JIT,
    log_level: 0,
});

let serial_line = "";
emulator.add_listener("serial0-output-byte", byte => {
    const c = String.fromCharCode(byte);
    if(c === "\n") { console.log(serial_line); serial_line = ""; }
    else serial_line += c;
});

emulator.add_listener("emulator-loaded", () => {
    const cpu = emulator.v86.cpu;
    cpu.reboot_internal();
    cpu.reset_memory();
    const mem = cpu.mem8;

    // ELF program headers
    const dv = new DataView(vmlinux.buffer, vmlinux.byteOffset, vmlinux.byteLength);
    const entry = dv.getBigUint64(24, true);
    const phoff = Number(dv.getBigUint64(32, true));
    const phentsize = dv.getUint16(54, true);
    const phnum = dv.getUint16(56, true);
    for(let i = 0; i < phnum; i++)
    {
        const ph = phoff + i * phentsize;
        if(dv.getUint32(ph, true) !== 1) continue;
        const off = Number(dv.getBigUint64(ph + 8, true));
        const paddr = Number(dv.getBigUint64(ph + 24, true));
        const filesz = Number(dv.getBigUint64(ph + 32, true));
        if(paddr + filesz > MEMORY) throw new Error("segment does not fit in memory");
        mem.set(vmlinux.subarray(off, off + filesz), paddr);
    }

    mem.set(initrd, INITRD);
    mem.set(new TextEncoder().encode(cmdline + "\0"), CMDLINE);

    // boot_params: copy the setup header of the bzImage and fill in the loader fields
    const hdr_end = 0x202 + bzimage[0x201];
    const bp = mem.subarray(BOOT_PARAMS, BOOT_PARAMS + 4096);
    bp.fill(0);
    bp.set(bzimage.subarray(0x1f1, hdr_end), 0x1f1);
    const bpv = new DataView(bp.buffer, bp.byteOffset, 4096);
    bp[0x210] = 0xFF;                             // type_of_loader: undefined
    bp[0x211] |= 1 | 0x80;                        // LOADED_HIGH | CAN_USE_HEAP
    bpv.setUint16(0x224, 0xde00, true);           // heap_end_ptr
    bpv.setUint32(0x228, CMDLINE, true);          // cmd_line_ptr
    bpv.setUint32(0x218, INITRD, true);           // ramdisk_image
    bpv.setUint32(0x21c, initrd.length, true);    // ramdisk_size
    // e820: conventional memory and everything above 1MB
    const e820 = [[0, 0x9F000, 1], [0x100000, MEMORY - 0x100000, 1]];
    bp[0x1e8] = e820.length;
    e820.forEach(([addr, size, type], i) => {
        bpv.setBigUint64(0x2d0 + i * 20, BigInt(addr), true);
        bpv.setBigUint64(0x2d0 + i * 20 + 8, BigInt(size), true);
        bpv.setUint32(0x2d0 + i * 20 + 16, type, true);
    });

    // trampoline at 0x7c00, entered through the reset vector
    mem.set(stub, 0x7C00);
    const sv = new DataView(mem.buffer, mem.byteOffset + 0x7C00 + PARAMS_OFFSET, 16);
    sv.setBigUint64(0, BigInt(BOOT_PARAMS), true);
    sv.setBigUint64(8, entry, true);
    mem.set([0xEA, 0x00, 0x7C, 0x00, 0x00], 0xFFFF0);

    emulator.run();
});

setTimeout(() => {
    if(serial_line) console.log(serial_line);
    const cpu = emulator.v86.cpu;
    console.error("--- timeout; eip=%s is_64=%s instructions=%d", (cpu.instruction_pointer[0] >>> 0).toString(16), new Uint8Array(cpu.wasm_memory.buffer)[1444], cpu.instruction_counter[0]);
    process.exit(1);
}, +timeout * 1000);
