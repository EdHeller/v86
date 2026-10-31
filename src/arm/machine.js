// The AArch64 machine: the virt board of QEMU 9.2 (as the v64 project's ARM core models it), driven from
// JavaScript. The core (build/v86-arm64.wasm, a64/) has the CPU, its page tier, the GICv3, the timers, PSCI
// and the physical address map; the devices are here: the PL011 serial port, the PL031 clock and virtio-mmio
// disks and rng. A Linux kernel (an arm64 Image, or an EFI zboot image holding one) and an initrd are loaded
// as QEMU's loader does, with a generated device tree.
//
//     const machine = await A64Machine.create({ wasm, kernel, initrd, cmdline, memory_size, cpu_cores, disks });
//     machine.add_listener("serial0-output-byte", byte => ...);
//     machine.run();
//     machine.serial0_send("root\n");

import { MachineClock } from "./machine_clock.js";
import { set_jit_switches } from "./jit_switches.js";
import { VirtDeviceSpace } from "./virt/io.js";
import { PL011 } from "./virt/pl011.js";
import { PL031 } from "./virt/pl031.js";
import { load_linux } from "./virt/loader.js";
import { CfiFlash, VIRT_FLASH_SIZE } from "./virt/pflash.js";
import { FwCfg, VIRT_FW_CFG, VIRT_FW_CFG_SIZE } from "./virt/fw_cfg.js";
import { flatten, virt_tree } from "./virt/dtb.js";
import {
    A64_PROFILES, ARM32_PROFILE, VIRT_MMIO, VIRT_MMIO_COUNT, VIRT_MMIO_SIZE, VIRT_PCIE_ECAM_HIGH, VIRT_PCIE_ECAM_SIZE, VIRT_RAM_BASE, virt_machine,
} from "./virt/platform.js";
import { VirtioMmioBlk, VirtioMmioNet, VirtioMmioRng } from "./virtio_mmio.js";
import { ParallelMachine } from "./parallel/machine.js";
import { instantiate_v86 } from "./parallel/relocate.js";
import { COMMAND_RELOAD, COMMAND_RESET } from "./parallel/control.js";
import { CORE_STATE_RANGES } from "./state_layout.js";

/** Function table: slots of generated page functions (the core's jitrt::table), at this offset */
export const WASM_TABLE_OFFSET = 1024;
export const WASM_TABLE_SIZE = 12000;

/** the little the devices need of a bus: named events */
function Events()
{
    this.listeners = new Map();
}
Events.prototype.register = function(name, fn, this_value)
{
    if(!this.listeners.has(name)) this.listeners.set(name, []);
    this.listeners.get(name).push(this_value ? fn.bind(this_value) : fn);
};
Events.prototype.send = function(name, value)
{
    for(const fn of this.listeners.get(name) || []) fn(value);
};

/** Run `fn` soon, after the host's events (setImmediate, or a message to ourselves: setTimeout(0) waits 4 ms in browsers) */
const yield_to_host = (() => {
    if(typeof setImmediate === "function") return fn => { setImmediate(fn); };
    const channel = new MessageChannel();
    const queue = [];
    channel.port1.onmessage = () => { queue.shift()(); };
    return fn => { queue.push(fn); channel.port2.postMessage(0); };
})();

/**
 * @constructor
 * @param {Object} options see A64Machine.create
 */
export function A64Machine(options)
{
    this.options = options;
    this.bus = new Events();
    this.cpu_type = "arm64";
    this.machine_type = "virt";
    this.running = false;
    this.stopped = false;
    this.timer = null;
    this.booted = false;
    this.devices = {};
    this.virtio = [];
    this.extended_pages = 0;
    this.extended_base = 0;
    this.ranchu_framebuffer = null;
    this.memory_size = new Uint32Array(1);
    this.mem8 = new Uint8Array(0);
    this.cores = 1;
    this.clock = new MachineClock({ now: () => performance.now() });
}

