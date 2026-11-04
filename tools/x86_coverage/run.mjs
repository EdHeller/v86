#!/usr/bin/env node
// Runs the probe of tools/x86_coverage/probe.mjs in v86 and in QEMU (TCG, -cpu max) and compares the outcomes.
//
//   node tools/x86_coverage/run.mjs [bits] [v86|v64|qemu|all|jit]       default: 32 all
//
// jit: runs the probes in v86 (v64 for 64) once interpreted and once compiled (several passes over the probes with the
// JIT's threshold lowered, the last pass compared) and writes tools/x86_coverage/jit-<bits>.md: where they differ.
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
const DONE = 0x45444F4E;

const stub = fs.readFileSync(path.join(OUT, `probe${bits}-stub.bin`));
const probes = fs.readFileSync(path.join(OUT, `probe${bits}-probes.bin`));
const { layout, probed, skipped } = JSON.parse(fs.readFileSync(path.join(OUT, `probe${bits}-forms.json`), "utf8"));
const { PROBES, RESULTS, DUMPS, DUMP_SIZE, MEMORY_MB } = layout;

async function run_v86(wasm, jit_passes = 0, jit = true)
{
    // (the release bundle: the source tree asserts on 32-bit reads of 8-bit ports, which the probe does on purpose)
    const { V86 } = await import(path.join(ROOT, "build/libv86.mjs"));
    // (x86_64: libv64.js registers the architecture next to the bundle)
    if(wasm.startsWith("v64")) await import(path.join(ROOT, "build/libv64.js"));
    const buf = p => { const b = fs.readFileSync(p); return b.buffer.slice(b.byteOffset, b.byteOffset + b.byteLength); };
    const emulator = new V86({
        wasm_path: path.join(ROOT, "build", wasm),
        bios: { buffer: buf(path.join(ROOT, "bios/seabios.bin")) },
        memory_size: MEMORY_MB << 20, cpu_type: wasm.startsWith("v64") ? "x86_64" : "x86", autostart: false, log_level: 0,
        ...(jit_passes && jit ? { jit64_threshold: 1, sync_jit: true } : { disable_jit: true }),
    });
    await new Promise(r => emulator.add_listener("emulator-loaded", r));
    const cpu = emulator.v86.cpu;
    // (the JIT: pages compiled after little heat (59: the 16/32-bit JIT; 4: jit64), the set run jit_passes times)
    if(jit_passes && jit)
    {
        cpu.wm.exports["set_jit_config"](59, 100);
        cpu.wm.exports["set_jit_config"](4, 1);
    }
    cpu.reboot_internal();
    cpu.reset_memory();
    cpu.mem8.set(stub, 0x7C00);
    cpu.mem8.set(probes, PROBES);
    cpu.mem8.set([0xEA, 0x00, 0x7C, 0x00, 0x00], 0xFFFF0);
    cpu.mem32s[0x634 >> 2] = jit_passes;
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
    // (the profiler build counts the compiled entry points: X86_COVERAGE_WASM=v86-prof.wasm)
    if(jit_passes && jit && cpu.wm.exports["jit_get_cache_size"]) { const st = n => cpu.wm.exports["profiler_stat_get"](n); console.log(`threshold ${cpu.wm.exports["get_jit_config"](59)} stats ${[0,1,2,3,4,5,6,7].map(st).join(" ")}; compiles ${st(0)}, instructions interpreted ${st(24)}, compiled ${st(26)}, different state ${st(18)}`); }
    const outcome = Array.from({ length: probed.length }, (_, i) => m32(RESULTS + i * 4) | 0);
    const dumps = Buffer.from(cpu.mem8.slice(DUMPS, DUMPS + probed.length * DUMP_SIZE));
    return { seconds: (Date.now() - t0) / 1000, outcome, dumps };
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
        "-cpu", "max", "-m", String(MEMORY_MB), "-bios", bios,
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
    // the outcomes and the state dumps, saved by QEMU's monitor
    const save = async (addr, size, file) => {
        if(fs.existsSync(file)) fs.unlinkSync(file);
        await ask(`pmemsave ${addr} ${size} "${file}"`);
        for(let i = 0; i < 300 && !(fs.existsSync(file) && fs.statSync(file).size === size); i++)
        {
            await new Promise(r => setTimeout(r, 100));
        }
        return fs.readFileSync(file);
    };
    const b = await save(RESULTS, probed.length * 4, dump);
    const dumps = await save(DUMPS, probed.length * DUMP_SIZE, path.join(OUT, `qemu${bits}-dumps.bin`));
    child.kill();
    const outcome = Array.from({ length: probed.length }, (_, i) => b.readInt32LE(i * 4));
    return { seconds: (Date.now() - t0) / 1000, outcome, dumps };
}

