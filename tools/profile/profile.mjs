#!/usr/bin/env node
// Profiles and times v86 (v86.wasm, the 32-bit core) and v64 (v64.wasm, x86_64) while a guest boots.
//
//   node tools/profile/profile.mjs --cdrom alpine-virt-3.24.3-x86.iso
//   node tools/profile/profile.mjs --arch x86_64 --cdrom alpine-virt-3.24.3-x86_64.iso --runs 5
//   node tools/profile/profile.mjs --cdrom x.iso --wasm old=/tmp/old.wasm --wasm new=build/v86.wasm --runs 5 --no-profile
//
// What it does, per build:
//   1. one boot under V8's sampling CPU profiler: where the time goes by area (compiled guest code, the
//      interpreter, the JIT itself, devices in JavaScript, ...), by phase of the boot and by function; the
//      .cpuprofile file opens in Chrome DevTools (Performance > Load profile) and in https://speedscope.app
//   2. --runs N: N more boots without the profiler, the builds taking turns, for the times: median, fastest
//      and the spread of each mark
//
// The guest's clock. By default the guest runs on a virtual clock: its time advances with the instructions it
// executes (--mips, 50 by default: 50 million instructions are one guest second) and jumps ahead while it is
// halted. Every run then does the same work (the instruction counts agree to about 0.1 %), timer interrupts
// arrive at the same places and waiting costs nothing, so the time measured is the emulator's and two builds
// can be compared. --clock real runs the guest on the host's clock as a browser does: the time a user waits,
// idle time included, but the guest's work then depends on the host's speed and load and the times wobble
// (three boots of 32-bit Alpine here: 12 % apart, against 3 % on the virtual clock).
//
// Options
//   --arch x86|x86_64        the core: build/v86.wasm or build/v64.wasm (default x86)
//   --wasm [name=]file[@lib][#k=v] a core to measure; repeat it to compare builds (the first is the baseline); @lib: the
//                            JavaScript of that build, when it differs from --lib; file#k=v,k=v: set_jit_config
//                            values of that build (one core with two settings)
//   --lib file               the JavaScript: src/main.js (default, no build needed) or a build/libv86.mjs
//   --plugin file            more JavaScript to load after --lib (build/libv64.js beside a built libv86.mjs)
//   --cdrom --hda --fda --bzimage --initrd --state <file>, --cmdline <text>, --memory <MiB> (512)
//   --bios --vga-bios <file> (bios/seabios.bin, bios/vgabios.bin)
//   --mark name=regex        a point of the boot, found in the serial output; in order; the last one ends the
//                            run. Default: login=login:
//   --screen-mark name=regex the same, found on the text screen (guests without a serial console)
//   --insn-mark name=N       the same, after N million guest instructions (any guest, also graphical ones)
//   --send name=text         type text on the serial console when mark <name> is reached (\n is enter)
//   --keys name=text         the same on the keyboard
//   --guest-map file         also sample the guest: which of its code the instructions go to. file is the
//                            System.map of the guest's kernel (boot it with nokaslr), or "none" for addresses
//   --step-profile           x86_64: the instructions jit64 does not compile, by opcode: those run by the
//                            interpreter inside a compiled block, and those a block ended at
//   --runs N                 timing runs per build (default 0); --no-profile leaves the profiled run out
//   --clock virtual|real     see above; --mips N the virtual clock's rate
//   --interval us            the profiler's sampling interval (default 200)
//   --timeout seconds        per run (default 600)
//   --top N                  rows of the function tables (default 30)
//   --jit-config k=v         set_jit_config(k, v) before the guest starts; repeat
//   --v86 json               more options for new V86({...})
//   --out dir                where the .cpuprofile files, serial logs and report.json go (default build/profile)

import fs from "node:fs";
import path from "node:path";
import url from "node:url";
import { spawn } from "node:child_process";
import { analyze, format } from "./analyze.mjs";

const here = path.dirname(url.fileURLToPath(import.meta.url));
const root = path.resolve(here, "../..");

