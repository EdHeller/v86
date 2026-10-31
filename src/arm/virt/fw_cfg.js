// QEMU's firmware configuration device on the virt machine (docs/arm64.md, "UEFI"): fw_cfg's MMIO interface at
// 0x0902_0000, as edk2's QemuFwCfgLibMmio reads it. The data register at +0 (an access of n bytes takes the next n
// bytes of the item, in order), the selector at +8 (16 bits, big-endian), the DMA address at +16 (not offered: the
// revision is 1, the traditional interface). The items: the signature, the revision and the file directory, with
// the files the machine gives it (none by default: no etc/table-loader, so the firmware hands the guest the device
// tree, not ACPI).

export const VIRT_FW_CFG = 0x09020000;
export const VIRT_FW_CFG_SIZE = 0x18;

const FW_CFG_SIGNATURE = 0x00, FW_CFG_ID = 0x01, FW_CFG_NB_CPUS = 0x05, FW_CFG_MAX_CPUS = 0x0F, FW_CFG_FILE_DIR = 0x19;
const FW_CFG_FILE_FIRST = 0x20;

/**
 * @constructor
 * @param {{cores: number, files: (Object<string, Uint8Array>|undefined)}} options
 */
export function FwCfg(options)
{
    const le32 = n => new Uint8Array(new Uint32Array([n]).buffer);
    const le16 = n => new Uint8Array(new Uint16Array([n]).buffer);
    /** @type {!Map<number, !Uint8Array>} */
    this.items = new Map([
        [FW_CFG_SIGNATURE, new TextEncoder().encode("QEMU")],
        [FW_CFG_ID, le32(1)],
        [FW_CFG_NB_CPUS, le16(options.cores)],
        [FW_CFG_MAX_CPUS, le16(options.cores)],
    ]);
    const files = Object.entries(options.files || {}).sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0);
    const dir = new Uint8Array(4 + 64 * files.length);
    const view = new DataView(dir.buffer);
    view.setUint32(0, files.length);
    files.forEach(([name, bytes], i) => {
        const select = FW_CFG_FILE_FIRST + i;
        view.setUint32(4 + 64 * i, bytes.length);
        view.setUint16(4 + 64 * i + 4, select);
        dir.set(new TextEncoder().encode(name).subarray(0, 55), 4 + 64 * i + 8);
        this.items.set(select, bytes);
    });
    this.items.set(FW_CFG_FILE_DIR, dir);
    this.selected = new Uint8Array(0);
    this.offset = 0;
}

FwCfg.prototype.reset = function()
{
    this.selected = new Uint8Array(0);
    this.offset = 0;
};

FwCfg.prototype.select = function(key)
{
    this.selected = this.items.get(key) || new Uint8Array(0);
    this.offset = 0;
};

FwCfg.prototype.next = function()
{
    return this.offset < this.selected.length ? this.selected[this.offset++] : 0;
};

FwCfg.prototype.read = function(offset, width)
{
    if(offset < 8)
    {
        let value = 0n;
        for(let i = 0; i < width; i++) value |= BigInt(this.next()) << BigInt(8 * i);
        return width === 8 ? value : Number(value);
    }
    // (the selector reads as 0, the DMA register as nothing)
    return 0;
};

FwCfg.prototype.write = function(offset, width, value)
{
    if(offset === 8 && width === 2)
    {
        const v = Number(value);
        this.select((v & 0xFF) << 8 | v >> 8);
    }
};
