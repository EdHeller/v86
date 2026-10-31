// A display adapter's virtio function (virtio-gpu as virtio-vga), from v64's
// src/virtio_devices.js: v86 owns the transport (the PCI function, its
// capabilities, the rings, the interrupt), the plugin's descriptor owns what
// the device does. Descriptors come from plugin code that Closure does not
// compile, so every field and every method of the handle uses a quoted name.

import {
    VirtIO, VIRTIO_F_VERSION_1, VIRTIO_STATUS_DRIVER_OK,
    VIRTIO_STATUS_DEVICE_NEEDS_RESET, VIRTIO_STATUS_FAILED, VirtQueueBufferChain,
} from "./virtio.js";

/** log the RangeErrors that make a device need a reset */
const DEBUG_VIRTIO_DEVICES = typeof process !== "undefined" && !!(process.env && process.env.DEBUG_VIRTIO_DEVICES);

// For Types Only
import { CPU } from "./cpu.js";

/**
 * @param {*} descriptor
 */
function check_adapter_descriptor(descriptor)
{
    const name = descriptor && descriptor["name"];
    const fail = message => { throw new TypeError("virtio device \"" + name + "\": " + message); };
    if(!descriptor || typeof descriptor !== "object" || typeof name !== "string" || !name) fail("a descriptor needs a name");
    const queues = descriptor["queues"];
    if(!Array.isArray(queues) || !queues.length) fail("queues must be a non-empty array");
    if(typeof descriptor["notify"] !== "function") fail("notify must be a function");
}

/**
 * A display adapter's virtio function (virtio-vga, in the VGA slot): the
 * descriptor's fields as for virtio_devices, plus "class_code", "revision",
 * the adapter's own memory BARs ("bars": quoted { bar, size, address,
 * prefetchable, on_move }), "capability_bar", the one memory BAR that holds
 * all four capabilities at their windows' offsets (viogpudo maps no I/O
 * BARs), the VGA BIOS as its
 * expansion ROM ("pci_rom_size", "pci_rom_address"), and "qemu_compatible"
 * (see VirtIO.qemu_compatible). Its state belongs to the
 * adapter's snapshot entry (the handle's "transport" and "set_transport").
 * @param {CPU} cpu
 * @param {*} descriptor
 * @param {number} pci_id
 * @return {!VirtioDevice}
 */
export function create_adapter_virtio_device(cpu, descriptor, pci_id)
{
    check_adapter_descriptor(descriptor);
    const bars = descriptor["bars"] || [];
    if(!Array.isArray(bars) || bars.some(bar => !bar || !Number.isInteger(bar["bar"]) || bar["bar"] < 0 || bar["bar"] > 5 ||
        !Number.isInteger(bar["size"]) || bar["size"] < 16 || (bar["size"] & (bar["size"] - 1))))
    {
        throw new TypeError("virtio device \"" + descriptor["name"] + "\": bars must be { bar, size (a power of two) }");
    }
    // (virtio-gpu's host visible memory: a region of one of those BARs)
    const shared_memory = descriptor["shared_memory"] || [];
    if(!Array.isArray(shared_memory) || shared_memory.some(region => !region ||
        !Number.isInteger(region["id"]) || region["id"] < 0 || region["id"] > 0xFF ||
        !bars.some(bar => bar["bar"] === region["bar"] && (region["offset"] || 0) + region["length"] <= bar["size"])))
    {
        throw new TypeError("virtio device \"" + descriptor["name"] + "\": shared_memory must be { id, bar, offset, length } within one of the bars");
    }
    // (no I/O ports: the windows' offsets in the memory BAR are 0x000, 0x100, 0x200, 0x300)
    return new VirtioDevice(cpu, /** @type {!Object} */ (descriptor), pci_id, 0, {
        class_code: descriptor["class_code"],
        revision: descriptor["revision"],
        bars: bars.map(bar => ({
            bar: bar["bar"],
            size: bar["size"],
            address: bar["address"] || 0,
            prefetchable: !!bar["prefetchable"],
            on_move: bar["on_move"],
        })),
        capability_bar: descriptor["capability_bar"],
        shared_memory: shared_memory.map(region => ({
            id: region["id"],
            bar: region["bar"],
            offset: region["offset"] || 0,
            length: region["length"],
        })),
        rom_size: descriptor["pci_rom_size"],
        rom_address: descriptor["pci_rom_address"],
        qemu_compatible: !!descriptor["qemu_compatible"],

    });
}