const multi = new Set(["wasm", "mark", "screen-mark", "insn-mark", "send", "keys", "jit-config", "plugin"]);
const flags = new Set(["no-profile", "help", "step-profile"]);
const args = {};
const order = [];       // (the marks keep the order of the command line, whatever their kind)
for(let i = 2; i < process.argv.length; i++)
{
    const a = process.argv[i];
    if(!a.startsWith("--")) { console.error("unexpected argument: " + a); process.exit(1); }
    const k = a.slice(2);
    if(flags.has(k)) { args[k] = true; continue; }
    const v = process.argv[++i];
    if(v === undefined) { console.error("missing value for " + a); process.exit(1); }
    if(multi.has(k)) { (args[k] = args[k] || []).push(v); if(k.endsWith("mark")) order.push([k, v]); }
    else args[k] = v;
}
if(args.help || process.argv.length < 3)
{
    const src = fs.readFileSync(url.fileURLToPath(import.meta.url), "utf8").split("\n");
    console.log(src.slice(1, src.findIndex(l => l.startsWith("import"))).map(l => l.replace(/^\/\/ ?/, "")).join("\n"));
    process.exit(0);
}

const arch = args.arch || "x86";
if(!["x86", "x86_64"].includes(arch)) { console.error("--arch: x86 or x86_64"); process.exit(1); }
const split = s => { const i = s.indexOf("="); return i < 0 ? [null, s] : [s.slice(0, i), s.slice(i + 1)]; };
const unescape = s => s.replace(/\\n/g, "\n").replace(/\\t/g, "\t");

const builds = (args.wasm || [path.join(root, arch === "x86_64" ? "build/v64.wasm" : "build/v86.wasm")]).map(w => {
    const [name, files] = split(w);
    const [paths, config] = files.split("#");
    const [file, lib] = paths.split("@");
    return { name: name || path.basename(file).replace(/\.wasm$/, ""), wasm: path.resolve(file), lib: lib && path.resolve(lib),
        jit_config: config && Object.fromEntries(config.split(",").map(split)) };
});
for(const b of builds) if(!fs.existsSync(b.wasm)) { console.error("no such file: " + b.wasm + " (make " + path.relative(root, b.wasm) + ")"); process.exit(1); }

const sends = Object.fromEntries((args.send || []).map(split)), keys = Object.fromEntries((args.keys || []).map(split));
const marks = (order.length ? order : [["mark", "login=login:"]]).map(([kind, text]) => {
    const [name, value] = split(text);
    const m = { name: name || value };
    if(kind === "insn-mark") m.insn = +value * 1e6;
    else { m.re = value; m.screen = kind === "screen-mark"; new RegExp(value); }
    if(sends[m.name] !== undefined) m.send = unescape(sends[m.name]);
    if(keys[m.name] !== undefined) m.keys = unescape(keys[m.name]);
    return m;
});

const out_dir = path.resolve(args.out || path.join(root, "build/profile"));
fs.mkdirSync(out_dir, { recursive: true });
const abs = p => p === undefined ? undefined : path.resolve(p);

// (a build with the profiler feature, make with-profiler: its counters by name)
let stat_names;
try
{
    const src = fs.readFileSync(path.join(root, "src/rust/profiler.rs"), "utf8");
    const from = src.indexOf("pub enum stat {");
    stat_names = src.slice(from, src.indexOf("}", from)).split("\n").slice(1).map(l => l.trim().replace(",", "")).filter(l => /^[A-Z_0-9]+$/.test(l));
}
catch(e) {}

const base = {
    arch, clock: args.clock || "virtual", mips: +args.mips || 50,
    lib: abs(args.lib || path.join(root, "src/main.js")), plugins: (args.plugin || []).map(abs),
    bios: abs(args.bios || path.join(root, "bios/seabios.bin")), vga_bios: abs(args["vga-bios"] || path.join(root, "bios/vgabios.bin")),
    cdrom: abs(args.cdrom), hda: abs(args.hda), fda: abs(args.fda), bzimage: abs(args.bzimage), initrd: abs(args.initrd),
    state: abs(args.state), cmdline: args.cmdline,
    memory: +args.memory || 512, timeout: +args.timeout || 600, interval: +args.interval || 200,
    marks, stat_names, guest_profile: !!args["guest-map"], step_profile: !!args["step-profile"],
    jit_config: Object.fromEntries((args["jit-config"] || []).map(split)),
    v86: args.v86 ? JSON.parse(args.v86) : undefined,
};
if(!["virtual", "real"].includes(base.clock)) { console.error("--clock: virtual or real"); process.exit(1); }

