#!/usr/bin/env node
// Differential fuzzer for the integer instructions of 64-bit mode.
//
// Random instruction instances are run twice: natively on the host cpu (as a Linux ELF64 binary)
// and inside v86 in long mode. After each instruction all 16 general purpose registers, rflags
// and a small scratch memory area are captured and compared. Flags that the instruction leaves
// undefined are masked.
//
//   fuzz.js [--seed n] [--count n] [--family regex] [--verbose]
//
// Needs nasm and ld on a x86-64 Linux host.

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import url from "node:url";
import { execFileSync } from "node:child_process";

const __dirname = url.fileURLToPath(new URL(".", import.meta.url));
const { V86 } = await import("../../src/main.js");

const args = process.argv.slice(2);
const opt = (name, def) => { const i = args.indexOf("--" + name); return i < 0 ? def : args[i + 1]; };
const SEED = +opt("seed", 1);
const COUNT = +opt("count", 500);
const FAMILY = new RegExp(opt("family", "."));
const VERBOSE = args.includes("--verbose");
const BATCH = 100;

// ---------------------------------------------------------------------------------------------
// random numbers

let rng_state = SEED >>> 0;
function rnd()
{
    rng_state = rng_state + 0x6D2B79F5 | 0;
    let t = Math.imul(rng_state ^ rng_state >>> 15, 1 | rng_state);
    t = t + Math.imul(t ^ t >>> 7, 61 | t) ^ t;
    return ((t ^ t >>> 14) >>> 0) / 4294967296;
}
const rint = n => Math.floor(rnd() * n);
const pick = a => a[rint(a.length)];

const INTERESTING = [0n, 1n, 2n, 3n, 0x7Fn, 0x80n, 0xFFn, 0x7FFFn, 0x8000n, 0xFFFFn, 0x7FFFFFFFn, 0x80000000n, 0xFFFFFFFFn,
    0x7FFFFFFFFFFFFFFFn, 0x8000000000000000n, 0xFFFFFFFFFFFFFFFFn, 0xFFFFFFFFFFFFFFFEn, 0x100000000n];
function rand64()
{
    const r = rnd();
    if(r < 0.35) return pick(INTERESTING);
    if(r < 0.5) return BigInt(rint(256)) & 0xFFn;
    if(r < 0.6) return (1n << BigInt(rint(64))) - 1n;
    if(r < 0.7) return 1n << BigInt(rint(64));
    return BigInt(rint(0x100000000)) << 32n | BigInt(rint(0x100000000));
}

// ---------------------------------------------------------------------------------------------
// instruction templates

const REG64 = ["rax", "rcx", "rdx", "rbx", "rbp", "rsi", "rdi", "r8", "r9", "r10", "r11", "r12", "r13", "r14", "r15"];
const NAMES = {
    64: REG64,
    32: ["eax", "ecx", "edx", "ebx", "ebp", "esi", "edi", "r8d", "r9d", "r10d", "r11d", "r12d", "r13d", "r14d", "r15d"],
    16: ["ax", "cx", "dx", "bx", "bp", "si", "di", "r8w", "r9w", "r10w", "r11w", "r12w", "r13w", "r14w", "r15w"],
    8: ["al", "cl", "dl", "bl", "bpl", "sil", "dil", "r8b", "r9b", "r10b", "r11b", "r12b", "r13b", "r14b", "r15b"],
};
const MEMSIZE = { 8: "byte", 16: "word", 32: "dword", 64: "qword" };
const CF = 1, PF = 4, AF = 16, ZF = 64, SF = 128, OF = 2048;
const ALL = CF | PF | AF | ZF | SF | OF;

const reg = (size, avoid = []) => {
    let r;
    do { r = rint(REG64.length); } while(avoid.includes(r));
    return r;
};
const rn = (size, r) => NAMES[size][r];
const mem = () => `[rel scratch + ${rint(5) * 8 + rint(4)}]`;
const HIGH8 = ["ah", "ch", "dh", "bh"], LOW8 = ["al", "cl", "dl", "bl"];
const imm = size => { const v = rand64() & (size === 64 ? 0xFFFFFFFFn : (1n << BigInt(size)) - 1n); return size === 64 ? sext32(v) : v; };
function sext32(v) { return v & 0x80000000n ? v - 0x100000000n : v; }

