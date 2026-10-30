// virtio over MMIO (virtio 1.2 section 4.2, version 2 "modern" registers) for the virt machine: the transport
// with split virtqueues, and the devices the machine needs to boot Linux from a disk image: a block device
// and a random number generator. The machine (src/arm/machine.js) gives guest RAM and the interrupt line.

const MAGIC = 0x74726976;
const VERSION = 2;
const VENDOR = 0x554D4551;

const VIRTIO_F_VERSION_1 = 32;

const STATUS_DRIVER_OK = 4;
const STATUS_FAILED = 0x80;

const QUEUE_SIZE = 256;

const DESC_NEXT = 1;
const DESC_WRITE = 2;
const DESC_INDIRECT = 4;

/**
 * @constructor
 * @param {Object} machine
 * @param {number} intid the SPI of the device
 * @param {number} id the device id (2 block, 4 rng)
 * @param {number} queues
 * @param {Array<number>} features bit numbers
 * @param {number} config_size
 */
function VirtioMmio(machine, intid, id, queues, features, config_size)
{
    this.machine = machine;
    this.intid = intid;
    this.device_id = id;
    this.queue_count = queues;
    this.features = BigInt(0);
    for(const f of features) this.features |= BigInt(1) << BigInt(f);
    this.config = new Uint8Array(config_size);
    this.reset();
}

VirtioMmio.prototype.reset = function()
{
    this.status = 0;
    this.interrupt_status = 0;
    this.device_features_sel = 0;
    this.driver_features_sel = 0;
    this.driver_features = BigInt(0);
    this.queue_sel = 0;
    this.config_generation = 0;
    this.queues = [];
    for(let i = 0; i < this.queue_count; i++)
    {
        this.queues.push({ num: QUEUE_SIZE, ready: false, desc: 0, avail: 0, used: 0, last_avail: 0 });
    }
    this.machine.set_irq(this.intid, false);
};

VirtioMmio.prototype.read = function(offset, width)
{
    if(offset >= 0x100)
    {
        const at = offset - 0x100;
        let v = 0;
        for(let i = 0; i < width; i++) v |= (this.config[at + i] || 0) << (8 * i);
        return v >>> 0;
    }
    const q = this.queues[this.queue_sel];
    switch(offset)
    {
        case 0x000: return MAGIC;
        case 0x004: return VERSION;
        case 0x008: return this.device_id;
        case 0x00C: return VENDOR;
        case 0x010: return Number(BigInt.asUintN(32, this.features >> BigInt(32 * this.device_features_sel)));
        case 0x034: return q ? QUEUE_SIZE : 0;
        case 0x044: return q && q.ready ? 1 : 0;
        case 0x060: return this.interrupt_status;
        case 0x070: return this.status;
        case 0x0FC: return this.config_generation;
        default: return 0;
    }
};

VirtioMmio.prototype.write = function(offset, width, value)
{
    value = Number(value) >>> 0;
    if(offset >= 0x100)
    {
        const at = offset - 0x100;
        for(let i = 0; i < width && at + i < this.config.length; i++) this.config[at + i] = value >>> (8 * i) & 0xFF;
        this.config_write?.();
        return;
    }
    const q = this.queues[this.queue_sel];
    switch(offset)
    {
        case 0x014: this.device_features_sel = value; break;
        case 0x020:
            if(this.driver_features_sel === 0) this.driver_features = this.driver_features & ~BigInt(0xFFFFFFFF) | BigInt(value);
            else this.driver_features = this.driver_features & BigInt(0xFFFFFFFF) | BigInt(value) << BigInt(32);
            break;
        case 0x024: this.driver_features_sel = value; break;
        case 0x030: this.queue_sel = value; break;
        case 0x038: if(q) q.num = Math.min(value, QUEUE_SIZE) || QUEUE_SIZE; break;
        case 0x044:
            if(q)
            {
                q.ready = value === 1;
                if(q.ready) q.last_avail = 0;
            }
            break;
        case 0x050: this.notify(value); break;
        case 0x064:
            this.interrupt_status &= ~value;
            if(!this.interrupt_status) this.machine.set_irq(this.intid, false);
            break;
        case 0x070:
            if(value === 0) this.reset();
            else this.status = value;
            break;
        case 0x080: if(q) q.desc = q.desc - (q.desc % 0x100000000) + value; break;
        case 0x084: if(q) q.desc = q.desc % 0x100000000 + value * 0x100000000; break;
        case 0x090: if(q) q.avail = q.avail - (q.avail % 0x100000000) + value; break;
        case 0x094: if(q) q.avail = q.avail % 0x100000000 + value * 0x100000000; break;
        case 0x0A0: if(q) q.used = q.used - (q.used % 0x100000000) + value; break;
        case 0x0A4: if(q) q.used = q.used % 0x100000000 + value * 0x100000000; break;
        default: break;
    }
};

