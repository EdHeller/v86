// The machine thread's side of a parallel machine (docs/parallel.md): starts the vCPU workers and answers their requests.

import * as C from "./control.js";
import { image_size, PAGE_SIZE } from "./relocate.js";

export class ParallelMachine
{
    /**
     * @param {Object} cpu the machine's CPU (the instance at base 0)
     * @param {{bytes: Uint8Array, worker_url: (string|URL), cores: number}} options
     */
    constructor(cpu, options)
    {
        this.cpu = cpu;
        this.exports = cpu.wm.exports;
        this.bytes = options.bytes;
        this.worker_url = options.worker_url;
        this.cores = options.cores;
        this.control = new SharedArrayBuffer(C.CONTROL_BYTES);
        this.ctl = new Int32Array(this.control);
        this.workers = [];
        this.ready = 0;
        this.failed = null;
        this.serviced = 0;
    }

    async start()
    {
        const cpu = this.cpu;
        const memory = cpu.wasm_memory;
        this.exports["machine_set_parallel"](1);
        const time_origin = typeof performance.timeOrigin === "number" ? performance.timeOrigin : Date.now() - performance.now();
        const table_size = 1024 + 5000;
        for(let core = 1; core < this.cores; core++)
        {
            const data = {
                bytes: this.bytes, memory, control: this.control, core, cores: this.cores,
                memory_ptr: cpu.mem8.byteOffset, memory_size: cpu.memory_size[0],
                tsc_low: this.exports["get_tsc_offset"](0), tsc_high: this.exports["get_tsc_offset"](1),
                time_origin, table_size,
            };
            this.workers[core] = spawn(this.worker_url, data, message => {
                if(message.ready) this.ready++;
                if(message.failed !== undefined) this.failed = message;
            });
        }
    }

    /** answer the requests of the workers and tell them what is waiting for them; called often by the machine thread */
    service()
    {
        const ctl = this.ctl;
        const e = this.exports;
        let any = false;
        for(let core = 1; core < this.cores; core++)
        {
            const slot = C.SLOT_BASE + core * C.SLOT_WORDS;
            if(Atomics.load(ctl, slot + C.SLOT_STATE) !== 1) continue;
            any = true;
            const a = i => ctl[slot + C.SLOT_ARG + i];
            let result = 0;
            const io = this.cpu.io;
            switch(ctl[slot + C.SLOT_KIND])
            {
                case C.REQ_PORT_READ8: result = io.port_read8(a(0)); break;
                case C.REQ_PORT_READ16: result = io.port_read16(a(0)); break;
                case C.REQ_PORT_READ32: result = io.port_read32(a(0)); break;
                case C.REQ_PORT_WRITE8: io.port_write8(a(0), a(1)); break;
                case C.REQ_PORT_WRITE16: io.port_write16(a(0), a(1)); break;
                case C.REQ_PORT_WRITE32: io.port_write32(a(0), a(1)); break;
                case C.REQ_MMAP_READ8: result = this.cpu.mmap_read8(a(0)); break;
                case C.REQ_MMAP_READ32: result = this.cpu.mmap_read32(a(0)); break;
                case C.REQ_MMAP_WRITE8: this.cpu.mmap_write8(a(0), a(1)); break;
                case C.REQ_MMAP_WRITE16: this.cpu.mmap_write16(a(0), a(1)); break;
                case C.REQ_MMAP_WRITE32: this.cpu.mmap_write32(a(0), a(1)); break;
                case C.REQ_MMAP_WRITE64: this.cpu.mmap_write64(a(0), a(1), a(2)); break;
                case C.REQ_MMAP_WRITE128: this.cpu.mmap_write128(a(0), a(1), a(2), a(3), a(4)); break;
                case C.REQ_APIC_READ: result = e["machine_apic_read32"](a(0), a(1)); break;
                case C.REQ_APIC_WRITE: e["machine_apic_write32"](a(0), a(1), a(2)); break;
                case C.REQ_IOAPIC_READ: result = e["machine_ioapic_read32"](a(0)); break;
                case C.REQ_IOAPIC_WRITE: e["machine_ioapic_write32"](a(0), a(1)); break;
                case C.REQ_APIC_ACK: result = e["machine_apic_ack"](a(0)); break;
                case C.REQ_TAKE_EVENTS: result = e["machine_take_events"](a(0)); break;
                case C.REQ_FAILED: this.failed = { failed: a(0), message: "worker failed" }; break;
            }
            ctl[slot + C.SLOT_RESULT] = result;
            Atomics.store(ctl, slot + C.SLOT_STATE, 2);
            Atomics.notify(ctl, slot + C.SLOT_STATE);
            this.serviced++;
        }
        this.refresh_pending();
        return any;
    }

    /** which cores have something waiting (interrupts, INIT, start-up IPIs) */
    refresh_pending()
    {
        const mask = this.exports["machine_pending_mask"]();
        for(let core = 1; core < this.cores; core++)
        {
            const value = mask >> core & 1;
            if(Atomics.load(this.ctl, C.PENDING + core) !== value)
            {
                Atomics.store(this.ctl, C.PENDING + core, value);
                if(value) Atomics.notify(this.ctl, C.PENDING + core);
            }
        }
    }

    stop()
    {
        Atomics.store(this.ctl, C.STOP, 1);
        for(let core = 1; core < this.cores; core++) Atomics.notify(this.ctl, C.PENDING + core);
        for(const worker of this.workers) worker && worker.terminate && worker.terminate();
    }
}

/** a worker running `url` (a module worker in the browser, a worker_threads worker in node) */
function spawn(url, data, on_message)
{
    if(typeof process !== "undefined" && process.versions && process.versions.node)
    {
        const { Worker } = process["getBuiltinModule"] ? process["getBuiltinModule"]("node:worker_threads") : require("worker_threads");
        const worker = new Worker(url, { workerData: data });
        worker.on("message", on_message);
        worker.on("error", e => on_message({ failed: -1, message: String(e && e.stack || e) }));
        worker.unref();
        return worker;
    }
    const worker = new Worker(url, { type: "module" });
    worker.onmessage = e => on_message(e.data);
    worker.onerror = e => on_message({ failed: -1, message: e.message });
    worker.postMessage(data);
    return worker;
}
