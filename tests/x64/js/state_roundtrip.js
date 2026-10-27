#!/usr/bin/env node
// EXPECT: pass
// DESC: save_state / restore_state keeps the 64-bit mode state (upper register halves, r8-r15, msrs, mode, xcr0)

import assert from "node:assert/strict";
import fs from "node:fs";
import url from "node:url";
import { execFileSync } from "node:child_process";

const { V86 } = await import("../../../src/main.js");
const __dirname = url.fileURLToPath(new URL(".", import.meta.url));
process.on("unhandledRejection", exn => { throw exn; });

const tmp = fs.mkdtempSync("/tmp/v86-state-");
fs.writeFileSync(tmp + "/t.asm", `
%include "inc/long.inc"
test_main:
    mov rax, 0x1122334455667788
    mov r8, 0x99AABBCCDDEEFF00
    mov r15, 0x0123456789ABCDEF
    mov rcx, 0xFEDCBA9876543210
    mov ecx, 0xC0000082
    mov eax, 0x11111111
    mov edx, 0x00007FFF
    wrmsr                       ; LSTAR
    mov rax, 0x1122334455667788
    pxor xmm9, xmm9
    mov rbx, 0xCAFEBABE12345678
    movq xmm9, rbx
    mov rax, cr4                ; enable xsave and the sse state component: xcr0 is part of the saved state
    or rax, 1 << 18
    mov cr4, rax
    xor ecx, ecx
    mov eax, 3
    xor edx, edx
    xsetbv
    PASS
`);
execFileSync("nasm", ["-f", "bin", "-i", __dirname + "/../", tmp + "/t.asm", "-o", tmp + "/t.bin"]);
const bin = fs.readFileSync(tmp + "/t.bin");

const buf = path => { const b = fs.readFileSync(path); return b.buffer.slice(b.byteOffset, b.byteOffset + b.byteLength); };
const make = () => new V86({
    bios: { buffer: buf(__dirname + "/../../../bios/seabios.bin") },
    vga_bios: { buffer: buf(__dirname + "/../../../bios/vgabios.bin") },
    autostart: false,
    memory_size: 32 * 1024 * 1024,
    log_level: 0,
});
const a = make();
await new Promise(res => a.add_listener("emulator-ready", res));
const cpu = a.v86.cpu;
// let the bios initialise the devices first, so that the state of a normally set up machine is saved
a.run();
await new Promise(res => setTimeout(res, 1500));
a.stop();
cpu.reboot_internal();
cpu.reset_memory();
cpu.mem8.set(bin, 0x7C00);
cpu.mem8.set([0xEA, 0x00, 0x7C, 0x00, 0x00], 0xFFFF0);
await new Promise(res => { a.bus.register("cpu-event-halt", res); a.run(); });
a.stop();
assert.equal(cpu.mem32s[0x600 >> 2] >>> 0, 0x53534150, "test program ran to completion");

const snapshot = c => ({
    long: Array.from(c.long_mode_state),
    reg32: Array.from(c.reg32),
    xmm: Array.from(c.reg_xmm32s),
    efer: Array.from(c.efer),
});
const before = snapshot(cpu);
assert.equal(before.long[1444 - 1344 >> 2] & 0xFF, 1, "was in 64-bit mode");
assert.equal(before.long[46], 3, "xcr0 was set by the test program");

const state = await a.save_state();
const b = make();
await new Promise(res => b.add_listener("emulator-ready", res));
b.run();
await new Promise(res => setTimeout(res, 1500));
b.stop();
await b.restore_state(state);
assert.deepEqual(snapshot(b.v86.cpu), before);
fs.rmSync(tmp, { recursive: true, force: true });
console.log("ok");
process.exit(0);
