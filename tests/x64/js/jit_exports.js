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
    for(const m of text.matchAll(/"((?:instr|mmx|sse|jit)[A-Za-z0-9_]{3,})"/g)) names.add(m[1]);
}

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