/**
 * @param {Object} options
 *   wasm: the core's bytes (ArrayBuffer / Uint8Array) or a WebAssembly.Module
 *   kernel, initrd: ArrayBuffers; cmdline: string
 *   memory_size (default 512 MiB), cpu_cores (default 1), a64_profile ("v2" default)
 *   disks: [{ buffer: ArrayBuffer, readonly: boolean }] on virtio-mmio, virtio_rng: boolean
 *   firmware: { code, vars } (ArrayBuffers: bios/aavmf-code.fd and a copy of bios/aavmf-vars.fd) boots UEFI firmware
 *     from the flash instead of a kernel; the variable store is written in place (firmware.vars)
 *   ir_sync_publication: compile page functions synchronously
 * @return {Promise<A64Machine>}
 */
A64Machine.create = async function(options)
{
    const machine = new A64Machine(options);
    await machine.load_core(options.wasm);
    machine.init(options);
    // cores in workers: options.parallel = { worker_url } with options.wasm the parallel build's bytes
    if(options.parallel && machine.cores > 1)
    {
        await machine.start_parallel({ bytes: options.wasm instanceof Uint8Array ? options.wasm : new Uint8Array(options.wasm), worker_url: options.parallel.worker_url });
    }
    return machine;
};

/** The machine object of a vCPU worker: only what publishes the page functions the worker's core compiles */
A64Machine.for_worker = function(exports, table, clock)
{
    const machine = Object.create(A64Machine.prototype);
    machine.wm = { exports, wasm_table: table };
    machine.wasm_memory = exports.memory;
    machine.execution_epoch = 0;
    machine.options = {};
    machine.clock = clock;
    machine.state_base = exports["state_base"]() >>> 0;
    machine.create_jit_imports();
    return machine;
};

A64Machine.prototype.load_core = async function(wasm)
{
    const table = new WebAssembly.Table({ element: "anyfunc", initial: WASM_TABLE_SIZE + WASM_TABLE_OFFSET });
    const decode = (offset, len) => new TextDecoder().decode(new Uint8Array(this.wasm_memory.buffer, offset, len));
    const env = {
        "console_log_from_wasm": (offset, len) => { console.error(decode(offset, len)); },
        "microtick": () => this.clock.now(),
        "a64_mmio_read": (owner, offset, width) => this.io.mmio_read(owner, offset, width),
        "a64_mmio_write": (owner, offset, width, value) => { this.io.mmio_write(owner, offset, width, value); },
        "extended_load": () => {},
        "extended_store": () => {},
        "jit_publish": (client, id, slot, pointer, length) => this.jit_publish(client, id, slot, pointer, length),
        "jit_clear_func": index => { table.set(index + WASM_TABLE_OFFSET, null); },
        // (the parallel build: wake a vCPU waiting on a word of the shared memory)
        "parallel_notify": address => { Atomics.notify(new Int32Array(this.wasm_memory.buffer), address >>> 2); },
        "__indirect_function_table": table,
    };
    // (the parallel build imports a shared memory: instantiate_v86 makes it, and relocates this instance to base 0)
    const instance = wasm instanceof WebAssembly.Module ? await WebAssembly.instantiate(wasm, { "env": env }) :
        (await instantiate_v86(wasm, { "env": env })).instance;
    this.wm = { exports: instance.exports, wasm_table: table };
    this.state_base = instance.exports["state_base"]() >>> 0;
    if(!instance.exports["isa_aarch64"]) throw new Error("the core is not the ARM core (build/v86-arm64.wasm)");
    this.wasm_memory = instance.exports.memory;
    instance.exports["rust_init"]();
    this.io = new VirtDeviceSpace(instance.exports);
    this.execution_epoch = 0;
    this.create_jit_imports();
};

