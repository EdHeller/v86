#!/usr/bin/env node
// The opcode tables of the test READMEs (docs: tests/Readme.md): which test covers each instruction.
//
//   node tools/opcode_coverage.mjs           rewrite the tables in tests/nasm/Readme.md (x86), tests/x64/Readme.md (x86_64)
//                                            and tests/a64/Readme.md (ARM)
//
// x86: every encoding of gen/x86_table.js (the table the interpreter, the JIT and the nasm test generator are made
// from), its mnemonic from ndisasm, whether tests/nasm/create_tests.js generates tests for it (register and memory
// forms), and the hand-written tests (tests/nasm/*.asm, tests/x64/**/*.asm) that use the mnemonic.
// x86_64: the mnemonics the x86_64 tests use (tests/x64/long, jit, stubs, smp, issues), per directory, and the
// instruction families of tests/x64/fuzz.js. ARM: the A64 encoding classes of tests/a64/encodings.json (from v64)
// with their status, and the instructions the ARM core's decoder knows (a64/src/aarch64/decode.rs, enum Op).
// Needs ndisasm (nasm).

import fs from "node:fs";
import path from "node:path";
import url from "node:url";
import { spawnSync } from "node:child_process";

const ROOT = path.resolve(path.dirname(url.fileURLToPath(import.meta.url)), "..");
const { default: encodings } = await import(path.join(ROOT, "gen/x86_table.js"));

const scratch = fs.mkdtempSync(path.join(process.env.TMPDIR || "/tmp", "opcodes-"));

/** The encoding's bytes in 32-bit mode: prefixes and opcode, a modrm byte (register or memory form), immediates */
function bytes_of(op, memory)
{
    let hex = op.opcode.toString(16);
    if(hex.length & 1) hex = "0" + hex;
    const out = [];
    for(let i = 0; i < hex.length; i += 2) out.push(parseInt(hex.slice(i, i + 2), 16));
    if(op.e)
    {
        const g = op.fixed_g === undefined ? 0 : op.fixed_g;
        // (memory: [eax]; register: eax / mm0 / xmm0 / st0)
        out.push((memory ? 0x00 : 0xC0) | g << 3);
    }
    const imm = (op.imm8 || op.imm8s ? 1 : 0) + (op.extra_imm8 ? 1 : 0) + (op.imm16 ? 2 : 0) + (op.extra_imm16 ? 2 : 0) +
        (op.imm1632 || op.imm32 || op.immaddr ? 4 : 0);
    for(let i = 0; i < imm; i++) out.push(0x01);
    return out;
}

/** ndisasm's reading of the bytes (32-bit mode): the mnemonic and operands */
function disassemble(list)
{
    const file = path.join(scratch, "op.bin");
    fs.writeFileSync(file, Uint8Array.from(list));
    const r = spawnSync("ndisasm", ["-b", "32", file], { encoding: "utf8" });
    const line = (r.stdout || "").split("\n")[0] || "";
    const text = line.slice(28).trim();
    return /^db\b/.test(text) ? "" : text;
}

const hex_of = list => list.map(b => b.toString(16).toUpperCase().padStart(2, "0")).join(" ");

// the mnemonics the hand-written tests use (the first word of each instruction line)
function mnemonics_in(dir, recursive)
{
    const counts = new Map();
    const walk = d => {
        for(const e of fs.readdirSync(d, { withFileTypes: true }))
        {
            const p = path.join(d, e.name);
            if(e.isDirectory()) { if(recursive && e.name !== "build") walk(p); continue; }
            if(!e.name.endsWith(".asm")) continue;
            const seen = new Set();
            for(let line of fs.readFileSync(p, "utf8").split("\n"))
            {
                line = line.replace(/;.*/, "").trim();
                const m = /^(?:[\w.]+:\s*)?(?:(?:lock|rep|repe|repz|repne|repnz|o16|o32|o64|a16|a32)\s+)*([a-z][a-z0-9]*)\b/.exec(line);
                if(!m || /^(section|bits|org|align|times|db|dw|dd|dq|equ|global|extern|default|resb|resd|resq|incbin|use16|use32|use64|cpu)$/.test(m[1])) continue;
                seen.add(m[1]);
            }
            for(const name of seen)
            {
                if(!counts.has(name)) counts.set(name, []);
                counts.get(name).push(path.relative(ROOT, p));
            }
        }
    };
    walk(path.join(ROOT, dir));
    return counts;
}

