#!/usr/bin/env node
// EXPECT: pass
// DESC: with acpi the PCI hotplug registers of the firmware's DSDT (0xAE00 PCIU, 0xAE04 PCID, ...) report no insertions/removals
// (they read as all ones before: Linux's acpiphp ejected every device, the IDE controller vanished and Alpine could not find its boot media)

import assert from "node:assert/strict";
import fs from "node:fs";
import url from "node:url";

const { V86 } = await import("../../../src/main.js");
const __dirname = url.fileURLToPath(new URL(".", import.meta.url));
const buf = path => { const b = fs.readFileSync(path); return b.buffer.slice(b.byteOffset, b.byteOffset + b.byteLength); };
const e = new V86({
    bios: { buffer: buf(__dirname + "/../../../bios/seabios.bin") },
    vga_bios: { buffer: buf(__dirname + "/../../../bios/vgabios.bin") },
    autostart: false, acpi: true, memory_size: 32 * 1024 * 1024, log_level: 0,
});
await new Promise(res => e.add_listener("emulator-ready", res));
const io = e.v86.cpu.io;
assert.equal(io.port_read32(0xAE00), 0, "PCIU: no slot is being inserted");
assert.equal(io.port_read32(0xAE04), 0, "PCID: no slot is being removed");
assert.equal(io.port_read32(0xAE0C), 0, "PCIB");
io.port_write32(0xAE08, 4);
io.port_write32(0xAE0C, 0);
console.log("pass");
process.exit(0);