/** What generated page functions import (module "e"): the core's exports, its memory and, with tail calls, its table */
A64Machine.prototype.create_jit_imports = function()
{
    const jit_imports = Object.create(null);
    jit_imports["m"] = this.wm.exports["memory"];
    for(const name of Object.keys(this.wm.exports))
    {
        if(name.startsWith("_") || name.startsWith("zstd") || name.endsWith("_js")) continue;
        jit_imports[name] = this.wm.exports[name];
    }
    // (module (func (return_call 0)))
    const probe = new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0, 1, 4, 1, 96, 0, 0, 3, 2, 1, 0, 10, 6, 1, 4, 0, 18, 0, 11]);
    const supported = WebAssembly.validate(probe);
    if(supported) jit_imports["t"] = this.wm.wasm_table;
    this.wm.exports["a64_page_set_chaining"]?.(supported ? 1 : 0);
    this.wm.exports["a64_page_set_relaxed_fma_fused"]?.(0);
    this.jit_imports = jit_imports;
};

/** (env.jit_publish) the module of a page function: instantiated, then its table slot set if the core still wants it */
A64Machine.prototype.jit_publish = function(client, id, slot, pointer, length)
{
    const wasm = this.wm, exports = wasm.exports, table = wasm.wasm_table;
    const epoch = this.execution_epoch;
    const code = new Uint8Array(exports["memory"].buffer, pointer >>> 0, length >>> 0).slice();
    const current = () => this.execution_epoch === epoch && this.wm === wasm;
    const cancel = () => exports["jit_cancel"](client, id, slot);
    const set = instance => {
        if(!current()) return false;
        const f = instance.exports["f"];
        if(typeof f !== "function" || !exports["jit_install"](client, id, slot))
        {
            cancel();
            return false;
        }
        table.set(slot + WASM_TABLE_OFFSET, f);
        exports["jit_ready"](client, id, slot);
        return true;
    };
    if(this.options.ir_sync_publication)
    {
        try
        {
            return Promise.resolve(set(new WebAssembly.Instance(new WebAssembly.Module(code), { "e": this.jit_imports })));
        }
        catch(error)
        {
            if(error instanceof WebAssembly.CompileError || error instanceof WebAssembly.LinkError)
            {
                if(current()) cancel();
                return Promise.resolve(false);
            }
            throw error;
        }
    }
    return WebAssembly.instantiate(code, { "e": this.jit_imports }).then(result => set(result.instance), () => {
        if(current()) cancel();
        return false;
    });
};

/** (env.jit_clear_func) a slot of a retired function */
A64Machine.prototype.jit_clear_func = function(index)
{
    this.wm.wasm_table.set(index + WASM_TABLE_OFFSET, null);
};