// each generator returns { asm, mask } (mask: flags that are defined after the instruction, undefined
// flags are not compared; `sets` lists registers whose value is undefined and must not be compared)
const families = {
    high8() {
        // legacy high byte registers (only encodable without a REX prefix)
        const op = pick(["add", "sub", "adc", "sbb", "and", "or", "xor", "cmp", "test", "mov", "xchg"]);
        const a = pick(rint(2) ? HIGH8 : LOW8), b = pick(rint(2) ? HIGH8 : LOW8);
        const form = pick(["rr", "ri", "m"]);
        const asm = form === "rr" ? `${op} ${a}, ${b}` : form === "ri" ? `${op} ${a}, ${rint(256)}` : `${op} ${a}, byte ${mem()}`;
        return { asm: op === "xchg" && form !== "rr" ? `xchg ${a}, ${b}` : asm, mask: op === "mov" || op === "xchg" ? 0 : op === "and" || op === "or" || op === "xor" || op === "test" ? ALL & ~AF : ALL };
    },
    string() {
        const op = pick(["movs", "stos", "lods", "scas", "cmps"]);
        const size = pick([8, 16, 32, 64]);
        const suffix = { 8: "b", 16: "w", 32: "d", 64: "q" }[size];
        const rep = pick(["", "rep ", "repe ", "repne "]);
        const prefix = (op === "movs" || op === "stos" || op === "lods") ? (rep.startsWith("rep") ? "rep " : "") : rep;
        const back = rint(2) === 0;
        const n = rint(5);
        const setup = `lea rsi, [rel scratch + ${back ? 56 : 8}]\nlea rdi, [rel scratch + ${back ? 100 : 72}]\nmov ecx, ${n}\n${back ? "std" : "cld"}`;
        // addresses differ between the environments, compare them relative to the scratch area
        return { asm: `${prefix}${op}${suffix}`, setup, post: "push rax\nlea rax, [rel scratch]\nsub rsi, rax\nsub rdi, rax\npop rax", mask: op === "scas" || op === "cmps" ? ALL : 0 };
    },
    stack() {
        const kind = pick(["pushpop", "pushimm", "pushf", "leave", "pushmem"]);
        const r = rn(64, reg(64, [4])), r2 = rn(64, reg(64, [4]));
        switch(kind) {
            case "pushpop": return { asm: `push ${r}\npop ${r2}`, mask: 0 };
            case "pushimm": return { asm: `push ${rint(2) ? "-" : ""}${rint(1000)}\npop ${r2}`, mask: 0 };
            case "pushf": return { asm: `pushfq\npop ${r2}`, post: `and ${r2}, 0x8D5`, mask: ALL };
            case "pushmem": return { asm: `push qword ${mem()}\npop ${r2}`, mask: 0 };
            default: return { asm: `push rbp\nmov rbp, rsp\nsub rsp, 24\nleave`, mask: 0 };
        }
    },
    alu() {
        const op = pick(["add", "or", "adc", "sbb", "and", "sub", "xor", "cmp", "test"]);
        const size = pick([8, 16, 32, 64]);
        const form = pick(["rr", "ri", "rm", "mr", "mi"]);
        const a = rn(size, reg(size)), b = rn(size, reg(size));
        const i = op === "test" && form === "mi" ? imm(size) : imm(size);
        let asm;
        if(form === "rr") asm = `${op} ${a}, ${b}`;
        else if(form === "ri") asm = `${op} ${a}, ${size === 64 ? i : i}`;
        else if(form === "rm") asm = `${op} ${a}, ${MEMSIZE[size]} ${mem()}`;
        else if(form === "mr") asm = `${op} ${MEMSIZE[size]} ${mem()}, ${b}`;
        else asm = `${op} ${MEMSIZE[size]} ${mem()}, ${i}`;
        return { asm, mask: (op === "and" || op === "or" || op === "xor" || op === "test") ? ALL & ~AF : ALL };
    },
    incdec() {
        const op = pick(["inc", "dec", "neg", "not"]);
        const size = pick([8, 16, 32, 64]);
        const operand = rint(4) === 0 ? `${MEMSIZE[size]} ${mem()}` : rn(size, reg(size));
        return { asm: `${op} ${operand}`, mask: op === "not" ? 0 : ALL };
    },
    shift() {
        const op = pick(["rol", "ror", "rcl", "rcr", "shl", "shr", "sar"]);
        const size = pick([8, 16, 32, 64]);
        const operand = rint(4) === 0 ? `${MEMSIZE[size]} ${mem()}` : rn(size, reg(size, [1]));
        const form = pick(["one", "imm", "cl"]);
        let count, asm;
        if(form === "one") { asm = `${op} ${operand}, 1`; count = 1; }
        else if(form === "imm") { count = rint(size === 64 ? 70 : 40); asm = `${op} ${operand}, ${count & 0xFF}`; }
        else { count = null; asm = `${op} ${operand}, cl`; }
        let mask;
        if(op === "rol" || op === "ror" || op === "rcl" || op === "rcr") mask = CF | (count === 1 ? OF : 0);
        else mask = CF | PF | ZF | SF | (count === 1 ? OF : 0);
        // flags are unchanged for a zero count, which is handled by the masks being a subset;
        // with cl the count is unknown so only compare the always defined subset
        if(count === null) mask = op === "rol" || op === "ror" || op === "rcl" || op === "rcr" ? CF : CF | PF | ZF | SF;
        return { asm, mask, cl: count === null };
    },
    muldiv() {
        const size = pick([8, 16, 32, 64]);
        const kind = pick(["mul", "imul1", "imul2", "imul3", "div", "idiv"]);
        const src = rn(size, reg(size, [0, 2]));
        const form = rint(4) === 0 ? `${MEMSIZE[size]} ${mem()}` : src;
        if(kind === "mul" || kind === "imul1") {
            return { asm: `${kind === "mul" ? "mul" : "imul"} ${form}`, mask: CF | OF };
        }
        if(kind === "imul2") {
            const s = pick([16, 32, 64]);
            return { asm: `imul ${rn(s, reg(s))}, ${rint(3) === 0 ? `${MEMSIZE[s]} ${mem()}` : rn(s, reg(s))}`, mask: CF | OF };
        }
        if(kind === "imul3") {
            const s = pick([16, 32, 64]);
            return { asm: `imul ${rn(s, reg(s))}, ${rn(s, reg(s))}, ${imm(s === 64 ? 32 : s)}`, mask: CF | OF };
        }
        // division: prepare a safe dividend / divisor
        const divisor = rn(size, reg(size, [0, 2]));
        let prep;
        if(kind === "div") prep = size === 8 ? "movzx eax, al" : "xor edx, edx";
        else prep = size === 8 ? "movsx ax, al" : size === 16 ? "cwd" : size === 32 ? "cdq" : "cqo";
        // make sure the divisor is neither 0 nor, for idiv, -1 (division overflow)
        let fix = `or ${divisor}, 1`;
        if(kind === "idiv") fix += `\ncmp ${divisor}, -1\njne .ok_{ID}\nmov ${divisor}, 3\n.ok_{ID}:`;
        return { asm: `${fix}\n${prep}\n${kind} ${divisor}`, mask: 0 };
    },
    bit() {
        const op = pick(["bt", "bts", "btr", "btc"]);
        const size = pick([16, 32, 64]);
        const form = pick(["rr", "ri", "mi"]);
        if(form === "rr") return { asm: `${op} ${rn(size, reg(size))}, ${rn(size, reg(size))}`, mask: CF };
        if(form === "ri") return { asm: `${op} ${rn(size, reg(size))}, ${rint(256)}`, mask: CF };
        return { asm: `${op} ${MEMSIZE[size]} ${mem()}, ${rint(size)}`, mask: CF };
    },
    bitscan() {
        const op = pick(["bsf", "bsr", "popcnt", "lzcnt", "tzcnt"]);
        const size = pick([16, 32, 64]);
        const d = reg(size), s = reg(size);
        const asm = `${op} ${rn(size, d)}, ${rint(3) === 0 ? `${MEMSIZE[size]} ${mem()}` : rn(size, s)}`;
        // destination of bsf/bsr is undefined for a zero source, which the comparison treats via `weak`
        return { asm, mask: op === "bsf" || op === "bsr" ? ZF : op === "popcnt" ? ALL : CF | ZF, weak: [d] , weakWhenZero: op === "bsf" || op === "bsr" };
    },
    mov() {
        const kind = pick(["movzx", "movsx", "movsxd", "lea", "xchg", "xadd", "cmpxchg", "bswap", "cmov", "set", "cdq", "movabs"]);
        const size = pick([16, 32, 64]);
        switch(kind) {
            case "movzx": { const ss = pick(size === 16 ? [8] : [8, 16]); return { asm: `movzx ${rn(size, reg(size))}, ${rint(3) === 0 ? `${MEMSIZE[ss]} ${mem()}` : rn(ss, reg(ss))}`, mask: 0 }; }
            case "movsx": { const ss = pick(size === 16 ? [8] : [8, 16]); return { asm: `movsx ${rn(size, reg(size))}, ${rn(ss, reg(ss))}`, mask: 0 }; }
            case "movsxd": return { asm: `movsxd ${rn(64, reg(64))}, ${rn(32, reg(32))}`, mask: 0 };
            case "lea": {
                const sz = pick([32, 64]);
                const scale = pick([1, 2, 4, 8]);
                const base = rn(64, reg(64)), idx = rn(64, reg(64, [4]));
                return { asm: `lea ${rn(sz, reg(sz))}, [${base} + ${idx} * ${scale} + ${rint(2000) - 1000}]`, mask: 0 };
            }
            case "xchg": return { asm: `xchg ${rn(size, reg(size))}, ${rn(size, reg(size))}`, mask: 0 };
            case "xadd": return { asm: `xadd ${rint(3) === 0 ? `${MEMSIZE[size]} ${mem()}` : rn(size, reg(size))}, ${rn(size, reg(size))}`, mask: ALL };
            case "cmpxchg": return { asm: `cmpxchg ${rint(2) === 0 ? `${MEMSIZE[size]} ${mem()}` : rn(size, reg(size, [0]))}, ${rn(size, reg(size, [0]))}`, mask: ALL };
            case "bswap": return { asm: `bswap ${rn(pick([32, 64]), reg(64))}`, mask: 0 };
            case "cmov": return { asm: `cmov${pick(["o", "no", "b", "ae", "e", "ne", "be", "a", "s", "ns", "p", "np", "l", "ge", "le", "g"])} ${rn(size, reg(size))}, ${rn(size, reg(size))}`, mask: 0 };
            case "set": return { asm: `set${pick(["o", "no", "b", "ae", "e", "ne", "be", "a", "s", "ns", "p", "np", "l", "ge", "le", "g"])} ${rn(8, reg(8))}`, mask: 0 };
            case "cdq": return { asm: pick(["cdqe", "cqo", "cwde", "cdq", "cbw", "cwd"]), mask: 0 };
            default: return { asm: `mov ${rn(64, reg(64))}, 0x${rand64().toString(16)}`, mask: 0 };
        }
    },
    shiftd() {
        const op = pick(["shld", "shrd"]);
        const size = pick([16, 32, 64]);
        const count = rint(size + 8);
        const form = pick(["imm", "cl"]);
        const a = rint(4) === 0 ? `${MEMSIZE[size]} ${mem()}` : rn(size, reg(size, [1]));
        const asm = form === "imm" ? `${op} ${a}, ${rn(size, reg(size, [1]))}, ${count}` : `${op} ${a}, ${rn(size, reg(size, [1]))}, cl`;
        // for counts above the operand size (16 bit) the result is undefined
        return { asm, mask: form === "imm" && count === 1 ? CF | PF | ZF | SF | OF : CF | PF | ZF | SF, cl: form === "cl", skip16over: size === 16 && (form === "cl" || count > 15) };
    },
};

