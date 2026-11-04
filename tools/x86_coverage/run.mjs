#!/usr/bin/env node
// Runs the probe of tools/x86_coverage/probe.mjs in v86 and in QEMU (TCG, -cpu max) and compares the outcomes.
//
//   node tools/x86_coverage/run.mjs [bits] [v86|v64|qemu|all]       default: 32 all
//
// Writes build/x86_coverage/outcome<bits>-<machine>.json (the vector each probe raised, -1 for none) and
// tools/x86_coverage/coverage-<bits>.md: the forms v86 rejects with #UD that QEMU executes (missing in v86), the ones
// v86 executes that QEMU rejects, and the counts.

import fs from "node:fs";
import path from "node:path";
import url from "node:url";
import { spawn } from "node:child_process";

const ROOT = path.join(path.dirname(url.fileURLToPath(import.meta.url)), "../..");
const OUT = path.join(ROOT, "build/x86_coverage");
const bits = +process.argv[2] || 32;
const which = process.argv[3] || "all";
const PROBES = 0x100000, RESULTS = 0x300000, DONE = 0x45444F4E;

const stub = fs.readFileSync(path.join(OUT, `probe${bits}-stub.bin`));
const probes = fs.readFileSync(path.join(OUT, `probe${bits}-probes.bin`));
const { probed, skipped } = JSON.parse(fs.readFileSync(path.join(OUT, `probe${bits}-forms.json`), "utf8"));

async function run_v86(wasm)
{
    // (the release bundle: the source tree asserts on 32-bit reads of 8-bit ports, which the probe does on purpose)
    const { V86 } = await import(path.join(ROOT, "build/libv86.mjs"));
    const buf = p => { const b = fs.readFileSync(p); return b.buffer.slice(b.byteOffset, b.byteOffset + b.byteLength); };
    const emulator = new V86({
        wasm_path: path.join(ROOT, "build", wasm),
        bios: { buffer: buf(path.join(ROOT, "bios/seabios.bin")) },
        memory_size: 64 << 20, cpu_type: wasm.startsWith("v64") ? "x86_64" : "x86", autostart: false, log_level: 0,
    });
    await new Promise(r => emulator.add_listener("emulator-loaded", r));
    const cpu = emulator.v86.cpu;
    cpu.reboot_internal();
    cpu.reset_memory();
    cpu.mem8.set(stub, 0x7C00);
    cpu.mem8.set(probes, PROBES);
    cpu.mem8.set([0xEA, 0x00, 0x7C, 0x00, 0x00], 0xFFFF0);
    emulator.run();
    const m32 = a => cpu.mem32s[a >> 2];
    const t0 = Date.now();
    let last = -2, since = Date.now();
    while(m32(0x600) >>> 0 !== DONE)
    {
        await new Promise(r => setTimeout(r, 200));
        const cur = m32(0x608);
        if(cur !== last) { last = cur; since = Date.now(); }
        if(Date.now() - since > 20000) { emulator.stop(); return { hang: cur, outcome: null }; }
    }
    emulator.stop();
    const outcome = Array.from({ length: probed.length }, (_, i) => m32(RESULTS + i * 4));
    return { seconds: (Date.now() - t0) / 1000, outcome };
}

async function run_qemu()
{
    const bios = path.join(OUT, "reset.bios");
    const img = Buffer.alloc(65536, 0xF4);
    Buffer.from([0xEA, 0x00, 0x7C, 0x00, 0x00]).copy(img, 0xFFF0);
    fs.writeFileSync(bios, img);
    const stub_file = path.join(OUT, `probe${bits}-stub.bin`), probes_file = path.join(OUT, `probe${bits}-probes.bin`);
    const dump = path.join(OUT, `qemu${bits}-results.bin`);
    const child = spawn("qemu-system-x86_64", [
        "-display", "none", "-monitor", "stdio", "-serial", "none", "-no-reboot", "-machine", "pc,accel=tcg",
        "-cpu", "max", "-m", "64", "-bios", bios,
        "-device", `loader,file=${stub_file},addr=0x7c00`, "-device", `loader,file=${probes_file},addr=${PROBES}`,
    ], { stdio: ["pipe", "pipe", "ignore"] });
    let out = "", exited = false;
    child.stdout.on("data", d => out += d);
    child.on("exit", () => { exited = true; });
    const ask = async cmd => { out = ""; child.stdin.write(cmd + "\n"); await new Promise(r => setTimeout(r, 150)); return out; };
    const t0 = Date.now();
    let last = "", since = Date.now();
    for(;;)
    {
        await new Promise(r => setTimeout(r, 300));
        if(exited) return { hang: -1, outcome: null, reason: "qemu exited (a triple fault)" };
        const m = /0000000000000600:((?: 0x[0-9a-f]{8}){4})/.exec(await ask("xp /4wx 0x600"));
        if(!m) continue;
        const w = m[1].trim().split(" ").map(Number);
        if(w[0] === DONE) break;
        if(String(w[2]) !== last) { last = String(w[2]); since = Date.now(); }
        if(Date.now() - since > 20000) { child.kill(); return { hang: w[2] | 0, outcome: null }; }
    }
    if(fs.existsSync(dump)) fs.unlinkSync(dump);
    await ask(`pmemsave ${RESULTS} ${probed.length * 4} "${dump}"`);
    for(let i = 0; i < 100 && !(fs.existsSync(dump) && fs.statSync(dump).size === probed.length * 4); i++)
    {
        await new Promise(r => setTimeout(r, 100));
    }
    child.kill();
    const b = fs.readFileSync(dump);
    const outcome = Array.from({ length: probed.length }, (_, i) => b.readInt32LE(i * 4));
    return { seconds: (Date.now() - t0) / 1000, outcome };
}

