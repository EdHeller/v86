#!/usr/bin/env node
// v86 speed comparison: boots 32-bit Alpine 3.24 in each build and times a few workloads in the guest.
//
//   node run.mjs [rounds] [builds...]        default: 3 rounds of every build in builds/ (in rotating order)
//
// Each round boots every build once (a fresh process each) and runs, after logging in on the serial console:
//   loop   a busybox shell loop of 100000 steps (the JIT's dispatch and the interpreter of ash)
//   gzip   8 MiB of random bytes through gzip and gunzip
//   sha    sha256sum of 64 MiB of zeros
//   fork   500 short processes (exec, page tables, the TLB)
// Times are wall-clock seconds measured outside the guest. The log is plain text; a summary follows at the end.

import fs from "node:fs";
import path from "node:path";
import url from "node:url";
import { spawn } from "node:child_process";

const here = path.dirname(url.fileURLToPath(import.meta.url));

// SCALE=n: every workload n times as long (small differences need longer runs to show)
const SCALE = Math.max(1, +process.env.SCALE || 1);
const TESTS = [
    ["loop", `i=0; while [ $i -lt ${100000 * SCALE} ]; do i=$((i+1)); done`],
    ["gzip", `dd if=/dev/urandom bs=1M count=${8 * SCALE} 2>/dev/null | gzip -c | gunzip -c | wc -c >/dev/null`],
    ["sha", `dd if=/dev/zero bs=1M count=${64 * SCALE} 2>/dev/null | sha256sum >/dev/null`],
    ["fork", `i=0; while [ $i -lt ${500 * SCALE} ]; do /bin/true; i=$((i+1)); done`],
];