// ---------------------------------------------------------------------------------------------
// building and running

const SCRATCH_QWORDS = 16;
const CASE_QWORDS = 16 + 1 + SCRATCH_QWORDS;     // registers, flags, scratch memory

function build_case(i, spec, initial)
{
    const lines = [];
    // flags
    lines.push(`mov rax, 0x${initial.flags.toString(16)}`, "push rax", "popfq");
    for(let k = 0; k < SCRATCH_QWORDS; k++) lines.push(`mov rax, 0x${initial.mem[k].toString(16)}`, `mov [rel scratch + ${k * 8}], rax`);
    REG64.forEach((r, k) => lines.push(`mov ${r}, 0x${initial.regs[k].toString(16)}`));
    if(spec.cl) {
        // an interesting shift count in cl
    }
    if(spec.setup) lines.push(spec.setup);
    lines.push(spec.asm.replace(/\{ID\}/g, String(i)).replace(/^\s+/gm, ""));
    if(spec.setup) lines.push("cld");
    // capture state: flags first (pushfq only changes rsp)
    lines.push("pushfq", `pop qword [rel results + ${i * CASE_QWORDS * 8 + 16 * 8}]`);
    if(spec.post) lines.push(spec.post);
    REG64.forEach((r, k) => lines.push(`mov [rel results + ${i * CASE_QWORDS * 8 + k * 8}], ${r}`));
    lines.push(`mov [rel results + ${i * CASE_QWORDS * 8 + 15 * 8}], rsp`);
    for(let k = 0; k < SCRATCH_QWORDS; k++) lines.push(`mov rax, [rel scratch + ${k * 8}]`, `mov [rel results + ${i * CASE_QWORDS * 8 + (17 + k) * 8}], rax`);
    return lines.join("\n");
}

