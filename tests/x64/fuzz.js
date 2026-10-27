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

const f32buf = new DataView(new ArrayBuffer(8));
function rand_float_bits(kind)
{
    const specials = [0, -0, 1, -1, 0.5, 2, 3.5, -7.25, 1e10, -1e10, 123456.789, 1e-3];
    const v = () => rnd() < 0.2 ? pick(specials) : (rnd() - 0.5) * Math.pow(10, rint(8) - 2);
    if(kind === "ps")
    {
        f32buf.setFloat32(0, v(), true);
        f32buf.setFloat32(4, v(), true);
        return f32buf.getBigUint64(0, true);
    }
    f32buf.setFloat64(0, v(), true);
    return f32buf.getBigUint64(0, true);
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
const XMM = n => "xmm" + n;
const xr = () => XMM(rint(16));
const xsrc = () => rint(4) === 0 ? mem_aligned() : xr();
const mem_aligned = () => `[rel scratch + ${rint(2) * 16}]`;
const CC = ["o", "no", "b", "ae", "e", "ne", "be", "a", "s", "ns", "p", "np", "l", "ge", "le", "g"];

const sse_int_ops = ["pand", "pandn", "por", "pxor", "paddb", "paddw", "paddd", "paddq", "psubb", "psubw", "psubd", "psubq",
    "paddsb", "paddsw", "paddusb", "paddusw", "psubsb", "psubsw", "psubusb", "psubusw", "pmullw", "pmulhw", "pmulhuw", "pmuludq",
    "pmaddwd", "pavgb", "pavgw", "psadbw", "pminub", "pmaxub", "pminsw", "pmaxsw", "pcmpeqb", "pcmpeqw", "pcmpeqd", "pcmpgtb",
    "pcmpgtw", "pcmpgtd", "packsswb", "packssdw", "packuswb", "punpcklbw", "punpcklwd", "punpckldq", "punpcklqdq", "punpckhbw",
    "punpckhwd", "punpckhdq", "punpckhqdq", "psllw", "pslld", "psllq", "psrlw", "psrld", "psrlq", "psraw", "psrad"];
const ssse3_ops = ["pshufb", "phaddw", "phaddd", "phaddsw", "phsubw", "phsubd", "phsubsw", "pmaddubsw", "psignb", "psignw", "psignd",
    "pmulhrsw", "pabsb", "pabsw", "pabsd"];
const sse41_ops = ["pmuldq", "pcmpeqq", "packusdw", "pminsb", "pminsd", "pminuw", "pminud", "pmaxsb", "pmaxsd", "pmaxuw", "pmaxud",
    "pmulld", "pcmpgtq", "pmovsxbw", "pmovsxbd", "pmovsxbq", "pmovsxwd", "pmovsxwq", "pmovsxdq", "pmovzxbw", "pmovzxbd", "pmovzxbq",
    "pmovzxwd", "pmovzxwq", "pmovzxdq", "phminposuw"];

const families = {
    sse_int() {
        const op = pick(sse_int_ops);
        const d = xr();
        // shifts by an immediate use the group forms
        if(rint(4) === 0 && /^ps(ll|rl|ra)[wdq]$/.test(op) && !/psraq/.test(op)) return { asm: `${op} ${d}, ${rint(70)}`, mask: 0, xmm: true };
        return { asm: `${op} ${d}, ${xsrc()}`, mask: 0, xmm: true };
    },
    sse_imm() {
        const kind = pick(["pshufd", "pshufhw", "pshuflw", "pslldq", "psrldq", "palignr", "pblendw", "mpsadbw", "shufps", "shufpd", "pinsrw", "pextrw", "pextrb", "pextrd", "pinsrb", "pinsrd", "insertps", "blendps", "blendpd", "roundps", "roundpd", "roundss", "roundsd"]);
        const d = xr(), src = xsrc(), imm = rint(256);
        switch(kind) {
            case "pslldq": case "psrldq": return { asm: `${kind} ${d}, ${rint(20)}`, mask: 0, xmm: true };
            case "pinsrw": return { asm: `pinsrw ${d}, ${rn(32, reg(32))}, ${imm}`, mask: 0, xmm: true };
            case "pinsrb": return { asm: `pinsrb ${d}, ${rn(32, reg(32))}, ${imm}`, mask: 0, xmm: true };
            case "pinsrd": return { asm: `pinsrd ${d}, ${rn(32, reg(32))}, ${imm}`, mask: 0, xmm: true };
            case "pextrw": case "pextrb": case "pextrd": return { asm: `${kind} ${rn(32, reg(32))}, ${d}, ${imm}`, mask: 0, xmm: true };
            case "roundps": case "roundpd": return { asm: `${kind} ${d}, ${xsrc()}, ${imm & 3 | 8}`, mask: 0, xmm: true, floats: kind.endsWith("d") ? "pd" : "ps" };
            case "roundss": return { asm: `roundss ${d}, ${rint(3) ? xr() : "dword [rel scratch + 4]"}, ${imm & 3 | 8}`, mask: 0, xmm: true, floats: "ps" };
            case "roundsd": return { asm: `roundsd ${d}, ${rint(3) ? xr() : "qword [rel scratch + 8]"}, ${imm & 3 | 8}`, mask: 0, xmm: true, floats: "pd" };
            case "insertps": return { asm: `insertps ${d}, ${rint(3) ? xr() : "dword [rel scratch + 4]"}, ${imm}`, mask: 0, xmm: true };
            case "shufps": case "shufpd": return { asm: `${kind} ${d}, ${src}, ${imm}`, mask: 0, xmm: true };
            default: return { asm: `${kind} ${d}, ${src}, ${imm}`, mask: 0, xmm: true };
        }
    },
    ssse3() {
        const op = pick(ssse3_ops);
        return { asm: `${op} ${xr()}, ${xsrc()}`, mask: 0, xmm: true };
    },
    sse41() {
        const op = pick(sse41_ops);
        const mem = rint(4) === 0;
        // pmovsx / pmovzx read a partial operand from memory
        const PMOV = { bw: "qword", bd: "dword", bq: "word", wd: "qword", wq: "dword", dq: "qword" };
        const operand = !mem ? xr() : /pmov/.test(op) ? `${PMOV[op.slice(-2)]} [rel scratch + 8]` : mem_aligned();
        return { asm: `${op} ${xr()}, ${operand}`, mask: 0, xmm: true };
    },
    sse41_blend() {
        const op = pick(["pblendvb", "blendvps", "blendvpd"]);
        return { asm: `${op} ${xr()}, ${xsrc()}`, mask: 0, xmm: true };
    },
    sse_ptest() {
        return { asm: `ptest ${xr()}, ${xsrc()}`, mask: ALL, xmm: true };
    },
    sse_fp() {
        const kind = pick(["ps", "pd"]);
        const scalar = rint(3) === 0;
        const ops = ["add", "sub", "mul", "div", "min", "max"];
        const op = pick(ops);
        const suffix = scalar ? (kind === "ps" ? "ss" : "sd") : kind;
        const src = rint(4) === 0 ? (scalar ? `${kind === "ps" ? "dword" : "qword"} [rel scratch + 8]` : mem_aligned()) : xr();
        return { asm: `${op}${suffix} ${xr()}, ${src}`, mask: 0, xmm: true, floats: kind };
    },
    sse_fp2() {
        const kind = pick(["ps", "pd"]);
        const k = pick(["sqrt", "cmp", "and", "andn", "or", "xor", "unpckl", "unpckh", "hadd", "hsub", "addsub", "cvt", "comi", "ucomi", "movmsk"]);
        const d = xr();
        const floats = kind;
        switch(k) {
            case "sqrt": return { asm: `sqrt${kind} ${d}, ${xsrc()}`, mask: 0, xmm: true, floats, positive: true };
            case "cmp": return { asm: `cmp${pick(["eq", "lt", "le", "unord", "neq", "nlt", "nle", "ord"])}${kind} ${d}, ${xsrc()}`, mask: 0, xmm: true, floats };
            case "and": case "andn": case "or": case "xor": return { asm: `${k}${kind} ${d}, ${xsrc()}`, mask: 0, xmm: true, floats };
            case "unpckl": case "unpckh": return { asm: `${k}${kind} ${d}, ${xsrc()}`, mask: 0, xmm: true, floats };
            case "hadd": case "hsub": case "addsub": return { asm: `${k}${kind} ${d}, ${xsrc()}`, mask: 0, xmm: true, floats };
            case "cvt": return { asm: pick(kind === "ps" ? ["cvtps2pd", "cvtps2dq", "cvttps2dq", "cvtdq2ps", "cvtss2sd"] : ["cvtpd2ps", "cvtpd2dq", "cvttpd2dq", "cvtdq2pd", "cvtsd2ss"]) + ` ${d}, ${xr()}`, mask: 0, xmm: true, floats };
            case "comi": case "ucomi": return { asm: `${k}${kind === "ps" ? "ss" : "sd"} ${d}, ${xr()}`, mask: CF | PF | ZF | SF | OF | AF, xmm: true, floats };
            case "movmsk": return { asm: `movmsk${kind} ${rn(32, reg(32))}, ${xr()}`, mask: 0, xmm: true, floats };
            // rcpps / rsqrtps return implementation specific approximations
            default: return { asm: `${k}ps ${d}, ${xr()}`, mask: 0, xmm: true, floats: "ps", approx: true };
        }
    },
    sse_move() {
        const k = pick(["movd", "movq", "movdqa", "movaps", "movhlps", "movlhps", "movss", "movsd", "movddup", "movshdup", "movsldup", "pmovmskb", "movq2", "cvtsi", "cvttsd"]);
        const d = xr(), s = xr();
        switch(k) {
            case "movd": return { asm: rint(2) ? `movd ${d}, ${rn(32, reg(32))}` : `movd ${rn(32, reg(32))}, ${s}`, mask: 0, xmm: true };
            case "movq": return { asm: rint(2) ? `movq ${d}, ${rn(64, reg(64))}` : `movq ${rn(64, reg(64))}, ${s}`, mask: 0, xmm: true };
            case "movq2": return { asm: rint(2) ? `movq ${d}, ${s}` : `movq ${d}, qword [rel scratch + 8]`, mask: 0, xmm: true };
            case "pmovmskb": return { asm: `pmovmskb ${rn(32, reg(32))}, ${s}`, mask: 0, xmm: true };
            case "cvtsi": return { asm: `cvtsi2${pick(["sd", "ss"])} ${d}, ${rn(pick([32, 64]), reg(32))}`, mask: 0, xmm: true, floats: "ps" };
            case "cvttsd": return { asm: `${pick(["cvttsd2si", "cvtsd2si"])} ${rn(pick([32, 64]), reg(32))}, ${s}`, mask: 0, xmm: true, floats: "pd" };
            default: return { asm: `${k} ${d}, ${s}`, mask: 0, xmm: true };
        }
    },
    high8() {
        // legacy high byte registers (only encodable without a REX prefix)
        const op = pick(["add", "sub", "adc", "sbb", "and", "or", "xor", "cmp", "test", "mov", "xchg"]);
        const a = pick(rint(2) ? HIGH8 : LOW8), b = pick(rint(2) ? HIGH8 : LOW8);
        const form = pick(["rr", "ri", "m"]);
        const asm = form === "rr" ? `${op} ${a}, ${b}` : form === "ri" ? `${op} ${a}, ${rint(256)}` : `${op} ${a}, byte ${mem()}`;
        return { asm: op === "xchg" && form !== "rr" ? `xchg ${a}, ${b}` : asm, mask: op === "mov" || op === "xchg" ? 0 : op === "and" || op === "or" || op === "xor" || op === "test" ? ALL & ~AF : ALL };
    },
    atomic() {
        // lock prefixed read-modify-write instructions on memory, as used by pthread / futex code
        const op = pick(["add", "sub", "and", "or", "xor", "adc", "sbb", "inc", "dec", "neg", "not", "xadd", "cmpxchg", "bts", "btr", "btc"]);
        const size = op === "bts" || op === "btr" || op === "btc" ? pick([16, 32, 64]) : pick([8, 16, 32, 64]);
        const m = `${MEMSIZE[size]} ${mem()}`;
        const src = op === "cmpxchg" ? rn(size, reg(size, [0])) : rn(size, reg(size));
        let asm, mask = ALL;
        switch(op)
        {
            case "inc": case "dec": case "neg": case "not": asm = `lock ${op} ${m}`; mask = op === "not" ? 0 : ALL; break;
            case "xadd": case "cmpxchg": asm = `lock ${op} ${m}, ${src}`; break;
            case "bts": case "btr": case "btc": asm = `lock ${op} ${m}, ${rint(size)}`; mask = CF; break;
            case "and": case "or": case "xor": asm = `lock ${op} ${m}, ${rint(2) ? src : rint(128)}`; mask = ALL & ~AF; break;
            default: asm = `lock ${op} ${m}, ${rint(2) ? src : rint(128)}`;
        }
        return { asm, mask };
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
const XINIT_SLOTS = 128;
const XMM_QWORDS = 32;
const CASE_QWORDS = 16 + 1 + SCRATCH_QWORDS + XMM_QWORDS;     // registers, flags, scratch memory, xmm0-15

function build_case(i, spec, initial)
{
    const lines = [];
    // flags
    lines.push(`mov rax, 0x${initial.flags.toString(16)}`, "push rax", "popfq");
    for(let k = 0; k < SCRATCH_QWORDS; k++) lines.push(`mov rax, 0x${initial.mem[k].toString(16)}`, `mov [rel scratch + ${k * 8}], rax`);
    if(spec.xmm) {
        for(let k = 0; k < 16; k++) lines.push(`movdqu xmm${k}, [rel xinit + ${(i % XINIT_SLOTS) * 256 + k * 16}]`);
    }
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
    if(spec.xmm) for(let k = 0; k < 16; k++) lines.push(`movdqu [rel results + ${i * CASE_QWORDS * 8 + (17 + SCRATCH_QWORDS) * 8 + k * 16}], xmm${k}`);
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
        const xmm = spec.xmm ? Array.from({ length: 32 }, () => spec.floats ? rand_float_bits(spec.floats) : rand64()) : null;
        cases.push({ spec, initial: { flags, regs, mem, xmm } });
    }
    return cases;
}

function program(cases, native)
{
    // xmm initial values, one 256 byte slot per case (slots are reused when a batch has more cases)
    const xinit = [];
    for(let i = 0; i < Math.min(cases.length, XINIT_SLOTS); i++) xinit.push(...(cases[i].initial.xmm || new Array(32).fill(0n)));
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
xinit: dq ${xinit.map(v => "0x" + v.toString(16)).join(", ")}
scratch: times ${SCRATCH_QWORDS} dq 0
results: times ${cases.length * CASE_QWORDS} dq 0
`;
}

const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "v86-fuzz-"));

function nasm(args)
{
    try { execFileSync("nasm", args, { stdio: "pipe" }); }
    catch(e)
    {
        const lines = e.stderr.toString().split("\n").filter(l => /error/.test(l)).slice(0, 5);
        for(const l of lines)
        {
            const m = /^(.*\.asm):(\d+):/.exec(l);
            console.error(l, m ? "\n    > " + fs.readFileSync(m[1], "utf8").split("\n")[+m[2] - 1] : "");
        }
        throw e;
    }
}

function run_native(cases)
{
    const asm = path.join(tmp, "native.asm");
    fs.writeFileSync(asm, program(cases, true));
    nasm(["-f", "elf64", "-DNATIVE", "-i", __dirname + "/", asm, "-o", path.join(tmp, "native.o")]);
    execFileSync("ld", ["-o", path.join(tmp, "native.elf"), path.join(tmp, "native.o")]);
    return execFileSync(path.join(tmp, "native.elf"), [], { maxBuffer: 1 << 26 });
}

let emulator;
let loaded;
async function run_guest(cases)
{
    const asm = path.join(tmp, "guest.asm");
    fs.writeFileSync(asm, program(cases, false));
    nasm(["-f", "bin", "-i", __dirname + "/", asm, "-o", path.join(tmp, "guest.bin")]);
    const bin = fs.readFileSync(path.join(tmp, "guest.bin"));
    if(!emulator)
    {
        emulator = new V86({ autostart: false, memory_size: 32 * 1024 * 1024, log_level: 0, jit64_threshold: process.env.JIT64 === undefined ? undefined : +process.env.JIT64, sync_jit: process.env.JIT64 !== undefined, jit64_disable_mask: +process.env.JIT64_MASK || 0 });
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
        if(c.spec.xmm)
        {
            for(let k = 0; k < 32; k++)
            {
                const e = q(expected, base + 17 + SCRATCH_QWORDS + k), a = q(actual, base + 17 + SCRATCH_QWORDS + k);
                if(e !== a) diffs.push(`xmm${k >> 1}.${k & 1}: expected ${e.toString(16)} actual ${a.toString(16)}`);
            }
        }
        if(diffs.length && !c.spec.skip16over)
        {
            // destination of bsf/bsr is undefined for a zero source
            if(c.spec.weakWhenZero && diffs.every(d => !d.startsWith("flags") && !d.startsWith("mem")) && (q(expected, base + 16) & 64n)) return;
            failures++;
            console.log(`FAIL [${c.spec.family}] ${c.spec.asm.replace(/\n\s*/g, "; ")}`);
            if(c.initial.xmm) console.log(`   initial xmm: ${c.initial.xmm.map((v, k) => (k & 1 ? "" : " xmm" + (k >> 1) + "=") + v.toString(16).padStart(16, "0") + (k & 1 ? "" : ":")).join("")}`);
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
if(emulator && process.env.JIT64 !== undefined)
{
    const g = i => emulator.v86.cpu.wm.exports.get_jit_config(i);
    console.log(`jit64: compiled ${g(5)}, failed ${g(6)}, run ${g(7)}, invalidated ${g(8)}`);
}
fs.rmSync(tmp, { recursive: true, force: true });
process.exit(failed ? 1 : 0);
