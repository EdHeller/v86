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

## Running programs from the command line

Type the command and press Enter (the runner's `type:` command takes `\n` for Enter, `\e` for Esc,
`\u \d \l \r` for the arrow keys; `mouse:dx,dy[,b]` and `down:<b>` drive the mouse, the pointer moves
about dx/2 pixels):

- `Dir;` lists the current directory (`Dir("::/Demo");` any other; subdirectories are blue links).
- `Cd("::/Demo/Graphics");` changes directory; `DocClear;` clears the screen (there is no `Cls`); `Help;` opens the help index.
- `#include "::/Demo/Graphics/Elephant";` runs a demo (`::` is the boot drive root; names are case sensitive).
- `Type("::/Demo/Graphics/Lattice.HC.Z");` shows a source file.
- Games: `#include "::/Apps/Titanium/Run";`, `KeepAway`, `X-Caliber`, `ToTheFront`.
- Esc or Shift-Esc ends most programs. `Life` is a drawing demo (hold the left button and drag, any key exits);
  `Lattice` is a turtle: Up/Down speed, Left/Right turn rate, Space draws a segment, `n` restarts, Esc exits
  (it slows down as speed and width grow, since every frame re-plots the whole path).

## Full test suite and DunGen repro

`TOS_SUITE=1 TOS_ISO=TempleOS.ISO python3 patch_iso.py out.ISO` makes an ISO that runs the whole OSTestSuite (Tour part 12)
and prints `TS <n>` per test on serial; `TOS_SUITE=dungen` runs only DunGen with a jiffies heartbeat. With
`templeos.js`: `TOS_FPS`, `TOS_SHOTS`, `TOS_SAMPLE`, `JIT64` (`4294967295` = jit off, `0` = compile everything) and
`JIT64_MASK`. Findings and the next steps are in `docs/next-session.md`.