VirtioMmio.prototype.mem = function(address, length)
{
    const view = this.machine.ram(address, length);
    if(!view) throw new RangeError("virtio: " + length + " bytes at 0x" + address.toString(16) + " are not RAM");
    return view;
};

function u16(view, at) { return view[at] | view[at + 1] << 8; }
function u32(view, at) { return (view[at] | view[at + 1] << 8 | view[at + 2] << 16 | view[at + 3] << 24) >>> 0; }
function u64(view, at) { return u32(view, at) + u32(view, at + 4) * 0x100000000; }

/** The requests the driver has made on queue `index`: each one a list of buffers {address, length, write} */
VirtioMmio.prototype.pop = function(index)
{
    const q = this.queues[index];
    if(!q || !q.ready || !(this.status & STATUS_DRIVER_OK)) return null;
    const avail = this.mem(q.avail, 4 + 2 * q.num);
    if(q.last_avail === u16(avail, 2)) return null;
    const head = u16(avail, 4 + 2 * (q.last_avail % q.num));
    q.last_avail = q.last_avail + 1 & 0xFFFF;
    const buffers = [];
    const table = this.mem(q.desc, 16 * q.num);
    let i = head, count = 0;
    let indirect = null;
    for(;;)
    {
        const d = indirect || table;
        const flags = u16(d, i * 16 + 12);
        if(flags & DESC_INDIRECT)
        {
            const length = u32(d, i * 16 + 8);
            indirect = this.mem(u64(d, i * 16), length);
            i = 0;
            if(++count > 1024) break;
            continue;
        }
        buffers.push({ address: u64(d, i * 16), length: u32(d, i * 16 + 8), write: !!(flags & DESC_WRITE) });
        if(!(flags & DESC_NEXT) || ++count > 1024) break;
        i = u16(d, i * 16 + 14);
    }
    return { head, buffers };
};

/** Complete a request: `written` bytes were written into its buffers */
VirtioMmio.prototype.push = function(index, request, written)
{
    const q = this.queues[index];
    const used = this.mem(q.used, 4 + 8 * q.num);
    const idx = u16(used, 2);
    const at = 4 + 8 * (idx % q.num);
    const head = request.head;
    used[at] = head & 0xFF; used[at + 1] = head >> 8 & 0xFF; used[at + 2] = 0; used[at + 3] = 0;
    used[at + 4] = written & 0xFF; used[at + 5] = written >> 8 & 0xFF; used[at + 6] = written >> 16 & 0xFF; used[at + 7] = written >>> 24;
    const next = idx + 1 & 0xFFFF;
    used[2] = next & 0xFF; used[3] = next >> 8;
    this.machine.ram_written(q.used, 4 + 8 * q.num);
};

VirtioMmio.prototype.interrupt = function()
{
    this.interrupt_status |= 1;
    this.machine.set_irq(this.intid, true);
};

VirtioMmio.prototype.notify = function(index)
{
    if(this.status & STATUS_FAILED) return;
    let any = false;
    for(let request; (request = this.pop(index));)
    {
        this.handle(index, request);
        any = true;
    }
    if(any) this.interrupt();
};

/**
 * @constructor
 * @param {Object} machine
 * @param {number} intid
 * @param {{buffer: ArrayBuffer, readonly: (boolean|undefined)}} disk
 */
export function VirtioMmioBlk(machine, intid, disk)
{
    VirtioMmio.call(this, machine, intid, 2, 1, [VIRTIO_F_VERSION_1, 9, 6], 0x18);
    this.disk = new Uint8Array(disk.buffer);
    this.readonly = !!disk.readonly;
    const sectors = Math.floor(this.disk.length / 512);
    const view = new DataView(this.config.buffer);
    view.setUint32(0, sectors >>> 0, true);
    view.setUint32(4, Math.floor(sectors / 0x100000000), true);
    view.setUint32(0x14, 512, true); // blk_size
}
VirtioMmioBlk.prototype = Object.create(VirtioMmio.prototype);

VirtioMmioBlk.prototype.handle = function(index, request)
{
    const buffers = request.buffers;
    let status = 0, written = 0;
    try
    {
        const header = this.mem(buffers[0].address, 16);
        const type = u32(header, 0);
        const sector = u64(header, 8);
        const data = buffers.slice(1, -1);
        const status_buffer = buffers[buffers.length - 1];
        let at = sector * 512;
        if(type === 0)
        {
            for(const b of data)
            {
                const target = this.mem(b.address, b.length);
                if(at + b.length > this.disk.length) { status = 1; break; }
                target.set(this.disk.subarray(at, at + b.length));
                this.machine.ram_written(b.address, b.length);
                at += b.length;
                written += b.length;
            }
        }
        else if(type === 1)
        {
            if(this.readonly) status = 1;
            else
            {
                for(const b of data)
                {
                    if(at + b.length > this.disk.length) { status = 1; break; }
                    this.disk.set(this.mem(b.address, b.length), at);
                    at += b.length;
                }
            }
        }
        else if(type === 4) { /* flush */ }
        else if(type === 8)
        {
            const id = new TextEncoder().encode("v86-a64-disk");
            for(const b of data) { this.mem(b.address, b.length).set(id.subarray(0, Math.min(id.length, b.length))); written += b.length; }
        }
        else status = 2;
        const st = this.mem(status_buffer.address, 1);
        st[0] = status;
        this.machine.ram_written(status_buffer.address, 1);
        written += 1;
    }
    catch(error)
    {
        console.error("virtio-blk: " + error.message);
    }
    this.push(index, request, written);
};