/**
 * @constructor
 * @param {CPU} cpu
 * @param {!Object} descriptor
 * @param {number} pci_id its function (behind a root port: device 0 of the port's bus)
 * @param {number} io_base
 * @param {{class_code: (number|undefined), revision: (number|undefined), bars: !Array,
 *     capability_bar: (number|undefined), shared_memory: (!Array|undefined),
 *     rom_size: (number|undefined), rom_address: (number|undefined),
 *     qemu_compatible: (boolean|undefined), transport: (?|undefined)}=} transport
 *     a display adapter's layout (create_adapter_virtio_device); its
 *     transport: the virt machine's virtio-mmio instead of the PCI function
 */
function VirtioDevice(cpu, descriptor, pci_id, io_base, transport)
{
    /** @const @type {CPU} */
    this.cpu = cpu;

    this.descriptor = descriptor;
    this.name = descriptor["name"];

    // Bumped by resets and restores: requests popped before one must not
    // complete into the new rings
    this.generation = 0;
    this.initialized = false;

    // Set by the embedder before a save, read by it after a restore
    this.host_state = undefined;
    this.restored_host_state = undefined;

    const features = descriptor["features"] || [];
    const config = descriptor["config"] || [];

    /** @type {VirtIO} */
    this.virtio = new VirtIO(cpu,
    {
        name: "virtio-" + this.name,
        pci_id,
        device_id: descriptor["device_id"],
        subsystem_device_id: descriptor["subsystem_device_id"] || 0,
        class_code: transport && transport.class_code,
        revision: transport && transport.revision,
        bars: transport && transport.bars,
        capability_bar: transport && transport.capability_bar,
        shared_memory: transport && transport.shared_memory,
        rom_size: transport && transport.rom_size,
        rom_address: transport && transport.rom_address,
        on_reset: () =>
        {
            this.generation++;
            const reset = this.descriptor["reset"];
            if(this.initialized && reset) reset();
        },
        common:
        {
            initial_port: io_base,
            queues: descriptor["queues"].map(queue => ({ size_supported: queue["size"], notify_offset: 0 })),
            features: features.includes(VIRTIO_F_VERSION_1) ? features : features.concat([VIRTIO_F_VERSION_1]),
            on_driver_ok: () => {},
        },
        notification:
        {
            initial_port: io_base + 0x100,
            single_handler: true,
            handlers: [
                (queue_id) => this.notify(queue_id),
            ],
        },
        isr_status:
        {
            initial_port: io_base + 0x200,
        },
        device_specific: config.length ? {
            initial_port: io_base + 0x300,
            struct: config.map((field, i) => ({
                bytes: field["bytes"],
                name: field["name"] || "config" + i,
                // (port reads are int32, or the field's width)
                read: field["bytes"] === 4 ? () => field["read"]() | 0 :
                    () => field["read"]() & (field["bytes"] === 2 ? 0xFFFF : 0xFF),
                write: data => { field["write"] && field["write"](data); },
            })),
        } : undefined,
    });

    this.handle = this.create_handle();
    this.initialized = true;

    const init = descriptor["init"];
    init && init(this.handle);
}

/**
 * What the descriptor gets to work with
 */
