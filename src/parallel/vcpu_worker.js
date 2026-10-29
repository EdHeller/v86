// A vCPU worker (docs/parallel.md): runs one core of a parallel machine in its own instance of v86-parallel.wasm. Everything that belongs
// to the machine (devices, the APIC registers of this core, the I/O APIC) is asked of the machine thread through the control block.
// Runs as a module worker in the browser and as a worker_threads worker in node.

import { instantiate_relocated } from "./relocate.js";
import * as C from "./control.js";

async function start(data, post)
{
    const { bytes, memory, control, core, cores, memory_ptr, memory_size, tsc_low, tsc_high, time_origin, table_size } = data;
    const ctl = new Int32Array(control);
    const slot = C.SLOT_BASE + core * C.SLOT_WORDS;
    const time_delta = (typeof performance.timeOrigin === "number" ? performance.timeOrigin : Date.now() - performance.now()) - time_origin;
    let exports = null;

    /** ask the machine thread; returns the first result word */
    const request = (kind, a0 = 0, a1 = 0, a2 = 0, a3 = 0, a4 = 0) =>
    {
        ctl[slot + C.SLOT_KIND] = kind;
        ctl[slot + C.SLOT_ARG] = a0; ctl[slot + C.SLOT_ARG + 1] = a1; ctl[slot + C.SLOT_ARG + 2] = a2; ctl[slot + C.SLOT_ARG + 3] = a3; ctl[slot + C.SLOT_ARG + 4] = a4;
        Atomics.store(ctl, slot + C.SLOT_STATE, 1);
        Atomics.add(ctl, C.WAKE, 1);
        Atomics.notify(ctl, C.WAKE);
        while(Atomics.load(ctl, slot + C.SLOT_STATE) !== 2)
        {
            if(Atomics.load(ctl, C.STOP)) throw new Error("stopped");
            Atomics.wait(ctl, slot + C.SLOT_STATE, 1, 100);
        }
        const result = ctl[slot + C.SLOT_RESULT];
        Atomics.store(ctl, slot + C.SLOT_STATE, 0);
        return result;
    };

    const env = {
        "microtick": () => performance.now() + time_delta,
        "get_rand_int": () => (Math.random() * 0x100000000) | 0,
        "stop_idling": () => {},
        "cpu_event_halt": () => {},
        "run_hardware_timers": () => 100,
        "jit_clear_func": () => {},
        "codegen_finalize": () => {},
        "cpu_exception_hook": () => false,
        "dbg_trace_from_wasm": () => {},
        "log_from_wasm": () => {},
        "console_log_from_wasm": (offset, len) => {
            const text = String.fromCharCode(...new Uint8Array(memory.buffer, offset >>> 0, len >>> 0).slice());
            console.error("core " + core + ": " + text);
        },
        "io_port_read8": a => request(C.REQ_PORT_READ8, a) & 0xFF,
        "io_port_read16": a => request(C.REQ_PORT_READ16, a) & 0xFFFF,
        "io_port_read32": a => request(C.REQ_PORT_READ32, a),
        "io_port_write8": (a, v) => { request(C.REQ_PORT_WRITE8, a, v); },
        "io_port_write16": (a, v) => { request(C.REQ_PORT_WRITE16, a, v); },
        "io_port_write32": (a, v) => { request(C.REQ_PORT_WRITE32, a, v); },
        "mmap_read8": a => request(C.REQ_MMAP_READ8, a) & 0xFF,
        "mmap_read32": a => request(C.REQ_MMAP_READ32, a),
        "mmap_write8": (a, v) => { request(C.REQ_MMAP_WRITE8, a, v); },
        "mmap_write16": (a, v) => { request(C.REQ_MMAP_WRITE16, a, v); },
        "mmap_write32": (a, v) => { request(C.REQ_MMAP_WRITE32, a, v); },
        "mmap_write64": (a, v0, v1) => { request(C.REQ_MMAP_WRITE64, a, v0, v1); },
        "mmap_write128": (a, v0, v1, v2, v3) => { request(C.REQ_MMAP_WRITE128, a, v0, v1, v2, v3); },
        "apic_read32": (core, addr) => request(C.REQ_APIC_READ, core, addr),
        "apic_write32": (core, addr, value) => { request(C.REQ_APIC_WRITE, core, addr, value); },
        "ioapic_read32": addr => request(C.REQ_IOAPIC_READ, addr),
        "ioapic_write32": (addr, value) => { request(C.REQ_IOAPIC_WRITE, addr, value); },
        "apic_ack": core => request(C.REQ_APIC_ACK, core),
        "core_pending": core => Atomics.load(ctl, C.PENDING + core),
        "__indirect_function_table": new WebAssembly.Table({ element: "anyfunc", initial: table_size }),
    };

    const { instance } = await instantiate_relocated(new Uint8Array(bytes), { env }, memory, core);
    exports = instance.exports;
    exports["rust_init"]();
    exports["worker_init"](core, cores, memory_ptr, memory_size, tsc_low, tsc_high);
    post({ ready: core });

    try
    {
        while(!Atomics.load(ctl, C.STOP))
        {
            if(Atomics.load(ctl, C.PENDING + core))
            {
                const events = request(C.REQ_TAKE_EVENTS, core);
                if(events) exports["worker_events"](events);
            }
            const status = exports["worker_run"](20000);
            Atomics.store(ctl, C.DEBUG + core, exports["smp_debug_field"](core, 556));
            if(status !== 0)
            {
                // halted or waiting for the start-up IPI: sleep until the machine has something (or a little, if what it has cannot be taken yet)
                if(Atomics.load(ctl, C.PENDING + core) === 0) Atomics.wait(ctl, C.PENDING + core, 0, 50);
                else Atomics.wait(ctl, C.SLOT_BASE - 1, 0, 1);
            }
        }
    }
    catch(e)
    {
        if(e.message !== "stopped") { request(C.REQ_FAILED, core); post({ failed: core, message: String(e && e.stack || e) }); throw e; }
    }
}

if(typeof process !== "undefined" && process.versions && process.versions.node)
{
    const { parentPort, workerData } = await import("node:worker_threads");
    if(parentPort) start(workerData, m => parentPort.postMessage(m)).catch(e => { console.error(e); process.exit(1); });
}
else if(typeof self !== "undefined")
{
    self.onmessage = e => start(e.data, m => self.postMessage(m)).catch(err => self.postMessage({ failed: -1, message: String(err && err.stack || err) }));
}