A64Machine.prototype.init = function(options)
{
    const exports = this.wm.exports;
    const { cores, memory_size } = virt_machine({ cpu_cores: options.cpu_cores, memory_size: options.memory_size });
    this.cores = cores;
    // the page tier (the JIT for AArch64) is on unless the option jit_switches turns it off ({ a64_page: 0 })
    this.jit_switches = Object.assign({ "a64_page": 1 }, options.jit_switches);
    // cpu_type "arm32": AArch32 throughout (a 32-bit kernel: a zImage, or a raw binary)
    this.cpu_type = options.cpu_type === "arm32" ? "arm32" : "arm64";
    const profile = options.a64_profile || "v2";
    if(!Object.prototype.hasOwnProperty.call(A64_PROFILES, profile))
    {
        throw new Error("a64_profile: " + Object.keys(A64_PROFILES).join(", ") + ", got " + JSON.stringify(profile));
    }
    set_jit_switches(exports, this.wasm_memory, this.jit_switches);
    const entry = this.cpu_type === "arm32" ? ARM32_PROFILE : A64_PROFILES[profile];
    exports["a64_set_granules"](entry[1]);
    exports["a64_set_features"](entry[2]);
    exports["a64_gic_set_version"]?.(3);
    if(!exports["a64_set_cores"](cores)) throw new Error("the GIC's frames overlap a device");
    // guest RAM at 0x4000_0000, one allocation in the 32-bit memory
    if(memory_size >= 0x80000000) throw new Error("memory_size: the ARM core takes less than 2 GiB of RAM");
    // (UEFI: the firmware's code is read-only guest memory after the RAM)
    const firmware = options.firmware;
    const code = firmware ? new Uint8Array(firmware.code) : null;
    if(code && (code.length > VIRT_FLASH_SIZE || !code.length)) throw new Error("firmware.code: at most 64 MiB");
    const code_size = code ? Math.ceil(code.length / 0x10000) * 0x10000 : 0;
    const pointer = exports["a64_allocate_memory"](memory_size + code_size) >>> 0;
    if(!exports["a64_bus_set_ram"](memory_size)) throw new Error("RAM of " + memory_size + " bytes does not fit");
    this.ram_pointer = pointer;
    this.mem_buffer = this.wasm_memory.buffer;
    this.mem8 = new Uint8Array(this.mem_buffer, pointer, memory_size);
    this.memory_size[0] = memory_size;
    this.devices = {
        "pl011": new PL011(this, this.bus),
        "pl031": new PL031(this),
    };
    if(firmware)
    {
        new Uint8Array(this.wasm_memory.buffer, pointer + memory_size, code_size).set(code);
        if(!exports["a64_bus_map_rom"](0, 0, code_size, 0, memory_size)) throw new Error("the flash's ROM does not map");
        // (the rest of the code's unit reads as the erased padding of QEMU's image: zeros)
        if(code_size < VIRT_FLASH_SIZE)
        {
            this.io.register_mmio(code_size, VIRT_FLASH_SIZE - code_size, { name: "pflash0", read: (o, w) => w === 8 ? 0n : 0, write: () => {} });
        }
        const vars = firmware.vars instanceof Uint8Array ? firmware.vars : new Uint8Array(firmware.vars || new ArrayBuffer(0));
        this.devices["pflash1"] = new CfiFlash(vars);
        this.io.register_mmio(VIRT_FLASH_SIZE, VIRT_FLASH_SIZE, {
            name: "pflash1",
            read: (offset, width) => this.devices["pflash1"].read(offset, width),
            write: (offset, width, value) => { this.devices["pflash1"].write(offset, width, value); },
        });
        this.devices["fw_cfg"] = new FwCfg({ cores });
        this.io.register_mmio(VIRT_FW_CFG, VIRT_FW_CFG_SIZE, {
            name: "fw_cfg",
            read: (offset, width) => this.devices["fw_cfg"].read(offset, width),
            write: (offset, width, value) => { this.devices["fw_cfg"].write(offset, width, value); },
        });
    }
    // The generic PCIe host the device tree describes, with no function on it: configuration reads give all ones
    // (no device), as the reference's host does for an empty slot
    this.io.register_mmio(VIRT_PCIE_ECAM_HIGH, VIRT_PCIE_ECAM_SIZE, {
        name: "pcie-ecam",
        read: (offset, width) => width === 8 ? 0xFFFFFFFFFFFFFFFFn : 2 ** (8 * width) - 1,
        write: () => {},
    });
    // virtio-mmio slots: a slot without a device reads as the reference's empty one; devices take the slots from
    // the last one down (the disks, then rng)
    this.virtio_mmio = new Array(VIRT_MMIO_COUNT).fill(null);
    for(let n = 0; n < VIRT_MMIO_COUNT; n++)
    {
        const slot = n;
        this.io.register_mmio(VIRT_MMIO + n * VIRT_MMIO_SIZE, VIRT_MMIO_SIZE, {
            name: "virtio-mmio",
            read: (offset, width) => this.virtio_mmio[slot] ? this.virtio_mmio[slot].read(offset, width) :
                width === 4 ? [0x74726976, 2, 0, 0x554D4551][offset >> 2] ?? 0 : 0,
            write: (offset, width, value) => { this.virtio_mmio[slot]?.write(offset, width, value); },
        });
    }
    const add = create =>
    {
        const slot = VIRT_MMIO_COUNT - 1 - this.virtio.length;
        const device = create(slot, 32 + 16 + slot);
        this.virtio_mmio[slot] = device;
        this.virtio.push(device);
    };
    for(const disk of options.disks || []) add((slot, intid) => new VirtioMmioBlk(this, intid, disk));
    // (net_device: { mac }: the frames are net0-send and net0-receive events on machine.bus)
    if(options.net_device) add((slot, intid) => new VirtioMmioNet(this, intid, options.net_device));
    if(options.virtio_rng) add((slot, intid) => new VirtioMmioRng(this, intid));
    if(firmware)
    {
        this.boot_files = { firmware: true, dtb: options.dtb, cores };
        this.load_boot();
    }
    else if(options.kernel)
    {
        this.boot_files = { kernel: options.kernel, initrd: options.initrd, cmdline: options.cmdline, dtb: options.dtb, cores };
        this.load_boot();
    }
};

