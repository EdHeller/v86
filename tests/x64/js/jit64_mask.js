// jit64_disable_mask: bit k keeps instructions of kind k (see `kind` in src/rust/cpu/jit64.rs) out of compiled blocks.
// Checks, on the jit tests, that
//   - every mask still gives the right result (the interpreter takes over for the masked instructions),
//   - an all-ones mask compiles nothing and an empty mask compiles something,
//   - bits above the last kind change nothing,
//   - every bit 0..24 changes how much code is compiled for at least one test (a bit that did nothing would be a
//     dead or mis-numbered kind).
// Needs nasm; runs worker.js once per (test, mask), a few at a time.

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import url from "node:url";
import { execFile } from "node:child_process";
import { promisify } from "node:util";

const exec = promisify(execFile);
const __dirname = path.dirname(url.fileURLToPath(import.meta.url));
const root = path.join(__dirname, "..");
const KINDS = 25;
const ALL = (1 << KINDS) - 1;
const TESTS = ["01_loops", "02_blit", "03_flags_conditions", "06_instruction_forms", "07_flag_liveness", "08_bit_string_misc", "09_masked_kinds"];

const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "jit64mask-"));
process.on("exit", () => fs.rmSync(tmp, { recursive: true, force: true }));

async function build(name)
{
    const bin = path.join(tmp, name + ".bin");
    await exec("nasm", ["-f", "bin", "-i", root + "/", path.join(root, "jit", name + ".asm"), "-o", bin]);
    return bin;
}

async function run(bin, mask)
{
    const { stdout } = await exec(process.execPath, [path.join(root, "worker.js"), bin, JSON.stringify({ jit64_threshold: 10, sync_jit: true })],
        { env: { ...process.env, JIT_STATS: "1", JIT64: "10", JIT64_MASK: String(mask) }, timeout: 60000 });
    return JSON.parse(stdout.trim().split("\n").pop());
}

const jobs = [];
const results = new Map();
for(const name of TESTS)
{
    const bin = await build(name);
    const masks = [0, ALL, 1 << 25, 1 << 31, ...Array.from({ length: KINDS }, (_, k) => 1 << k)];
    for(const m of masks) jobs.push([name, bin, m]);
}
let next = 0;
await Promise.all(Array.from({ length: Math.max(1, os.cpus().length) }, async () => {
    while(next < jobs.length)
    {
        const [name, bin, m] = jobs[next++];
        results.set(name + ":" + m, await run(bin, m));
    }
}));

const errors = [];
const effect = new Array(KINDS).fill(false);
for(const name of TESTS)
{
    const r = m => results.get(name + ":" + m);
    for(const [key, res] of results) if(key.startsWith(name + ":") && res.status !== "pass")
        errors.push(`${name} mask 0x${(+key.split(":")[1] >>> 0).toString(16)}: status ${res.status} (check ${res.id}, actual ${res.actual}, expected ${res.expected})`);
    if(!(r(0).jit64.insns > 0)) errors.push(`${name}: nothing compiled with an empty mask`);
    if(r(ALL).jit64.compiled !== 0 || r(ALL).jit64.insns !== 0) errors.push(`${name}: all-ones mask still compiled ${r(ALL).jit64.compiled} blocks / ${r(ALL).jit64.insns} instructions`);
    for(const m of [1 << 25, 1 << 31])
        if(r(m).jit64.insns !== r(0).jit64.insns || r(m).jit64.compiled !== r(0).jit64.compiled)
            errors.push(`${name}: reserved mask bit 0x${(m >>> 0).toString(16)} changed the compiled code`);
    for(let k = 0; k < KINDS; k++)
        if(r(1 << k).jit64.insns !== r(0).jit64.insns || r(1 << k).jit64.compiled !== r(0).jit64.compiled) effect[k] = true;
}
for(let k = 0; k < KINDS; k++) if(!effect[k]) errors.push(`mask bit ${k} changed nothing in any test (kind ${k} unused by the tests, or not wired up)`);

if(errors.length)
{
    console.error(errors.join("\n"));
    process.exit(1);
}
console.log("jit64 mask: %d runs ok", results.size);
