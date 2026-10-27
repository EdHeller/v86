#!/usr/bin/env node
// Boots a TempleOS ISO in the emulator and reports progress (cpu mode, eip, vga mode).
// Not part of the automated run: needs an ISO supplied by the caller.
//
//   templeos.js <TempleOS.ISO> [timeout seconds] [screenshot.png]

import fs from "node:fs";
import url from "node:url";

const { V86 } = await import("../../../src/main.js");
const __dirname = url.fileURLToPath(new URL(".", import.meta.url));

const [iso, timeout = "120", shot] = process.argv.slice(2);
if(!iso)
{
    console.error("usage: templeos.js <TempleOS.ISO> [timeout seconds] [screenshot.png]");
    process.exit(2);
}

const buf = path => { const b = fs.readFileSync(path); return b.buffer.slice(b.byteOffset, b.byteOffset + b.byteLength); };

// HDA_FILE=<path> (HDA_MB=<size> creates it blank when it does not exist): hard disk image that is
// written back when the run ends; NO_CDROM=1 boots from it alone
const hda_path = process.env.HDA_FILE;
let hda_buffer = null;
if(hda_path)
{
    hda_buffer = fs.existsSync(hda_path) ? buf(hda_path) : new ArrayBuffer((+process.env.HDA_MB || 256) * 1024 * 1024);
}
// restoring a state replaces the buffer of the emulated disk by a copy: always ask the ide
function current_hda()
{
    try { return emulator.v86.cpu.devices.ide.primary.master.buffer.buffer; } catch(e) { return hda_buffer; }
}
process.on("exit", () => { if(hda_path && hda_buffer) fs.writeFileSync(hda_path, Buffer.from(current_hda())); });

const emulator = new V86({
    bios: { buffer: buf(__dirname + "/../../../bios/seabios.bin") },
    vga_bios: { buffer: buf(__dirname + "/../../../bios/vgabios.bin") },
    cdrom: process.env.NO_CDROM ? undefined : { buffer: buf(iso) },
    hda: hda_buffer ? { buffer: hda_buffer } : undefined,
    initial_state: process.env.TOS_LOAD ? { buffer: buf(process.env.TOS_LOAD) } : undefined,
    boot_order: process.env.NO_CDROM ? 0x213 : 0x123,
    memory_size: 512 * 1024 * 1024,
    vga_memory_size: 8 * 1024 * 1024,
    autostart: true,
    disable_jit: +process.env.DISABLE_JIT,
    jit64_threshold: process.env.JIT64 === undefined ? undefined : +process.env.JIT64,
    sync_jit: !!process.env.SYNC_JIT,
    jit64_disable_mask: +process.env.JIT64_MASK || 0,
    wasm_path: process.env.WASM || undefined,
    log_level: 0,
});

let last_screen = null;
emulator.add_listener("screen-set-size", s => { last_screen = s; });
let serial = "";
emulator.add_listener("serial0-output-byte", b => {
    process.stdout.write(String.fromCharCode(b));
    serial += String.fromCharCode(b);
    if(serial.includes("SELFTEST DONE") && process.env.EXIT_ON_DONE)
    {
        console.error("selftest done after %ds", Math.round(process.uptime() * 10) / 10);
        process.exit(0);
    }
});

function pixel(x, y)
{
    const vga = emulator.v86.cpu.devices.vga;
    const a = y * 80 + (x >> 3), bit = 7 - (x & 7);
    return (vga.plane0[a] >> bit & 1) | (vga.plane1[a] >> bit & 1) << 1 | (vga.plane2[a] >> bit & 1) << 2 | (vga.plane3[a] >> bit & 1) << 3;
}

function report()
{
    const cpu = emulator.v86.cpu;
    const m = new Uint8Array(cpu.wasm_memory.buffer);
    const ps2 = cpu.devices.ps2;
    if(process.env.TOS_DEBUG) console.error("ps2 kbd_buffer=%d mouse_buffer=%d stream=%s ready=%s aux=%s cmd=%s", ps2.kbd_buffer.length, ps2.mouse_buffer.length, ps2.enable_keyboard_stream, ps2.next_byte_is_ready, ps2.next_byte_is_aux, ps2.command_register);
    if(hda_buffer)
    {
        const w = new Uint32Array(current_hda());
        let used = 0, last = 0;
        for(let i = 0; i < w.length; i += 1024) if(w[i] | w[i + 1]) { used++; last = i / 1024; }
        console.error("hda: %d of %d pages written to, last page %d", used, w.length / 1024, last);
    }
    const g = i => cpu.wm.exports.get_jit_config(i);
    console.error("jit64 compiled=%d failed=%d run=%d invalidated=%d", g(5), g(6), g(7), g(8));
    console.error("t=%ds eip=%s cs=%s is_64=%s instr=%d screen=%j", Math.round(process.uptime()),
        (cpu.instruction_pointer[0] >>> 0).toString(16), cpu.sreg[1].toString(16), m[1444],
        cpu.instruction_counter[0], last_screen);
}