/**
 * Load what boots (again, at a reset) and start core 0: the kernel, or with UEFI firmware the device tree at RAM's
 * base, where edk2's ArmVirtQemu looks for it (QEMU puts it there when it boots firmware), and the reset vector at 0
 */
A64Machine.prototype.load_boot = function()
{
    if(this.boot_files.firmware)
    {
        const rng_seed = new Uint8Array(32);
        globalThis.crypto?.getRandomValues?.(rng_seed);
        const dtb = this.boot_files.dtb ? new Uint8Array(this.boot_files.dtb) :
            flatten(virt_tree({ cores: this.cores, memory_size: this.memory_size[0], rng_seed, uefi: true }));
        this.write_blob_physical(dtb, VIRT_RAM_BASE);
        this.boot(0, 0);
        return;
    }
    const { entry, x0 } = load_linux(this, this.boot_files);
    this.boot(entry, x0);
};

A64Machine.prototype.boot = function(entry, x0)
{
    if(this.cpu_type === "arm32")
    {
        // (a 32-bit kernel: SVC mode, r0 0, r1 ~0 (a device tree, not a board), r2 the device tree)
        this.wm.exports["a32_boot"](entry >>> 0, x0 >>> 0);
    }
    else
    {
        this.wm.exports["a64_boot"](entry >>> 0, Math.floor(entry / 0x100000000), x0 >>> 0, Math.floor(x0 / 0x100000000));
    }
    this.boot_entry = entry;
    this.boot_x0 = x0;
    this.booted = true;
};

/** guest RAM as a view (the memory may have grown since the last one: page functions allocate) */
A64Machine.prototype.ram8 = function()
{
    if(this.mem_buffer !== this.wasm_memory.buffer)
    {
        this.mem_buffer = this.wasm_memory.buffer;
        this.mem8 = new Uint8Array(this.mem_buffer, this.ram_pointer, this.memory_size[0]);
    }
    return this.mem8;
};

/** (the loader) write into guest RAM */
A64Machine.prototype.write_blob_physical = function(blob, address)
{
    const offset = address - VIRT_RAM_BASE;
    if(offset < 0 || offset + blob.length > this.memory_size[0]) throw new RangeError("write_blob_physical: not RAM");
    this.ram8().set(blob, offset);
    this.wm.exports["a64_bus_written"](offset, blob.length);
};

/** The RAM at [address, address + length) as a view, or null when it is not all RAM */
A64Machine.prototype.ram = function(address, length)
{
    const offset = address - VIRT_RAM_BASE;
    return offset >= 0 && offset + length <= this.memory_size[0] ? this.ram8().subarray(offset, offset + length) : null;
};

/** A device wrote guest RAM: compiled code of those pages is stale */
A64Machine.prototype.ram_written = function(address, length)
{
    this.wm.exports["a64_bus_written"](address - VIRT_RAM_BASE, length);
};

/** A device's interrupt line, an SPI of the GIC */
A64Machine.prototype.set_irq = function(intid, level)
{
    this.wm.exports["a64_gic_set_spi"](intid, level);
};

A64Machine.prototype.add_listener = function(name, fn)
{
    this.bus.register(name, fn);
};

/** Type into the serial port */
A64Machine.prototype.serial0_send = function(text)
{
    for(let i = 0; i < text.length; i++) this.bus.send("serial0-input", text.charCodeAt(i));
};

