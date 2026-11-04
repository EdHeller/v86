#!/usr/bin/env node
// Micro benchmarks of v86 builds: micro.bin (micro.asm) runs one kind of work per benchmark without an OS; each run is
// a fresh reset of the emulator, timed from start to the guest's halt.
//
//   node micro/run.mjs [rounds] [builds...]     default: 5 rounds of every build in ../builds (rotating order)
//
// Uses the same builds/ folders as run.mjs (libv86.mjs, v86.wasm, optional options.json). Plain text out.

import fs from "node:fs";
import path from "node:path";
import url from "node:url";
import { spawn } from "node:child_process";

const here = path.dirname(url.fileURLToPath(import.meta.url));
const root = path.join(here, "..");
const NAMES = ["alu", "flags", "memory", "stack", "call_near", "call_far", "indirect", "string", "muldiv", "sse", "x87",
    "pagewalk", "smc", "port", "interrupt", "pit", "call_deep", "setcc", "bitops", "unaligned", "rep_small", "scalar_fp",
    "lock", "rdtsc", "pagefault", "tlb4k", "code16", "segment", "sysenter", "cli_sti", "spinlock", "cpuid", "memcpy_bytes"];

// ---------------------------------------------------------------------------------------------------------------
// child: every benchmark once in one build, a JSON line of seconds
if(process.argv[2] === "--child")
{
    const build = process.argv[3];
    const dir = path.join(root, "builds", build);
    const { V86 } = await import(url.pathToFileURL(path.join(dir, "libv86.mjs")).href);
    const extra = fs.existsSync(path.join(dir, "options.json")) ? JSON.parse(fs.readFileSync(path.join(dir, "options.json"), "utf8")) : {};
    const buf = p => { const b = fs.readFileSync(p); return b.buffer.slice(b.byteOffset, b.byteOffset + b.byteLength); };
    const bin = fs.readFileSync(path.join(here, "micro.bin"));
    const e = new V86({
        wasm_path: path.join(dir, extra.wasm || "v86.wasm"),
        bios: { buffer: buf(path.join(root, "bios/seabios.bin")) },
        memory_size: 64 << 20, cpu_type: "x86", autostart: false, log_level: 0,
        ...(extra.v86 || {}),
    });
    await new Promise(r => e.add_listener("emulator-loaded", r));
    for(const [k, v] of Object.entries(extra.jit_config || {})) e.v86.cpu.wm.exports["set_jit_config"](+k, +v);
    let halted = null;
    e.add_listener("cpu-event-halt", () => { if(halted) halted(); });
    const result = { build };
    for(let i = 0; i < NAMES.length; i++)
    {
        const cpu = e.v86.cpu;
        e.stop();
        await new Promise(r => setTimeout(r, 20));
        cpu.reboot_internal();
        cpu.reset_memory();
        cpu.mem8.set(bin, 0x7C00);
        cpu.mem8.set([0xEA, 0x00, 0x7C, 0x00, 0x00], 0xFFFF0);
        cpu.mem8[0x500] = i;
        const done = new Promise(r => { halted = r; });
        const t0 = performance.now();
        e.run();
        const timeout = new Promise(r => setTimeout(() => r("timeout"), 120000));
        const outcome = await Promise.race([done, timeout]);
        result[NAMES[i]] = outcome === "timeout" ? null : (performance.now() - t0) / 1000;
        halted = null;
    }
    console.log(JSON.stringify(result));
    process.exit(0);
}

// ---------------------------------------------------------------------------------------------------------------
const rounds = +process.argv[2] || 5;
const all = fs.readdirSync(path.join(root, "builds")).filter(b => fs.existsSync(path.join(root, "builds", b, "libv86.mjs")));
const first = ["base", "upstream", "ours"].filter(b => all.includes(b));
const builds = process.argv.slice(3).length ? process.argv.slice(3) : first.concat(all.filter(b => !first.includes(b)).sort());
const results = Object.fromEntries(builds.map(b => [b, []]));
const W = Math.max(14, ...builds.map(b => b.length + 1));

console.log("v86 micro benchmarks: " + rounds + " rounds of " + builds.join(", ") + ", node " + process.version);
console.log("seconds per benchmark (lower is better)");
console.log("".padEnd(10) + "build".padEnd(W) + NAMES.map(n => n.padStart(10)).join(""));
for(let r = 0; r < rounds; r++)
{
    const order = builds.map((_, i) => builds[(i + r) % builds.length]);
    for(const build of order)
    {
        const line = await new Promise(resolve => {
            const child = spawn(process.execPath, [url.fileURLToPath(import.meta.url), "--child", build], { stdio: ["ignore", "pipe", "ignore"] });
            let out = "";
            child.stdout.on("data", d => { out += d; });
            child.on("close", () => resolve(out.trim().split("\n").pop()));
        });
        let res;
        try { res = JSON.parse(line); } catch(e) { res = { build }; }
        results[build].push(res);
        console.log(("round " + (r + 1)).padEnd(10) + build.padEnd(W) + NAMES.map(n => (res[n] == null ? "-" : res[n].toFixed(3)).padStart(10)).join(""));
    }
}

// the median of the rounds (a stray slow round does not move it), and the change against upstream
const median = (b, n) => {
    const v = results[b].map(x => x[n]).filter(x => x != null).sort((p, q) => p - q);
    return v.length ? v[v.length >> 1] : null;
};
console.log("\nmedian of the rounds; second line: change against upstream (negative is faster)");
console.log("".padEnd(10) + "build".padEnd(W) + NAMES.map(n => n.padStart(10)).join(""));
for(const b of builds)
{
    console.log("".padEnd(10) + b.padEnd(W) + NAMES.map(n => { const m = median(b, n); return (m == null ? "-" : m.toFixed(3)).padStart(10); }).join(""));
    if(results.upstream && b !== "upstream")
    {
        console.log("".padEnd(10) + "".padEnd(W) + NAMES.map(n => {
            const m = median(b, n), u = median("upstream", n);
            return (m == null || !u ? "-" : (m > u ? "+" : "") + ((m / u - 1) * 100).toFixed(1) + "%").padStart(10);
        }).join(""));
    }
}
