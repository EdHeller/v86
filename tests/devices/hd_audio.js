#!/usr/bin/env node
// HD Audio driven by tests/devices/hd_audio.asm: its registers, the codec, a stream's samples to the speaker.
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import url from "node:url";

const __dirname = url.fileURLToPath(new URL(".", import.meta.url));
const TEST_RELEASE_BUILD = +process.env.TEST_RELEASE_BUILD;
const { V86 } = await import(TEST_RELEASE_BUILD ? "../../build/libv86.mjs" : "../../src/main.js");

const dir = fs.mkdtempSync(path.join(os.tmpdir(), "hda-"));
const bin = path.join(dir, "hda.bin");
execFileSync("nasm", ["-f", "bin", "-o", bin, path.join(__dirname, "hd_audio.asm")]);
const image = new Uint8Array(1474560);
image.set(fs.readFileSync(bin));

const EXPECTED = "00000001 00001001 1AF40012 00020002 0000004A 00000001 00000004 80000001 00000024 00000000 00000000 00000000 00000001 Z";

const emulator = new V86({
    bios: { url: path.join(__dirname, "../../bios/seabios.bin") },
    vga_bios: { url: path.join(__dirname, "../../bios/vgabios.bin") },
    wasm_path: path.join(__dirname, "../../build/" + (TEST_RELEASE_BUILD ? "v86.wasm" : "v86-debug.wasm")),
    fda: { buffer: image.buffer },
    audio_device: "hd_audio",
    autostart: true,
    memory_size: 32 * 1024 * 1024,
    log_level: 0,
});

let serial = "";
const left = [], right = [];
let rate = 0, enabled = 0, disabled = 0, pcm_bytes = 0;
emulator.bus.register("dac-send-data", data => { left.push(...data[0]); right.push(...data[1]); });
emulator.bus.register("hd-audio-pcm", pcm => { for(const c of pcm["chunks"]) pcm_bytes += c.length; });
emulator.bus.register("dac-enable", () => enabled++);
emulator.bus.register("dac-disable", () => disabled++);
emulator.bus.register("dac-tell-sampling-rate", r => { rate = r; });
const timeout = setTimeout(() => finish("timeout"), 60_000);

emulator.add_listener("serial0-output-byte", function(byte)
{
    serial += String.fromCharCode(byte);
    if(serial.endsWith("Z")) setTimeout(() => finish(), 100);
});

function finish(why)
{
    clearTimeout(timeout);
    emulator.destroy();
    const failures = [];
    if(serial !== EXPECTED) failures.push(`serial "${serial}", expected "${EXPECTED}"`);
    // the ramp: frame i's left word is (2i) * 0x0101, right (2i + 1) * 0x0101 (the list loops)
    if(left.length < 64) failures.push(`${left.length} frames`);
    for(let i = 0; i < left.length; i++)
    {
        const f = i % 128;
        const l = ((2 * f * 0x101) << 16 >> 16) / 32768, r = (((2 * f + 1) * 0x101) << 16 >> 16) / 32768;
        if(Math.abs(left[i] - l) > 1e-6 || Math.abs(right[i] - r) > 1e-6) { failures.push(`frame ${i}: ${left[i]}, ${right[i]}, expected ${l}, ${r}`); break; }
    }
    if(pcm_bytes !== 4 * left.length) failures.push(`pcm ${pcm_bytes} bytes for ${left.length} frames`);
    if(rate !== 48000) failures.push(`rate ${rate}`);
    if(!enabled || !disabled) failures.push(`enable ${enabled}, disable ${disabled}`);
    if(why) failures.push(why);
    if(failures.length) { console.log("FAIL\n" + failures.join("\n")); process.exit(1); }
    console.log(`hd_audio: ok (${left.length} frames at ${rate} Hz)`);
    process.exit(0);
}
