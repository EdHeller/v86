// virtio-blk on the PCI bus (virtio 1.2 section 5.2, ported from v64):
// reads, writes, FLUSH, read-only disks, capacity, SEG_MAX, BLK_SIZE and
// GET_ID, over a buffer of src/buffer.js.

import { VirtIO, VIRTIO_F_VERSION_1 } from "./virtio.js";

// For Types Only
import { CPU } from "./cpu.js";

// feature bits
const VIRTIO_BLK_F_SEG_MAX = 2;
const VIRTIO_BLK_F_RO = 5;
const VIRTIO_BLK_F_BLK_SIZE = 6;
const VIRTIO_BLK_F_FLUSH = 9;

// request types
const VIRTIO_BLK_T_IN = 0;
const VIRTIO_BLK_T_OUT = 1;
const VIRTIO_BLK_T_FLUSH = 4;
const VIRTIO_BLK_T_GET_ID = 8;

// request status
const VIRTIO_BLK_S_OK = 0;
const VIRTIO_BLK_S_IOERR = 1;
const VIRTIO_BLK_S_UNSUPP = 2;

const SECTOR = 512;
const QUEUE_SIZE = 256;
const ID_BYTES = 20;

/**
 * @constructor
 * @param {CPU} cpu
 * @param {Object} buffer the image (src/buffer.js)
 * @param {{id: (string|undefined), readonly: (boolean|undefined), index: (number|undefined)}} options
 */
export function VirtioBlk(cpu, buffer, options)
{
    this.cpu = cpu;
    this.buffer = buffer;
    this.readonly = !!options.readonly;
    this.id = new Uint8Array(ID_BYTES);
    this.id.set(new TextEncoder().encode(options.id || "").subarray(0, ID_BYTES));
    this.sectors = Math.floor(buffer.byteLength / SECTOR);
    this.block_size = SECTOR;

    const features = [VIRTIO_BLK_F_SEG_MAX, VIRTIO_BLK_F_BLK_SIZE, VIRTIO_BLK_F_FLUSH, VIRTIO_F_VERSION_1];
    if(this.readonly) features.push(VIRTIO_BLK_F_RO);
    const constant = (bytes, name, value) => ({ bytes, name, read: () => value, write: () => {} });

    /** @type {VirtIO} */
    this.virtio = new VirtIO(cpu,
    {
        name: "virtio-blk",
        pci_id: 0x0E << 3,
        device_id: 0x1042,
        subsystem_device_id: 2,
        common:
        {
            initial_port: 0xF800,
            queues: [{ size_supported: QUEUE_SIZE, notify_offset: 0 }],
            features,
            on_driver_ok: () => {},
        },
        notification:
        {
            initial_port: 0xF900,
            single_handler: true,
            handlers: [queue_id => this.process(this.virtio.queues[queue_id])],
        },
        isr_status: { initial_port: 0xF700 },
        device_specific:
        {
            initial_port: 0xF600,
            struct:
            [
                // capacity, in 512-byte sectors (a u64: the driver reads it as two words)
                constant(4, "capacity (low dword)", this.sectors >>> 0),
                constant(4, "capacity (high dword)", Math.floor(this.sectors / 0x100000000)),
                constant(4, "size_max", 0),
                constant(4, "seg_max", QUEUE_SIZE - 2),
                constant(2, "cylinders", 0),
                constant(1, "heads", 0),
                constant(1, "sectors", 0),
                constant(4, "blk_size", this.block_size),
            ],
        },
    });
}

/**
 * The requests the driver queued
 * @param {?} queue
 */
VirtioBlk.prototype.process = function(queue)
{
    while(queue.has_request())
    {
        const chain = queue.pop_request();
        const header = new Uint8Array(16);
        chain.get_next_blob(header);
        const view = new DataView(header.buffer);
        const type = view.getUint32(0, true);
        const sector = view.getUint32(8, true) + view.getUint32(12, true) * 0x100000000;
        // (the status byte is the last writable one)
        const readable = chain.length_readable - 16;
        const writable = chain.length_writable - 1;
        const done = status =>
        {
            // (a read that failed, or a short GET_ID: the status goes after
            // the data area all the same, which reads as zeroes)
            if(chain.length_written < writable) chain.set_next_blob(new Uint8Array(writable - chain.length_written));
            chain.set_next_blob(new Uint8Array([status]));
            queue.push_reply(chain);
            queue.flush_replies();
        };
        // (QEMU's virtio_blk_sect_range_ok: whole logical sectors, within the disk)
        const in_range = length => sector * SECTOR + length <= this.sectors * SECTOR &&
            sector * SECTOR % this.block_size === 0 && length % this.block_size === 0;
        switch(type)
        {
            case VIRTIO_BLK_T_IN:
                if(writable < 0 || !in_range(writable)) { done(VIRTIO_BLK_S_IOERR); break; }
                if(writable === 0) { done(VIRTIO_BLK_S_OK); break; }
                this.buffer.get(sector * SECTOR, writable, data =>
                {
                    chain.set_next_blob(data);
                    done(VIRTIO_BLK_S_OK);
                });
                break;
            case VIRTIO_BLK_T_OUT:
            {
                if(this.readonly || !in_range(readable)) { done(VIRTIO_BLK_S_IOERR); break; }
                const data = new Uint8Array(readable);
                chain.get_next_blob(data);
                this.buffer.set(sector * SECTOR, data, () => done(VIRTIO_BLK_S_OK));
                break;
            }
            case VIRTIO_BLK_T_FLUSH:
                done(VIRTIO_BLK_S_OK);
                break;
            case VIRTIO_BLK_T_GET_ID:
                chain.set_next_blob(this.id.subarray(0, Math.max(0, Math.min(ID_BYTES, writable))));
                done(VIRTIO_BLK_S_OK);
                break;
            default:
                done(VIRTIO_BLK_S_UNSUPP);
        }
    }
};

VirtioBlk.prototype.reset = function()
{
    this.virtio.reset();
};

VirtioBlk.prototype.get_state = function()
{
    return [this.virtio, this.readonly ? null : this.buffer.get_state()];
};

VirtioBlk.prototype.set_state = function(state)
{
    this.virtio.set_state(state[0]);
    if(state[1] && !this.readonly) this.buffer.set_state(state[1]);
};
