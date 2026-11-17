import { LOG_VIRTIO, MMAP_BLOCK_SIZE } from "./const.js";
import { h, int_log2 } from "./lib.js";
import { dbg_assert, dbg_log } from "./log.js";

// For Types Only
import { CPU } from "./cpu.js";
import { PCI } from "./pci.js";

// http://docs.oasis-open.org/virtio/virtio/v1.0/virtio-v1.0.html

const VIRTIO_PCI_VENDOR_ID = 0x1AF4;
// Identifies vendor-specific PCI capability.
const VIRTIO_PCI_CAP_VENDOR = 0x09;
// Length (bytes) of VIRTIO_PCI_CAP linked list entry.
const VIRTIO_PCI_CAP_LENGTH = 16;

// Capability types.

const VIRTIO_PCI_CAP_COMMON_CFG = 1;
const VIRTIO_PCI_CAP_NOTIFY_CFG = 2;
const VIRTIO_PCI_CAP_ISR_CFG = 3;
const VIRTIO_PCI_CAP_DEVICE_CFG = 4;
const VIRTIO_PCI_CAP_PCI_CFG = 5;

// Status bits (device_status values).

const VIRTIO_STATUS_ACKNOWLEDGE = 1;
const VIRTIO_STATUS_DRIVER = 2;
export const VIRTIO_STATUS_DRIVER_OK = 4;
const VIRTIO_STATUS_FEATURES_OK = 8;
export const VIRTIO_STATUS_DEVICE_NEEDS_RESET = 64;
export const VIRTIO_STATUS_FAILED = 128;
const VIRTIO_PCI_CAP_SHARED_MEMORY_CFG = 8;

// ISR bits (isr_status values).

const VIRTIO_ISR_QUEUE = 1;
const VIRTIO_ISR_DEVICE_CFG = 2;

// Feature bits (bit positions).

export const VIRTIO_F_RING_INDIRECT_DESC = 28;
export const VIRTIO_F_RING_EVENT_IDX = 29;
export const VIRTIO_F_VERSION_1 = 32;

// Queue struct sizes.

// Size (bytes) of the virtq_desc struct per queue size.
const VIRTQ_DESC_ENTRYSIZE = 16;
// Size (bytes) of the virtq_avail struct ignoring ring entries.
const VIRTQ_AVAIL_BASESIZE = 6;
// Size (bytes) of the virtq_avail struct per queue size.
const VIRTQ_AVAIL_ENTRYSIZE = 2;
// Size (bytes) of the virtq_used struct ignoring ring entries.
const VIRTQ_USED_BASESIZE = 6;
// Size (bytes) of the virtq_desc struct per queue size.
const VIRTQ_USED_ENTRYSIZE = 8;
// Mask for wrapping the idx field of the virtq_used struct so that the value
// naturally overflows after 65535 (idx is a word).
const VIRTQ_IDX_MASK = 0xFFFF;

// Queue flags.

const VIRTQ_DESC_F_NEXT = 1;
const VIRTQ_DESC_F_WRITE = 2;
const VIRTQ_DESC_F_INDIRECT = 4;
const VIRTQ_AVAIL_F_NO_INTERRUPT = 1;
const VIRTQ_USED_F_NO_NOTIFY = 1;

// Closure Compiler Types.

/**
 * @typedef {!Array<{
 *     bytes: number,
 *     name: string,
 *     read: function():number,
 *     write: function(number)
 * }>}
 */
var VirtIO_CapabilityStruct;

/**
 * @typedef {
 * {
 *     type: number,
 *     bar: number,
 *     port: number,
 *     use_mmio: boolean,
 *     offset: number,
 *     extra: Uint8Array,
 *     struct: VirtIO_CapabilityStruct,
 * }}
 */
var VirtIO_CapabilityInfo;

/**
 * @typedef {
 * {
 *     size_supported: number,
 *     notify_offset: number,
 * }}
 */
var VirtQueue_Options;

/**
 * @typedef {
 * {
 *     initial_port: number,
 *     queues: !Array<VirtQueue_Options>,
 *     features: !Array<number>,
 *     on_driver_ok: function(),
 * }}
 */
var VirtIO_CommonCapabilityOptions;

/**
 * @typedef {
 * {
 *     initial_port: number,
 *     single_handler: boolean,
 *     handlers: !Array<function()>,
 * }}
 */
var VirtIO_NotificationCapabilityOptions;

/**
 * @typedef {
 * {
 *     initial_port: number,
 * }}
 */
var VirtIO_ISRCapabilityOptions;

/**
 * @typedef {
 * {
 *     initial_port: number,
 *     struct: VirtIO_CapabilityStruct,
 * }}
 */
var VirtIO_DeviceSpecificCapabilityOptions;

/**
 * @typedef {
 * {
 *     name: string,
 *     pci_id: number,
 *     device_id: number,
 *     subsystem_device_id: number,
 *     common: VirtIO_CommonCapabilityOptions,
 *     notification: VirtIO_NotificationCapabilityOptions,
 *     isr_status: VirtIO_ISRCapabilityOptions,
 *     device_specific: (undefined | VirtIO_DeviceSpecificCapabilityOptions),
 *     class_code: (undefined | number),
 *     on_reset: (undefined | function()),
 * }}
 */
var VirtIO_Options;

/**
 * @constructor
 * @param {CPU} cpu
 * @param {VirtIO_Options} options
 */
