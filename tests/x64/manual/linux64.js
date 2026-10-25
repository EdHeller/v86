#!/usr/bin/env node
// Boots an x86_64 Linux kernel (bzImage) with an initramfs and prints the serial console.
// Not part of the automated run: needs a kernel and initrd supplied by the caller.
//
//   linux64.js <bzImage> <initrd.gz> [timeout seconds] [cmdline]

import fs from "node:fs";
import url from "node:url";

const { V86 } = await import("../../../src/main.js");
const __dirname = url.fileURLToPath(new URL(".", import.meta.url));

const [kernel, initrd, timeout = "120", cmdline = "console=ttyS0 earlyprintk=serial,ttyS0 loglevel=7 nokaslr"] = process.argv.slice(2);
if(!kernel || !initrd)
{
    console.error("usage: linux64.js <bzImage> <initrd.gz> [timeout seconds] [cmdline]");
    process.exit(2);
}

const buf = path => { const b = fs.readFileSync(path); return b.buffer.slice(b.byteOffset, b.byteOffset + b.byteLength); };

const emulator = new V86({
    bios: { buffer: buf(__dirname + "/../../../bios/seabios.bin") },
    vga_bios: { buffer: buf(__dirname + "/../../../bios/vgabios.bin") },
    bzimage: { buffer: buf(kernel) },
    initrd: { buffer: buf(initrd) },
    cmdline,
    memory_size: 256 * 1024 * 1024,
    autostart: true,
    disable_jit: +process.env.DISABLE_JIT,
    log_level: 0,
});

let line = "";
emulator.add_listener("serial0-output-byte", byte => {
    const c = String.fromCharCode(byte);
    if(c === "\n") { console.log(line); line = ""; }
    else line += c;
});

emulator.bus.register("cpu-event-halt", () => {});

setTimeout(() => {
    if(line) console.log(line);
    const cpu = emulator.v86.cpu;
    console.error("--- timeout; eip=%s cs=%s is_64=%s instructions=%d", (cpu.instruction_pointer[0] >>> 0).toString(16), cpu.sreg[1].toString(16), new Uint8Array(cpu.wasm_memory.buffer)[1444], cpu.instruction_counter[0]);
    process.exit(1);
}, +timeout * 1000);