let run_count = 0;
function run(build, extra)
{
    return new Promise(resolve => {
        const result = path.join(out_dir, ".result-" + process.pid + "-" + (run_count++) + ".json");
        const child = spawn(process.execPath, [path.join(here, "child.mjs"), JSON.stringify({ ...base, label: build.name, wasm: build.wasm, ...(build.lib ? { lib: build.lib } : {}), ...(build.jit_config ? { jit_config: { ...base.jit_config, ...build.jit_config } } : {}), result, ...extra })],
            { stdio: ["ignore", "inherit", "inherit"] });
        child.on("close", () => {
            try { resolve(JSON.parse(fs.readFileSync(result, "utf8"))); }
            catch(e) { resolve({ build: build.name, ok: false, error: "no result", marks: [] }); }
            fs.rmSync(result, { force: true });
        });
    });
}

const lines = [];
const say = s => { console.log(s); lines.push(s); };
const top = +args.top || 30;
const runs = +args.runs || 0;
const report = { options: base, builds: {} };

say("v86 profile: " + arch + ", " + (base.clock === "virtual" ? "virtual clock (" + base.mips + " MIPS)" : "real clock") + ", node " + process.version +
    ", marks: " + marks.map(m => m.name).join(", "));
for(const b of builds) { say("  " + b.name.padEnd(12) + path.relative(process.cwd(), b.wasm)); report.builds[b.name] = { wasm: b.wasm, runs: [] }; }

const describe_run = r => r.marks.map(m => m.name + " " + m.wall.toFixed(2) + " s").join(", ") + (r.ok ? "" : "   FAILED: " + r.error);