const machines = which === "all" ? ["v86", "qemu"] : [which];
const results = {};
for(const m of machines)
{
    const r = m === "qemu" ? await run_qemu() : await run_v86(m === "v64" ? "v64.wasm" : "v86.wasm");
    if(r.hang !== undefined && !r.outcome)
    {
        const f = probed[r.hang];
        console.log(`${m}: hangs at probe ${r.hang}: ${f ? f.full + " " + f.text : "?"}`);
        process.exit(1);
    }
    results[m] = r.outcome;
    fs.writeFileSync(path.join(OUT, `outcome${bits}-${m}.json`), JSON.stringify(r.outcome));
    const ud = r.outcome.filter(v => v === 6).length;
    console.log(`${m}: ${r.outcome.length} probes in ${r.seconds.toFixed(1)} s, ${ud} #UD`);
}

// the comparison: by mnemonic, the forms one machine rejects with #UD and the other runs
const ours = results.v86 || results.v64, ref = results.qemu;
if(ours && ref)
{
    const missing = new Map(), extra = new Map(), both = new Map();
    const add = (map, f) => { if(!map.has(f.mnemonic)) map.set(f.mnemonic, []); map.get(f.mnemonic).push(f); };
    probed.forEach((f, i) => {
        if(ours[i] === 6 && ref[i] !== 6) add(missing, f);
        else if(ours[i] !== 6 && ref[i] === 6) add(extra, f);
        else if(ours[i] === 6 && ref[i] === 6) add(both, f);
    });
    const list = map => [...map].sort((a, b) => a[0].localeCompare(b[0])).map(([m, fs]) =>
        `| \`${m}\` | ${fs.length} | ${fs.slice(0, 3).map(f => "`" + f.full + "` " + f.text.replace(/\|/g, "\\|")).join("<br>")} |`).join("\n");
    const md = `# x86 instruction coverage, ${bits}-bit protected mode

Generated by \`tools/x86_coverage\` (\`reference.py\`, \`probe.mjs\`, \`run.mjs\`): every instruction form GNU objdump
decodes (the one-byte, 0F, 0F 38 and 0F 3A maps with no prefix, 66, F2 and F3, each ModRM reg field, register and memory
forms; no VEX), executed once in v86 and in QEMU (TCG, \`-cpu max\`), comparing whether it raises #UD.

- ${probed.length} forms probed, ${skipped.length} skipped (control flow, descriptor tables, control registers, MSR writes,
  waits: covered by the hand-written tests)
- **${[...missing.values()].flat().length} forms of ${missing.size} mnemonics run in QEMU and are #UD in v86** (missing in v86)
- ${[...extra.values()].flat().length} forms of ${extra.size} mnemonics run in v86 and are #UD in QEMU
- ${[...both.values()].flat().length} forms of ${both.size} mnemonics are #UD in both (not on QEMU's CPU either: AMD-only,
  VMX/SVM outside a VM, encodings objdump accepts but CPUs do not)

## Missing in v86 (#UD in v86, not in QEMU)

| Mnemonic | Forms | Examples |
|---|---|---|
${list(missing)}

## Accepted by v86, #UD in QEMU

| Mnemonic | Forms | Examples |
|---|---|---|
${list(extra)}

## #UD in both

${[...both.keys()].sort().map(m => "`" + m + "`").join(", ")}
`;
    fs.writeFileSync(path.join(ROOT, `tools/x86_coverage/coverage-${bits}.md`), md);
    console.log(`missing in v86: ${[...missing.values()].flat().length} forms of ${missing.size} mnemonics; ` +
        `accepted only by v86: ${[...extra.values()].flat().length} of ${extra.size}; #UD in both: ${both.size} mnemonics`);
}