// ---------------------------------------------------------------------------------------------------------------
// child: one boot of one build, printing one JSON line
if(process.argv[2] === "--child")
{
    const build = process.argv[3];
    const dir = path.join(here, "builds", build);
    const { V86 } = await import(url.pathToFileURL(path.join(dir, "libv86.mjs")).href);
    // ARCH=x86_64: the 64-bit images on v64.wasm (libv64.js next to libv86.mjs in the build's folder)
    const x64 = process.env.ARCH === "x86_64";
    if(x64) await import(url.pathToFileURL(path.join(dir, "libv64.js")).href);
    const buf = p => { const b = fs.readFileSync(path.join(here, p)); return b.buffer.slice(b.byteOffset, b.byteOffset + b.byteLength); };
    // options.json (optional): { "wasm": "file in builds/<name>", "v86": { extra V86 options }, "jit_config": { "1": 6 } }
    const extra = fs.existsSync(path.join(dir, "options.json")) ? JSON.parse(fs.readFileSync(path.join(dir, "options.json"), "utf8")) : {};
    const e = new V86({
        wasm_path: path.join(dir, extra.wasm || (x64 ? "v64.wasm" : "v86.wasm")),
        bios: { buffer: buf("bios/seabios.bin") }, vga_bios: { buffer: buf("bios/vgabios.bin") },
        bzimage: { buffer: buf(x64 ? "images/vmlinuz-x86_64" : "images/vmlinuz-x86") },
        initrd: { buffer: buf(x64 ? "images/initramfs-x86_64" : "images/initramfs-x86") },
        cdrom: { buffer: buf(x64 ? "images/alpine-virt-3.24.3-x86_64.iso" : "images/alpine-virt-3.24.3-x86.iso") },
        cmdline: "modules=loop,squashfs,sd-mod console=ttyS0,115200 tsc=reliable mitigations=off random.trust_cpu=on",
        memory_size: 512 << 20, cpu_type: x64 ? "x86_64" : "x86", autostart: true, log_level: 0,
        ...(extra.v86 || {}),
    });
    // (JIT settings: set_jit_config(index, value) before the first compile)
    e.add_listener("emulator-ready", () => {
        for(const [k, v] of Object.entries(extra.jit_config || {})) e.v86.cpu.wm.exports["set_jit_config"](+k, +v);
    });
    let serial = "";
    e.add_listener("serial0-output-byte", b => { serial += String.fromCharCode(b); });
    const t0 = performance.now();
    const until = (re, seconds) => new Promise((resolve, reject) => {
        const from = performance.now();
        const check = () => {
            if(re.test(serial)) return resolve();
            if(performance.now() - from > seconds * 1000) return reject(new Error("timeout waiting for " + re));
            setTimeout(check, 10);
        };
        check();
    });
    const result = { build };
    try
    {
        await until(/localhost login:/, 900);
        result.login = (performance.now() - t0) / 1000;
        e.serial0_send("root\n");
        await until(/localhost:~# /, 120);
        for(const [name, command] of TESTS)
        {
            serial = "";
            const from = performance.now();
            const insn0 = e.v86.cpu.instruction_counter[0] >>> 0;
            e.serial0_send(command + "; echo DONE$((40+2))\n");
            await until(/DONE42/, 900);
            result[name] = (performance.now() - from) / 1000;
            // (INSN=1: the guest instructions each workload ran, in millions: the same work should be the same count)
            if(process.env.INSN) result[name + "_Minsn"] = Math.round(((e.v86.cpu.instruction_counter[0] >>> 0) - insn0 >>> 0) / 1e6);
        }
    }
    catch(error)
    {
        result.error = error.message;
    }
    // (PROF_STATS=path/to/src/rust/profiler.rs with a profiler build, options.json "wasm": the counters by name)
    if(process.env.PROF_STATS && e.v86.cpu.wm.exports["profiler_stat_get"])
    {
        const src = fs.readFileSync(process.env.PROF_STATS, "utf8");
        const names = src.slice(src.indexOf("pub enum stat {"), src.indexOf("}", src.indexOf("pub enum stat {")))
            .split("\n").slice(1).map(l => l.trim().replace(",", "")).filter(l => /^[A-Z_0-9]+$/.test(l));
        names.forEach((n, i) => { const v = e.v86.cpu.wm.exports["profiler_stat_get"](i); if(v) console.error(n.padEnd(48) + v); });
    }
    // (JIT64_STATS=1: jit64's counters: modules, generated bytes by kind)
    if(process.env.JIT64_STATS)
    {
        const g = i => e.v86.cpu.wm.exports["get_jit_config"](i) >>> 0;
        const cat = ["", "", "", "get_reg", "set_reg", "ea", "flags", "read_mem", "write_mem", "chain"];
        console.error(`jit64: ${g(5)} modules compiled, ${g(6)} failed, ${g(8)} invalidated, ${g(16)} blocks, ` +
            `${(g(17) / 1e6).toFixed(1)} MB generated, ${g(13)} instructions compiled`);
        console.error("  bytes by part: " + cat.map((n, i) => n && n + " " + (g(95000 + i) / 1e6).toFixed(1) + " MB").filter(x => x).join(", "));
    }
    console.log(JSON.stringify(result));
    process.exit(0);
}

// ---------------------------------------------------------------------------------------------------------------
// parent: the rounds and the summary
const rounds = +process.argv[2] || 3;
const all = fs.readdirSync(path.join(here, "builds")).filter(b => fs.existsSync(path.join(here, "builds", b, "libv86.mjs")));
const first = ["base", "upstream", "ours"].filter(b => all.includes(b));
const builds = process.argv.slice(3).length ? process.argv.slice(3) : first.concat(all.filter(b => !first.includes(b)).sort());
const columns = ["login", ...TESTS.map(t => t[0])];
const results = Object.fromEntries(builds.map(b => [b, []]));
const fmt = v => v === undefined ? "    -  " : v.toFixed(2).padStart(7);

console.log("v86 speed comparison: " + rounds + " rounds of " + builds.join(", ") + ", node " + process.version + (SCALE > 1 ? ", SCALE " + SCALE : ""));
for(const b of builds)
{
    const info = path.join(here, "builds", b, "VERSION.txt");
    if(fs.existsSync(info)) console.log("  " + b.padEnd(14) + fs.readFileSync(info, "utf8").trim());
}
console.log("seconds (wall clock):   " + "build".padEnd(14) + columns.map(c => c.padStart(7)).join(" "));

for(let r = 0; r < rounds; r++)
{
    // (the order rotates each round, so that no build always runs first or last)
    const order = builds.map((_, i) => builds[(i + r) % builds.length]);
    for(const build of order)
    {
        const line = await new Promise(resolve => {
            const child = spawn(process.execPath, [url.fileURLToPath(import.meta.url), "--child", build], { stdio: ["ignore", "pipe", "ignore"] });
            let out = "";
            child.stdout.on("data", d => { out += d; });
            child.on("close", () => resolve(out.trim().split("\n").pop()));
        });
        let result;
        try { result = JSON.parse(line); } catch(e) { result = { build, error: "no result" }; }
        results[build].push(result);
        console.log(("round " + (r + 1)).padEnd(24) + build.padEnd(14) + columns.map(c => fmt(result[c])).join(" ") +
            (result.error ? "   ERROR: " + result.error : ""));
    }
}

console.log("\nsummary (mean of the rounds; in brackets the change against upstream, negative is faster):");
console.log("".padEnd(24) + "build".padEnd(14) + columns.map(c => c.padStart(15)).join(""));
const mean = (b, c) => { const v = results[b].map(x => x[c]).filter(x => x !== undefined); return v.length ? v.reduce((a, x) => a + x, 0) / v.length : undefined; };
for(const b of builds)
{
    console.log("".padEnd(24) + b.padEnd(14) + columns.map(c => {
        const m = mean(b, c), u = results["upstream"] ? mean("upstream", c) : undefined;
        if(m === undefined) return "-".padStart(15);
        const rel = u && b !== "upstream" ? " (" + (m > u ? "+" : "") + ((m / u - 1) * 100).toFixed(1) + "%)" : "";
        return (m.toFixed(2) + rel).padStart(15);
    }).join(""));
}
