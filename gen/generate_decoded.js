#!/usr/bin/env node

// The decoded-instruction table (src/rust/gen/decoded.rs) for the interpreter's cache of decoded instructions
// (src/rust/cpu/decoded.rs), made from the generated interpreter (src/rust/gen/interpreter.rs, interpreter0f.rs):
// for each 32-bit opcode whose arm has a simple shape, a handler with the arm's own calls, the decoding (modrm, the
// group's register field, the immediate) replaced by its values: x (the address of a memory operand, or the modrm's
// register), mem, r (the modrm's register field), imm. Arms of other shapes, or with anything else that reads the
// instruction stream or the prefixes, are left to the interpreter.

import fs from "node:fs";
import path from "node:path";
import url from "node:url";

const __dirname = path.dirname(url.fileURLToPath(import.meta.url));
const SRC = path.join(__dirname, "../src/rust/gen");

const IMM = {
    "read_imm8()": 1,
    "read_imm8s()": 2,
    "read_imm16()": 3,
    "read_imm32s()": 4,
};

/** The arms of a generated `run` (header text -> body lines) */
function arms(file)
{
    const lines = fs.readFileSync(path.join(SRC, file), "utf8").split("\n");
    const result = [];
    let header = null, body = [];
    for(const line of lines)
    {
        const m = line.match(/^    (0x[0-9A-F]+(?: \| 0x[0-9A-F]+)*) => \{$/);
        if(m)
        {
            header = m[1].split(" | ").map(x => parseInt(x, 16));
            body = [];
            continue;
        }
        if(header && line === "    },")
        {
            result.push({ opcodes: header, body: body.join("\n") });
            header = null;
            continue;
        }
        if(header) body.push(line);
    }
    return result;
}

/** A body's decoding replaced by the handler's arguments; null when something is left that it cannot have */
function substitute(body)
{
    if(body.split("\n").some(line => (line.match(/read_imm|read_moffs/g) || []).length > 1)) return null;
    let imm = 0;
    let text = body
        .replace(/^\s*let modrm_byte = match read_imm8\(\) \{ Ok\(o\) => o, Err\(\(\)\) => return \};\n?/m, "")
        .replace(/if modrm_byte < 0xC0 \{/g, "if mem {")
        .replace(/match modrm_resolve\(modrm_byte\) \{ Ok\(a\) => a, Err\(\(\)\) => return \}/g, "x")
        .replace(/modrm_byte >> 3 & 7/g, "r")
        .replace(/modrm_byte & 7/g, "x")
        .replace(/match (read_imm8\(\)|read_imm8s\(\)|read_imm16\(\)|read_imm32s\(\)) \{ Ok\(o\) => o, Err\(\(\)\) => return \}/g, (_, f) => {
            if(imm && imm !== IMM[f]) imm = -1;
            else imm = IMM[f];
            return "imm";
        });
    if(imm < 0) return null;
    if(/modrm_byte|read_imm|read_moffs|modrm_resolve|prefixes|instruction_pointer|dbg_log|task_switch_test|trigger_ud|return|\?/.test(text))
    {
        return null;
    }
    return { text, imm };
}

// prefixes, and instructions that run the next one (sti) or read further opcode bytes themselves
const EXCLUDED = new Set([0x0F, 0x26, 0x2E, 0x36, 0x3E, 0x64, 0x65, 0x66, 0x67, 0xF0, 0xF2, 0xF3, 0xFB, 256 + 0x38, 256 + 0x3A]);

const handlers = [];
// per table index (0..256 one byte, 256..512 0F xx): [modrm, group, [ [handler, imm] per register field or one ]]
const table = new Array(512).fill(null);

function add(index, body)
{
    if(EXCLUDED.has(index)) return;
    const group = body.match(/^\s*let modrm_byte = match read_imm8\(\) [^\n]*\n\s*match modrm_byte >> 3 & 7 \{\n([\s\S]*)\n\s*\}\s*$/);
    const modrm = /let modrm_byte = match read_imm8\(\)/.test(body);
    if(group)
    {
        // the arms of the register field: "        N => {" ... "        },"
        const per_reg = new Array(8).fill(null);
        const re = /^ {12}([0-7]) => \{\n([\s\S]*?)\n {12}\},?$/gm;
        let m;
        while((m = re.exec(group[1])))
        {
            const s = substitute(m[2]);
            if(s)
            {
                handlers.push(s.text);
                per_reg[+m[1]] = [handlers.length - 1, s.imm];
            }
        }
        if(per_reg.some(x => x)) table[index] = { modrm: true, group: true, per_reg };
        return;
    }
    const s = substitute(body);
    if(!s) return;
    handlers.push(s.text);
    table[index] = { modrm, group: false, per_reg: [[handlers.length - 1, s.imm]] };
}

for(const { opcodes, body } of arms("interpreter.rs"))
{
    for(const op of opcodes)
    {
        // 32-bit operand size only; 0F is decoded through its own table
        if(op >= 0x100 && op < 0x200 && op !== 0x10F) add(op - 0x100, body);
    }
}
for(const { opcodes, body } of arms("interpreter0f.rs"))
{
    for(const op of opcodes)
    {
        if(op >= 0x100 && op < 0x200) add(256 + op - 0x100, body);
    }
}

// lea: the address without a segment (decoded.rs leaves it out for this opcode)
{
    handlers.push("        if mem {\n            crate::cpu::cpu::write_reg32(r, x);\n        }\n        else {\n            instructions::instr32_8D_reg(x, r);\n        }");
    table[0x8D] = { modrm: true, group: false, per_reg: [[handlers.length - 1, 0]] };
}

const out = [];
out.push("// Generated by gen/generate_decoded.js from interpreter.rs and interpreter0f.rs: do not edit");
out.push("#![cfg_attr(rustfmt, rustfmt_skip)]");
out.push("#![allow(unused_variables)]");
out.push("use crate::cpu::cpu::after_block_boundary;");
out.push("use crate::cpu::instructions;");
out.push("use crate::cpu::instructions_0f;");
out.push("");
out.push("/// the index of a handler in `run`");
out.push("pub type Handler = u16;");
out.push("");
out.push("/// x: the memory operand's address or the modrm's register; mem: which; r: the modrm's register field; imm");
out.push("#[inline(always)]");
out.push("pub unsafe fn run(h: Handler, x: i32, mem: bool, r: i32, imm: i32) {");
out.push("match h {");
handlers.forEach((text, i) => {
    out.push(`    ${i} => {`);
    out.push(text);
    out.push("    },");
});
out.push("    _ => std::hint::unreachable_unchecked(),");
out.push("}");
out.push("}");
out.push("");
out.push("/// One opcode: whether a modrm follows, whether its register field picks the handler, and per register field");
out.push("/// (or one for all) the handler and the immediate (0 none, 1 imm8, 2 imm8s, 3 imm16, 4 imm32)");
out.push("#[derive(Clone, Copy)]");
out.push("pub struct Entry { pub modrm: bool, pub group: bool, pub handlers: [Option<(Handler, u8)>; 8] }");
out.push("");
out.push("pub static TABLE: [Option<Entry>; 512] = [");
for(let i = 0; i < 512; i++)
{
    const e = table[i];
    if(!e)
    {
        out.push("    None,");
        continue;
    }
    const hs = new Array(8).fill("None").map((_, k) => {
        const x = e.group ? e.per_reg[k] : (k === 0 ? e.per_reg[0] : null);
        return x ? `Some((${x[0]}, ${x[1]}))` : "None";
    });
    out.push(`    Some(Entry { modrm: ${e.modrm}, group: ${e.group}, handlers: [${hs.join(", ")}] }),`);
}
out.push("];");
out.push("");

fs.writeFileSync(path.join(SRC, "decoded.rs"), out.join("\n"));
const covered = table.filter(x => x).length;
console.log(`decoded.rs: ${handlers.length} handlers, ${covered} opcodes`);
