# TempleOS under the x86_64 emulation

    curl -o TempleOS.ISO https://templeos.org/Downloads/TempleOS.ISO
    TOS_ISO=TempleOS.ISO python3 patch_iso.py TempleOS-selftest.ISO
    WASM=build/v86.wasm node ../templeos.js TempleOS-selftest.ISO 360 shot.png

The patched ISO answers the install and tour prompts itself and prints `SELFTEST DONE` plus
some integer and floating point results on the serial port (expected: `1+2=3`, `fact10=3628800`,
`sqrt2=1.414214`, `sin1=0.841471`). Boot takes about two minutes with the release wasm.

Why a patched ISO: after boot TempleOS only reads the keyboard through IRQ1 and its window
manager polls the mouse port for stray bytes, so scripted keystrokes sent to the install prompt
were not accepted reliably (see the notes in `../templeos.js`). `tosfile.py` reads (and
decompresses, `tosz.py`) files of the ISO's RedSea file system.

## Installing to a blank hard disk

Cold boot with `HDA_FILE=disk.img HDA_MB=256`, keys `TOS_KEYS='[[85,"y"],[205,"y"],[225," "]]'`
(install? yes; VM? yes; press a key), and wait until `Option <ENTER> when done` shows (about 2 hours
of emulated time at current speed; watch the `hda:` page counter and use `TOS_CMD` `shot:`/`save:`/`quit`).
The installer in VM mode leaves the MBR without boot code, so write `chainload_mbr.asm`
(`nasm -f bin`, 85 bytes) into the first 446 bytes of the image, then boot with `NO_CDROM=1 HDA_FILE=disk.img`.
Verified: TempleOS boots from the disk alone to the desktop with C:/Home populated.