function make_cases(count, family_names)
{
    const cases = [];
    for(let i = 0; i < count; i++)
    {
        const family = pick(family_names);
        const spec = families[family]();
        spec.family = family;
        const flags = Number(rand64() & BigInt(ALL | 2)) | 2;
        const regs = REG64.map(() => rand64());
        if(spec.cl) {
            const rcl = REG64.indexOf("rcx");
            regs[rcl] = (regs[rcl] & ~0xFFn) | BigInt(rint(72));
        }
        const mem = Array.from({ length: SCRATCH_QWORDS }, () => rand64());
        cases.push({ spec, initial: { flags, regs, mem } });
    }
    return cases;
}

function program(cases, native)
{
    const body = cases.map((c, i) => `; case ${i}: ${c.spec.asm.replace(/\n/g, "; ")}\n` + build_case(i, c.spec, c.initial)).join("\n");
    const size = cases.length * CASE_QWORDS * 8;
    return `%include "inc/long.inc"
test_main:
%ifndef NATIVE
    lea rax, [rel results]
    mov [abs 0x650], rax
%endif
${body}
%ifdef NATIVE
    mov eax, 1
    mov edi, 1
    lea rsi, [rel results]
    mov edx, ${size}
    syscall
%endif
    PASS
section .data
align 16
scratch: times ${SCRATCH_QWORDS} dq 0
results: times ${cases.length * CASE_QWORDS} dq 0
`;
}

