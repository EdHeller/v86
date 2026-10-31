// The virt machine's flash for UEFI (docs/arm64.md, "UEFI"): QEMU 9.2 virt's two pflash_cfi01 units, 64 MiB each,
// at 0 (the firmware's code) and 0x0400_0000 (its variable store). Each is two 16-bit Intel/Sharp devices side by
// side (bank width 4, device width 2): a command is written to both (0x00FF00FF), status reads come back from both.
//
// The code is read-only guest memory (a ROM of the core's bus: the cores run the firmware from it in place); the
// variable store is a device: in read array mode its bytes, after a command its status or identifiers, as QEMU's
// pflash_cfi01. The commands are those edk2's VirtNorFlashDxe uses: read array (0xFF), read status (0x70), clear
// status (0x50), read identifier (0x90), CFI query (0x98), program a word (0x40/0x10), buffered program (0xE8 ...
// 0xD0), erase a 256 KiB block (0x20, 0xD0), block lock setup (0x60, then 0x01 / 0xD0: no blocks are ever locked).

/** a unit's size, as QEMU's virt machine maps it */
export const VIRT_FLASH_SIZE = 0x04000000;
export const VIRT_FLASH_SECTOR = 0x40000;

const STATUS_READY = 0x80;

/**
 * The variable store: a pflash_cfi01 unit backed by `bytes` (erased beyond them)
 * @constructor
 * @param {!Uint8Array} bytes the store's contents (written in place)
 */
export function CfiFlash(bytes)
{
    this.bytes = bytes;
    /** what a read returns: 0 the array, 1 the status, 2 identifiers, 3 the CFI query */
    this.mode = 0;
    /** the command waiting for its next cycle (0x40, 0xE8, 0x20, 0x60), or 0 */
    this.cmd = 0;
    this.cycle = 0;
    this.counter = 0;
    this.status = 0;
    /** called after the store's bytes changed (offset, length) */
    this.on_write = null;
}

CfiFlash.prototype.reset = function()
{
    this.mode = 0;
    this.cmd = 0;
    this.cycle = 0;
    this.status = 0;
};

/** both devices' answer of a 16-bit value */
const both = (value, width) => {
    const word = (value & 0xFFFF) * 0x10001;
    return width === 8 ? BigInt(word >>> 0) * 0x100000001n : width === 4 ? word >>> 0 : value & (2 ** (8 * width) - 1);
};

// (the CFI query table at word offsets 0x10 and up, per device: "QRY", the Intel command set, sizes: 2^26 / 2 bytes
// a device, a write buffer of 2^6 bytes, 256 blocks of 128 KiB per device)
const CFI = {
    0x10: 0x51, 0x11: 0x52, 0x12: 0x59, 0x13: 0x01, 0x14: 0x00, 0x15: 0x31, 0x16: 0x00,
    0x1B: 0x45, 0x1C: 0x55, 0x1F: 0x07, 0x20: 0x07, 0x21: 0x0A, 0x23: 0x04, 0x24: 0x04, 0x25: 0x04,
    0x27: 25, 0x28: 0x01, 0x2A: 6, 0x2C: 0x01, 0x2D: 0xFF, 0x2E: 0x00, 0x2F: 0x00, 0x30: 0x02,
};

CfiFlash.prototype.read = function(offset, width)
{
    if(this.mode === 0)
    {
        let value = 0n;
        for(let i = width - 1; i >= 0; i--)
        {
            const at = offset + i;
            value = value << 8n | BigInt(at < this.bytes.length ? this.bytes[at] : 0xFF);
        }
        return width === 8 ? value : Number(value);
    }
    if(this.mode === 1) return both(this.status, width);
    // (identifiers and the query: per device's word, at the bank's word offset)
    const index = offset >>> 2;
    if(this.mode === 2)
    {
        // (Intel's manufacturer code and QEMU's device code; the block's lock state: unlocked)
        return both(index === 0 ? 0x89 : index === 1 ? 0x18 : 0, width);
    }
    return both(CFI[index] || 0, width);
};

CfiFlash.prototype.write = function(offset, width, value)
{
    const data = Number(BigInt.asUintN(32, BigInt(value)));
    const cmd = data & 0xFF;
    if(this.cmd === 0x40 || this.cmd === 0x10)
    {
        this.program(offset, width, value);
        this.cmd = 0;
        this.mode = 1;
        this.status |= STATUS_READY;
        return;
    }
    if(this.cmd === 0xE8)
    {
        if(this.cycle === 1)
        {
            // (the count less one, in the bank's words: masked to a device's width)
            this.counter = data & 0xFFFF;
            this.cycle = 2;
            return;
        }
        if(this.cycle === 2)
        {
            this.program(offset, width, value);
            if(this.counter-- === 0) this.cycle = 3;
            return;
        }
        // (the confirmation)
        if(cmd !== 0xD0) this.status |= 0x30;
        this.cmd = 0;
        this.cycle = 0;
        this.mode = 1;
        this.status |= STATUS_READY;
        return;
    }
    if(this.cmd === 0x20)
    {
        if(cmd === 0xD0)
        {
            const start = offset & ~(VIRT_FLASH_SECTOR - 1);
            const end = Math.min(start + VIRT_FLASH_SECTOR, this.bytes.length);
            if(start < end)
            {
                this.bytes.fill(0xFF, start, end);
                this.on_write?.(start, end - start);
            }
        }
        else this.status |= 0x30;
        this.cmd = 0;
        this.mode = 1;
        this.status |= STATUS_READY;
        return;
    }
    if(this.cmd === 0x60)
    {
        // (lock, unlock, lock down: nothing is locked)
        this.cmd = 0;
        this.mode = 1;
        this.status |= STATUS_READY;
        return;
    }
    switch(cmd)
    {
        case 0xFF: case 0x00: case 0xF0:
            this.mode = 0;
            break;
        case 0x70:
            this.mode = 1;
            break;
        case 0x50:
            this.status = 0;
            this.mode = 0;
            break;
        case 0x90:
            this.mode = 2;
            break;
        case 0x98:
            this.mode = 3;
            break;
        case 0x40: case 0x10: case 0x20: case 0x60:
            this.cmd = cmd;
            this.mode = 1;
            this.status |= STATUS_READY;
            break;
        case 0xE8:
            this.cmd = cmd;
            this.cycle = 1;
            this.mode = 1;
            this.status |= STATUS_READY;
            break;
        default:
            // (an unknown command: back to read array, as QEMU)
            this.mode = 0;
            break;
    }
};

CfiFlash.prototype.program = function(offset, width, value)
{
    let v = BigInt.asUintN(64, BigInt(value));
    let changed = false;
    for(let i = 0; i < width; i++, v >>= 8n)
    {
        if(offset + i < this.bytes.length)
        {
            this.bytes[offset + i] = Number(v & 0xFFn);
            changed = true;
        }
    }
    if(changed) this.on_write?.(offset, width);
};
