#!/usr/bin/env node
// The SB16 (src/rust/cpu/sb16.rs) driven by tests/devices/sb16.asm; this plays the speaker: asks for samples, takes them.
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import url from "node:url";

const __dirname = url.fileURLToPath(new URL(".", import.meta.url));
const TEST_RELEASE_BUILD = +process.env.TEST_RELEASE_BUILD;
const { V86 } = await import(TEST_RELEASE_BUILD ? "../../build/libv86.mjs" : "../../src/main.js");

const dir = fs.mkdtempSync(path.join(os.tmpdir(), "sb16-"));
const bin = path.join(dir, "sb16.bin");
execFileSync("nasm", ["-f", "bin", "-o", bin, path.join(__dirname, "sb16.asm")]);
const image = new Uint8Array(1474560);
image.set(fs.readFileSync(bin));

const EXPECTED = "AA 04 05 A5 33 43 4F A0 50 A5 02 22 21 20 Z";

const emulator = new V86({
    bios: { url: path.join(__dirname, "../../bios/seabios.bin") },
    vga_bios: { url: path.join(__dirname, "../../bios/vgabios.bin") },
    wasm_path: path.join(__dirname, "../../build/" + (TEST_RELEASE_BUILD ? "v86.wasm" : "v86-debug.wasm")),
    fda: { buffer: image.buffer },
    autostart: true,
    memory_size: 32 * 1024 * 1024,
    disable_speaker: false,
    log_level: 0,
});

let serial = "";
const samples = [[], []];
let enabled = 0;
let rate = 0;
emulator.bus.register("dac-send-data", function(data)
{
    samples[0].push(...data[0]);
    samples[1].push(...data[1]);
});
emulator.bus.register("dac-enable", () => enabled++);
emulator.bus.register("dac-tell-sampling-rate", r => { rate = r; });

const timer = setInterval(() => emulator.bus.send("dac-request-data"), 10);
const timeout = setTimeout(() => finish("timeout"), 60_000);

emulator.add_listener("serial0-output-byte", function(byte)
{
    serial += String.fromCharCode(byte);
    if(serial.endsWith("Z")) setTimeout(() => finish(), 100);
});

function finish(why)
{
    clearInterval(timer);
    clearTimeout(timeout);
    emulator.destroy();
    const failures = [];
    const out = serial.slice(serial.indexOf("AA"));
    if(out !== EXPECTED) failures.push(`serial "${out}", expected "${EXPECTED}"`);
    if(samples[0].length !== 256 || samples[1].length !== 256) failures.push(`${samples[0].length}/${samples[1].length} samples, expected 256`);
    for(let i = 0; i < Math.min(256, samples[0].length); i++)
    {
        const want = Math.fround(Math.min(1, Math.max(-1, i / 127.5 - 1)));
        if(samples[0][i] !== want || samples[1][i] !== want) { failures.push(`sample ${i}: ${samples[0][i]}, ${samples[1][i]}, expected ${want}`); break; }
    }
    if(!enabled) failures.push("the DAC was never enabled");
    if(Math.abs(rate - 1000000 / (256 - 0xA6)) > 1e-6) failures.push(`rate ${rate}`);
    if(why) failures.push(why);
    if(failures.length) { console.log("FAIL\n" + failures.join("\n")); process.exit(1); }
    console.log("sb16: ok (" + out + ", " + samples[0].length + " samples at " + rate.toFixed(1) + " Hz)");
    process.exit(0);
}