import zlib from "node:zlib";

// 640x480 16 colour planar mode -> PNG (default VGA palette)
function screenshot(path)
{
    const vga = emulator.v86.cpu.devices.vga;
    const w = 640, h = 480;
    const pal = [0x000000, 0x0000AA, 0x00AA00, 0x00AAAA, 0xAA0000, 0xAA00AA, 0xAA5500, 0xAAAAAA,
                 0x555555, 0x5555FF, 0x55FF55, 0x55FFFF, 0xFF5555, 0xFF55FF, 0xFFFF55, 0xFFFFFF];
    const raw = Buffer.alloc((w * 3 + 1) * h);
    for(let y = 0; y < h; y++)
    {
        raw[y * (w * 3 + 1)] = 0;
        for(let x = 0; x < w; x++)
        {
            const a = y * 80 + (x >> 3), bit = 7 - (x & 7);
            const c = (vga.plane0[a] >> bit & 1) | (vga.plane1[a] >> bit & 1) << 1 |
                      (vga.plane2[a] >> bit & 1) << 2 | (vga.plane3[a] >> bit & 1) << 3;
            const o = y * (w * 3 + 1) + 1 + x * 3;
            raw[o] = pal[c] >> 16; raw[o + 1] = pal[c] >> 8 & 255; raw[o + 2] = pal[c] & 255;
        }
    }
    const crcTable = Array.from({ length: 256 }, (_, n) => { let c = n; for(let k = 0; k < 8; k++) c = c & 1 ? 0xEDB88320 ^ c >>> 1 : c >>> 1; return c >>> 0; });
    const crc = b => { let c = ~0; for(const x of b) c = crcTable[(c ^ x) & 255] ^ c >>> 8; return ~c >>> 0; };
    const chunk = (t, d) => { const l = Buffer.alloc(4); l.writeUInt32BE(d.length); const td = Buffer.concat([Buffer.from(t), d]); const c = Buffer.alloc(4); c.writeUInt32BE(crc(td)); return Buffer.concat([l, td, c]); };
    const ihdr = Buffer.alloc(13); ihdr.writeUInt32BE(w, 0); ihdr.writeUInt32BE(h, 4); ihdr[8] = 8; ihdr[9] = 2;
    fs.writeFileSync(path, Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk("IHDR", ihdr), chunk("IDAT", zlib.deflateSync(raw)), chunk("IEND", Buffer.alloc(0))]));
}

// US layout, scancode set 1
const SC = {};
SC["\x1b"] = [0x01, false];
// arrow keys are extended (0xE0-prefixed) scancodes; type: text writes them as \u \d \l \r
SC["\x11"] = [0x48, false, true]; SC["\x12"] = [0x50, false, true]; SC["\x13"] = [0x4B, false, true]; SC["\x14"] = [0x4D, false, true];
"1234567890-=".split("").forEach((c, i) => SC[c] = [0x02 + i, false]);
"qwertyuiop[]".split("").forEach((c, i) => SC[c] = [0x10 + i, false]);
"asdfghjkl;'".split("").forEach((c, i) => SC[c] = [0x1E + i, false]);
"zxcvbnm,./".split("").forEach((c, i) => SC[c] = [0x2C + i, false]);
"!@#$%^&*()_+".split("").forEach((c, i) => SC[c] = [0x02 + i, true]);
"QWERTYUIOP{}".split("").forEach((c, i) => SC[c] = [0x10 + i, true]);
"ASDFGHJKL:\"".split("").forEach((c, i) => SC[c] = [0x1E + i, true]);
"ZXCVBNM<>?".split("").forEach((c, i) => SC[c] = [0x2C + i, true]);
SC["\b"] = [0x0E, false]; SC[" "] = [0x39, false]; SC["\n"] = [0x1C, false]; SC["\\"] = [0x2B, false]; SC["`"] = [0x29, false];

async function type(text)
{
    for(const ch of text)
    {
        const [code, shift, ext] = SC[ch] || [0x39, false];
        if(ext) emulator.bus.send("keyboard-code", 0xE0);
        if(shift)
        {
            emulator.bus.send("keyboard-code", 0x2A);
            await new Promise(r => setTimeout(r, 150));   // the guest polls: let it see shift first
        }
        emulator.bus.send("keyboard-code", code);
        await new Promise(r => setTimeout(r, +process.env.TOS_HOLD || 150));
        if(ext) emulator.bus.send("keyboard-code", 0xE0);
        emulator.bus.send("keyboard-code", code | 0x80);
        if(shift) emulator.bus.send("keyboard-code", 0xAA);
        await new Promise(r => setTimeout(r, 150));
    }
}

