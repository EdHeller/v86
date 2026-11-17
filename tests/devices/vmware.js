#!/usr/bin/env node
// The VMware backdoor's pointer (src/rust/cpu/vmware.rs) driven by tests/devices/vmware.asm; this moves the mouse.
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import url from "node:url";

const __dirname = url.fileURLToPath(new URL(".", import.meta.url));
const TEST_RELEASE_BUILD = +process.env.TEST_RELEASE_BUILD;
const lib = process.env.V86_LIB || (TEST_RELEASE_BUILD ? "../../build/libv86.mjs" : "../../src/main.js");
const wasm = process.env.V86_WASM || path.join(__dirname, "../../build/" + (TEST_RELEASE_BUILD ? "v86.wasm" : "v86-debug.wasm"));
const { V86 } = await import(lib);

const dir = fs.mkdtempSync(path.join(os.tmpdir(), "vmware-"));
const bin = path.join(dir, "vmware.bin");
execFileSync("nasm", ["-f", "bin", "-o", bin, path.join(__dirname, "vmware.asm")]);
const image = new Uint8Array(1474560);
image.set(fs.readFileSync(bin));

const EXPECTED = "00000006 564D5868 00000001 3442554A W00000004 00000020 00004000 0000BFFF Z";

const emulator = new V86({
    bios: { url: path.join(__dirname, "../../bios/seabios.bin") },
    vga_bios: { url: path.join(__dirname, "../../bios/vgabios.bin") },
    wasm_path: wasm,
    fda: { buffer: image.buffer },
    autostart: true,
    memory_size: 32 * 1024 * 1024,
    log_level: 0,
});

let serial = "", absolute = null;
emulator.bus.register("vmware-absolute-mouse", a => { absolute = a; });
const timeout = setTimeout(() => finish("timeout"), 60_000);
emulator.add_listener("serial0-output-byte", function(byte)
{
    serial += String.fromCharCode(byte);
    if(byte === 0x57) // W: move with the left button held
    {
        emulator.bus.send("mouse-click", [true, false, false]);
        emulator.bus.send("mouse-absolute", [256, 768, 1024, 1024]);
    }
    if(serial.endsWith("Z")) setTimeout(() => finish(), 50);
});

function finish(why)
{
    clearTimeout(timeout);
    emulator.destroy();
    const failures = [];
    // (the click before the first move queues nothing: no position yet)
    if(serial !== EXPECTED) failures.push(`serial "${serial}", expected "${EXPECTED}"`);
    if(absolute !== true) failures.push("vmware-absolute-mouse: " + absolute);
    if(why) failures.push(why);
    if(failures.length) { console.log("FAIL\n" + failures.join("\n")); process.exit(1); }
    console.log("vmware: ok (" + serial + ")");
    process.exit(0);
}