export function VirtIO(cpu, options)
{
    const io = cpu.io;

    /** @const @type {CPU} */
    this.cpu = cpu;
    this.on_reset = options.on_reset;

    /** @const @type {PCI} */
    this.pci = cpu.devices.pci;

    this.device_id = options.device_id;

    this.pci_space =
    [
        // Vendor ID
        VIRTIO_PCI_VENDOR_ID & 0xFF, VIRTIO_PCI_VENDOR_ID >> 8,
        // Device ID
        options.device_id & 0xFF, options.device_id >> 8,
        // Command
        0x07, 0x05,
        // Status - enable capabilities list
        0x10, 0x00,
        // Revision ID
        options.revision === undefined ? 0x01 : options.revision,
        // Prof IF, Subclass, Class code
        ...(options.class_code === undefined ? [0x00, 0x02, 0x00] :
            [options.class_code & 0xFF, options.class_code >> 8 & 0xFF, options.class_code >> 16 & 0xFF]),
        // Cache line size
        0x00,
        // Latency Timer
        0x00,
        // Header Type
        0x00,
        // Built-in self test
        0x00,
        // BAR0
        0x01, 0xa8, 0x00, 0x00,
        // BAR1
        0x00, 0x10, 0xbf, 0xfe,
        // BAR2
        0x00, 0x00, 0x00, 0x00,
        // BAR3
        0x00, 0x00, 0x00, 0x00,
        // BAR4
        0x00, 0x00, 0x00, 0x00,
        // BAR5
        0x00, 0x00, 0x00, 0x00,
        // CardBus CIS pointer
        0x00, 0x00, 0x00, 0x00,
        // Subsystem vendor ID
        VIRTIO_PCI_VENDOR_ID & 0xFF, VIRTIO_PCI_VENDOR_ID >> 8,
        // Subsystem ID
        options.subsystem_device_id & 0xFF, options.subsystem_device_id >> 8,
        // Expansion ROM base address
        0x00, 0x00, 0x00, 0x00,
        // Capabilities pointer
        0x40,
        // Reserved
        0x00, 0x00, 0x00,
        // Reserved
        0x00, 0x00, 0x00, 0x00,
        // Interrupt line
        0x00,
        // Interrupt pin
        0x01,
        // Min grant
        0x00,
        // Max latency
        0x00,
    ];

    // Prevent sparse arrays by preallocating.
    this.pci_space = this.pci_space.concat(Array(256 - this.pci_space.length).fill(0));
    // Remaining PCI space is appended by capabilities further below.

    this.pci_id = options.pci_id;

    // PCI bars gets filled in by capabilities further below.
    this.pci_bars = [];

    this.name = options.name;

    // Feature bits grouped in dwords, dword selected by decive_feature_select.
    this.device_feature_select = 0;
    this.driver_feature_select = 0;

    // Unspecified upper bound. Assume 4*32=128 bits.
    this.device_feature = new Uint32Array(4);
    this.driver_feature = new Uint32Array(4);
    for(const f of options.common.features)
    {
        dbg_assert(f >= 0,
            "VirtIO device<" + this.name + "> feature bit numbers must be non-negative");
        dbg_assert(f < 128,
            "VirtIO device<" + this.name + "> feature bit numbers assumed less than 128 in implementation");

        // Feature bits are grouped in 32 bits.
        this.device_feature[f >>> 5] |= 1 << (f & 0x1F);
        this.driver_feature[f >>> 5] |= 1 << (f & 0x1F);
    }

    dbg_assert(options.common.features.includes(VIRTIO_F_VERSION_1),
        "VirtIO device<" + this.name + "> only non-transitional devices are supported");

    // Indicates whether driver_feature bits is subset of device_feature bits.
    this.features_ok = true;

    this.device_status = 0;

    this.config_has_changed = false;
    this.config_generation = 0;

    /** @type {!Array<VirtQueue>} */
    this.queues = [];
    for(const queue_options of options.common.queues)
    {
        this.queues.push(new VirtQueue(cpu, this, queue_options));
    }
    this.queue_select = 0;
    this.queue_selected = this.queues[0];

    this.isr_status = 0;

    // Verify notification options.
    if(DEBUG)
    {
        const offsets = new Set();
        for(const offset of this.queues.map(q => q.notify_offset))
        {
            const effective_offset = options.notification.single_handler ? 0 : offset;
            offsets.add(effective_offset);
            dbg_assert(options.notification.handlers[effective_offset],
                "VirtIO device<" + this.name + "> every queue's notifier must exist");
        }
        for(const [index, handler] of options.notification.handlers.entries())
        {
            dbg_assert(!handler || offsets.has(index),
                "VirtIO device<" + this.name +"> no defined notify handler should be unused");
        }
    }

    /** @type {!Array<VirtIO_CapabilityInfo>} */
    const capabilities = [];
    capabilities.push(this.create_common_capability(options.common));
    capabilities.push(this.create_notification_capability(options.notification));
    capabilities.push(this.create_isr_capability(options.isr_status));
    if(options.device_specific)
    {
        capabilities.push(this.create_device_specific_capability(options.device_specific));
    }
    // A display adapter's function (src/virtio_devices.js, from v64): its
    // own memory BARs (a frame buffer), the VGA BIOS as its expansion ROM,
    // and all four capabilities in one memory BAR at their windows' offsets
    // from the common configuration's port (viogpudo maps no I/O BARs)
    for(const bar of options.bars || [])
    {
        this.pci_bars[bar.bar] = { size: bar.size, on_move: bar.on_move };
        const value = (bar.address & ~0xF | (bar.prefetchable ? 8 : 0)) >>> 0;
        for(let i = 0; i < 4; i++) this.pci_space[0x10 + 4 * bar.bar + i] = value >>> 8 * i & 0xFF;
    }
    this.pci_rom_size = options.rom_size || 0;
    this.pci_rom_address = options.rom_address || 0;

    /** the capabilities' memory BAR, when they are in one: its fields by offset, where it is decoded */
    this.mmio_fields = [];
    this.mmio_index = null;
    this.mmio_base = 0;
    this.mmio_size = 0;
    if(options.capability_bar !== undefined)
    {
        const base = options.common.initial_port || 0;
        for(const cap of capabilities)
        {
            cap.offset = cap.port - base;
            cap.port = 0;
            cap.bar = options.capability_bar;
            cap.use_mmio = true;
        }
    }
    this.init_capabilities(capabilities, options.shared_memory || []);

    /** the core's transport (src/rust/cpu/virtio_pci.rs): its index there, or -1 */
    this.core = -1;
    /** @type {Object} the device whose model is the core's (it answers the core's requests for its backend) */
    this.model = null;
    const exports = cpu.wm && cpu.wm.exports;
    if(exports && exports["virtio_pci_new"] && options.capability_bar === undefined && !(cpu.devices.pci && cpu.devices.pci.q35))
    {
        const index = exports["virtio_pci_new"]();
        if(index >= 0) this.use_core(index, options);
    }

    cpu.devices.pci.register_device(this);
    this.reset();
    if(this.core >= 0) exports["virtio_pci_activate"](this.core);
}

// the core's fields of a device (src/rust/cpu/virtio_pci.rs)
const CORE_DEV_FSEL = 0, CORE_DRV_FSEL = 1, CORE_DEV_FEAT = 2, CORE_DRV_FEAT = 6, CORE_FEATURES_OK = 10, CORE_STATUS = 11,
    CORE_CFG_CHANGED = 12, CORE_CFG_GEN = 13, CORE_QSEL = 14, CORE_ISR = 15, CORE_NUM_QUEUES = 16, CORE_BDF = 17,
    CORE_NOTIFY_LEN = 18, CORE_DEVCFG_LEN = 19, CORE_DEVICE_FIELDS = 32, CORE_QUEUE_FIELDS = 10, CORE_FIELDS = 32 + 10 * 16;

/**
 * The transport is the core's: the fields here become accessors of its fields, and it asks this device for what its
 * model has to do (core_event, core_config)
 * @param {number} index
 * @param {!Object} options
 */
VirtIO.prototype.use_core = function(index, options)
{
    const cpu = this.cpu;
    this.core = index;
    const fields = () => new Int32Array(cpu.wasm_memory.buffer, cpu.wm.exports["virtio_pci_fields"](index), CORE_FIELDS);
    this.core_notify = options.notification.handlers;
    this.core_on_driver_ok = options.common.on_driver_ok;
    /** the device-specific configuration's fields with their offsets */
    this.core_devcfg = [];
    let offset = 0;
    for(const field of (options.device_specific ? options.device_specific.struct : []))
    {
        this.core_devcfg.push({ offset, bytes: field.bytes, read: field.read, write: field.write });
        offset += field.bytes;
    }

    const f = fields();
    f.fill(0);
    f[CORE_BDF] = this.pci_id;
    f[CORE_NUM_QUEUES] = this.queues.length;
    f[CORE_NOTIFY_LEN] = 2 * options.notification.handlers.length;
    f[CORE_DEVCFG_LEN] = offset;
    f.set(this.device_feature, CORE_DEV_FEAT);
    f.set(this.driver_feature, CORE_DRV_FEAT);

    const scalar = (name, i, bool) => Object.defineProperty(this, name, {
        get: () => bool ? !!fields()[i] : fields()[i],
        set: value => { fields()[i] = +value; },
    });
    scalar("device_feature_select", CORE_DEV_FSEL, false);
    scalar("driver_feature_select", CORE_DRV_FSEL, false);
    scalar("features_ok", CORE_FEATURES_OK, true);
    scalar("device_status", CORE_STATUS, false);
    scalar("config_has_changed", CORE_CFG_CHANGED, true);
    scalar("config_generation", CORE_CFG_GEN, false);
    scalar("queue_select", CORE_QSEL, false);
    scalar("isr_status", CORE_ISR, false);
    const words = (name, i) => Object.defineProperty(this, name, {
        get: () => new Uint32Array(cpu.wasm_memory.buffer, cpu.wm.exports["virtio_pci_fields"](index) + 4 * i, 4),
        set: value => { new Uint32Array(cpu.wasm_memory.buffer, cpu.wm.exports["virtio_pci_fields"](index) + 4 * i, 4).set(value); },
    });
    words("device_feature", CORE_DEV_FEAT);
    words("driver_feature", CORE_DRV_FEAT);
    Object.defineProperty(this, "queue_selected", {
        get: () => this.queues[this.queue_select] || null,
        set: value => {},
    });
    this.queues.forEach((queue, i) => queue.use_core(fields, CORE_DEVICE_FIELDS + CORE_QUEUE_FIELDS * i));
    cpu.virtio_core_devices[index] = this;
};

