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

const emulator = new V86({
    bios: { buffer: buf(__dirname + "/../../../bios/seabios.bin") },
    vga_bios: { buffer: buf(__dirname + "/../../../bios/vgabios.bin") },
    cdrom: { buffer: buf(iso) },
    boot_order: 0x123,
    memory_size: 512 * 1024 * 1024,
    vga_memory_size: 8 * 1024 * 1024,
    autostart: true,
    disable_jit: +process.env.DISABLE_JIT,
    log_level: 0,
});

let last_screen = null;
emulator.add_listener("screen-set-size", s => { last_screen = s; });
emulator.add_listener("serial0-output-byte", b => process.stdout.write(String.fromCharCode(b)));

function report()
{
    const cpu = emulator.v86.cpu;
    const m = new Uint8Array(cpu.wasm_memory.buffer);
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

const iv = setInterval(report, 10000);
setTimeout(() => {
    clearInterval(iv);
    report();
    if(shot) screenshot(shot);
    process.exit(0);
}, +timeout * 1000);
