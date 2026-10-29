#!/usr/bin/env node
// EXPECT: pass
// DESC: v86-parallel.wasm instantiated twice in one shared memory: each instance has its own statics and its own CPU state block (slot 0 and slot 1)

import assert from "node:assert/strict";
import fs from "node:fs";
import url from "node:url";

const __dirname = url.fileURLToPath(new URL(".", import.meta.url));
const wasm = __dirname + "/../../../build/v86-parallel.wasm";
if(!fs.existsSync(wasm))
{
    console.log("skipped: build/v86-parallel.wasm missing (make build/v86-parallel.wasm)");
    process.exit(0);
}
const { instantiate_v86, instantiate_relocated } = await import("../../../src/parallel/relocate.js");

const bytes = new Uint8Array(fs.readFileSync(wasm));
const stubs = () => ({
    mmap_read8() { return 0; }, mmap_read32() { return 0; }, mmap_write8() {}, mmap_write16() {}, mmap_write32() {}, mmap_write64() {}, mmap_write128() {},
    io_port_read8() { return 0; }, io_port_read16() { return 0; }, io_port_read32() { return 0; },
    io_port_write8() {}, io_port_write16() {}, io_port_write32() {},
    get_rand_int() { return 4; }, microtick() { return performance.now(); }, jit_clear_func() {}, codegen_finalize() {},
    stop_idling() {}, run_hardware_timers() { return 100; }, cpu_event_halt() {}, console_log_from_wasm() {},
    __indirect_function_table: new WebAssembly.Table({ element: "anyfunc", initial: 1024 + 5000 }),
});

const env0 = stubs();
const { instance: a } = await instantiate_v86(bytes, { env: env0 });
const memory = env0.memory;
assert.ok(memory.buffer instanceof SharedArrayBuffer, "the memory is shared");
a.exports.rust_init();
const { instance: b, base } = await instantiate_relocated(bytes, { env: stubs() }, memory, 1);
b.exports.rust_init();
assert.ok(base > 0);

const mem32 = new Int32Array(memory.buffer);
mem32[(0 * 4096 + 64) >> 2] = 0x11111111;     // eax of slot 0
mem32[(1 * 4096 + 64) >> 2] = 0x22222222;     // eax of slot 1
// (smp_debug_field reads a field of the running core's block through the instance's own state block)
assert.equal(a.exports.smp_debug_field(0, 64), 0x11111111, "instance 0 sees slot 0");
assert.equal(b.exports.smp_debug_field(0, 64), 0x22222222, "instance 1 sees slot 1");
// statics are separate: a different core count in each
a.exports.apic_set_core_count(1);
b.exports.apic_set_core_count(4);
b.exports.apic_set_current_core(3);
assert.equal(a.exports.apic_get_current_core(), 0);
assert.equal(b.exports.apic_get_current_core(), 3);
console.log("pass");
process.exit(0);