/**
 * From the core: a queue notification (value: the written value, the handler's index << 16), a reset, DRIVER_OK
 * @param {number} kind
 * @param {number} value
 */
VirtIO.prototype.core_event = function(kind, value)
{
    if(kind === 0)
    {
        const handler = this.core_notify[value >>> 16];
        if(handler) handler(value & 0xFFFF);
    }
    else if(kind === 1)
    {
        this.reset();
    }
    else if(kind === 2)
    {
        this.core_on_driver_ok();
    }
};

/**
 * From the core: an access to the device-specific configuration (as the I/O handlers' shims: a narrower read takes its
 * part of the field, a wider one the whole field; writes of a whole field)
 * @param {number} offset
 * @param {number} size in bits
 * @param {boolean} write
 * @param {number} value
 * @return {number}
 */
VirtIO.prototype.core_config = function(offset, size, write, value)
{
    const field = this.core_devcfg.find(f => offset >= f.offset && offset < f.offset + f.bytes);
    if(!field) return write ? 0 : (size === 32 ? -1 : (1 << size) - 1);
    if(write)
    {
        if(offset === field.offset && size === 8 * field.bytes) field.write(value);
        return 0;
    }
    const v = field.read();
    if(size >= 8 * field.bytes) return v;
    return v >> ((offset - field.offset) << 3) & (1 << size) - 1;
};

/**
 * @param {VirtIO_CommonCapabilityOptions} options
 * @return {VirtIO_CapabilityInfo}
 */
VirtIO.prototype.create_common_capability = function(options)
{
    return {
        type: VIRTIO_PCI_CAP_COMMON_CFG,
        bar: 0,
        port: options.initial_port,
        use_mmio: false,
        offset: 0,
        extra: new Uint8Array(0),
        struct:
        [
            {
                bytes: 4,
                name: "device_feature_select",
                read: () => this.device_feature_select,
                write: data =>
                {
                    this.device_feature_select = data;
                },
            },
            {
                bytes: 4,
                name: "device_feature",
                read: () => this.device_feature[this.device_feature_select] || 0,
                write: data => { /* read only */ },
            },
            {
                bytes: 4,
                name: "driver_feature_select",
                read: () => this.driver_feature_select,
                write: data =>
                {
                    this.driver_feature_select = data;
                },
            },
            {
                bytes: 4,
                name: "driver_feature",
                read: () => this.driver_feature[this.driver_feature_select] || 0,
                write: data =>
                {
                    const supported_feature = this.device_feature[this.driver_feature_select];

                    if(this.driver_feature_select < this.driver_feature.length)
                    {
                        // Note: only set subset of device_features is set.
                        // Required in our implementation for is_feature_negotiated().
                        this.driver_feature[this.driver_feature_select] = data & supported_feature;
                    }

                    // Check that driver features is an inclusive subset of device features.
                    const invalid_bits = data & ~supported_feature;
                    this.features_ok = this.features_ok && !invalid_bits;
                },
            },
            {
                bytes: 2,
                name: "msix_config",
                read: () =>
                {
                    dbg_log("No msi-x capability supported.", LOG_VIRTIO);
                    return 0xFFFF;
                },
                write: data =>
                {
                    dbg_log("No msi-x capability supported.", LOG_VIRTIO);
                },
            },
            {
                bytes: 2,
                name: "num_queues",
                read: () => this.queues.length,
                write: data => { /* read only */ },
            },
            {
                bytes: 1,
                name: "device_status",
                read: () => this.device_status,
                write: data =>
                {
                    if(data === 0)
                    {
                        dbg_log("Reset device<" + this.name + ">", LOG_VIRTIO);
                        this.reset();
                    }
                    else if(data & VIRTIO_STATUS_FAILED)
                    {
                        dbg_log("Warning: Device<" + this.name + "> status failed", LOG_VIRTIO);
                    }
                    else
                    {
                        dbg_log("Device<" + this.name +"> status: " +
                                ((data & VIRTIO_STATUS_ACKNOWLEDGE) ? "ACKNOWLEDGE " : "") +
                                ((data & VIRTIO_STATUS_DRIVER) ? "DRIVER " : "") +
                                ((data & VIRTIO_STATUS_DRIVER_OK) ? "DRIVER_OK" : "") +
                                ((data & VIRTIO_STATUS_FEATURES_OK) ? "FEATURES_OK " : "") +
                                ((data & VIRTIO_STATUS_DEVICE_NEEDS_RESET) ? "DEVICE_NEEDS_RESET" : ""),
                                LOG_VIRTIO);
                    }

                    if((data & ~this.device_status & VIRTIO_STATUS_DRIVER_OK) &&
                        (this.device_status & VIRTIO_STATUS_DEVICE_NEEDS_RESET))
                    {
                        // We couldn't notify NEEDS_RESET earlier because DRIVER_OK was not set.
                        // Now it has been set, notify now.
                        this.notify_config_changes();
                    }

                    // Don't set FEATURES_OK if our device doesn't support requested features.
                    if(!this.features_ok)
                    {
                        if(DEBUG && (data & VIRTIO_STATUS_FEATURES_OK))
                        {
                            dbg_log("Removing FEATURES_OK", LOG_VIRTIO);
                        }
                        data &= ~VIRTIO_STATUS_FEATURES_OK;
                    }

                    this.device_status = data;

                    if(data & ~this.device_status & VIRTIO_STATUS_DRIVER_OK)
                    {
                        options.on_driver_ok();
                    }
                },
            },
            {
                bytes: 1,
                name: "config_generation",
                read: () => this.config_generation,
                write: data => { /* read only */ },
            },
            {
                bytes: 2,
                name: "queue_select",
                read: () => this.queue_select,
                write: data =>
                {
                    this.queue_select = data;

                    if(this.queue_select < this.queues.length)
                    {
                        this.queue_selected = this.queues[this.queue_select];
                    }
                    else
                    {
                        // Allow queue_select >= num_queues.
                        this.queue_selected = null;
                        // Drivers can then detect that the queue is not available
                        // using the below fields.
                    }
                },
            },
            {
                bytes: 2,
                name: "queue_size",
                read: () => this.queue_selected ? this.queue_selected.size : 0,
                write: data =>
                {
                    if(!this.queue_selected)
                    {
                        return;
                    }
                    if(data & data - 1)
                    {
                        dbg_log("Warning: dev<" + this.name +"> " +
                                "Given queue size was not a power of 2. " +
                                "Rounding up to next power of 2.", LOG_VIRTIO);
                        data = 1 << (int_log2(data - 1) + 1);
                    }
                    if(data > this.queue_selected.size_supported)
                    {
                        dbg_log("Warning: dev<" + this.name +"> " +
                                "Trying to set queue size greater than supported. " +
                                "Clamping to supported size.", LOG_VIRTIO);
                        data = this.queue_selected.size_supported;
                    }
                    this.queue_selected.set_size(data);
                },
            },
            {
                bytes: 2,
                name: "queue_msix_vector",
                read: () =>
                {
                    dbg_log("No msi-x capability supported.", LOG_VIRTIO);
                    return 0xFFFF;
                },
                write: data =>
                {
                    dbg_log("No msi-x capability supported.", LOG_VIRTIO);
                },
            },
            {
                bytes: 2,
                name: "queue_enable",
                read: () => this.queue_selected ? this.queue_selected.enabled | 0 : 0,
                write: data =>
                {
                    if(!this.queue_selected)
                    {
                        return;
                    }
                    if(data === 1)
                    {
                        if(this.queue_selected.is_configured())
                        {
                            this.queue_selected.enable();
                        }
                        else
                        {
                            dbg_log("Driver bug: tried enabling unconfigured queue", LOG_VIRTIO);
                        }
                    }
                    else if(data === 0)
                    {
                        dbg_log("Driver bug: tried writing 0 to queue_enable", LOG_VIRTIO);
                    }
                },
            },
            {
                bytes: 2,
                name: "queue_notify_off",
                read: () => this.queue_selected ? this.queue_selected.notify_offset : 0,
                write: data => { /* read only */ },
            },
            {
                bytes: 4,
                name: "queue_desc (low dword)",
                read: () => this.queue_selected ? this.queue_selected.desc_addr : 0,
                write: data =>
                {
                    if(this.queue_selected) this.queue_selected.desc_addr = data;
                },
            },
            {
                bytes: 4,
                name: "queue_desc (high dword)",
                read: () => 0,
                write: data =>
                {
                    if(data !== 0) dbg_log("Warning: High dword of 64 bit queue_desc ignored:" + data, LOG_VIRTIO);
                },
            },
            {
                bytes: 4,
                name: "queue_avail (low dword)",
                read: () => this.queue_selected ? this.queue_selected.avail_addr : 0,
                write: data =>
                {
                    if(this.queue_selected) this.queue_selected.avail_addr = data;
                },
            },
            {
                bytes: 4,
                name: "queue_avail (high dword)",
                read: () => 0,
                write: data =>
                {
                    if(data !== 0) dbg_log("Warning: High dword of 64 bit queue_avail ignored:" + data, LOG_VIRTIO);
                },
            },
            {
                bytes: 4,
                name: "queue_used (low dword)",
                read: () => this.queue_selected ? this.queue_selected.used_addr : 0,
                write: data =>
                {
                    if(this.queue_selected) this.queue_selected.used_addr = data;
                },
            },
            {
                bytes: 4,
                name: "queue_used (high dword)",
                read: () => 0,
                write: data =>
                {
                    if(data !== 0) dbg_log("Warning: High dword of 64 bit queue_used ignored:" + data, LOG_VIRTIO);
                },
            },
        ],
    };
};