// TOS_KEYS='[[seconds, "text"], ...]' types text at the given uptimes
for(const [sec, text] of JSON.parse(process.env.TOS_KEYS || "[]"))
{
    setTimeout(() => type(text), sec * 1000);
}

// TOS_AUTO='["text to type", ...]': answers the install prompt with "n" once it shows up, then types
// each entry 90 seconds after the previous one
function ink(x0, y0, x1, y1)
{
    const vga = emulator.v86.cpu.devices.vga;
    let n = 0;
    for(let y = y0; y < y1; y++) for(let x = x0; x < x1; x += 8) n += vga.plane0[y * 80 + (x >> 3)] ? 1 : 0;
    return n;
}
if(process.env.TOS_AUTO)
{
    const lines = JSON.parse(process.env.TOS_AUTO);
    let stage = 0;
    const t = setInterval(async () => {
        if(stage === 0 && emulator.v86.cpu.devices.vga.graphical_mode && ink(8, 132, 280, 146) > 15 && ink(320, 100, 330, 400) > 30)
        {
            stage = 1;
            console.error("install prompt at t=%ds", Math.round(process.uptime()));
            await new Promise(r => setTimeout(r, 3000));
            await type("n");
            await new Promise(r => setTimeout(r, 150000));
            for(const l of lines) { await type(l); await new Promise(r => setTimeout(r, 90000)); }
            stage = 2;
        }
    }, 2000);
}

// TOS_RAW='[[seconds, [scancode, ...]], ...]': raw scancodes (set 1)
for(const [sec, codes] of JSON.parse(process.env.TOS_RAW || "[]"))
{
    setTimeout(async () => { for(const c of codes) { emulator.bus.send("keyboard-code", c); await new Promise(r => setTimeout(r, 120)); } }, sec * 1000);
}

// TOS_MOUSE='[[seconds, dx, dy, buttons], ...]': relative mouse move, then buttons (bit0 left)
for(const [sec, dx, dy, b] of JSON.parse(process.env.TOS_MOUSE || "[]"))
{
    setTimeout(async () => {
        if(dx || dy) emulator.bus.send("mouse-delta", [dx, -dy]);
        emulator.bus.send("mouse-click", [!!(b & 1), !!(b & 2), !!(b & 4)]);
        await new Promise(r => setTimeout(r, 300));
        emulator.bus.send("mouse-click", [false, false, false]);
    }, sec * 1000);
}

// TOS_FPS=<interval seconds> [TOS_FPS_LOG=<csv file>]: our own frame counter. The planes of the 640x480 mode are
// hashed about every 5 ms; every change of the hash is a frame that was really drawn, counted in wall
// clock time (TempleOS' own fps counter runs on its emulated clock and is not trustworthy here).
// Also reports guest MIPS and the share of instructions retired inside jit64 blocks.
if(process.env.TOS_FPS)
{
    const interval = +process.env.TOS_FPS;
    let frames = 0, last_hash = 0, last_t = Date.now(), last_instr = 0, last_jit = 0, total_frames = 0;
    const started = Date.now();
    const log = process.env.TOS_FPS_LOG;
    if(log) fs.writeFileSync(log, "wall_s,fps,mips,jit_percent\n");
    const hash = () => {
        const vga = emulator.v86 && emulator.v86.cpu.devices.vga;
        if(!vga || !vga.graphical_mode) return 0;
        let h = 0;
        for(const plane of [vga.plane0, vga.plane1, vga.plane2, vga.plane3])
        {
            const w = new Int32Array(plane.buffer, plane.byteOffset, 38400 >> 2);
            for(let i = 0; i < w.length; i++) h = Math.imul(h ^ w[i], 16777619) + i | 0;
        }
        return h;
    };
    setInterval(() => { const h = hash(); if(h !== last_hash) { frames++; total_frames++; last_hash = h; } }, 5);
    setInterval(() => {
        if(!emulator.v86) return;
        const cpu = emulator.v86.cpu, now = Date.now(), dt = (now - last_t) / 1000;
        const instr = cpu.instruction_counter[0] >>> 0, jit = cpu.wm.exports.get_jit_config(13) >>> 0;
        const di = (instr - last_instr) >>> 0, dj = (jit - last_jit) >>> 0;
        const row = [((now - started) / 1000).toFixed(0), (frames / dt).toFixed(1), (di / dt / 1e6).toFixed(1), (di ? dj / di * 100 : 0).toFixed(0)];
        console.error("fps=%s mips=%s jit=%s%% (t=%ss)", row[1], row[2], row[3], row[0]);
        if(process.env.TOS_SLOW) console.error("  slow accesses read[cross,nomatch,nofast] write[...] last entry fast/perm/user, phys: " +
            Array.from({ length: 8 }, (_, i) => (cpu.wm.exports.get_jit_config(20 + i) >>> 0).toString(16)).join(" "));
        if(log) fs.appendFileSync(log, row.join(",") + "\n");
        frames = 0; last_t = now; last_instr = instr; last_jit = jit;
    }, interval * 1000);
}

