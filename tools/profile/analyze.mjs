// Reads a V8 .cpuprofile of a v86 run (tools/profile/profile.mjs writes them; Chrome DevTools' Performance
// panel exports the same format) and sorts the samples by where the emulator spends its time.
//
//   node tools/profile/analyze.mjs <file.cpuprofile> [--top N]

import fs from "node:fs";
import url from "node:url";

// The areas, in the order of the report
export const AREAS = [
    ["compiled", "compiled guest code (the JIT's modules)"],
    ["helper", "core functions called from compiled code"],
    ["dispatch", "main loop and dispatch (with what is inlined there)"],
    ["interpreter", "interpreter and the rest of the core"],
    ["jit", "JIT: generating code (Rust)"],
    ["instantiate", "JIT: compiling and instantiating modules (the engine)"],
    ["devices", "devices in JavaScript (called from the core)"],
    ["js", "JavaScript outside the core (loop, timers, harness)"],
    ["gc", "garbage collector"],
    ["idle", "idle (the guest halted, or waiting for the engine)"],
    ["other", "other (the engine, node)"],
];

const GENERATORS = /(^|::)(jit_analyze_and_generate|jit_generate_module|jit_find_basic_blocks|jit64_compile|compile_part|jit_force_generate_unsafe)$|jit64::(compile|generate|decode|flag_liveness)$|^codegen::|^wasmgen::/;
const DISPATCH = /(^|::)(main_loop|do_many_cycles_native|cycle_internal|jit64::run|try_run_block|call_block|verified_call|jit_find_cache_entry|jit_find_cache_entry_in_page)$/;

// Rust's v0 mangling, as far as plain paths go (_RNvNtNtCs..._3v863cpu3cpu9read_imm8 -> cpu::cpu::read_imm8); for
// anything else (generics, impls) the identifiers found in the name
export function demangle(name)
{
    if(!name.startsWith("_R")) return name;
    let i = 2;
    const base62 = () => { while(i < name.length && name[i] !== "_") i++; i++; };
    const ident = () => {
        if(name[i] === "s") { i++; base62(); }
        if(name[i] === "u") i++;
        let n = 0;
        if(!/[0-9]/.test(name[i])) throw new Error("ident");
        while(/[0-9]/.test(name[i])) n = n * 10 + +name[i++];
        if(name[i] === "_") i++;
        const s = name.slice(i, i + n);
        i += n;
        return s;
    };
    const path = () => {
        const c = name[i++];
        if(c === "C") return [ident()];
        if(c === "N") { i++; const p = path(); p.push(ident()); return p; }
        throw new Error("path");
    };
    try
    {
        const p = path();
        return (p.length > 1 ? p.slice(1) : p).join("::");
    }
    catch(e)
    {
        const out = [];
        const rest = name.slice(2).replace(/(?<=[CMX])s[0-9A-Za-z]*?_/g, " ");
        const re = /(\d+)_?/g;
        let m, at = 0;
        while((m = re.exec(rest)))
        {
            if(m.index < at) continue;
            const n = +m[1], s = rest.slice(re.lastIndex, re.lastIndex + n);
            if(n > 0 && s.length === n && /^[A-Za-z_][A-Za-z0-9_]*$/.test(s)) { out.push(s); at = re.lastIndex + n; re.lastIndex = at; }
        }
        const core = out.filter(s => s !== "v86");
        return core.length ? core.slice(-4).join("::") + " <generic>" : name;
    }
}

const is_wasm = u => u.startsWith("wasm://");
// (the core has a name section, so the engine calls it wasm://wasm/v86.wasm-<hash>; generated modules have none)
const is_core = u => is_wasm(u) && /wasm:\/\/wasm\/.+-[0-9a-f]+$/.test(u);
const is_generated = u => is_wasm(u) && !is_core(u);