/**
 * @param {VirtIO_NotificationCapabilityOptions} options
 * @return {VirtIO_CapabilityInfo}
 */
VirtIO.prototype.create_notification_capability = function(options)
{
    const notify_struct = [];
    let notify_off_multiplier;

    if(options.single_handler)
    {
        dbg_assert(options.handlers.length === 1,
            "VirtIO device<" + this.name + "> too many notify handlers specified: expected single handler");

        // Forces all queues to use the same address for notifying.
        notify_off_multiplier = 0;
    }
    else
    {
        notify_off_multiplier = 2;
    }

    for(const [i, handler] of options.handlers.entries())
    {
        notify_struct.push(
        {
            bytes: 2,
            name: "notify" + i,
            read: () => 0xFFFF,
            write: handler || (data => {}),
        });
    }

    return {
        type: VIRTIO_PCI_CAP_NOTIFY_CFG,
        bar: 1,
        port: options.initial_port,
        use_mmio: false,
        offset: 0,
        extra: new Uint8Array(
        [
            notify_off_multiplier & 0xFF,
            (notify_off_multiplier >> 8) & 0xFF,
            (notify_off_multiplier >> 16) & 0xFF,
            notify_off_multiplier >> 24,
        ]),
        struct: notify_struct,
    };
};

/**
 * @param {VirtIO_ISRCapabilityOptions} options
 * @return {VirtIO_CapabilityInfo}
 */
VirtIO.prototype.create_isr_capability = function(options)
{
    return {
        type: VIRTIO_PCI_CAP_ISR_CFG,
        bar: 2,
        port: options.initial_port,
        use_mmio: false,
        offset: 0,
        extra: new Uint8Array(0),
        struct:
        [
            {
                bytes: 1,
                name: "isr_status",
                read: () =>
                {
                    const isr_status = this.isr_status;
                    this.lower_irq();
                    return isr_status;
                },
                write: data => { /* read only */ },
            },
        ],
    };
};

/**
 * @param {VirtIO_DeviceSpecificCapabilityOptions} options
 * @return {VirtIO_CapabilityInfo}
 */
VirtIO.prototype.create_device_specific_capability = function(options)
{
    dbg_assert(~options.offset & 0x3,
            "VirtIO device<" + this.name + "> device specific cap offset must be 4-byte aligned");

    return {
        type: VIRTIO_PCI_CAP_DEVICE_CFG,
        bar: 3,
        port: options.initial_port,
        use_mmio: false,
        offset: 0,
        extra: new Uint8Array(0),
        struct: options.struct,
    };
};

/**
 * Writes capabilities into pci_space and hook up IO/MMIO handlers.
 * Call only within constructor.
 * @param {!Array<VirtIO_CapabilityInfo>} capabilities
 */