/**
 * @constructor
 * @param {Object} machine
 * @param {number} intid
 */
export function VirtioMmioRng(machine, intid)
{
    VirtioMmio.call(this, machine, intid, 4, 1, [VIRTIO_F_VERSION_1], 0);
}
VirtioMmioRng.prototype = Object.create(VirtioMmio.prototype);

VirtioMmioRng.prototype.handle = function(index, request)
{
    let written = 0;
    for(const b of request.buffers)
    {
        if(!b.write) continue;
        const target = this.mem(b.address, b.length);
        // (the memory may be shared, which getRandomValues refuses: fill a copy)
        for(let at = 0; at < b.length; at += 65536)
        {
            const part = new Uint8Array(Math.min(b.length, at + 65536) - at);
            crypto.getRandomValues(part);
            target.set(part, at);
        }
        this.machine.ram_written(b.address, b.length);
        written += b.length;
    }
    this.push(index, request, written);
};

/**
 * virtio-net (virtio 1.2 section 5.1): frames go out as net0-send and come in as net0-receive on the machine's bus, the
 * interface of v86's network adapters (src/browser/network.js: fetch, wisp, inbrowser)
 * @constructor
 * @param {Object} machine
 * @param {number} intid
 * @param {{mac: (Array<number>|undefined)}=} options
 */
export function VirtioMmioNet(machine, intid, options)
{
    // VERSION_1, MAC (5), STATUS (16), MRG_RXBUF is not offered: a frame is one buffer chain
    VirtioMmio.call(this, machine, intid, 1, 2, [VIRTIO_F_VERSION_1, 5, 16], 10);
    const mac = (options && options.mac) || [0x52, 0x54, 0x00, 0x12, 0x34, 0x56];
    this.config.set(mac, 0);
    this.config[6] = 1; // status: link up
    this.waiting = [];
    machine.bus.register("net0-receive", frame => this.receive(frame), this);
}
VirtioMmioNet.prototype = Object.create(VirtioMmio.prototype);

const NET_HEADER = 12;

VirtioMmioNet.prototype.handle = function(index, request)
{
    if(index === 1)
    {
        // transmit: the header, then the frame
        const parts = [];
        let skip = NET_HEADER;
        for(const b of request.buffers)
        {
            if(b.write) continue;
            const bytes = this.mem(b.address, b.length);
            if(skip >= bytes.length) { skip -= bytes.length; continue; }
            parts.push(bytes.subarray(skip));
            skip = 0;
        }
        const length = parts.reduce((n, p) => n + p.length, 0);
        const frame = new Uint8Array(length);
        let at = 0;
        for(const p of parts) { frame.set(p, at); at += p.length; }
        this.machine.bus.send("net0-send", frame);
        this.push(1, request, 0);
    }
    else
    {
        // receive buffers the driver offers: kept until a frame arrives
        this.waiting.push(request);
        this.deliver();
    }
};

VirtioMmioNet.prototype.receive = function(frame)
{
    this.pending = this.pending || [];
    this.pending.push(new Uint8Array(frame));
    if(this.pending.length > 256) this.pending.shift();
    this.deliver();
};

/** Put the frames that wait into the buffers that wait */
VirtioMmioNet.prototype.deliver = function()
{
    let any = false;
    while(this.pending && this.pending.length && this.waiting.length)
    {
        const frame = this.pending.shift();
        const request = this.waiting.shift();
        const header = new Uint8Array(NET_HEADER);
        header[10] = 1; // num_buffers
        const data = new Uint8Array(NET_HEADER + frame.length);
        data.set(header, 0);
        data.set(frame, NET_HEADER);
        let at = 0, written = 0;
        for(const b of request.buffers)
        {
            if(!b.write || at >= data.length) continue;
            const count = Math.min(b.length, data.length - at);
            this.mem(b.address, count).set(data.subarray(at, at + count));
            this.machine.ram_written(b.address, count);
            at += count;
            written += count;
        }
        this.push(0, request, written);
        any = true;
    }
    if(any) this.interrupt();
};

VirtioMmioNet.prototype.reset = function()
{
    VirtioMmio.prototype.reset.call(this);
    this.waiting = [];
    this.pending = [];
};
