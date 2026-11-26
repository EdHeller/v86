#!/usr/bin/env node
// Boots the i686 buildroot image with each build in turn and times the same guest work on the host's clock.
//
//   node bench.mjs --image /path/to/buildroot-bzimage68.bin [--runs 3] [--builds original,rust]
//
// Per build and round: seconds from start to the shell prompt, then each workload below (typed on the serial
// console, timed from the command to its end marker). Builds take turns round by round so drift on the host
// affects both alike. The medians are printed at the end, with rust against original.
import fs from "node:fs";
import path from "node:path";
import url from "node:url";

const here = path.dirname(url.fileURLToPath(import.meta.url));
const args = Object.fromEntries(process.argv.slice(2).reduce((a, x, i, all) => (x.startsWith("--") ? a.push([x.slice(2), all[i + 1]]) : 0, a), []));
const image = args.image || path.join(here, "buildroot-bzimage68.bin");
const runs = +(args.runs || 3);
const builds = (args.builds || "original,rust").split(",");
if(!fs.existsSync(image)) { console.error("no image at " + image + " (pass --image)"); process.exit(1); }

const WORKLOADS = [
    ["shell loop", "i=0; while [ $i -lt 300000 ]; do i=$((i+1)); done"],
    ["md5 64 MiB", "dd if=/dev/zero bs=1M count=64 2>/dev/null | md5sum >/dev/null"],
    ["sha256 32 MiB", "dd if=/dev/zero bs=1M count=32 2>/dev/null | sha256sum >/dev/null"],
    ["gzip 16 MiB", "dd if=/dev/urandom bs=1M count=16 2>/dev/null | gzip -1 >/dev/null"],
    ["fork 500", "i=0; while [ $i -lt 500 ]; do /bin/true; i=$((i+1)); done"],
];

const file = p => { const b = fs.readFileSync(p); return b.buffer.slice(b.byteOffset, b.byteOffset + b.byteLength); };

async function run(build)
{
    const dir = path.join(here, build);
    const { V86 } = await import(url.pathToFileURL(path.join(dir, "libv86.mjs")).href + "?" + Math.random());
    const t0 = performance.now();
    const e = new V86({
        wasm_path: path.join(dir, "v86.wasm"),
        bios: { buffer: file(path.join(here, "bios/seabios.bin")) },
        vga_bios: { buffer: file(path.join(here, "bios/vgabios.bin")) },
        bzimage: { buffer: file(image) },
        cmdline: "tsc=reliable mitigations=off random.trust_cpu=on console=ttyS0",
        memory_size: 256 << 20, autostart: true, log_level: 0, filesystem: {}, disable_speaker: true,
    });
    let out = "";
    e.add_listener("serial0-output-byte", c => { out += String.fromCharCode(c); });
    const until = (re, ms) => new Promise((resolve, reject) => {
        const timer = setTimeout(() => { clearInterval(iv); reject(new Error("timeout waiting for " + re)); }, ms);
        const iv = setInterval(() => { if(re.test(out)) { clearInterval(iv); clearTimeout(timer); resolve(); } }, 5);
    });
    await until(/~% |# $/, 300000);
    const result = { boot: (performance.now() - t0) / 1000 };
    for(const [name, cmd] of WORKLOADS)
    {
        out = "";
        const t = performance.now();
        e.serial0_send(cmd + "; echo BENCH_END\n");
        await until(/\nBENCH_END/, 600000);
        result[name] = (performance.now() - t) / 1000;
    }
    await e.destroy();
    return result;
}

const results = Object.fromEntries(builds.map(b => [b, []]));
for(let r = 1; r <= runs; r++)
{
    for(const b of builds)
    {
        const res = await run(b);
        results[b].push(res);
        console.log(`round ${r} ${b.padEnd(9)} ` + Object.entries(res).map(([k, v]) => `${k} ${v.toFixed(2)}s`).join(", "));
    }
}
const median = xs => { const s = [...xs].sort((a, b) => a - b); return s[s.length >> 1]; };
console.log("\nmedians (seconds, host clock)");
const keys = ["boot", ...WORKLOADS.map(w => w[0])];
console.log("".padEnd(16) + builds.map(b => b.padStart(10)).join("") + (builds.length === 2 ? "    change" : ""));
for(const k of keys)
{
    const m = builds.map(b => median(results[b].map(x => x[k])));
    const change = builds.length === 2 ? `   ${((m[1] / m[0] - 1) * 100).toFixed(1).padStart(6)} %` : "";
    console.log(k.padEnd(16) + m.map(x => x.toFixed(2).padStart(10)).join("") + change);
}
process.exit(0);