VirtIO.prototype.init_capabilities = function(capabilities, shared_memory = [])
{
    // (the extent of each BAR: several capabilities may share one)
    const bar_extent = [];
    for(const cap of capabilities)
    {
        const end = cap.offset + cap.struct.reduce((bytes, field) => bytes + field.bytes, 0);
        bar_extent[cap.bar] = Math.max(bar_extent[cap.bar] || 0, end);
    }

    // Next available offset for capabilities linked list.
    let cap_next = this.pci_space[0x34] = 0x40;

    // Current offset.
    let cap_ptr = cap_next;

    for(const cap of capabilities)
    {
        const cap_len = VIRTIO_PCI_CAP_LENGTH + cap.extra.length;

        cap_ptr = cap_next;
        cap_next = cap_ptr + cap_len;

        dbg_assert(cap_next <= 256,
            "VirtIO device<" + this.name + "> can't fit all capabilities into 256byte configspace");

        dbg_assert(0 <= cap.bar && cap.bar < 6,
            "VirtIO device<" + this.name + "> capability invalid bar number");

        let bar_size = bar_extent[cap.bar];

        // Round up to next power of 2,
        // Minimum 16 bytes for its size to be detectable in general (esp. mmio).
        bar_size = bar_size < 16 ? 16 : 1 << (int_log2(bar_size - 1) + 1);
        // (memory is decoded in blocks: the BAR has whole ones to itself)
        if(cap.use_mmio) bar_size = Math.max(bar_size, MMAP_BLOCK_SIZE);
        // (this capability's own length: its struct, or for I/O the whole BAR, as before)
        const cap_size = cap.use_mmio ? cap.struct.reduce((bytes, field) => bytes + field.bytes, 0) : bar_size;

        dbg_assert((cap.port & (bar_size - 1)) === 0,
            "VirtIO device<" + this.name + "> capability port should be aligned to pci bar size");

        this.pci_bars[cap.bar] = cap.use_mmio ?
            { size: bar_size, on_move: base => this.move_mmio(base) } :
            { size: bar_size };
        if(cap.use_mmio) this.mmio_size = bar_size;

        this.pci_space[cap_ptr] = VIRTIO_PCI_CAP_VENDOR;
        this.pci_space[cap_ptr + 1] = cap_next;
        this.pci_space[cap_ptr + 2] = cap_len;
        this.pci_space[cap_ptr + 3] = cap.type;
        this.pci_space[cap_ptr + 4] = cap.bar;

        this.pci_space[cap_ptr + 5] = 0; // Padding.
        this.pci_space[cap_ptr + 6] = 0; // Padding.
        this.pci_space[cap_ptr + 7] = 0; // Padding.

        this.pci_space[cap_ptr + 8] = cap.offset & 0xFF;
        this.pci_space[cap_ptr + 9] = (cap.offset >>> 8) & 0xFF;
        this.pci_space[cap_ptr + 10] = (cap.offset >>> 16) & 0xFF;
        this.pci_space[cap_ptr + 11] = cap.offset >>> 24;

        this.pci_space[cap_ptr + 12] = cap_size & 0xFF;
        this.pci_space[cap_ptr + 13] = (cap_size >>> 8) & 0xFF;
        this.pci_space[cap_ptr + 14] = (cap_size >>> 16) & 0xFF;
        this.pci_space[cap_ptr + 15] = cap_size >>> 24;

        for(const [i, extra_byte] of cap.extra.entries())
        {
            this.pci_space[cap_ptr + 16 + i] = extra_byte;
        }

        const bar_offset = 0x10 + 4 * cap.bar;
        this.pci_space[bar_offset] = (cap.port & 0xFE) | !cap.use_mmio;
        this.pci_space[bar_offset + 1] = (cap.port >>> 8) & 0xFF;
        this.pci_space[bar_offset + 2] = (cap.port >>> 16) & 0xFF;
        this.pci_space[bar_offset + 3] = (cap.port >>> 24) & 0xFF;

        let port = cap.port + cap.offset;

        for(const field of cap.struct)
        {
            let read = field.read;
            let write = field.write;

            if(DEBUG)
            {
                read = () =>
                {
                    const val = field.read();

                    dbg_log("Device<" + this.name + "> " +
                            "cap[" + cap.type + "] " +
                            "read[" + field.name + "] " +
                            "=> " + h(val, field.bytes * 8),
                        LOG_VIRTIO);

                    return val;
                };
                write = data =>
                {
                    dbg_log("Device<" + this.name + "> " +
                            "cap[" + cap.type + "] " +
                            "write[" + field.name + "] " +
                            "<= " + h(data, field.bytes * 8),
                        LOG_VIRTIO);

                    field.write(data);
                };
            }

            if(cap.use_mmio)
            {
                // (port is the offset in the BAR: move_mmio maps it)
                this.mmio_fields.push({ offset: port, bytes: field.bytes, read, write, pending: 0 });
            }
            else
            {
                // DSL (2.4 kernel) does these reads
                const shim_read8_on_16 = function(addr)
                {
                    dbg_log("Warning: 8-bit read from 16-bit virtio port", LOG_VIRTIO);
                    return read(addr & ~1) >> ((addr & 1) << 3) & 0xFF;
                };
                const shim_read16_on_32 = function(addr)
                {
                    dbg_log("Warning: 16-bit read from 32-bit virtio port", LOG_VIRTIO);
                    return read(addr & ~3) >> ((addr & 2) << 3) & 0xFFFF;
                };
                const shim_read8_on_32 = function(addr)
                {
                    dbg_log("Warning: 8-bit read from 32-bit virtio port", LOG_VIRTIO);
                    return read(addr & ~3) >> ((addr & 3) << 3) & 0xFF;
                };

                // archhurd does these reads
                const shim_read32_on_16 = function(addr)
                {
                    dbg_log("Warning: 32-bit read from 16-bit virtio port", LOG_VIRTIO);
                    return read(addr);
                };

                switch(field.bytes)
                {
                    case 4:
                        this.cpu.io.register_read(port, this, shim_read8_on_32, shim_read16_on_32, read);
                        this.cpu.io.register_read(port + 1, this, shim_read8_on_32);
                        this.cpu.io.register_read(port + 2, this, shim_read8_on_32, shim_read16_on_32);
                        this.cpu.io.register_read(port + 3, this, shim_read8_on_32);
                        this.cpu.io.register_write(port, this, undefined, undefined, write);
                        break;
                    case 2:
                        this.cpu.io.register_read(port, this, shim_read8_on_16, read, shim_read32_on_16);
                        this.cpu.io.register_read(port + 1, this, shim_read8_on_16);
                        this.cpu.io.register_write(port, this, undefined, write);
                        break;
                    case 1:
                        this.cpu.io.register_read(port, this, read);
                        this.cpu.io.register_write(port, this, write);
                        break;
                    default:
                        dbg_assert(false,
                            "VirtIO device <" + this.name + "> invalid capability field width of " +
                            field.bytes + " bytes");
                        break;
                }
            }

            port += field.bytes;
        }
    }

    if(this.mmio_fields.length)
    {
        const end = Math.max(...this.mmio_fields.map(field => field.offset + field.bytes));
        this.mmio_index = new Int16Array(end).fill(-1);
        this.mmio_fields.forEach((field, i) => this.mmio_index.fill(i, field.offset, field.offset + field.bytes));
    }

    // Shared memory regions: struct virtio_pci_cap64, the id in the byte
    // after the BAR, the offset's and length's high dwords after the struct
    for(const region of shared_memory)
    {
        const cap_len = VIRTIO_PCI_CAP_LENGTH + 8;
        cap_ptr = cap_next;
        cap_next = cap_ptr + cap_len;
        dbg_assert(cap_next <= 256,
            "VirtIO device<" + this.name + "> can't fit all capabilities into 256byte configspace");
        this.pci_space[cap_ptr] = VIRTIO_PCI_CAP_VENDOR;
        this.pci_space[cap_ptr + 1] = cap_next;
        this.pci_space[cap_ptr + 2] = cap_len;
        this.pci_space[cap_ptr + 3] = VIRTIO_PCI_CAP_SHARED_MEMORY_CFG;
        this.pci_space[cap_ptr + 4] = region.bar;
        this.pci_space[cap_ptr + 5] = region.id;
        for(let i = 0; i < 4; i++)
        {
            this.pci_space[cap_ptr + 8 + i] = region.offset >>> 8 * i & 0xFF;
            this.pci_space[cap_ptr + 12 + i] = region.length >>> 8 * i & 0xFF;
        }
        this.pci_space.fill(0, cap_ptr + 16, cap_ptr + 24);
    }

    // Terminate linked list with the pci config access capability.

    const cap_len = VIRTIO_PCI_CAP_LENGTH + 4;
    dbg_assert(cap_next + cap_len <= 256,
        "VirtIO device<" + this.name + "> can't fit all capabilities into 256byte configspace");
    this.pci_space[cap_next] = VIRTIO_PCI_CAP_VENDOR;
    this.pci_space[cap_next + 1] = 0; // cap next (null terminator)
    this.pci_space[cap_next + 2] = cap_len;
    this.pci_space[cap_next + 3] = VIRTIO_PCI_CAP_PCI_CFG; // cap type
    this.pci_space[cap_next + 4] = 0; // bar (written by device)
    this.pci_space[cap_next + 5] = 0; // Padding.
    this.pci_space[cap_next + 6] = 0; // Padding.
    this.pci_space[cap_next + 7] = 0; // Padding.

    // Remaining fields are configured by driver when needed.

    // offset
    this.pci_space[cap_next + 8] = 0;
    this.pci_space[cap_next + 9] = 0;
    this.pci_space[cap_next + 10] = 0;
    this.pci_space[cap_next + 11] = 0;

    // bar size
    this.pci_space[cap_next + 12] = 0;
    this.pci_space[cap_next + 13] = 0;
    this.pci_space[cap_next + 14] = 0;
    this.pci_space[cap_next + 15] = 0;

    // cfg_data
    this.pci_space[cap_next + 16] = 0;
    this.pci_space[cap_next + 17] = 0;
    this.pci_space[cap_next + 18] = 0;
    this.pci_space[cap_next + 19] = 0;

    //
    // TODO
    // The pci config access capability is required by spec, but so far, devices
    // seem to work well without it.
    // This capability provides a cfg_data field (at cap_next + 16 for 4 bytes)
    // that acts like a window to the previous bars. The driver writes the bar number,
    // offset, and length values in this capability, and the cfg_data field should
    // mirror the data referred by the bar, offset and length. Here, length can be
    // 1, 2, or 4.
    //
    // This requires some sort of pci devicespace read and write handlers.
};