VirtioDevice.prototype.create_handle = function()
{
    return {
        "name": this.name,
        "is_ready": () => this.is_ready(),
        "has_request": queue_id => this.has_request(queue_id),
        "pop_request": queue_id => this.pop_request(queue_id),
        "request_at": (queue_id, head) => this.request_at(queue_id, head),
        "flush": queue_id => this.flush(queue_id),
        "is_ram": (address, length) => this.is_ram(address, length),
        "read_memory": (address, length) => this.read_memory(address, length),
        "write_memory": (bytes, address) => this.write_memory(bytes, address),
        "is_feature_negotiated": bit => this.virtio.is_feature_negotiated(bit),
        "needs_reset": () => this.virtio.needs_reset(),
        "config_changed": () => { if(this.is_ready()) this.virtio.notify_config_changes(); },
        // (a display adapter's function: its transport in the adapter's
        // snapshot entry, and the adapter's resets)
        "transport": () => this.virtio,
        "set_transport": state => { this.generation++; this.virtio.set_state(state); },
        "reset": () => this.reset(),
    };
};

/**
 * The driver has set DRIVER_OK, and neither side has given up
 */
VirtioDevice.prototype.is_ready = function()
{
    const status = this.virtio.device_status;
    return (status & (VIRTIO_STATUS_DRIVER_OK | VIRTIO_STATUS_DEVICE_NEEDS_RESET | VIRTIO_STATUS_FAILED)) ===
        VIRTIO_STATUS_DRIVER_OK;
};

VirtioDevice.prototype.notify = function(queue_id)
{
    if(!this.is_ready() || queue_id >= this.virtio.queues.length) return;
    try
    {
        this.descriptor["notify"](queue_id);
    }
    catch(error)
    {
        // A RangeError is a malformed ring or buffer; anything else is a bug in the device
        if(!(error instanceof RangeError) || DEBUG_VIRTIO_DEVICES) console.error("virtio device " + this.name + ":", error);
        this.virtio.needs_reset();
    }
};

VirtioDevice.prototype.get_queue = function(queue_id)
{
    const queue = this.virtio.queues[queue_id];
    if(!queue) throw new Error("virtio device " + this.name + " has no queue " + queue_id);
    return queue;
};

VirtioDevice.prototype.has_request = function(queue_id)
{
    const queue = this.get_queue(queue_id);
    if(!this.is_ready() || !queue.is_configured()) return false;
    // The rings, like every buffer, are DMA: guest RAM, never a device's window
    if(!this.is_ram(queue.desc_addr, queue.size * 16) ||
        !this.is_ram(queue.avail_addr, 6 + queue.size * 2) ||
        !this.is_ram(queue.used_addr, 6 + queue.size * 8))
    {
        this.virtio.needs_reset();
        return false;
    }
    return queue.has_request();
};

/**
 * @return {Object} a request, or null when there is none or the chain is malformed
 * (the driver has been told to reset then)
 */
VirtioDevice.prototype.pop_request = function(queue_id)
{
    const queue = this.get_queue(queue_id);
    if(!this.has_request(queue_id)) return null;

    let chain;
    try
    {
        chain = queue.pop_request();
    }
    catch(error)
    {
        if(!(error instanceof RangeError)) throw error;
        this.virtio.needs_reset();
        return null;
    }
    return this.request(queue, chain);
};

/**
 * A request popped before, again: its chain from its head (a snapshot keeps
 * the heads of the requests a device had not answered yet)
 * @return {Object} null when the chain is malformed
 */
VirtioDevice.prototype.request_at = function(queue_id, head)
{
    const queue = this.get_queue(queue_id);
    if(!this.is_ready() || !queue.is_configured() || !(head >= 0 && head < queue.size)) return null;
    return this.request(queue, new VirtQueueBufferChain(queue, head));
};

/**
 * @param {!Object} queue its VirtQueue
 * @param {!VirtQueueBufferChain} chain
 * @return {Object}
 */