// 1. the profiled boot
if(!args["no-profile"])
{
    for(const b of builds)
    {
        const file = path.join(out_dir, b.name + ".cpuprofile");
        const r = await run(b, { profile: file, serial_log: path.join(out_dir, b.name + ".serial.txt") });
        say("\n==== " + b.name + ": profiled boot (" + describe_run(r) + ") ====");
        if(!fs.existsSync(file)) { say("no profile written" + (r.serial_tail ? "; the serial output ended with:\n" + r.serial_tail : "")); continue; }
        if(!r.ok && r.serial_tail) say("the serial output ended with:\n" + r.serial_tail + "\n");
        const a = analyze(JSON.parse(fs.readFileSync(file, "utf8")));
        report.builds[b.name].profile = { file, run: r, by_area: a.by_area, phases: a.phases, self: a.self.slice(0, 200), inclusive: a.inclusive.slice(0, 200) };

        say("Phases (profiler on: about 20 % slower than the timing runs)");
        say("  " + "mark".padEnd(14) + "wall s".padStart(9) + "cpu s".padStart(9) + "insn M".padStart(10) + "MIPS".padStart(8) + "idle s".padStart(9) + "modules".padStart(9) + "wasm MB".padStart(9));
        let prev = { wall: 0, cpu_main: 0, insn: 0, idle: 0, modules: 0, wasm_bytes: 0 };
        for(const m of r.marks)
        {
            const d = k => m[k] - prev[k];
            say("  " + m.name.slice(0, 13).padEnd(14) + d("wall").toFixed(2).padStart(9) + d("cpu_main").toFixed(2).padStart(9) + (d("insn") / 1e6).toFixed(0).padStart(10) +
                (d("insn") / 1e6 / Math.max(d("cpu_main"), 1e-9)).toFixed(1).padStart(8) + d("idle").toFixed(2).padStart(9) + String(d("modules")).padStart(9) + (d("wasm_bytes") / 1e6).toFixed(1).padStart(9));
            prev = m;
        }
        say("  (cpu: the emulator's thread; idle: " + (base.clock === "virtual" ? "guest time skipped while halted, costs nothing" : "time waited while the guest was halted") +
            "; modules: WebAssembly modules the JIT generated)");
        if(r.jit64) say("  jit64: " + r.jit64.modules + " modules, " + r.jit64.failed + " failed, " + r.jit64.invalidated + " invalidated, " + r.jit64.blocks + " blocks, " +
            (r.jit64.bytes / 1e6).toFixed(1) + " MB, " + r.jit64.instructions + " instructions compiled");
        if(r.timeline && r.timeline.length > 1)
        {
            // the lines after which the most time passed: what the guest was doing while the user waited
            const end = r.end, tl = r.timeline;
            const rows = tl.map((l, i) => { const n = tl[i + 1] || end; return { ...l, dt: n.wall - l.wall, di: n.insn - l.insn }; });
            const slow = rows.filter(x => x.dt >= 0.01 * end.wall).sort((x, y) => y.dt - x.dt).slice(0, 25).sort((x, y) => x.wall - y.wall);
            say("\nBoot timeline: the console lines that were followed by 1 % of the run or more (" + tl.length + " lines; all of them in report.json)");
            say("  " + "at s".padStart(8) + "then s".padStart(8) + "share".padStart(8) + "insn M".padStart(8) + "  line");
            for(const x of slow) say("  " + x.wall.toFixed(2).padStart(8) + x.dt.toFixed(2).padStart(8) + (100 * x.dt / end.wall).toFixed(1).padStart(6) + " %" + (x.di / 1e6).toFixed(0).padStart(8) + "  " + x.text.slice(0, 90));
        }
        if(r.guest && r.guest.length)
        {
            // symbols of the kernel (text), sorted; a sample belongs to the last symbol at or below it
            let syms = [];
            if(args["guest-map"] !== "none")
            {
                syms = fs.readFileSync(args["guest-map"], "utf8").split("\n").map(l => l.split(" ")).filter(p => p.length >= 3 && /^[tTwW]$/.test(p[1]))
                    .map(p => [BigInt("0x" + p[0]), p[2]]).sort((x, y) => x[0] < y[0] ? -1 : x[0] > y[0] ? 1 : 0);
            }
            const first = syms.length ? syms[0][0] : 0n, last = syms.length ? syms[syms.length - 1][0] : 0n;
            const resolve = (cpl, addr) => {
                if(cpl === 3) return "[user space]";
                if(!syms.length || addr < first || addr > last + 0x100000n) return syms.length ? "[kernel, outside the map: modules, the decompressor, firmware] " + (addr >> 20n).toString(16) + "xxxxx" : "0x" + addr.toString(16);
                let lo = 0, hi = syms.length - 1;
                while(lo < hi) { const mid = lo + hi + 1 >> 1; if(syms[mid][0] <= addr) lo = mid; else hi = mid - 1; }
                return syms[lo][1];
            };
            const by = new Map();
            let all = 0, user = 0;
            for(const [key, w] of r.guest)
            {
                const cpl = +key[0], name = resolve(cpl, BigInt("0x" + key.slice(1)));
                by.set(name, (by.get(name) || 0) + w); all += w; if(cpl === 3) user += w;
            }
            say("\nGuest instructions by function (sampled; " + (all / 1e6).toFixed(0) + " M instructions, " + (100 * user / all).toFixed(1) + " % in user space)");
            for(const [name, w] of [...by].sort((x, y) => y[1] - x[1]).slice(0, top)) say("  " + (w / 1e6).toFixed(0).padStart(8) + " M " + (100 * w / all).toFixed(1).padStart(5) + " %  " + name);
            report.builds[b.name].guest = [...by].sort((x, y) => y[1] - x[1]).slice(0, 500);
        }
        if(r.step_profile)
        {
            const name = k => ["", "0F ", "0F 38 ", "0F 3A "][k >>> 24] + ["", "66 ", "F3 ", "F2 "][k >>> 20 & 3] + (k & 0xFF).toString(16).toUpperCase().padStart(2, "0") + " /" + (k >>> 16 & 7);
            const hex = b => b ? "   " + [b[0], b[1]].map(w => Array.from({ length: 4 }, (_, i) => (w >>> 8 * i & 0xFF).toString(16).padStart(2, "0")).join(" ")).join(" ") : "";
            for(const [title, list] of [["Instructions the interpreter ran inside compiled blocks (map, prefix, opcode, /reg)", r.step_profile.in_block],
                ["Instructions compiled blocks ended at (with the first bytes seen)", r.step_profile.block_end]])
            {
                const sum = list.reduce((s, e) => s + e.count, 0);
                say("\n" + title + ": " + (sum / 1e6).toFixed(1) + " M");
                for(const e of list.slice(0, top)) say("  " + String(e.count).padStart(10) + (100 * e.count / sum).toFixed(1).padStart(6) + " %  " + name(e.key) + hex(e.bytes));
            }
        }
        say("");
        say(format(a, top));
        if(r.stats)
        {
            say("\nCounters of the profiler build");
            for(const [k, v] of Object.entries(r.stats)) say("  " + k.padEnd(52) + v);
        }
        say("\n  " + path.relative(process.cwd(), file) + ": open in Chrome DevTools (Performance > Load profile) or speedscope.app");
    }
}