/**
 * The capabilities' memory BAR moved (the BIOS or the OS placed it)
 * @param {number} base
 */
VirtIO.prototype.move_mmio = function(base)
{
    if(base === this.mmio_base) return;
    if(this.mmio_base) this.cpu.io.mmap_unregister(this.mmio_base, this.mmio_size);
    this.mmio_base = base;
    if(base)
    {
        this.cpu.io.mmap_register(base, this.mmio_size,
            addr => this.mmio_read8(addr), (addr, value) => this.mmio_write8(addr, value),
            addr => this.mmio_read32(addr), (addr, value) => this.mmio_write32(addr, value));
    }
};

VirtIO.prototype.mmio_field = function(addr)
{
    // (addresses above 2 GiB arrive as negative int32s)
    const offset = addr - this.mmio_base >>> 0;
    const i = offset < this.mmio_index.length ? this.mmio_index[offset] : -1;
    return i >= 0 ? this.mmio_fields[i] : null;
};

VirtIO.prototype.mmio_read8 = function(addr)
{
    const field = this.mmio_field(addr);
    return field ? field.read() >>> 8 * ((addr - this.mmio_base >>> 0) - field.offset) & 0xFF : 0;
};

/** A byte of a field: a wider field takes effect with its last byte (16-bit writes arrive a byte at a time) */
VirtIO.prototype.mmio_write8 = function(addr, value)
{
    const field = this.mmio_field(addr);
    if(!field) return;
    const at = (addr - this.mmio_base >>> 0) - field.offset;
    field.pending = (at ? field.pending & (1 << 8 * at) - 1 : 0) | value << 8 * at;
    if(at === field.bytes - 1) field.write(field.bytes === 4 ? field.pending | 0 : field.pending);
};

VirtIO.prototype.mmio_read32 = function(addr)
{
    const field = this.mmio_field(addr);
    if(field && field.bytes === 4 && (addr - this.mmio_base >>> 0) === field.offset) return field.read() | 0;
    return this.mmio_read8(addr) | this.mmio_read8(addr + 1) << 8 | this.mmio_read8(addr + 2) << 16 | this.mmio_read8(addr + 3) << 24;
};

VirtIO.prototype.mmio_write32 = function(addr, value)
{
    const field = this.mmio_field(addr);
    if(field && field.bytes === 4 && (addr - this.mmio_base >>> 0) === field.offset)
    {
        field.write(value);
        return;
    }
    for(let i = 0; i < 4; i++) this.mmio_write8(addr + i, value >>> 8 * i & 0xFF);
};

VirtIO.prototype.get_state = function()
{
    let state = [];

    state[0] = this.device_feature_select;
    state[1] = this.driver_feature_select;
    state[2] = this.device_feature.slice();
    state[3] = this.driver_feature.slice();
    state[4] = this.features_ok;
    state[5] = this.device_status;
    state[6] = this.config_has_changed;
    state[7] = this.config_generation;
    state[8] = this.isr_status;
    state[9] = this.queue_select;
    state = state.concat(this.queues);

    return state;
};

VirtIO.prototype.set_state = function(state)
{
    this.device_feature_select = state[0];
    this.driver_feature_select = state[1];
    this.device_feature = state[2];
    this.driver_feature = state[3];
    this.features_ok = state[4];
    this.device_status = state[5];
    this.config_has_changed = state[6];
    this.config_generation = state[7];
    this.isr_status = state[8];
    this.queue_select = state[9];
    let i = 0;
    for(const queue of state.slice(10))
    {
        this.queues[i].set_state(queue);
        i++;
    }
    this.queue_selected = this.queues[this.queue_select] || null;
};

VirtIO.prototype.reset = function()
{
    if(this.on_reset) this.on_reset();
    this.device_feature_select = 0;
    this.driver_feature_select = 0;
    this.driver_feature.set(this.device_feature);

    this.features_ok = true;
    this.device_status = 0;

    this.queue_select = 0;
    this.queue_selected = this.queues[0];

    for(const queue of this.queues)
    {
        queue.reset();
    }

    this.config_has_changed = false;
    this.config_generation = 0;

    this.lower_irq();
};

/**
 * Call this when device-specific configuration state changes.
 * Also called when status DEVICE_NEEDS_RESET is set.
 */
VirtIO.prototype.notify_config_changes = function()
{
    this.config_has_changed = true;

    if(this.device_status & VIRTIO_STATUS_DRIVER_OK)
    {
        this.raise_irq(VIRTIO_ISR_DEVICE_CFG);
    }
    else
    {
        dbg_assert(false,
            "VirtIO device<" + this.name + "> attempted to notify driver before DRIVER_OK");
    }
};

/**
 * To be called after reading any field whose write can trigger notify_config_changes().
 */
VirtIO.prototype.update_config_generation = function()
{
    if(this.config_has_changed)
    {
        this.config_generation++;
        this.config_generation &= 0xFF;
        this.config_has_changed = false;
    }
};

VirtIO.prototype.is_feature_negotiated = function(feature)
{
    // Feature bits are grouped in 32 bits.
    // Note: earlier we chose not to set invalid features into driver_feature.
    return (this.driver_feature[feature >>> 5] & (1 << (feature & 0x1F))) > 0;
};

