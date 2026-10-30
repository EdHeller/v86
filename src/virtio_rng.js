// virtio-rng on the PCI bus (virtio 1.2 section 5.4, ported from v64): every
// buffer the driver offers on its queue is filled from
// crypto.getRandomValues (the guest's /dev/hwrng)

import { VirtIO, VIRTIO_F_VERSION_1 } from "./virtio.js";

// For Types Only
import { CPU } from "./cpu.js";

/** crypto.getRandomValues fills at most this many bytes per call */
const ENTROPY_CHUNK = 65536;

/**
 * @constructor
 * @param {CPU} cpu
 */
export function VirtioRng(cpu)
{
    /** @type {VirtIO} */
    this.virtio = new VirtIO(cpu,
    {
        name: "virtio-rng",
        pci_id: 0x0D << 3,
        device_id: 0x1044,
        subsystem_device_id: 4,
        common:
        {
            initial_port: 0xE800,
            queues: [{ size_supported: 64, notify_offset: 0 }],
            features: [VIRTIO_F_VERSION_1],
            on_driver_ok: () => {},
        },
        notification:
        {
            initial_port: 0xE900,
            single_handler: true,
            handlers: [queue_id => this.fill(this.virtio.queues[queue_id])],
        },
        isr_status:
        {
            initial_port: 0xE700,
        },
        device_specific:
        {
            initial_port: 0xE600,
            struct: [],
        },
    });
}

VirtioRng.prototype.fill = function(queue)
{
    while(queue.has_request())
    {
        const chain = queue.pop_request();
        const bytes = new Uint8Array(chain.length_writable);
        for(let at = 0; at < bytes.length; at += ENTROPY_CHUNK)
        {
            globalThis.crypto.getRandomValues(bytes.subarray(at, at + ENTROPY_CHUNK));
        }
        chain.set_next_blob(bytes);
        queue.push_reply(chain);
    }
    queue.flush_replies();
};

VirtioRng.prototype.reset = function()
{
    this.virtio.reset();
};

VirtioRng.prototype.get_state = function()
{
    return [this.virtio];
};

VirtioRng.prototype.set_state = function(state)
{
    this.virtio.set_state(state[0]);
};
