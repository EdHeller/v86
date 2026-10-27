#!/usr/bin/env node
// Boots an Alpine Linux x86_64 ISO (https://alpinelinux.org/downloads/, "Virtual") through SeaBIOS and
// reports progress: text screen (when it changes), serial output, cpu mode and jit64 statistics.
// Not part of the automated run: needs an ISO supplied by the caller.
//
//   alpine.js <alpine-virt-x86_64.iso> [timeout seconds]
//
// Environment:
//   ALP_KEYS='[[seconds, "text"], ...]'  types on the keyboard (\n = enter)
//   ALP_SERIAL='[[seconds, "text"], ...]' sends on the first serial port
//   JIT64=<n> (hotness threshold, 0 = interpreter only), DISABLE_JIT=1 (32-bit jit off), WASM=<path>
//   HDA_FILE=<path> HDA_MB=<size>        blank/persistent hard disk
//   MEM_MB=<n> (default 512), QUIET=1 (no screen dumps)
//   ALP_SAVE=<file>:<seconds>  writes a state snapshot at that uptime; ALP_LOAD=<file> starts from one
//   ALP_KERNEL=<bzImage> ALP_INITRD=<initramfs> ALP_CMDLINE=<text>  boot the kernel directly (skips isolinux)

import fs from "node:fs";
import url from "node:url";

const { V86 } = await import("../../../src/main.js");
const __dirname = url.fileURLToPath(new URL(".", import.meta.url));

const [iso, timeout = "120"] = process.argv.slice(2);
if(!iso) { console.error("usage: alpine.js <alpine-virt-x86_64.iso> [timeout seconds]"); process.exit(2); }

const buf = path => { const b = fs.readFileSync(path); return b.buffer.slice(b.byteOffset, b.byteOffset + b.byteLength); };

let hda;
if(process.env.HDA_FILE)
{
    hda = fs.existsSync(process.env.HDA_FILE) ? buf(process.env.HDA_FILE) : new ArrayBuffer((+process.env.HDA_MB || 512) * 1024 * 1024);
    process.on("exit", () => fs.writeFileSync(process.env.HDA_FILE, Buffer.from(emulator.v86.cpu.devices.ide.primary.master.buffer.buffer)));
}

const emulator = new V86({
    wasm_path: process.env.WASM,
    bios: { buffer: buf(__dirname + "/../../../bios/seabios.bin") },
    vga_bios: { buffer: buf(__dirname + "/../../../bios/vgabios.bin") },
    cdrom: { buffer: buf(iso) },
    hda: hda ? { buffer: hda } : undefined,
    bzimage: process.env.ALP_KERNEL ? { buffer: buf(process.env.ALP_KERNEL) } : undefined,
    initrd: process.env.ALP_INITRD ? { buffer: buf(process.env.ALP_INITRD) } : undefined,
    cmdline: process.env.ALP_CMDLINE,
    initial_state: process.env.ALP_LOAD ? { buffer: buf(process.env.ALP_LOAD) } : undefined,
    memory_size: (+process.env.MEM_MB || 512) * 1024 * 1024,
    vga_memory_size: 8 * 1024 * 1024,
    autostart: true,
    disable_jit: +process.env.DISABLE_JIT,
    jit64_threshold: process.env.JIT64 === undefined ? undefined : +process.env.JIT64,
});

let serial = "";
emulator.add_listener("emulator-ready", () => {
    if(process.env.ALP_TRACE) emulator.v86.cpu.wm.exports.set_jit_config(10, 1);
    // ALP_TRACE_SYSCALL=<number>: log every guest syscall with this number (202 = futex)
    // ALP_WATCH=<hex virtual address>: debug build logs every guest write that touches it (interpreter only: JIT64=4294967295)
    if(process.env.ALP_WATCH)
    {
        const a = BigInt("0x" + process.env.ALP_WATCH);
        emulator.v86.cpu.wm.exports.set_jit_config(15, Number(a & 0xFFFFFFFFn));
        emulator.v86.cpu.wm.exports.set_jit_config(16, Number(a >> 32n));
    }
    if(process.env.ALP_TRACE_SYSCALL) emulator.v86.cpu.wm.exports.set_jit_config(14, +process.env.ALP_TRACE_SYSCALL + 1);
});
emulator.add_listener("serial0-output-byte", b => { process.stdout.write(String.fromCharCode(b)); serial += String.fromCharCode(b); });

for(const [sec, text] of JSON.parse(process.env.ALP_KEYS || "[]"))
    setTimeout(() => emulator.keyboard_send_text(text), sec * 1000);
// ALP_SYSRQ='[[seconds, "w"], ...]': Alt+SysRq+<key> on the emulated keyboard (w = blocked tasks, t = all tasks, l = cpu backtrace)
const SYSRQ_SCANCODES = { w: 0x11, t: 0x14, l: 0x26, m: 0x32, p: 0x19 };
for(const [sec, key] of JSON.parse(process.env.ALP_SYSRQ || "[]"))
    setTimeout(() => { const c = SYSRQ_SCANCODES[key]; emulator.keyboard_send_scancodes([0x38, 0x54, c, c | 0x80, 0xD4, 0xB8]); }, sec * 1000);
for(const [sec, text] of JSON.parse(process.env.ALP_SERIAL || "[]"))
    setTimeout(() => emulator.serial0_send(text), sec * 1000);

if(process.env.ALP_SAVE)
{
    const [file, sec] = process.env.ALP_SAVE.split(":");
    setTimeout(async () => {
        const st = await emulator.save_state();
        fs.writeFileSync(file, Buffer.from(st));
        console.error("state saved: %s (%d bytes)", file, st.byteLength);
    }, +sec * 1000);
}