function replace_section(file, name, body)
{
    const begin = `<!-- ${name}:begin (tools/opcode_coverage.mjs) -->`, end = `<!-- ${name}:end -->`;
    let text = fs.readFileSync(file, "utf8");
    const i = text.indexOf(begin), j = text.indexOf(end);
    if(i < 0 || j < 0) throw new Error(file + ": no " + begin + " ... " + end);
    text = text.slice(0, i + begin.length) + "\n" + body + "\n" + text.slice(j);
    fs.writeFileSync(file, text);
}

// ---- x86 (32-bit)

const nasm_tests = mnemonics_in("tests/nasm", false);
const x64_tests = mnemonics_in("tests/x64", true);
const rows = [];
let generated = 0, skipped = 0, prefixes = 0;
for(const op of encodings)
{
    const reg = bytes_of(op, false), mem = bytes_of(op, true);
    let text = op.e && op.only_mem ? disassemble(mem) : disassemble(reg) || (op.e ? disassemble(mem) : "");
    const hex = op.opcode.toString(16).toUpperCase().padStart(op.opcode > 0xFFFF ? 6 : op.opcode > 0xFF ? 4 : 2, "0");
    const g = op.fixed_g;
    // (what ndisasm does not decode: undocumented aliases, the escapes and opcodes that are #UD without a prefix)
    if(!text)
    {
        const alias = hex === "82" ? ["add", "or", "adc", "sbb", "and", "sub", "xor", "cmp"][g] + " r/m8, imm8 (alias of 80 /" + g + ")" :
            ["C0", "C1", "D0", "D1", "D2", "D3"].includes(hex) && g === 6 ? "sal (undocumented alias of /4)" :
            ["F6", "F7"].includes(hex) && g === 1 ? "test r/m, imm (undocumented alias of /0)" :
            hex === "0F38" || hex === "0F3A" ? "(three-byte escape: SSSE3, SSE4)" :
            hex === "0FB8" ? "jmpe (#UD; F3 0F B8 is popcnt)" :
            hex.startsWith("0F") ? "(#UD here: reserved, or only valid with a 66/F2/F3 prefix)" : "";
        text = alias;
    }
    const mnemonic = (text.split(/\s+/).find(w => !/^(rep|repe|repne|lock|o16|o32|a16|a32)$/.test(w)) || "").toLowerCase();
    let gen;
    if(op.prefix) { gen = "prefix"; prefixes++; }
    else if(op.skip) { gen = "no (skip)"; skipped++; }
    else
    {
        gen = op.e ? [op.skip_reg ? "" : "reg", op.skip_mem ? "" : "mem"].filter(Boolean).join(" + ") : "yes";
        generated++;
    }
    const hand = [...new Set([...(nasm_tests.get(mnemonic) || []), ...(x64_tests.get(mnemonic) || [])].map(f => f.replace(/^tests\//, "")))];
    const name = hex + (g !== undefined ? " /" + g : "");
    rows.push(`| \`${name}\` | ${text ? "`" + text.replace(/\|/g, "\\|") + "`" : (op.prefix ? "(prefix)" : "?")} | ${gen} | ` +
        `${hand.length ? hand.slice(0, 3).map(f => "`" + f + "`").join(", ") + (hand.length > 3 ? ` +${hand.length - 3}` : "") : ""} |`);
}
replace_section(path.join(ROOT, "tests/nasm/Readme.md"), "opcodes", [
    `${encodings.length} encodings: ${generated} with generated tests, ${skipped} skipped by the generator (they change control flow, ` +
    `segments or the machine state in ways a generated test cannot compare: jumps, calls, \`int\`, \`iret\`, \`hlt\`, ` +
    `port I/O, \`pop ss\`, ...), ${prefixes} prefixes. The disassembly is ndisasm's for a register (or, where only memory is valid, ` +
    `a memory) operand and 0x01 immediates; "Hand-written" lists up to three test files that use the mnemonic.`,
    "",
    "| Opcode | Disassembly (example) | Generated | Hand-written |",
    "|---|---|---|---|",
    ...rows,
].join("\n"));

// ---- x86_64

const dirs = ["long", "jit", "stubs", "smp", "issues"];
const per_dir = Object.fromEntries(dirs.map(d => [d, mnemonics_in("tests/x64/" + d, false)]));
const all = new Set(dirs.flatMap(d => [...per_dir[d].keys()]));
const ignore = new Set(["pass", "check_eq", "check_cc", "trap_begin", "trap_end", "fail", "jmp", "test_main", "call", "ret"]);
const rows64 = [...all].filter(m => !ignore.has(m) && !/^[A-Z_]+$/.test(m)).sort().map(m =>
    `| \`${m}\` | ${dirs.map(d => per_dir[d].has(m) ? per_dir[d].get(m).length : "").join(" | ")} |`);
const fuzz = fs.readFileSync(path.join(ROOT, "tests/x64/fuzz.js"), "utf8");
const families = [...fuzz.slice(fuzz.indexOf("const families = {")).matchAll(/^    ([a-z_0-9]+)\(\) \{/gm)].map(m => m[1]);
replace_section(path.join(ROOT, "tests/x64/Readme.md"), "opcodes", [
    `The mnemonics the 64-bit tests use, with the number of test files per directory that use each one (macros of ` +
    `\`inc/\` excluded), ${rows64.length} in all. \`fuzz.js\` also compares these instruction families against the host CPU, ` +
    `in long mode with random operands: ${families.map(f => "`" + f + "`").join(", ")}.`,
    "",
    "| Mnemonic | " + dirs.map(d => "`" + d + "/`").join(" | ") + " |",
    "|---|" + dirs.map(() => "---").join("|") + "|",
    ...rows64,
].join("\n"));

// ---- ARM (A64)

const enc = JSON.parse(fs.readFileSync(path.join(ROOT, "tests/a64/encodings.json"), "utf8"));
const class_rows = enc.classes.map(c => `| ${c.group} | ${c.class.replace(/\|/g, "\\|")} | ${c.status} | ${c.feature || ""} | ` +
    `${(c.tests || []).map(t => "`" + t + "`").join(", ")} |`);
const decode = fs.readFileSync(path.join(ROOT, "a64/src/aarch64/decode.rs"), "utf8");
const op_block = decode.slice(decode.indexOf("pub enum Op {"), decode.indexOf("\n}", decode.indexOf("pub enum Op {")));
const ops = [];
let doc = [], group = "";
for(const raw of op_block.split("\n").slice(1))
{
    const line = raw.trim();
    if(line.startsWith("///")) { doc.push(line.slice(3).trim()); continue; }
    if(line.startsWith("//")) { group = line.slice(2).trim(); doc = []; continue; }
    const m = /^([A-Z][A-Za-z0-9]*)\b/.exec(line);
    if(m) { ops.push(`| \`${m[1]}\` | ${group} | ${doc.join(" ").replace(/\|/g, "\\|")} |`); doc = []; }
}
replace_section(path.join(ROOT, "tests/a64/Readme.md"), "opcodes", [
    `### Encoding classes`,
    "",
    `The ${enc.classes.length} classes of the Arm ARM's A64 encoding index (${enc.profile ? "profile " + enc.profile + ", " : ""}` +
    `DDI 0487 L.b C4.1), as v64's \`tests/a64/encodings.json\` records them for this core. Status: ` +
    Object.entries(enc.statuses).map(([k, v]) => `**${k}**: ${v}`).join("; ") + `. "v64 tests" are the suites that check ` +
    `the class in the v64 project; they are not in this repository yet (below).`,
    "",
    "| Group | Class | Status | Feature | v64 tests |",
    "|---|---|---|---|---|",
    ...class_rows,
    "",
    `### The decoder's instructions`,
    "",
    `The ${ops.length} instruction kinds of \`a64/src/aarch64/decode.rs\` (\`enum Op\`; the FP, SIMD and AArch32 instructions are ` +
    `decoded in \`simd_decode.rs\`, \`fp.rs\` and \`a32*.rs\`).`,
    "",
    "| Op | Group | Forms |",
    "|---|---|---|",
    ...ops,
].join("\n"));

fs.rmSync(scratch, { recursive: true, force: true });
console.log(`tests/nasm/Readme.md: ${encodings.length} encodings; tests/x64/Readme.md: ${rows64.length} mnemonics, ${families.length} fuzz families; ` +
    `tests/a64/Readme.md: ${enc.classes.length} classes, ${ops.length} ops`);