// TOS_FAKE_MOUSE=<start seconds>,<events per second>: from then on a synthetic mouse draws circles with the
// left button held, like a user scribbling in Doodle. Profile with `node --cpu-prof`.
if(process.env.TOS_FAKE_MOUSE)
{
    const [start, hz] = process.env.TOS_FAKE_MOUSE.split(",").map(Number);
    setTimeout(() => {
        emulator.bus.send("mouse-click", [true, false, false]);
        let t = 0;
        setInterval(() => {
            t += 0.15;
            emulator.bus.send("mouse-delta", [Math.round(8 * Math.cos(t)), Math.round(8 * Math.sin(t))]);
        }, 1000 / (hz || 100));
    }, start * 1000);
}

// TOS_IRQ0_MS=<n>: deliver at most one timer interrupt per n milliseconds (experiment)
if(process.env.TOS_IRQ0_MS)
{
    emulator.add_listener("emulator-ready", () => {
        const cpu = emulator.v86.cpu;
        const raise = cpu.device_raise_irq.bind(cpu);
        let last = 0;
        cpu.device_raise_irq = n => {
            if(n === 0) { const now = performance.now(); if(now - last < +process.env.TOS_IRQ0_MS) return; last = now; }
            return raise(n);
        };
    });
}

// TOS_CMD=<file>: control channel. Once a second the file is read and deleted; each line is
//   type:<text>   types the text (\n is an enter key)
//   shot:<path>   writes a screenshot
//   mouse:dx,dy[,b]  relative mouse move (y down) and an optional click; down:<b> holds buttons, down:0 releases
//   save:<path>   writes the hard disk image
//   quit          saves the disk (HDA_FILE) and exits
if(process.env.TOS_CMD)
{
    setInterval(async () => {
        if(!fs.existsSync(process.env.TOS_CMD)) return;
        const lines = fs.readFileSync(process.env.TOS_CMD, "utf8").split("\n");
        fs.unlinkSync(process.env.TOS_CMD);
        for(const l of lines)
        {
            if(l.startsWith("type:")) await type(l.slice(5).replace(/\\n/g, "\n").replace(/\\e/g, "\x1b").replace(/\\u/g, "\x11").replace(/\\d/g, "\x12").replace(/\\l/g, "\x13").replace(/\\r/g, "\x14"));
            else if(l.startsWith("mouse:"))
            {
                // mouse:dx,dy[,buttons]  moves (screen coordinates, y down), then holds the buttons for 300 ms
                const [dx, dy, b] = l.slice(6).split(",").map(Number);
                if(dx || dy) emulator.bus.send("mouse-delta", [dx, -dy]);
                if(b)
                {
                    emulator.bus.send("mouse-click", [!!(b & 1), !!(b & 2), !!(b & 4)]);
                    await new Promise(r => setTimeout(r, 300));
                    emulator.bus.send("mouse-click", [false, false, false]);
                }
            }
            else if(l.startsWith("down:")) emulator.bus.send("mouse-click", [!!(+l.slice(5) & 1), !!(+l.slice(5) & 2), false]);
            if(l.startsWith("mouse:") || l.startsWith("down:")) await new Promise(r => setTimeout(r, 250));
            else if(l.startsWith("shot:")) screenshot(l.slice(5));
            else if(l.startsWith("save:")) fs.writeFileSync(l.slice(5), Buffer.from(current_hda()));
            else if(l.trim() === "quit") process.exit(0);
        }
    }, 1000);
}

// TOS_SHOTS='[seconds, ...]' writes <screenshot>.<seconds>.png at those uptimes
for(const sec of JSON.parse(process.env.TOS_SHOTS || "[]"))
{
    setTimeout(() => shot && screenshot(shot.replace(/\.png$/, "") + "." + sec + ".png"), sec * 1000);
}

// TOS_SAVE=<file> TOS_SAVE_AT=<seconds>: write a state snapshot
if(process.env.TOS_SAVE)
{
    setTimeout(async () => {
        const st = await emulator.save_state();
        fs.writeFileSync(process.env.TOS_SAVE, Buffer.from(st));
        console.error("state saved (%d bytes)", st.byteLength);
    }, +process.env.TOS_SAVE_AT * 1000);
}

const iv = setInterval(report, 10000);
setTimeout(() => {
    clearInterval(iv);
    report();
    if(process.env.TOS_PIXEL) console.error("PIXEL=" + pixel(300, 200));
    if(shot) screenshot(shot);
    process.exit(0);
}, +timeout * 1000);