const t0 = Date.now();
let last_screen = "";
const wasm_mem = () => new Uint8Array(emulator.v86.cpu.wasm_memory.buffer);
const status = () => {
    const cpu = emulator.v86.cpu;
    const mem = wasm_mem();
    const g = i => cpu.wm.exports.get_jit_config(i);
    const stats = `jit64 compiled=${g(5)} failed=${g(6)} run=${g(7) >>> 0} jit_insns=${g(13) >>> 0} invalidated=${g(8)}`;
    return `t=${((Date.now() - t0) / 1000).toFixed(0)}s eip=${(cpu.instruction_pointer[0] >>> 0).toString(16)} is_64=${mem[1444]} instr=${cpu.instruction_counter[0]} ${stats}`;
};

setInterval(() => {
    if(!emulator.v86) return;
    if(!process.env.QUIET)
    {
        const text = emulator.screen_adapter.get_text_screen().map(l => l.replace(/\s+$/, "")).join("\n").replace(/\n+$/, "");
        if(text !== last_screen && text) { console.error("---- screen ----\n" + text + "\n----------------"); last_screen = text; }
    }
    console.error(status());
}, 10000);

// ALP_DUMP=<hex virtual address>:<bytes>[,...] at exit: dump guest virtual memory, walked through the current cr3
function dump_virtual() {
    const cpu = emulator.v86.cpu, mem = cpu.mem8, cr3 = (cpu.cr[3] >>> 0) & ~0xfff;
    const q = a => { let v = 0n; for(let i = 7; i >= 0; i--) v = v << 8n | BigInt(mem[a + i]); return v; };
    const phys = va => {
        let t = BigInt(cr3);
        for(let lvl = 3; lvl >= 0; lvl--)
        {
            const e = q(Number(t) + Number((va >> BigInt(12 + 9 * lvl)) & 0x1ffn) * 8);
            if(!(e & 1n)) return null;
            if(lvl > 0 && e & 0x80n) { const sz = (1n << BigInt(12 + 9 * lvl)) - 1n; return (e & 0x000FFFFFFFFFF000n & ~sz) | (va & sz); }
            t = e & 0x000FFFFFFFFFF000n;
            if(lvl === 0) return t | (va & 0xfffn);
        }
    };
    console.error("cr3=" + cr3.toString(16));
    for(const spec of process.env.ALP_DUMP.split(","))
    {
        const [a, n] = spec.split(":"); const base = BigInt("0x" + a);
        let line = "";
        for(let i = 0; i < +n; i += 4)
        {
            const p = phys(base + BigInt(i));
            if(i % 32 === 0) { if(line) console.error(line); line = (base + BigInt(i)).toString(16) + ": "; }
            line += p === null ? "-------- " : ((mem[Number(p)] | mem[Number(p) + 1] << 8 | mem[Number(p) + 2] << 16 | mem[Number(p) + 3] << 24) >>> 0).toString(16).padStart(8, "0") + " ";
        }
        console.error(line);
    }
}
// ALP_UNSUPPORTED=1: at the end print the opcodes that stopped 64-bit jit block discovery most often
function unsupported() {
    const g = i => emulator.v86.cpu.wm.exports.get_jit_config(1000 + i);
    const rows = [];
    for(let i = 0; i < 512; i++) { const n = g(i); if(n) rows.push([n, i < 256 ? i.toString(16).padStart(2, "0") : "0f " + (i - 256).toString(16).padStart(2, "0")]); }
    rows.sort((a, b) => b[0] - a[0]);
    console.error("opcodes that ended jit64 block discovery (count opcode):");
    for(const [n, op] of rows.slice(0, 25)) console.error("  %s  %s", String(n).padStart(8), op);
}
setTimeout(() => { if(process.env.ALP_TRACE) dump(); if(process.env.ALP_UNSUPPORTED) unsupported(); if(process.env.ALP_DUMP) dump_virtual(); process.exit(0); }, +timeout * 1000);

// on a crash print the registers and the code around eip, so the failing instruction can be disassembled
function dump() {
    try {
        const cpu = emulator.v86.cpu;
        const m = new Uint8Array(cpu.wasm_memory.buffer);
        const eip = cpu.instruction_pointer[0] >>> 0;
        console.error(status());
        console.error("regs", Array.from(cpu.reg32).map(x => (x >>> 0).toString(16)).join(" "));
        const base = (eip & ~0xf) - 0x30;
        console.error("code @0x" + base.toString(16) + ":", Buffer.from(cpu.mem8.subarray(base, base + 0x80)).toString("hex"));
        if(process.env.ALP_TRACE)
        {
            const ex = cpu.wm.exports, base = ex.get_jit_config(11), pos = ex.get_jit_config(12);
            const w = new Uint32Array(cpu.wasm_memory.buffer, base, 256 * 4);
            console.error("last instructions (oldest first): rip cs/mode eflags");
            for(let k = 0; k < 256; k++)
            {
                const i = ((pos + k) % 256) * 4;
                if(pos + k < 256 && pos < 256 && k < 256 - pos) continue;
                const rip = (BigInt(w[i + 1]) << 32n) | BigInt(w[i]);
                console.error("  %s cs=%s %s flags=%s", rip.toString(16).padStart(8), (w[i + 2] & 0xffff).toString(16), (w[i + 2] >> 16 & 1 ? "64" : "  ") + (w[i + 2] >> 17 & 1 ? " lma" : ""), w[i + 3].toString(16));
            }
        }
    } catch(x) { console.error("dump failed:", x.message); }
}
process.on("uncaughtException", e => {
    console.error("CRASH:", String(e.message).split("\n")[0]);
    if(process.env.ALP_STACK) console.error(String(e.stack).split("\n").slice(0, +process.env.ALP_STACK).join("\n"));
    dump();
    process.exit(1);
});