if(which === "jit")
{
    const wasm = process.env.X86_COVERAGE_WASM || (bits === 64 ? "v64.wasm" : "v86.wasm");
    // (both six passes: a form leaves memory outside scratch changed for the next pass, bts beyond it for one)
    // (jit64 compiles synchronously; the 16/32-bit JIT one module per turn of the event loop, about 80% of the
    // probes' pages compiled after 300 passes)
    const passes = +process.env.X86_COVERAGE_PASSES || (bits === 64 ? 6 : 300);
    const runs = [await run_v86(wasm, passes, false), await run_v86(wasm, passes)];
    for(const r of runs) if(!r.outcome) { const f = probed[r.hang]; console.log(`hangs at probe ${r.hang}: ${f ? f.full + " " + f.text : "?"}`); process.exit(1); }
    const [a, b] = runs;
    const SKIP = /^(rdtsc|rdtscp|rdrand|rdseed|rdpid|rdpmc|in|ins[bwd]?|fn?stenv|fn?save|fxsave|xsave)/;
    const lines = [];
    probed.forEach((f, i) => {
        if(SKIP.test(f.mnemonic)) return;
        const what = [];
        if(a.outcome[i] !== b.outcome[i]) what.push(`outcome ${a.outcome[i]} / ${b.outcome[i]}`);
        else if(a.outcome[i] === -1)
        {
            const x = a.dumps.subarray(i * DUMP_SIZE, (i + 1) * DUMP_SIZE), y = b.dumps.subarray(i * DUMP_SIZE, (i + 1) * DUMP_SIZE);
            for(let k = 0; k < DUMP_SIZE; k += 4) if(x.readUInt32LE(k) !== y.readUInt32LE(k)) what.push(`+${k}: ${x.readUInt32LE(k).toString(16)} / ${y.readUInt32LE(k).toString(16)}`);
        }
        if(what.length) lines.push(`| \`${f.full}\` | ${f.text.replace(/\|/g, "\\|")} | ${what.slice(0, 4).join("<br>")} |`);
    });
    fs.writeFileSync(path.join(ROOT, `tools/x86_coverage/jit-${bits}.md`), `# ${bits}-bit: the interpreter against the JIT

\`run.mjs ${bits} jit\`: ${passes} passes over the probes interpreted (disable_jit) and ${passes} compiled (the pages
compiled after little heat); the last passes compared. Interpreted ${a.seconds.toFixed(1)} s, compiled ${b.seconds.toFixed(1)} s.
Left out: time stamps, random numbers, port input, saved FPU environments (their instruction pointers differ).
Dump offsets: ${bits === 64 ? "rax..r15 at 0..127, rflags 128, xmm0..15 at 144, memory at 400" : "eax..edi at 0..31, eflags 32, xmm0..7 at 48, memory at 176"}.

**${lines.length} forms differ** (interpreted / compiled)

| Form | Instruction | Differences |
|---|---|---|
${lines.join("\n")}
`);
    console.log(`interpreted ${a.seconds.toFixed(1)} s, compiled ${b.seconds.toFixed(1)} s: ${lines.length} forms differ`);
    process.exit(0);
}