VirtioDevice.prototype.request = function(queue, chain)
{
    if(chain.valid === false) return null;
    for(const buffer of chain.read_buffers.concat(chain.write_buffers))
    {
        // (v86's descriptors: addr_low/addr_high; guest RAM is below 4 GiB)
        if(buffer.len && (buffer.addr_high || !this.is_ram(buffer.addr_low >>> 0, buffer.len >>> 0)))
        {
            this.virtio.needs_reset();
            return null;
        }
    }

    const generation = this.generation;
    let completed = false;
    return {
        "head": chain.head_idx,
        "readable": chain.length_readable,
        "writable": chain.length_writable,
        "read": () =>
        {
            const bytes = new Uint8Array(chain.length_readable);
            chain.get_next_blob(bytes);
            return bytes;
        },
        "write": bytes =>
        {
            return generation === this.generation ? chain.set_next_blob(bytes) : 0;
        },
        // Staged until flush
        "complete": () =>
        {
            if(completed || generation !== this.generation) return;
            completed = true;
            queue.push_reply(chain);
        },
    };
};

VirtioDevice.prototype.flush = function(queue_id)
{
    const queue = this.get_queue(queue_id);
    if(queue.is_configured() && queue.num_staged_replies) queue.flush_replies();
};

/**
 * Plain or extended RAM on the physical bus: not a device window, not the
 * legacy VGA/ROM range and not a hole
 * @param {number} address
 * @param {number} length
 */
VirtioDevice.prototype.is_ram = function(address, length)
{
    return this.classify(address, length) !== -2;
};

/**
 * @param {number} address
 * @param {number} length
 * @return {number} the offset of one contiguous plain-RAM backing,
 *     -1 for other RAM, -2 for anything that is not RAM
 */
VirtioDevice.prototype.classify = function(address, length)
{
    // (guest RAM below its end, outside the VGA window and the ROMs at 0xA0000-0xFFFFF)
    const end = address + length;
    if(!(address >= 0 && length >= 0 && end <= this.cpu.memory_size[0])) return -2;
    if(address < 0x100000 && end > 0xA0000) return -2;
    return address;
};

/**
 * @param {number} address
 * @param {number} length
 * @return {!Uint8Array} a view of guest RAM where one contiguous backing
 *     holds the range, a copy otherwise
 */
VirtioDevice.prototype.read_memory = function(address, length)
{
    const backing = this.classify(address, length);
    if(backing === -2)
    {
        throw new RangeError("virtio device " + this.name + ": 0x" + address.toString(16) +
            " (" + length + " bytes) is not guest RAM");
    }
    return this.cpu.mem8.subarray(backing, backing + length);
};

/**
 * Through the bus, which invalidates compiled code
 * @param {!Uint8Array} bytes
 * @param {number} address
 */
VirtioDevice.prototype.write_memory = function(bytes, address)
{
    if(!bytes.length) return;
    if(!this.is_ram(address, bytes.length))
    {
        throw new RangeError("virtio device " + this.name + ": 0x" + address.toString(16) +
            " (" + bytes.length + " bytes) is not guest RAM");
    }
    this.cpu.write_blob(bytes, address);
};

/**
 * What the device still waits for from its host side (its optional busy hook):
 * a snapshot needs none
 */
VirtioDevice.prototype.busy = function()
{
    const busy = this.descriptor["busy"];
    return busy ? busy() : 0;
};

/**
 * A machine reset; also used when a snapshot restores without this device
 */
VirtioDevice.prototype.reset = function()
{
    this.virtio.reset();
};

VirtioDevice.prototype.get_state = function()
{
    const get_state = this.descriptor["get_state"];
    return [this.name, this.virtio, get_state ? get_state() : undefined, this.host_state];
};

/**
 * @param {!Array} entry [name, transport state, device state, host state]
 */
VirtioDevice.prototype.set_state = function(entry)
{
    this.generation++;
    this.virtio.set_state(entry[1]);
    const set_state = this.descriptor["set_state"];
    set_state && set_state(entry[2]);
    this.restored_host_state = entry[3];
};