export function analyze(profile)
{
    const nodes = new Map(profile.nodes.map(n => [n.id, n]));
    const parent = new Map();
    for(const n of profile.nodes) for(const c of n.children || []) parent.set(c, n.id);

    // per node: the area, the display name and the core functions on its stack
    const info = new Map();
    const describe = id => {
        if(info.has(id)) return info.get(id);
        const frames = [];
        for(let x = id; x !== undefined; x = parent.get(x)) frames.push(nodes.get(x).callFrame);
        const leaf = frames[0], u = leaf.url, raw = leaf.functionName;
        const names = frames.map(f => is_core(f.url) ? demangle(f.functionName) : f.functionName);
        let area, name;
        if(raw === "(idle)") { area = "idle"; name = "(idle)"; }
        else if(raw === "(garbage collector)") { area = "gc"; name = raw; }
        else if(is_generated(u)) { area = "compiled"; name = "[compiled guest code]"; }
        else if(is_core(u))
        {
            name = names[0];
            if(frames.some((f, i) => is_core(f.url) && GENERATORS.test(names[i]))) area = "jit";
            else if(frames.some(f => is_generated(f.url))) area = "helper";
            else if(DISPATCH.test(name)) area = "dispatch";
            else area = "interpreter";
        }
        else if(frames.some(f => f.functionName === "codegen_finalize" || /^(Module|instantiate|Instance)$/.test(f.functionName) && !f.url))
        {
            area = "instantiate"; name = "[js] " + (raw || "(anonymous)");
        }
        else if(u.startsWith("file://") || u.startsWith("http"))
        {
            const file = u.slice(u.lastIndexOf("/") + 1);
            area = frames.some(f => is_wasm(f.url)) ? "devices" : "js";
            name = "[js] " + file + ": " + (raw || "(anonymous)");
        }
        else { area = "other"; name = raw || "(anonymous)"; }
        const core_stack = new Set(frames.map((f, i) => is_core(f.url) && !/^(js-to-wasm|wasm-to-js)/.test(names[i]) ? names[i] : null).filter(x => x));
        const d = { area, name, core_stack };
        info.set(id, d);
        return d;
    };

    // phases: from the marks of the run (microseconds from the profile's start), or one phase
    const marks = (profile.v86_marks || []).filter(m => m.us > 0);
    const phases = marks.length ? marks.map(m => m.name) : ["all"];
    const phase_of = us => { for(let i = 0; i < marks.length; i++) if(us <= marks[i].us) return i; return marks.length ? -1 : 0; };

    const by_area = phases.map(() => ({}));
    const self = new Map(), inclusive = new Map(), area_of = new Map();
    let t = 0, total = 0;
    profile.samples.forEach((id, i) => {
        const dt = profile.timeDeltas[i] || 0;
        t += dt;
        const p = phase_of(t);
        if(p < 0) return;
        const d = describe(id);
        total += dt;
        by_area[p][d.area] = (by_area[p][d.area] || 0) + dt;
        self.set(d.name, (self.get(d.name) || 0) + dt);
        area_of.set(d.name, d.area);
        for(const f of d.core_stack) inclusive.set(f, (inclusive.get(f) || 0) + dt);
    });
    const sorted = m => [...m].sort((a, b) => b[1] - a[1]).map(([name, us]) => ({ name, seconds: us / 1e6, area: area_of.get(name) }));
    return {
        total: total / 1e6,
        phases,
        by_area: by_area.map(a => Object.fromEntries(Object.entries(a).map(([k, v]) => [k, v / 1e6]))),
        self: sorted(self),
        inclusive: sorted(inclusive),
    };
}

const pct = (x, total) => (total ? (100 * x / total).toFixed(1) : "0.0").padStart(5) + " %";

export function format(a, top = 30)
{
    const out = [];
    const cols = a.phases.length > 1 ? [...a.phases, "total"] : ["total"];
    const sums = AREAS.map(([k]) => a.by_area.reduce((s, p) => s + (p[k] || 0), 0));
    out.push("Seconds by area" + (a.phases.length > 1 ? " and phase (a phase ends at its mark)" : ""));
    out.push("  " + cols.map(c => c.slice(0, 10).padStart(10)).join("") + "  share   area");
    AREAS.forEach(([k, label], i) => {
        if(!sums[i]) return;
        const cells = a.phases.length > 1 ? [...a.by_area.map(p => p[k] || 0), sums[i]] : [sums[i]];
        out.push("  " + cells.map(v => v.toFixed(2).padStart(10)).join("") + " " + pct(sums[i], a.total) + "  " + label);
    });
    const totals = a.phases.length > 1 ? [...a.by_area.map(p => Object.values(p).reduce((s, v) => s + v, 0)), a.total] : [a.total];
    out.push("  " + totals.map(v => v.toFixed(2).padStart(10)).join("") + "          total");

    out.push("", "Self time by function (the " + top + " largest)");
    for(const f of a.self.slice(0, top)) out.push("  " + f.seconds.toFixed(2).padStart(8) + " " + pct(f.seconds, a.total) + "  " + f.name.slice(0, 100) + (f.area === "helper" || f.area === "jit" ? "  (" + f.area + ")" : ""));

    out.push("", "Inclusive time of core functions (with everything they call; the " + top + " largest)");
    for(const f of a.inclusive.slice(0, top)) out.push("  " + f.seconds.toFixed(2).padStart(8) + " " + pct(f.seconds, a.total) + "  " + f.name.slice(0, 100));
    return out.join("\n");
}

if(process.argv[1] === url.fileURLToPath(import.meta.url))
{
    const file = process.argv[2];
    if(!file) { console.log("usage: node tools/profile/analyze.mjs <file.cpuprofile> [--top N]"); process.exit(1); }
    const top = process.argv.includes("--top") ? +process.argv[process.argv.indexOf("--top") + 1] : 30;
    console.log(format(analyze(JSON.parse(fs.readFileSync(file, "utf8"))), top));
}