const machines = which === "all" ? [bits === 64 ? "v64" : "v86", "qemu"] : [which];
const results = {};
const dumps = {};
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
    dumps[m] = r.dumps;
    if(r.dumps) fs.writeFileSync(path.join(OUT, `dumps${bits}-${m}.bin`), r.dumps);
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
    // why a form is #UD on one machine only (checked by hand)
    const UD_WHY = {
        extrq: "SSE4a (AMD only)", insertq: "SSE4a (AMD only)", movntsd: "SSE4a (AMD only)", movntss: "SSE4a (AMD only)",
        femms: "3DNow! (AMD only)", xbeginw: "TSX (not on v86's CPU)", xbegind: "TSX (not on v86's CPU)",
        movhpd: "QEMU: runs the register form (SDM: memory only, #UD)", movlpd: "QEMU: runs the register form (SDM: memory only, #UD)",
        movntdq: "QEMU: runs the register form (SDM: memory only, #UD)", movntq: "QEMU: runs the register form (SDM: memory only, #UD)",
        nop: "0F 18 with a register operand: a hint NOP on Intel CPUs; QEMU raises #UD",
        test: "F6/F7 /1: the undocumented alias of test /0, which CPUs run; QEMU raises #UD",
        xgetbv: "v86 ignores a 66/F2/F3 prefix here (SDM: #UD)", xsetbv: "v86 ignores a 66/F2/F3 prefix here (SDM: #UD)",
    };
    const list = map => [...map].sort((a, b) => a[0].localeCompare(b[0])).map(([m, fs]) =>
        `| \`${m}\` | ${fs.length} | ${fs.slice(0, 3).map(f => "`" + f.full + "` " + f.text.replace(/\|/g, "\\|")).join("<br>")} | ${UD_WHY[m] || "**to check**"} |`).join("\n");
    let md = `# x86 instruction coverage, ${{ 16: "16-bit code (16-bit code segments, protected mode)", 32: "32-bit protected mode", 64: "64-bit long mode" }[bits]}

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

| Mnemonic | Forms | Examples | Why |
|---|---|---|---|
${list(missing)}

## Accepted by v86, #UD in QEMU

| Mnemonic | Forms | Examples | Why |
|---|---|---|---|
${list(extra)}

## #UD in both

${[...both.keys()].sort().map(m => "`" + m + "`").join(", ")}
`;
    // values: where both ran the form without an exception, the registers, flags, the xmm registers and memory afterwards
    const d_ours = dumps.v86 || dumps.v64, d_ref = dumps.qemu;
    const NOVALUE = new Set(["rdtsc", "rdtscp", "rdrand", "rdseed", "rdpid", "cpuid", "rdpmc", "xgetbv", "rdmsr", "sgdtd", "sidtd",
        "sgdt", "sidt", "smsw", "str", "sldt", "lsl", "lar", "verr", "verw", "in", "ins", "insb", "insw", "insd", "fnstenv",
        "fnsave", "fxsave", "xsave", "xsaveopt", "xsavec", "xsaves", "fstenv", "fnstsw", "fstsw"]);
    const W = bits === 64 ? 8 : 4, NXMM = bits === 64 ? 16 : 8;
    const REGS = bits === 64 ? ["rax", "rcx", "rdx", "rbx", "rsp", "rbp", "rsi", "rdi", "r8", "r9", "r10", "r11", "r12", "r13", "r14", "r15"] :
        ["eax", "ecx", "edx", "ebx", "esp", "ebp", "esi", "edi"];
    const FLAGS = bits === 64 ? 128 : 32, XMM = bits === 64 ? 144 : 48, MEM = bits === 64 ? 400 : 176;
    const value_diffs = new Map(), flag_diffs = new Map();
    let compared = 0;
    if(d_ours && d_ref)
    {
        probed.forEach((f, i) => {
            if(ours[i] !== -1 || ref[i] !== -1 || NOVALUE.has(f.mnemonic)) return;
            compared++;
            const a = d_ours.subarray(i * DUMP_SIZE, (i + 1) * DUMP_SIZE), b = d_ref.subarray(i * DUMP_SIZE, (i + 1) * DUMP_SIZE);
            const what = [];
            REGS.forEach((name, k) => { if(!a.subarray(W * k, W * k + W).equals(b.subarray(W * k, W * k + W))) what.push(name); });
            for(let k = 0; k < NXMM; k++) if(!a.subarray(XMM + 16 * k, XMM + 16 + 16 * k).equals(b.subarray(XMM + 16 * k, XMM + 16 + 16 * k))) what.push("xmm" + k);
            if(!a.subarray(MEM, MEM + 64).equals(b.subarray(MEM, MEM + 64))) what.push("memory");
            // (the arithmetic flags: CF PF AF ZF SF OF, DF; many instructions leave some undefined)
            const fa = a.readUInt32LE(FLAGS) & 0xCD5, fb = b.readUInt32LE(FLAGS) & 0xCD5;
            const show = list => list.concat(f);
            if(what.length) { if(!value_diffs.has(f.mnemonic)) value_diffs.set(f.mnemonic, []); value_diffs.get(f.mnemonic).push([f, what]); }
            else if(fa !== fb) { if(!flag_diffs.has(f.mnemonic)) flag_diffs.set(f.mnemonic, []); flag_diffs.get(f.mnemonic).push([f, (fa ^ fb).toString(16)]); }
        });
    }
    // differences checked by hand: QEMU's deviations from the SDM, and flags the SDM leaves undefined
    const WHY = {
        fst: "QEMU: no x87 stack underflow (stores an empty register's stale bits; v86 stores the indefinite, as CPUs)",
        fstp: "QEMU: no x87 stack underflow", fist: "QEMU: no x87 stack underflow", fistp: "QEMU: no x87 stack underflow",
        fisttp: "QEMU: no x87 stack underflow", fcomi: "QEMU: no x87 stack underflow (v86: unordered)",
        fucomi: "QEMU: no x87 stack underflow (v86: unordered)",
        fnsavew: "QEMU: an MMX write leaves the exponent field (SDM: bits 64..79 become all ones, as in v86)",
        enterw: "QEMU: writes all of EBP (SDM: with a 16-bit operand size only BP)",
        mov: "reads beyond the end of RAM (unmapped memory reads differently)",
        rcl: "OF is undefined for counts above 1", shld: "OF is undefined for counts above 1",
        shrd: "OF is undefined for counts above 1",
        tzcnt: "without BMI1 (v86 does not report it) tzcnt is bsf: ZF/CF differ by design",
        rol: "OF is undefined for counts above 1", shl: "OF is undefined for counts above 1",
        bsf: "PF is undefined", bsr: "PF is undefined", lzcnt: "PF is undefined",
        movbe: "QEMU: 66 beats REX.W (SDM: REX.W wins, a 64-bit movbe as in v86)",
        fnstenvw: "the reserved upper halves of the 32-bit environment (66 48: REX.W wins, the 32-bit format)",
        fxsave64: "QEMU: an MMX write leaves the exponent field (SDM: all ones, as in v86)",
    };
    if(bits === 16) Object.assign(WHY, {
        crc32: "QEMU: a 32-bit operand in 16-bit code without 66 (v86 checked by hand: CRC-32C of the 16-bit value)",
        popcnt: "QEMU: a 32-bit operand in 16-bit code without 66", movbe: "QEMU: a 32-bit operand in 16-bit code without 66",
        enter: "QEMU: writes all of EBP (SDM: with a 16-bit operand size only BP)",
        fnsaved: "the reserved upper halves of the 32-bit image: v86 writes ones, QEMU zeros; QEMU's MMX exponent",
        fnstenvd: "the reserved upper halves of the 32-bit environment: v86 writes ones, QEMU zeros",
        bt: "v86: a bit offset beyond the segment does not wrap the 16-bit effective address at 64 KiB (rare)",
        btc: "v86: a bit offset beyond the segment does not wrap the 16-bit effective address at 64 KiB (rare)",
        btr: "v86: a bit offset beyond the segment does not wrap the 16-bit effective address at 64 KiB (rare)",
        bts: "v86: a bit offset beyond the segment does not wrap the 16-bit effective address at 64 KiB (rare)",
    });
    // (QEMU extends mmx registers with REX.R and REX.B, reading and writing past mm7: SDM, REX does not apply)
    const why = (m, f) => WHY[m] || (bits === 64 && /^(66|f2|f3)?4[1-7c-f]/.test(f.full) && /\bmm\d/.test(f.text) ?
        "QEMU: REX.R/REX.B select mm8..mm15 (into the xmm state); SDM: REX does not apply to mmx registers" : null);
    const vlist = map => [...map].sort((a, b) => a[0].localeCompare(b[0])).map(([m, l]) =>
        `| \`${m}\` | ${l.length} | ${l.slice(0, 3).map(([f, w]) => "`" + f.full + "` " + f.text.replace(/\|/g, "\\|") + ": " + (Array.isArray(w) ? w.join(" ") : "flags " + w)).join("<br>")} | ${why(m, l[0][0]) || "**to check**"} |`).join("\n");
    md += `
## Results that differ from QEMU's

Where both ran a form without an exception (${compared} forms; the time stamp counter, random numbers, cpuid, port input,
descriptor table addresses and FPU or XSAVE state images left out): the registers, the xmm registers and the 64 bytes of memory
the memory forms point at.

| Mnemonic | Forms | Examples (what differs) | Why |
|---|---|---|---|
${vlist(value_diffs)}

## Only the arithmetic flags differ

(CF PF AF ZF SF OF DF; many instructions leave some of them undefined, so not every one of these is a bug)

| Mnemonic | Forms | Examples (flags that differ) | Why |
|---|---|---|---|
${vlist(flag_diffs)}
`;
    fs.writeFileSync(path.join(ROOT, `tools/x86_coverage/coverage-${bits}.md`), md);
    console.log(`values differ: ${[...value_diffs.values()].flat().length} forms of ${value_diffs.size} mnemonics; ` +
        `only flags differ: ${[...flag_diffs.values()].flat().length} of ${flag_diffs.size}`);
    console.log(`missing in v86: ${[...missing.values()].flat().length} forms of ${missing.size} mnemonics; ` +
        `accepted only by v86: ${[...extra.values()].flat().length} of ${extra.size}; #UD in both: ${both.size} mnemonics`);
}
