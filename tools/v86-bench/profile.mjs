#!/usr/bin/env node
// Where the time goes in two (or more) builds: one boot and the workloads of run.mjs per build, under V8's CPU
// profiler, compared by area (compiled code, the main WebAssembly module, JavaScript) and by function. Text out.
//
//   node profile.mjs upstream ours8 | tee profile.txt        (SCALE=n and ARCH=x86_64 as for run.mjs)

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import url from "node:url";
import { spawnSync } from "node:child_process";

const here = path.dirname(url.fileURLToPath(import.meta.url));
const builds = process.argv.slice(2);
if(builds.length < 1) { console.log("usage: node profile.mjs <build> [build...]"); process.exit(1); }

const profiles = {};
for(const build of builds)
{
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "v86-prof-"));
    const r = spawnSync(process.execPath, ["--cpu-prof", "--cpu-prof-dir=" + dir, path.join(here, "run.mjs"), "--child", build],
        { env: { ...process.env, INSN: "1" }, encoding: "utf8", maxBuffer: 1 << 26 });
    console.log(build + ": " + (r.stdout || "").trim());
    const file = fs.readdirSync(dir).find(f => f.endsWith(".cpuprofile"));
    const p = JSON.parse(fs.readFileSync(path.join(dir, file), "utf8"));
    fs.rmSync(dir, { recursive: true });

    const nodes = new Map(p.nodes.map(n => [n.id, n]));
    const ticks = new Map();
    p.samples.forEach((s, i) => ticks.set(s, (ticks.get(s) || 0) + (p.timeDeltas[i] || 0)));
    const by_fn = {}, by_area = {};
    for(const [id, us] of ticks)
    {
        const n = nodes.get(id), u = n.callFrame.url;
        let name = n.callFrame.functionName.replace(/^_R[A-Za-z0-9_]*?_3v86/, "").replace(/^_RINv.*/, "(generic)");
        let area;
        if(u.includes("v86.wasm")) area = "main module";
        else if(u.startsWith("wasm://")) { area = "compiled code"; name = "[compiled code]"; }
        else if(name.startsWith("(")) area = "other";
        else { area = "javascript"; name = "[js] " + name; }
        by_fn[name] = (by_fn[name] || 0) + us / 1e6;
        by_area[area] = (by_area[area] || 0) + us / 1e6;
    }
    profiles[build] = { by_fn, by_area };
}

const cols = builds.map(b => b.slice(0, 12).padStart(13)).join("");
console.log("\nseconds by area\n" + "".padEnd(16) + cols);
for(const a of ["compiled code", "main module", "javascript", "other"])
{
    console.log(a.padEnd(16) + builds.map(b => (profiles[b].by_area[a] || 0).toFixed(2).padStart(13)).join(""));
}
const names = new Set(builds.flatMap(b => Object.keys(profiles[b].by_fn)));
const rows = [...names].map(n => [n, builds.map(b => profiles[b].by_fn[n] || 0)])
    .sort((x, y) => Math.max(...y[1]) - Math.max(...x[1])).slice(0, 40);
console.log("\nseconds by function (the 40 largest)\n" + cols);
for(const [n, v] of rows) console.log(v.map(x => x.toFixed(2).padStart(13)).join("") + "  " + n.slice(0, 90));