// 2. the timing runs
if(runs > 0)
{
    say("\n==== timing: " + runs + " runs per build, taking turns ====");
    for(let i = 0; i < runs; i++)
    {
        for(let j = 0; j < builds.length; j++)
        {
            const b = builds[(i + j) % builds.length];      // (the order rotates: no build always runs first)
            const r = await run(b, {});
            report.builds[b.name].runs.push(r);
            say("  run " + String(i + 1).padStart(2) + "  " + b.name.padEnd(12) + describe_run(r) + "   cpu " + (r.end ? r.end.cpu_main.toFixed(2) : "-") +
                " s, " + (r.end ? (r.end.insn / 1e6).toFixed(0) : "-") + " M instructions");
        }
    }
    const median = v => { const s = [...v].sort((a, b) => a - b); return s.length % 2 ? s[s.length >> 1] : (s[s.length / 2 - 1] + s[s.length / 2]) / 2; };
    say("\nSummary (seconds from the start to each mark; cpu: the emulator's thread; spread: slowest against fastest)");
    say("  " + "build".padEnd(12) + "mark".padEnd(14) + "median".padStart(8) + "fastest".padStart(9) + "spread".padStart(8) + "  cpu median" + "   insn M" + (builds.length > 1 ? "   against " + builds[0].name : ""));
    const medians = {};
    for(const b of builds)
    {
        const good = report.builds[b.name].runs.filter(r => r.ok);
        if(!good.length) { say("  " + b.name.padEnd(12) + "no successful run"); continue; }
        marks.forEach((m, i) => {
            const wall = good.map(r => r.marks[i].wall), cpu = good.map(r => r.marks[i].cpu_main), insn = good.map(r => r.marks[i].insn);
            const med = median(wall);
            medians[b.name + "/" + m.name] = med;
            const ref = medians[builds[0].name + "/" + m.name];
            say("  " + b.name.padEnd(12) + m.name.slice(0, 13).padEnd(14) + med.toFixed(2).padStart(8) + Math.min(...wall).toFixed(2).padStart(9) +
                ((Math.max(...wall) / Math.min(...wall) - 1) * 100).toFixed(1).padStart(6) + " %" + median(cpu).toFixed(2).padStart(12) + (median(insn) / 1e6).toFixed(0).padStart(9) +
                (builds.length > 1 && b !== builds[0] && ref ? ((med / ref - 1) * 100 > 0 ? "+" : "") + ((med / ref - 1) * 100).toFixed(1).padStart(8) + " %" : ""));
        });
    }
    say("  A difference between builds that is smaller than the spread is noise: run more rounds, and nothing else on the machine.");
}

fs.writeFileSync(path.join(out_dir, "report.json"), JSON.stringify(report, null, 1));
fs.writeFileSync(path.join(out_dir, "report.txt"), lines.join("\n") + "\n");
console.log("\nreport: " + path.relative(process.cwd(), path.join(out_dir, "report.txt")) + " (and report.json)");
