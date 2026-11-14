#!/usr/bin/env node
// One boot of one build for profile.mjs: runs the guest to its last mark and prints one JSON line.
// Not meant to be started by hand; see profile.mjs for the options (they arrive as one JSON argument).

import fs from "node:fs";
import path from "node:path";
import url from "node:url";
/* global setImmediate */
import inspector from "node:inspector";

const opt = JSON.parse(process.argv[2]);
const real_now = performance.now.bind(performance);
const VIRTUAL = opt.clock === "virtual";

// ---------------------------------------------------------------------------------------------------------------
// The virtual clock: guest time is a function of the guest's own work (instructions executed), not of the
// host's clock, so every run of one build executes the same instructions and timer interrupts arrive at the
// same places. Time spent halted is skipped (see the yield below). Random numbers are seeded for the same reason.
let emulator = null;
let insn_high = 0, insn_last = 0, insn_view = null;
function instructions()
{
    // (its own view of the counter: cpu.instruction_counter goes through a proxy; a grown memory detaches the view)
    if(!insn_view || insn_view.length === 0)
    {
        const cpu = emulator && emulator.v86 && emulator.v86.cpu;
        if(!cpu) return insn_high + insn_last;
        const c = cpu.instruction_counter;
        insn_view = new Uint32Array(cpu.wasm_memory.buffer, c.byteOffset, 1);
    }
    const v = insn_view[0];
    if(v < insn_last) insn_high += 0x100000000;
    insn_last = v;
    return insn_high + v;
}
const MS_PER_INSN = 1 / (opt.mips * 1000);
let skipped_ms = 0;          // virtual time skipped while the guest was halted
let idle_ms = 0;             // real clock: time the main loop asked to sleep (the guest was halted)
const virtual_now = () => instructions() * MS_PER_INSN + skipped_ms;

if(VIRTUAL)
{
    Object.defineProperty(performance, "now", { value: virtual_now, configurable: true });
    const EPOCH = Date.UTC(2026, 0, 1);
    Date.now = () => EPOCH + Math.floor(virtual_now());
    let seed = 0x2545F491;
    const next = () => { seed ^= seed << 13; seed ^= seed >>> 17; seed ^= seed << 5; return seed >>> 0; };
    Math.random = () => next() / 0x100000000;
    const fill = array => {
        const bytes = new Uint8Array(array.buffer, array.byteOffset, array.byteLength);
        for(let i = 0; i < bytes.length; i++) bytes[i] = next() & 0xFF;
        return array;
    };
    Object.defineProperty(globalThis.crypto, "getRandomValues", { value: fill, configurable: true });
}

// ---------------------------------------------------------------------------------------------------------------
// Generated code: every WebAssembly module the JIT hands to the engine (count, bytes, synchronous compile time)
const wasm = { modules: 0, bytes: 0, sync_ms: 0 };
{
    const size = code => code.byteLength === undefined ? 0 : code.byteLength;
    const instantiate = WebAssembly.instantiate, Module = WebAssembly.Module;
    WebAssembly.instantiate = function(code, imports)
    {
        if(!(code instanceof Module)) { wasm.modules++; wasm.bytes += size(code); }
        return instantiate.call(WebAssembly, code, imports);
    };
    WebAssembly.Module = new Proxy(Module, {
        construct(target, args)
        {
            const t = real_now();
            const m = new target(...args);
            wasm.sync_ms += real_now() - t; wasm.modules++; wasm.bytes += size(args[0]);
            return m;
        },
    });
}

// ---------------------------------------------------------------------------------------------------------------
const { V86 } = await import(url.pathToFileURL(path.resolve(opt.lib)).href);
for(const extra of opt.plugins || []) await import(url.pathToFileURL(path.resolve(extra)).href);

const buf = p => { const b = fs.readFileSync(p); return b.buffer.slice(b.byteOffset, b.byteOffset + b.byteLength); };
const file = p => p ? { buffer: buf(p) } : undefined;

emulator = new V86({
    wasm_path: opt.wasm,
    bios: file(opt.bios), vga_bios: file(opt.vga_bios),
    cdrom: file(opt.cdrom), hda: file(opt.hda), fda: file(opt.fda),
    bzimage: file(opt.bzimage), initrd: file(opt.initrd), cmdline: opt.cmdline,
    initial_state: file(opt.state),
    memory_size: opt.memory * 1024 * 1024,
    vga_memory_size: 8 * 1024 * 1024,
    cpu_type: opt.arch,
    autostart: false,
    log_level: 0,
    ...(opt.v86 || {}),
});

let serial = "";
emulator.add_listener("serial0-output-byte", b => { serial += String.fromCharCode(b); check(); });

const screen_text = () => {
    try { return emulator.screen_adapter.get_text_screen().join("\n"); }
    catch(e) { return ""; }
};

