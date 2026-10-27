#!/usr/bin/env node
// Runs a snapshot of a booted guest in headless Chromium (playwright) and times a shell loop on the serial console.
// Compare with tests/x64/manual/alpine.js in node: this is the engine users actually run.
//
//   browser_bench.mjs <alpine.iso> <login.state> [iterations] [jit64 threshold]
//
// The page is served from a temporary directory with symlinks to build/, bios/ and the two files.

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import http from "node:http";
import url from "node:url";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const { chromium } = require("playwright");
const root = path.resolve(url.fileURLToPath(new URL(".", import.meta.url)), "../../..");
const [iso, state, iterations = "40000", threshold = "150"] = process.argv.slice(2);
if(!iso || !state) { console.error("usage: browser_bench.mjs <alpine.iso> <login.state> [iterations] [jit64 threshold]"); process.exit(2); }

const web = fs.mkdtempSync(path.join(os.tmpdir(), "v86-bench-"));
for(const [name, target] of [["build", root + "/build"], ["bios", root + "/bios"], ["alpine.iso", path.resolve(iso)], ["login.state", path.resolve(state)]])
    fs.symlinkSync(target, path.join(web, name));

const server = http.createServer((req, res) => {
    const file = path.join(web, decodeURIComponent(req.url.split("?")[0]));
    fs.stat(file, (err, st) => {
        if(err || !st.isFile()) { res.writeHead(404); res.end(); return; }
        const type = file.endsWith(".html") ? "text/html" : file.endsWith(".js") ? "text/javascript" : file.endsWith(".wasm") ? "application/wasm" : "application/octet-stream";
        res.writeHead(200, { "content-type": type, "content-length": st.size });
        fs.createReadStream(file).pipe(res);
    });
}).listen(0);
const port = server.address().port;

const browser = await chromium.launch();
const page = await browser.newPage();
page.on("console", m => { if(process.env.VERBOSE) console.error("[page]", m.text()); });
page.on("pageerror", e => console.error("[page error]", e.message));
page.on("crash", () => console.error("[page crashed]"));
browser.on("disconnected", () => console.error("[browser disconnected]"));
fs.writeFileSync(path.join(web, "index.html"), `<!doctype html><script src="/build/libv86.js"></script>`);
await page.goto(`http://localhost:${port}/index.html`);
await page.waitForFunction(() => typeof V86 !== "undefined");
console.error("browser:", browser.version());

if(process.env.VERBOSE) setInterval(async () => {
    try { console.error("[poll]", JSON.stringify(await page.evaluate(() => window.__emu ? { run: window.__emu.is_running(), ser: window.__serial().slice(-80) } : "no emu"))); } catch(e) {}
}, 10000);
const result = await page.evaluate(async ({ port, iterations, threshold }) => {
    const base = `http://localhost:${port}/`;
    let serial = "";
    const emulator = new V86({
        wasm_path: base + "build/v86.wasm",
        bios: { url: base + "bios/seabios.bin" },
        vga_bios: { url: base + "bios/vgabios.bin" },
        cdrom: { url: base + "alpine.iso" },
        memory_size: 512 * 1024 * 1024,
        vga_memory_size: 8 * 1024 * 1024,
        initial_state: { url: base + "login.state" },
        jit64_threshold: +threshold,
        autostart: true,
    });
    emulator.add_listener("serial0-output-byte", b => { serial += String.fromCharCode(b); });
    window.__emu = emulator; window.__serial = () => serial;
    const wait = ms => new Promise(r => setTimeout(r, ms));
    const until = async (re, ms) => { const t0 = performance.now(); while(!re.test(serial)) { if(performance.now() - t0 > ms) throw new Error("timeout waiting for " + re); await wait(200); } };
    await new Promise(r => emulator.add_listener("emulator-started", r));
    // the snapshot was taken after the prompt was printed: poke the getty until it prints it again
    for(let i = 0; i < 100 && !/login: /.test(serial); i++) { emulator.serial0_send("\n"); await wait(1000); }
    await until(/login: /, 60000);
    emulator.serial0_send("root\n");
    await until(/localhost:~# /, 60000);
    serial = "";
    emulator.serial0_send(`time sh -c 'i=0; while [ $i -lt ${iterations} ]; do i=$((i+1)); done'; echo BENCH-DONE\n`);
    await until(/BENCH-DONE\n/, 900000);
    const m = serial.match(/real\s+(\d+)m\s+([\d.]+)s/);
    return { seconds: m ? +m[1] * 60 + +m[2] : null, raw: serial.slice(-200) };
}, { port, iterations, threshold });
console.log(`chromium shell loop (${iterations} iterations): ${result.seconds} s`);
await browser.close();
server.close();
fs.rmSync(web, { recursive: true, force: true });