const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "v86-fuzz-"));

function run_native(cases)
{
    const asm = path.join(tmp, "native.asm");
    fs.writeFileSync(asm, program(cases, true));
    execFileSync("nasm", ["-f", "elf64", "-DNATIVE", "-i", __dirname + "/", asm, "-o", path.join(tmp, "native.o")]);
    execFileSync("ld", ["-o", path.join(tmp, "native.elf"), path.join(tmp, "native.o")]);
    return execFileSync(path.join(tmp, "native.elf"), [], { maxBuffer: 1 << 26 });
}

let emulator;
let loaded;
async function run_guest(cases)
{
    const asm = path.join(tmp, "guest.asm");
    fs.writeFileSync(asm, program(cases, false));
    execFileSync("nasm", ["-f", "bin", "-i", __dirname + "/", asm, "-o", path.join(tmp, "guest.bin")]);
    const bin = fs.readFileSync(path.join(tmp, "guest.bin"));
    if(!emulator)
    {
        emulator = new V86({ autostart: false, memory_size: 32 * 1024 * 1024, log_level: 0 });
        loaded = new Promise(res => emulator.add_listener("emulator-loaded", res));
    }
    await loaded;
    const cpu = emulator.v86.cpu;
    return new Promise((resolve, reject) => {
        const timer = setTimeout(() => reject(new Error("guest timeout")), 30000);
        const finish = () => {
            clearTimeout(timer);
            emulator.stop();
            const status = cpu.mem32s[0x600 >> 2] >>> 0;
            const ptr = cpu.mem32s[0x650 >> 2] >>> 0;
            if(status !== 0x53534150) { reject(new Error("guest failed: status " + status.toString(16) + " id " + (cpu.mem32s[0x604 >> 2] >>> 0).toString(16))); return; }
            resolve(Buffer.from(cpu.mem8.slice(ptr, ptr + cases.length * CASE_QWORDS * 8)));
        };
        emulator.bus.register("cpu-event-halt", finish);
        cpu.reboot_internal();
        cpu.reset_memory();
        cpu.mem8.set(bin, 0x7C00);
        cpu.mem8.set([0xEA, 0x00, 0x7C, 0x00, 0x00], 0xFFFF0);
        emulator.run();
    });
}