/**
 * Call this if an irrecoverable error has been occured.
 * Notifies driver if DRIVER_OK, or when DRIVER_OK gets set.
 */
VirtIO.prototype.needs_reset = function()
{
    dbg_log("Device<" + this.name + "> experienced error - requires reset", LOG_VIRTIO);
    this.device_status |= VIRTIO_STATUS_DEVICE_NEEDS_RESET;

    if(this.device_status & VIRTIO_STATUS_DRIVER_OK)
    {
        this.notify_config_changes();
    }
};

VirtIO.prototype.raise_irq = function(type)
{
    dbg_log("Raise irq " + h(type), LOG_VIRTIO);
    if(this.core >= 0)
    {
        this.cpu.wm.exports["virtio_pci_raise_irq"](this.core, type);
        return;
    }
    this.isr_status |= type;
    this.pci.raise_irq(this.pci_id);
};

VirtIO.prototype.lower_irq = function()
{
    dbg_log("Lower irq ", LOG_VIRTIO);
    if(this.core >= 0)
    {
        this.cpu.wm.exports["virtio_pci_lower_irq"](this.core);
        return;
    }
    this.isr_status = 0;
    this.pci.lower_irq(this.pci_id);
};

/**
 * @constructor
 * @param {CPU} cpu
 * @param {VirtQueue_Options} options
 */
function VirtQueue(cpu, virtio, options)
{
    /** @const @type {CPU} */
    this.cpu = cpu;

    /** @const @type {VirtIO} */
    this.virtio = virtio;

    // Number of entries.
    this.size = options.size_supported;
    this.size_supported = options.size_supported;
    this.mask = this.size - 1;
    this.enabled = false;
    this.notify_offset = options.notify_offset;

    this.desc_addr = 0;

    this.avail_addr = 0;
    this.avail_last_idx = 0;

    this.used_addr = 0;
    this.num_staged_replies = 0;

    this.reset();
}

/**
 * The queue's fields as accessors of the core's (VirtIO.use_core)
 * @param {function():!Int32Array} fields
 * @param {number} base
 */
VirtQueue.prototype.use_core = function(fields, base)
{
    const names = ["size", "size_supported", "enabled", "notify_offset", "desc_addr", "avail_addr", "avail_last_idx",
        "used_addr", "num_staged_replies", "fix_wrapping"];
    const f = fields();
    names.forEach((name, i) => { f[base + i] = +this[name] || 0; });
    names.forEach((name, i) => {
        const bool = name === "enabled" || name === "fix_wrapping";
        Object.defineProperty(this, name, {
            get: () => bool ? !!fields()[base + i] : fields()[base + i],
            set: value => { fields()[base + i] = +value || 0; },
        });
    });
    Object.defineProperty(this, "mask", { get: () => fields()[base] - 1, set: value => {} });
};

VirtQueue.prototype.get_state = function()
{
    const state = [];

    state[0] = this.size;
    state[1] = this.size_supported;
    state[2] = this.enabled;
    state[3] = this.notify_offset;
    state[4] = this.desc_addr;
    state[5] = this.avail_addr;
    state[6] = this.avail_last_idx;
    state[7] = this.used_addr;
    state[8] = this.num_staged_replies;
    state[9] = 1;

    return state;
};

VirtQueue.prototype.set_state = function(state)
{
    this.size = state[0];
    this.size_supported = state[1];
    this.enabled = state[2];
    this.notify_offset = state[3];
    this.desc_addr = state[4];
    this.avail_addr = state[5];
    this.avail_last_idx = state[6];
    this.used_addr = state[7];
    this.num_staged_replies = state[8];

    this.mask = this.size - 1;
    this.fix_wrapping = state[9] !== 1;
};

VirtQueue.prototype.reset = function()
{
    this.enabled = false;
    this.desc_addr = 0;
    this.avail_addr = 0;
    this.avail_last_idx = 0;
    this.used_addr = 0;
    this.num_staged_replies = 0;
    this.set_size(this.size_supported);
};

VirtQueue.prototype.is_configured = function()
{
    return this.desc_addr && this.avail_addr && this.used_addr;
};

VirtQueue.prototype.enable = function()
{
    dbg_assert(this.is_configured(), "VirtQueue must be configured before enabled");
    this.enabled = true;
};

VirtQueue.prototype.set_size = function(size)
{
    dbg_assert((size & size - 1) === 0, "VirtQueue size must be power of 2 or zero");
    dbg_assert(size <= this.size_supported, "VirtQueue size must be within supported size");
    this.size = size;
    this.mask = size - 1;
};

/**
 * @return {number}
 */
VirtQueue.prototype.count_requests = function()
{
    dbg_assert(this.avail_addr, "VirtQueue addresses must be configured before use");
    if(this.fix_wrapping) {
        this.fix_wrapping = false;
        this.avail_last_idx = (this.avail_get_idx() & ~this.mask) + (this.avail_last_idx & this.mask);
    }
    return this.cpu.virtio_count_requests(this.avail_addr, this.avail_last_idx);
};

/**
 * @return {boolean}
 */
VirtQueue.prototype.has_request = function()
{
    dbg_assert(this.avail_addr, "VirtQueue addresses must be configured before use");
    return this.count_requests() !== 0;
};

/**
 * @return {VirtQueueBufferChain}
 */
VirtQueue.prototype.pop_request = function()
{
    dbg_assert(this.avail_addr, "VirtQueue addresses must be configured before use");
    dbg_assert(this.has_request(), "VirtQueue must not pop nonexistent request");

    const desc_idx = this.cpu.virtio_avail_entry(this.avail_addr, this.avail_last_idx, this.mask);
    dbg_log("Pop request: avail_last_idx=" + this.avail_last_idx +
        " desc_idx=" + desc_idx, LOG_VIRTIO);

    const bufchain = new VirtQueueBufferChain(this, desc_idx);

    this.avail_last_idx = (this.avail_last_idx + 1) & 0xFFFF;

    return bufchain;
};

/**
 * Stage a buffer chain into the used ring.
 * Can call push_reply many times before flushing to batch replies together.
 * Note: this reply is not visible to driver until flush_replies is called.
 * @param {VirtQueueBufferChain} bufchain
 */
VirtQueue.prototype.push_reply = function(bufchain)
{
    dbg_assert(this.used_addr, "VirtQueue addresses must be configured before use");
    dbg_assert(this.num_staged_replies < this.size, "VirtQueue replies must not exceed queue size");
    this.cpu.virtio_push_reply(this.used_addr, this.mask, this.num_staged_replies, bufchain.head_idx, bufchain.length_written);
    this.num_staged_replies++;
};

/**
 * Makes replies visible to driver by updating the used ring idx and
 * firing appropriate interrupt if needed.
 */
VirtQueue.prototype.flush_replies = function()
{
    dbg_assert(this.used_addr, "VirtQueue addresses must be configured before use");

    if(this.num_staged_replies === 0)
    {
        return;
    }
    // (the rings in the core: src/rust/cpu/virtio.rs)
    const interrupt = this.cpu.virtio_flush_replies(this.used_addr, this.avail_addr, this.size, this.num_staged_replies,
        this.virtio.is_feature_negotiated(VIRTIO_F_RING_EVENT_IDX));
    this.num_staged_replies = 0;
    if(interrupt)
    {
        this.virtio.raise_irq(VIRTIO_ISR_QUEUE);
    }
};

