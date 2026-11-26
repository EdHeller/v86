#!/usr/bin/env node
// Runs a single flat test binary inside v86 and prints one JSON line:
//   { status: "pass"|"fail"|"timeout", id, actual, expected, exceptions: [...] }
// A JS exception or wasm trap (e.g. a dbg_assert in the debug build) makes the
// process exit non-zero without printing JSON; run.js reports that as a crash.
//
// usage: worker.js <test.bin> [json with extra V86 options]

import fs from "node:fs";
import url from "node:url";

const TEST_RELEASE_BUILD = +process.env.TEST_RELEASE_BUILD;
const { V86 } = await import(TEST_RELEASE_BUILD ? "../../build/libv86.mjs" : "../../src/main.js");

const LOAD_ADDR = 0x7C00;
const RESULT = 0x600;
const STATUS_PASS = 0x53534150;
const STATUS_FAIL = 0x4C494146;
const TIMEOUT_MS = +process.env.X64_TEST_TIMEOUT || 10000;

const bin_path = process.argv[2];
const bin = fs.readFileSync(bin_path);

const exceptions = [];
const extra_options = JSON.parse(process.argv[3] || "{}");
// (paths in the options of a test are relative to the repository)
for(const key of ["wasm_path", "vcpu_worker_url"])
{
    if(extra_options[key] && !extra_options[key].startsWith("/"))
    {
        extra_options[key] = url.fileURLToPath(new URL("../../" + extra_options[key], import.meta.url));
    }
}
const emulator = new V86({
    autostart: false,
    memory_size: 32 * 1024 * 1024,
    disable_jit: +process.env.DISABLE_JIT,
    jit64_threshold: process.env.JIT64 === undefined ? undefined : +process.env.JIT64,
    sync_jit: process.env.JIT64 !== undefined,
    jit64_disable_mask: +process.env.JIT64_MASK || 0,
    log_level: 0,
    wasm_path: process.env.X64_TEST_WASM || undefined,
    ...extra_options,
});

let done = false;

function finish(status)
{
    if(done) return;
    done = true;
    clearTimeout(timer);
    emulator.stop();

    const cpu = emulator.v86.cpu;
    const m32 = (addr) => cpu.mem32s[addr >> 2] >>> 0;
    const result = { status, exceptions };

    if(status === "halt")
    {
        const s = m32(RESULT);
        if(s === STATUS_PASS) result.status = "pass";
        else if(s === STATUS_FAIL) result.status = "fail";
        else result.status = "halt-without-result";
        result.id = m32(RESULT + 4);
        result.actual = "0x" + m32(RESULT + 12).toString(16).padStart(8, "0") + m32(RESULT + 8).toString(16).padStart(8, "0");
        result.expected = "0x" + m32(RESULT + 0x14).toString(16).padStart(8, "0") + m32(RESULT + 0x10).toString(16).padStart(8, "0");
        result.eip = "0x" + (cpu.instruction_pointer[0] >>> 0).toString(16);
    }

    if(process.env.JIT_STATS)
    {
        const g = i => emulator.v86.cpu.wm.exports.get_jit_config(i);
        result.jit64 = { compiled: g(5), failed: g(6), run: g(7), invalidated: g(8), insns: g(13) >>> 0, steps: g(14) >>> 0 };
    }
    if(process.env.PROFILER_STATS)
    {
        // a profiler build's counters (X64_TEST_WASM=build/v86-prof.wasm), by the index of src/rust/profiler.rs's stat
        const ex = emulator.v86.cpu.wm.exports;
        result.stats = {};
        for(let i = 0; ex["profiler_stat_get"] && i < 400; i++) { const v = ex["profiler_stat_get"](i); if(v) result.stats[i] = v; }
        if(ex["interp_addrs"])
        {
            const at = ex["interp_addrs"](2) >>> 0, f = new Float64Array(emulator.v86.cpu.wasm_memory.buffer, at, 256);
            result.interp_addrs = [];
            for(let i = 0; i < 16 && f[4 * i + 1]; i++) result.interp_addrs.push([f[4 * i].toString(16), f[4 * i + 1], f[4 * i + 2]]);
        }
    }
    console.log(JSON.stringify(result));
    process.exit(0);
}

const timer = setTimeout(() => finish("timeout"), TIMEOUT_MS);

emulator.cpu_exception_hook = function(n)
{
    exceptions.push(n);
    return false; // let the guest handle it through its own IVT/IDT
};

emulator.bus.register("cpu-event-halt", () => finish("halt"));

emulator.add_listener("emulator-loaded", function()
{
    const cpu = emulator.v86.cpu;
    if(process.env.DUMP_MODULES)
    {
        // the 32-bit JIT's modules, as <physical start>_<n>.wasm (wasm2wat reads them)
        fs.mkdirSync(process.env.DUMP_MODULES, { recursive: true });
        const finalize = cpu.codegen_finalize;
        let n = 0;
        cpu.codegen_finalize = function(index, start, state_flags, ptr, len)
        {
            fs.writeFileSync(process.env.DUMP_MODULES + "/" + (start >>> 0).toString(16) + "_" + n++ + ".wasm",
                new Uint8Array(cpu.wasm_memory.buffer, ptr >>> 0, len >>> 0));
            return finalize.call(cpu, index, start, state_flags, ptr, len);
        };
    }
    cpu.reboot_internal();
    cpu.reset_memory();
    cpu.mem8.set(bin, LOAD_ADDR);
    // reset vector f000:fff0 -> jmp 0000:7c00
    cpu.mem8.set([0xEA, 0x00, 0x7C, 0x00, 0x00], 0xFFFF0);
    if(cpu.parallel)
    {
        // the other cores are in workers that take a moment to start
        const wait = setInterval(() => {
            if(cpu.parallel.ready >= cpu.parallel.cores - 1 || cpu.parallel.failed)
            {
                clearInterval(wait);
                emulator.run();
            }
        }, 10);
    }
    else
    {
        emulator.run();
    }
});