function q(buf, i) { return buf.readBigUInt64LE(i * 8); }

function compare(cases, expected, actual)
{
    let failures = 0;
    cases.forEach((c, i) => {
        const diffs = [];
        const base = i * CASE_QWORDS;
        // registers (rsp is slot 15 and replaces r15 in the capture, so capture r15 separately below)
        for(let k = 0; k < 16; k++)
        {
            if(k === 15) continue;   // rsp is different in the two environments
            const e = q(expected, base + k), a = q(actual, base + k);
            if(e !== a) diffs.push(`${k === 15 ? "rsp" : REG64[k]}: expected ${e.toString(16)} actual ${a.toString(16)}`);
        }
        const mask = BigInt(c.spec.mask);
        const ef = q(expected, base + 16) & mask, af = q(actual, base + 16) & mask;
        if(ef !== af) diffs.push(`flags: expected ${ef.toString(16)} actual ${af.toString(16)} (mask ${mask.toString(16)})`);
        for(let k = 0; k < SCRATCH_QWORDS; k++)
        {
            const e = q(expected, base + 17 + k), a = q(actual, base + 17 + k);
            if(e !== a) diffs.push(`mem[${k}]: expected ${e.toString(16)} actual ${a.toString(16)}`);
        }
        if(diffs.length && !c.spec.skip16over)
        {
            // destination of bsf/bsr is undefined for a zero source
            if(c.spec.weakWhenZero && diffs.every(d => !d.startsWith("flags") && !d.startsWith("mem")) && (q(expected, base + 16) & 64n)) return;
            failures++;
            console.log(`FAIL [${c.spec.family}] ${c.spec.asm.replace(/\n\s*/g, "; ")}`);
            console.log(`   initial flags ${c.initial.flags.toString(16)} regs ${c.initial.regs.map((r, k) => REG64[k] + "=" + r.toString(16)).join(" ")}`);
            diffs.forEach(d => console.log("   " + d));
        }
    });
    return failures;
}

const family_names = Object.keys(families).filter(f => FAMILY.test(f));
let total = 0, failed = 0;
for(let done = 0; done < COUNT; done += BATCH)
{
    const cases = make_cases(Math.min(BATCH, COUNT - done), family_names);
    const expected = run_native(cases);
    let actual;
    try
    {
        actual = await run_guest(cases);
    }
    catch(e)
    {
        console.log("batch failed:", e.message);
        failed++;
        if(VERBOSE) fs.copyFileSync(path.join(tmp, "guest.asm"), "/tmp/fuzz-failed.asm");
        continue;
    }
    total += cases.length;
    failed += compare(cases, expected, actual);
}
console.log(`${total} cases, ${failed} failures (seed ${SEED})`);
fs.rmSync(tmp, { recursive: true, force: true });
process.exit(failed ? 1 : 0);
