#!/usr/bin/env node
// Every function that jit compiled code calls by name has to be exported by the main wasm module.
// A function that is missing (a new instruction handler without #[no_mangle]) only shows up as a
// "LinkError: Import ... function import requires a callable" when a compiled block that uses
// the instruction is instantiated, i.e. when a guest happens to run that instruction hot.
//
// Checks build/v86-debug.wasm (and build/v86.wasm when it exists) against the names the jit code
// generators mention (src/rust/gen/jit*.rs, jit_instructions.rs, codegen.rs).

import fs from "node:fs";
import path from "node:path";
import url from "node:url";

const root = path.join(url.fileURLToPath(new URL(".", import.meta.url)), "..", "..", "..");
const sources = ["src/rust/gen/jit.rs", "src/rust/gen/jit0f.rs", "src/rust/jit_instructions.rs", "src/rust/codegen.rs"];
const names = new Set();
for(const f of sources)
{
    const text = fs.readFileSync(path.join(root, f), "utf8");
    for(const m of text.matchAll(/"([A-Za-z_][A-Za-z0-9_]{3,})"/g)) names.add(m[1]);
}
// (every quoted name that is a Rust function: fpu_*, safe_*, ... too, not only instr*)
const rust = [];
const walk = d => { for(const e of fs.readdirSync(d, { withFileTypes: true })) e.isDirectory() ? walk(path.join(d, e.name)) : e.name.endsWith(".rs") && rust.push(fs.readFileSync(path.join(d, e.name), "utf8")); };
walk(path.join(root, "src/rust"));
const defined = new Set(rust.join("\n").match(/\bfn [A-Za-z_][A-Za-z0-9_]*/g).map(m => m.slice(3)));
// (report_*: called only by the profiler build, bug_*: only by the debug build, where they are defined)
for(const n of [...names]) if(!defined.has(n) || /^(report_|bug_)/.test(n)) names.delete(n);

let errors = 0;
for(const wasm of ["build/v86-debug.wasm", "build/v86.wasm"])
{
    const file = path.join(root, wasm);
    if(!fs.existsSync(file)) continue;
    const module = await WebAssembly.compile(fs.readFileSync(file));
    const exported = new Set(WebAssembly.Module.exports(module).map(e => e.name));
    const missing = [...names].filter(n => !exported.has(n));
    if(missing.length)
    {
        console.error(`${wasm}: ${missing.length} functions used by jit code are not exported (add #[no_mangle]): ${missing.join(" ")}`);
        errors++;
    }
}
if(names.size < 300) { console.error("found only " + names.size + " function names: the pattern needs updating"); errors++; }
process.exit(errors ? 1 : 0);