/**
 * If using VIRTIO_F_RING_EVENT_IDX, device must tell driver when
 * to get notifications or else driver won't notify regularly.
 * If not using VIRTIO_F_RING_EVENT_IDX, driver will ignore avail_event
 * and notify every request regardless unless NO_NOTIFY is set (TODO implement when needed).
 * @param {number} num_skipped_requests Zero = get notified in the next request.
 */
VirtQueue.prototype.notify_me_after = function(num_skipped_requests)
{
    dbg_assert(num_skipped_requests >= 0, "Must skip a non-negative number of requests");
    this.cpu.virtio_notify_me_after(this.used_addr, this.avail_addr, this.size, num_skipped_requests);
};

/**
 * @param {number} table_address The physical address of the start of the desc table.
 * @param {number} i
 */
VirtQueue.prototype.get_descriptor = function(table_address, i)
{
    return {
        addr_low: this.cpu.read32s(table_address + i * VIRTQ_DESC_ENTRYSIZE),
        addr_high: this.cpu.read32s(table_address + i * VIRTQ_DESC_ENTRYSIZE + 4),
        len: this.cpu.read32s(table_address + i * VIRTQ_DESC_ENTRYSIZE + 8),
        flags: this.cpu.read16(table_address + i * VIRTQ_DESC_ENTRYSIZE + 12),
        next: this.cpu.read16(table_address + i * VIRTQ_DESC_ENTRYSIZE + 14),
    };
};

// Avail ring fields

VirtQueue.prototype.avail_get_flags = function()
{
    return this.cpu.read16(this.avail_addr);
};

VirtQueue.prototype.avail_get_idx = function()
{
    return this.cpu.read16(this.avail_addr + 2);
};

VirtQueue.prototype.avail_get_entry = function(i)
{
    return this.cpu.read16(this.avail_addr + 4 + VIRTQ_AVAIL_ENTRYSIZE * (i & this.mask));
};

VirtQueue.prototype.avail_get_used_event = function()
{
    return this.cpu.read16(this.avail_addr + 4 + VIRTQ_AVAIL_ENTRYSIZE * this.size);
};

// Used ring fields

VirtQueue.prototype.used_get_flags = function()
{
    return this.cpu.read16(this.used_addr);
};

VirtQueue.prototype.used_set_flags = function(value)
{
    this.cpu.write16(this.used_addr, value);
};

VirtQueue.prototype.used_get_idx = function()
{
    return this.cpu.read16(this.used_addr + 2);
};

VirtQueue.prototype.used_set_idx = function(value)
{
    this.cpu.write16(this.used_addr + 2, value);
};

VirtQueue.prototype.used_set_entry = function(i, desc_idx, length_written)
{
    this.cpu.write32(this.used_addr + 4 + VIRTQ_USED_ENTRYSIZE * i, desc_idx);
    this.cpu.write32(this.used_addr + 8 + VIRTQ_USED_ENTRYSIZE * i, length_written);
};

VirtQueue.prototype.used_set_avail_event = function(value)
{
    this.cpu.write16(this.used_addr + 4 + VIRTQ_USED_ENTRYSIZE * this.size, value);
};

/**
 * Traverses through descriptor chain starting at head_id.
 * Provides means to read/write to buffers represented by the descriptors.
 * @constructor
 * @param {VirtQueue} virtqueue
 * @param {number} head_idx
 */
export function VirtQueueBufferChain(virtqueue, head_idx)
{
    /** @const @type {CPU} */
    this.cpu = virtqueue.cpu;

    /** @const @type {VirtIO} */
    this.virtio = virtqueue.virtio;

    this.head_idx = head_idx;

    this.read_buffers = [];
    // Pointers for sequential consumption via get_next_blob.
    this.read_buffer_idx = 0;
    this.read_buffer_offset = 0;
    this.length_readable = 0;

    this.write_buffers = [];
    // Pointers for sequential write via set_next_blob.
    this.write_buffer_idx = 0;
    this.write_buffer_offset = 0;
    this.length_written = 0;
    this.length_writable = 0;

    // Traverse chain to discover buffers.
    // - There shouldn't be an excessive amount of descriptor elements.
    // the chain's descriptors, read in the core (src/rust/cpu/virtio.rs): readable ones, then writable ones
    const cpu = this.cpu;
    const n = cpu.virtio_read_chain(virtqueue.desc_addr, virtqueue.size, head_idx,
        this.virtio.is_feature_negotiated(VIRTIO_F_RING_INDIRECT_DESC));
    const chain = new Int32Array(cpu.wasm_memory.buffer, cpu.virtio_chain_addr(), 4 * n);
    for(let i = 0; i < n; i++)
    {
        const desc = { addr_low: chain[4 * i], addr_high: chain[4 * i + 1], len: chain[4 * i + 2], flags: chain[4 * i + 3] };
        if(desc.flags & VIRTQ_DESC_F_WRITE)
        {
            this.write_buffers.push(desc);
            this.length_writable += desc.len;
        }
        else
        {
            this.read_buffers.push(desc);
            this.length_readable += desc.len;
        }
    }
    dbg_log("Descriptor chain end >>>", LOG_VIRTIO);
}

/**
 * Reads the next blob of memory represented by the buffer chain into dest_buffer.
 * @param {Uint8Array} dest_buffer
 * @return {number} Number of bytes successfully read.
 */
VirtQueueBufferChain.prototype.get_next_blob = function(dest_buffer)
{
    let dest_offset = 0;
    let remaining = dest_buffer.length;

    while(remaining)
    {
        if(this.read_buffer_idx === this.read_buffers.length)
        {
            dbg_log("Device<" + this.virtio.name + "> Read more than device-readable buffers has", LOG_VIRTIO);
            break;
        }

        const buf = this.read_buffers[this.read_buffer_idx];
        const read_address = buf.addr_low + this.read_buffer_offset;
        let read_length = buf.len - this.read_buffer_offset;

        if(read_length > remaining)
        {
            read_length = remaining;
            this.read_buffer_offset += remaining;
        }
        else
        {
            this.read_buffer_idx++;
            this.read_buffer_offset = 0;
        }

        dest_buffer.set(this.cpu.read_blob(read_address, read_length), dest_offset);

        dest_offset += read_length;
        remaining -= read_length;
    }

    return dest_offset;
};

/**
 * Appends contents of src_buffer into the memory represented by the buffer chain.
 * @param {Uint8Array} src_buffer
 * @return {number} Number of bytes successfully written.
 */
VirtQueueBufferChain.prototype.set_next_blob = function(src_buffer)
{
    let src_offset = 0;
    let remaining = src_buffer.length;

    while(remaining)
    {
        if(this.write_buffer_idx === this.write_buffers.length)
        {
            dbg_log("Device<" + this.virtio.name + "> Write more than device-writable capacity", LOG_VIRTIO);
            break;
        }

        const buf = this.write_buffers[this.write_buffer_idx];
        const write_address = buf.addr_low + this.write_buffer_offset;
        let write_length = buf.len - this.write_buffer_offset;

        if(write_length > remaining)
        {
            write_length = remaining;
            this.write_buffer_offset += remaining;
        }
        else
        {
            this.write_buffer_idx++;
            this.write_buffer_offset = 0;
        }

        const src_end = src_offset + write_length;
        this.cpu.write_blob(src_buffer.subarray(src_offset, src_end), write_address);

        src_offset += write_length;
        remaining -= write_length;
    }

    this.length_written += src_offset;
    return src_offset;
};
