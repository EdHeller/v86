// Reading the files of an ISO 9660 image (Alpine's boot files), enough to take a kernel and an initramfs out of it

/** the files of an ISO 9660 image: path (upper case, without ;1) -> [offset, length] */
export function iso_files(image) {
    const bytes = new Uint8Array(image), view = new DataView(image);
    const files = new Map();
    const walk = (lba, size, prefix) => {
        for(let i = 0; i < size;)
        {
            const length = bytes[lba * 2048 + i];
            if(!length) { i = (Math.floor(i / 2048) + 1) * 2048; continue; }
            const at = lba * 2048 + i;
            const extent = view.getUint32(at + 2, true), file_size = view.getUint32(at + 10, true), flags = bytes[at + 25];
            const name = new TextDecoder().decode(bytes.subarray(at + 33, at + 33 + bytes[at + 32])).replace(/;1$/, "").replace(/\.$/, "");
            i += length;
            if(name === "\0" || name === "\x01") continue;
            if(flags & 2) walk(extent, file_size, prefix + name + "/");
            else files.set(prefix + name, [extent * 2048, file_size]);
        }
    };
    const root_record = 16 * 2048 + 156;
    walk(view.getUint32(root_record + 2, true), view.getUint32(root_record + 10, true), "");
    return files;
}