// The boot's timeline: every line the guest prints (serial console and text screen) with the time it appeared
const timeline = [], seen_lines = new Set();
let serial_line = "";
function note_line(source, text)
{
    text = text.replace(/\x1b\[[0-9;?]*[A-Za-z]/g, "").replace(/[\x00-\x1f]/g, " ").trim();
    if(!text || !t0 || seen_lines.has(text)) return;
    seen_lines.add(text);
    timeline.push({ wall: (real_now() - t0) / 1000, insn: instructions(), source, text: text.slice(0, 110) });
}
emulator.add_listener("serial0-output-byte", b => {
    if(b === 10 || b === 13) { note_line("serial", serial_line); serial_line = ""; }
    else serial_line += String.fromCharCode(b);
});
function note_screen()
{
    const rows = screen_text().split("\n");
    // (the row the cursor is on is still being written: the rows above it are complete)
    for(let i = 0; i < rows.length - 1; i++) if(rows[i + 1].trim()) note_line("screen", rows[i]);
}

// The guest's side: where its instructions go. The main loop comes back about every 100,000 instructions; the
// guest's instruction pointer at those moments, before interrupts are delivered, is a sample (weighted with the instructions since the last one).
// Addresses are resolved in profile.mjs with a System.map of the guest's kernel (--guest-map).
const guest = new Map();
let guest_view = null, guest_last = 0;
function guest_sample()
{
    if(!guest_view || guest_view.length === 0)
    {
        const cpu = emulator.v86.cpu;
        guest_view = new Int32Array(cpu.wasm_memory.buffer, 0, 512);
    }
    const now = instructions(), weight = now - guest_last;
    guest_last = now;
    if(weight <= 0) return;
    const cpl = guest_view[612 >> 2] & 3;
    const lo = guest_view[556 >> 2] >>> 0, hi = opt.arch === "x86_64" ? guest_view[1440 >> 2] >>> 0 : 0;
    // (64 bytes per bucket; the key is cpl, then the address in hex)
    const key = cpl + (hi ? hi.toString(16) + ((lo & ~63) >>> 0).toString(16).padStart(8, "0") : ((lo & ~63) >>> 0).toString(16));
    guest.set(key, (guest.get(key) || 0) + weight);
}

const session = new inspector.Session();
const post = (method, params) => new Promise((resolve, reject) => session.post(method, params, (e, r) => e ? reject(e) : resolve(r)));

const result = { build: opt.label, marks: [], ok: false };
let t0 = 0, cpu0 = null, thread0 = null;
let mark_index = 0, serial_from = 0, finished = false;

function snapshot(name)
{
    const all = process.cpuUsage(cpu0), main = process.threadCpuUsage(thread0);
    return {
        name,
        wall: (real_now() - t0) / 1000,
        cpu_main: (main.user + main.system) / 1e6,
        cpu_all: (all.user + all.system) / 1e6,
        insn: instructions(),
        idle: (VIRTUAL ? skipped_ms : idle_ms) / 1000,
        modules: wasm.modules, wasm_bytes: wasm.bytes, wasm_sync: wasm.sync_ms / 1000,
    };
}

function check()
{
    while(!finished && mark_index < opt.marks.length)
    {
        const m = opt.marks[mark_index];
        let hit = false;
        if(m.insn !== undefined) hit = instructions() >= m.insn;
        else if(m.screen) hit = new RegExp(m.re).test(screen_text());
        else
        {
            const found = new RegExp(m.re).exec(serial.slice(serial_from));
            if(found) { hit = true; serial_from += found.index + found[0].length; }
        }
        if(!hit) return;
        result.marks.push(snapshot(m.name));
        mark_index++;
        if(m.send) emulator.serial0_send(m.send);
        if(m.keys) emulator.keyboard_send_text(m.keys);
    }
    if(!finished && mark_index >= opt.marks.length) finish(true);
}

async function finish(ok, error)
{
    if(finished) return;
    finished = true;
    result.ok = ok;
    if(error) result.error = error;
    result.end = snapshot("(end)");
    note_screen(); note_line("serial", serial_line);
    result.timeline = timeline;
    if(opt.guest_profile) result.guest = [...guest].sort((a, b) => b[1] - a[1]).slice(0, 20000);
    if(opt.profile)
    {
        const { profile } = await post("Profiler.stop");
        // (the marks on the profile's own time axis: its samples are microseconds from startTime)
        profile.v86_marks = result.marks.map(m => ({ name: m.name, us: m.wall * 1e6 + (t0 - result.profile_t0) * 1000 }));
        fs.writeFileSync(opt.profile, JSON.stringify(profile));
    }
    const cpu = emulator.v86.cpu, ex = cpu.wm.exports;
    if(opt.arch === "x86_64" && ex["get_jit_config"])
    {
        const g = i => ex["get_jit_config"](i) >>> 0;
        result.jit64 = { modules: g(5), failed: g(6), invalidated: g(8), blocks: g(16), bytes: g(17), instructions: g(13) };
        // the work, by how it was done (the core's own counters; 32-bit ones, so they wrap at 4295 M)
        result.counters = {
            "block runs (entries into compiled code)": g(7),
            "instructions in compiled code": g(13),
            "instructions interpreted outside compiled code": g(14),
            "instructions interpreted inside compiled blocks": g(6017),
            "helper calls: arithmetic and SSE (j64_alu)": g(6016),
            "helper calls: flags (j64_flags)": g(6018),
            "memory accesses on the slow path": [20, 21, 22, 23, 24, 25, 26, 27].reduce((n, i) => n + g(i), 0),
            "clock reads: rdtsc": g(30),
            "clock reads: main loop": g(31),
        };
    }
    if(opt.step_profile && ex["step_profile_len"])
    {
        // jit64's step profile: the instructions it left to the interpreter, by opcode
        const list = (len, key, count, bytes) => Array.from({ length: ex[len]() }, (_, i) => ({ key: ex[key](i) >>> 0, count: ex[count](i) >>> 0,
            bytes: bytes ? [ex[bytes](i, 0) >>> 0, ex[bytes](i, 1) >>> 0] : undefined })).sort((a, b) => b.count - a.count);
        result.step_profile = {
            in_block: ex["inblock_profile_len"] ? list("inblock_profile_len", "inblock_profile_key", "inblock_profile_count") : [],
            cold: list("step_profile_len", "step_profile_key", "step_profile_count"),
            block_end: list("interp_profile_len", "interp_profile_key", "interp_profile_count", "interp_profile_bytes"),
        };
    }
    if(ex["profiler_stat_get"] && opt.stat_names)
    {
        result.stats = {};
        opt.stat_names.forEach((n, i) => { const v = ex["profiler_stat_get"](i); if(v) result.stats[n] = v; });
        if(!Object.keys(result.stats).length) delete result.stats;
    }
    if(opt.serial_log) fs.writeFileSync(opt.serial_log, serial);
    if(!ok) result.serial_tail = serial.slice(-400);
    // (to a file: a long line on a pipe is cut off by process.exit)
    fs.writeFileSync(opt.result, JSON.stringify(result));
    process.exit(0);
}

emulator.add_listener("emulator-ready", async () => {
    const v86 = emulator.v86;
    // The yield of the main loop: t is how long the halted guest wants to sleep. With the virtual clock that
    // time is skipped at once (nothing else can wake the guest), with the real clock it is waited out and counted.
    v86.yield = function(t, tick)
    {
        instructions();     // (read often enough to see the 32-bit counter wrap)
        if(VIRTUAL)
        {
            // (a timer that is due exactly now reports 0: without a step the halted guest's clock would stand still)
            if(this.cpu.in_hlt[0]) skipped_ms += Math.max(t, 0.001);
            setImmediate(() => this.yield_callback(tick));
        }
        else if(t < 1) setImmediate(() => this.yield_callback(tick));
        else
        {
            const from = real_now();
            setTimeout(() => { idle_ms += real_now() - from; this.yield_callback(tick); }, t);
        }
    };
    if(opt.guest_profile)
    {
        // (the core calls this after each batch of instructions, before it delivers interrupts: sampled later, the
        // instruction pointer would mostly be an interrupt handler's first instruction)
        const cpu = v86.cpu, timers = cpu.run_hardware_timers;
        cpu.run_hardware_timers = function(acpi_enabled, now)
        {
            if(!cpu.in_hlt[0]) guest_sample(); else guest_last = instructions();
            return timers.call(cpu, acpi_enabled, now);
        };
    }
    if(opt.step_profile && v86.cpu.wm.exports["step_profile_enable"]) v86.cpu.wm.exports["step_profile_enable"](1);
    for(const [k, v] of Object.entries(opt.jit_config || {})) v86.cpu.wm.exports["set_jit_config"](+k, +v);

    if(opt.profile)
    {
        session.connect();
        await post("Profiler.enable");
        await post("Profiler.setSamplingInterval", { interval: opt.interval });
        await post("Profiler.start");
        result.profile_t0 = real_now();
    }
    t0 = real_now(); cpu0 = process.cpuUsage(); thread0 = process.threadCpuUsage();
    timeline.push({ wall: 0, insn: 0, source: "", text: "(the start: firmware, boot loader, the kernel before its first line)" });
    emulator.run();

    // (screen marks, instruction marks and the time limit are polled; serial marks are checked on every byte)
    let polls = 0;
    const poll = setInterval(() => {
        if(polls++ % 5 === 0) note_screen();
        check();
        if(!finished && real_now() - t0 > opt.timeout * 1000)
        {
            clearInterval(poll);
            finish(false, "timeout at mark " + (opt.marks[mark_index] ? opt.marks[mark_index].name : "?"));
        }
    }, 20);
});