A64Machine.prototype.reset = function()
{
    if(!this.booted) return;
    for(const device of Object.values(this.devices)) device.reset?.();
    for(const device of this.virtio) device.reset();
    this.load_boot();
};

/** One turn: the cores' slices and the devices' timers; milliseconds until the machine needs to run again */
/**
 * Cores 1 and up run in vCPU workers (the parallel build, build/v86-arm64-parallel.wasm); this thread runs core 0 and
 * the devices, serves the workers' device accesses and stops them at safe points
 * @param {{bytes: Uint8Array, worker_url: (string|URL)}} options the parallel core's bytes, the worker's entry
 */
A64Machine.prototype.start_parallel = async function(options)
{
    this.core_state_ranges = CORE_STATE_RANGES;
    this.parallel = new ParallelMachine(this, { bytes: options.bytes, settings: { "jit_switches": this.jit_switches || {} }, worker_url: options.worker_url });
    await this.parallel.start();
};

/** A worker kicked core 0 (I/O, parking, an IPI): run the machine's loop now */
A64Machine.prototype.stop_idling = function()
{
    if(!this.running || this.stopped) return;
    if(this.timer) { clearTimeout(this.timer); this.timer = null; }
    if(this.step) yield_to_host(this.step);
};

/** One turn of a machine with cores in workers: the workers' device accesses, core 0's slice, the workers' stops */
A64Machine.prototype.run_parallel = function()
{
    const machine = this.parallel, exports = this.wm.exports;
    if(machine.check_failure())
    {
        this.bus.send("emulator-error", machine.failure);
        this.stopped = true;
        return 100;
    }
    machine.service();
    if(machine.pending_reset)
    {
        machine.request_stop();
        if(!machine.all_parked()) return 0;
        machine.pending_reset = null;
        this.reboot_workers();
        return 0;
    }
    if(machine.stopping()) machine.resume();
    exports["parallel_idle"](false);
    const t = this.run_machine_core();
    machine.service();
    if(t > 0) exports["parallel_idle"](true);
    return t;
};

A64Machine.prototype.reboot_workers = function()
{
    this.parallel.schedule(COMMAND_RESET);
    for(const device of Object.values(this.devices)) device.reset?.();
    for(const device of this.virtio) device.reset();
    this.load_boot();
};

A64Machine.prototype.run_once = function()
{
    if(this.parallel) return this.run_parallel();
    return this.run_machine_core();
};

/** One slice of the cores this instance runs: milliseconds until it needs to run again */
A64Machine.prototype.run_machine_core = function()
{
    const exports = this.wm.exports;
    if(!this.booted) return 100;
    const devices = this.devices["pl031"].timer();
    const next = Math.min(exports["a64_run_slice"](this.clock.now()), devices);
    if(exports["a64_take_reset_request"]())
    {
        if(this.parallel)
        {
            this.parallel.pending_reset = { reason: "restart" };
            this.parallel.request_stop();
        }
        else this.reset();
    }
    return next;
};

A64Machine.prototype.run = function()
{
    if(this.running) return;
    this.running = true;
    this.stopped = false;
    const step = this.step = () => {
        this.timer = null;
        if(this.stopped) { this.running = false; return; }
        if(this.wm.exports["a64_powered_off"]?.())
        {
            this.running = false;
            this.bus.send("emulator-stopped");
            return;
        }
        const wait = this.run_once();
        if(wait > 1) this.timer = setTimeout(step, Math.min(wait, 16));
        else yield_to_host(step);
    };
    step();
};

A64Machine.prototype.stop = function()
{
    this.stopped = true;
    this.parallel?.destroy?.();
    if(this.timer) clearTimeout(this.timer);
    this.running = false;
};

/** retired instructions of core 0 (the core's counters) */
A64Machine.prototype.retired_instructions = function()
{
    const exports = this.wm.exports;
    return exports["core_statistics_get"] ? Number(exports["core_statistics_get"](0, 0)) : 0;
};
