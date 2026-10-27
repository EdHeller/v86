# Test targets: everything copy.sh/v86 lists

Note for future sessions: the JIT work (the 32-bit JIT in `jit.rs` and the x86_64 JIT in `jit64.rs`) must make **all** of
these guests faster, or at least not break them. The list is the catalogue on https://copy.sh/v86/ (given by the user).
Check any JIT or CPU change against a representative from each family, not only Alpine and TempleOS, and measure
A/B (sequentially, never in parallel).

Images come from the catalogue (`debug.html`/`index.html` profile lists in this repo and `images.v86.copy.sh`);
existing runners and docs: `docs/dos.md`, `docs/windows-31x.md`, `docs/windows-9x.md`, `docs/windows-nt.md`,
`docs/archlinux.md`, `tests/x64/manual/` (TempleOS, Alpine x86_64, i686).

Sizes are the catalogue's download sizes. Family, bitness and medium in brackets.

## Linux and Linux-like

Android x86 1.6-r2 (54+ MB, CD) · Arch Linux (15+ MB, 9pfs; Xorg, Firefox) · Buildroot Linux (4.9 MB, bzImage) ·
Damn Small Linux (50 MB, 2.4/4.x kernel) · ELKS 4.11.rc2 with Firefox 2.0 (1.2 MB) · Linux 8086 (ELKS, 16-bit) ·
NodeOS (14 MB, bzImage) · Tilck (16 MB, Linux-compatible kernel)

## BSD, Hurd, other Unix

386BSD 0.1 (5+ MB) · FreeBSD 12.0 (16+ MB) · NetBSD 4.0 with Xorg (23+ MB) · OpenBSD 6.6 (11+ MB) ·
Arch Hurd (16+ MB) · BSD/OS (10+ MB, proprietary, gcc and perl5) · FiwixOS with Doom (4.2+ MB) · Minix 3.3 (30+ MB) ·
Redox (31+ MB, Rust) · SerenityOS (16+ MB, browser, games) · Sortix (67 MB) · Soso (7.6 MB) · Syllable (28+ MB) ·
Unix V7 x86 port (0.5+ MB) · QuantixOS (3.1 MB, asm + C)

## BeOS, Amiga-like, Plan 9

BeOS 5 (34+ MB) · Haiku (36+ MB) · AROS Broadway (25+ MB) · Icaros Desktop (60+ MB) · Tiny Aros (17+ MB) ·
9front (5.2+ MB) · 9legacy (13 MB)

## Windows

ReactOS (17+ MB) · Windows 1.01 (0.7 MB) · 2.03 (1.8 MB) · 3.0 (16 MB) · 3.1 (17 MB) · 95 (19+ MB) · 98 (13+ MB) ·
ME (28+ MB) · NT 3.1 (18+ MB) · NT 3.51 (28+ MB) · NT 4.0 (16+ MB) · 2000 (28+ MB)

## DOS and DOS-like

86-DOS 0.1 (0.1 MB) · Exploring IBM (0.1 MB) · FreeDOS (0.6 MB) · FreeGEM (1.6+ MB) · Xcom (1.3 MB) · MS-DOS 4 (0.5 MB) ·
MS-DOS 6.22 (2.4+ MB; Doom, Sim City, Turbo C) · PC-MOS/386 (0.7 MB) · PsychDOS (4.6+ MB) · lEEt/OS (0.5 MB) ·
DOOF (1.1 MB, Doom on one floppy) · Chimaera OS (29 KB)

## Custom and hobby OSes

BleskOS · Blue Jay · BoneOS · BootChess · CatK · Crazierl (Erlang) · Dusk OS (Forth) · Floppy Bird · GentleOS/16 and /32 ·
HelenOS · Hello v86 · House (Haskell) · JX (Java) · KolibriOS · Little Kernel · M/CP · MikeOS · Mobius · Mu ·
NanoShell · NewOS and NewOS Notion · Nope OS · Oberon (Native Oberon 2.3.6) · QNX 4.05 · SectorLISP · Skift ·
Snowdrop · Sol OS · Still Alive · T3XFORTH · TetrOS · ToaruOS · bootBASIC · Pillman · bootLogo · bootRogue · dino ·
Invaders · Sanos · sectorforth · Dancy · CuriOS · OS64 (C64 emulator OS, slow) · netboot.xyz (slow) · SqueakNOS ·
Chokanji 4 · Mojo OS · Vanadium OS · PrettyOS · Asuro · XENUS · bootOS · Chip4504 · ForthOS

(The pasted table was one row out of step with its names; this list matches names to their notes by hand. Check the
catalogue before relying on a size.)

## Not in the catalogue but in scope here

TempleOS (x86_64, `tests/x64/manual/templeos`) and Alpine x86_64 / i686 (`tests/x64/manual/alpine.js`).

## Suggested representative set for quick regression checks

DOS/16-bit: FreeDOS, MS-DOS 6.22 · Win 3.1, Win 98, Win 2000 · Linux: Buildroot, Arch · BSD: FreeBSD, OpenBSD ·
KolibriOS (asm, graphics) · Haiku or BeOS (heavy 32-bit C++) · ReactOS · TempleOS and Alpine x86_64 (64-bit JIT).
