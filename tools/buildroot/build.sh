#!/usr/bin/env bash
#
# build.sh — small Buildroot 2026.08 development images for the v86 emulator
#            (https://github.com/copy/v86), built under WSL or any Linux.
#
# Builds ONE environment per run; each gets its own bzImage + rootfs.cpio.zstd:
#
#   --no-gui              console only: bash, nano, gcc/g++, SDL3 headers, sound
#   --qemu                Xorg modesetting on bochs-drm (v86's Bochs/QEMU VGA), 1024x768x24
#   --vesa WxHxBPP        VESA framebuffer, like Damn Small Linux's vga=: the kernel sets
#                         the VBE mode at boot, then vesafb + Xorg's fbdev driver
#                         WxH: 800x600 or 1024x768      BPP: 16, 24 or 32
#                         (24 bpp runs Xvesa like DSL: packed pixels, 3/4 the video memory)
#   --smoke               smallest real build (prebuilt Bootlin toolchain + BusyBox, a few
#                         minutes): checks downloads, the mirror and ccache on your machine.
#                         Run it twice: the 2nd run should be much faster.
#   --min                 fast graphics for a slow host: --vesa 800x600x16, windows move
#                         as an OUTLINE (with their position) instead of being redrawn,
#                         no font antialiasing anywhere, 1 workspace
#
# Named builds (shortcuts; the same options spelled out get the same short name):
#   --dev-env             --min --SDL3-net --man --no-shellcheck --upx -> v86-dev-env-*
#   --bash-env            --min --no-SDL3 --man --tcc --wincc         -> v86-bash-env-*
#                         With --release: v86-dev-env-release-*, v86-bash-env-release-*.
#                         Other options added after one change the build and give it the
#                         usual long name, e.g. --dev-env --xz -> min-sdl3net-...-xz
#
# GUI images: Xorg + Fluxbox (dark style by default, stock styles one click away),
# xterm, XFE, nano, SDL3 (X11) + gcc/g++ with full C/C++ headers.
# Every image: UTF-8 (LANG=C.UTF-8, BusyBox Unicode on). GUI images also have mlterm
# (right-click menu > Terminal), which joins flags, skin tones and ZWJ emoji (HarfBuzz; --no-mlterm
# leaves it out). GUI images: GNU Unifont (every
# Unicode character, 8x16/16x16 bitmap design) + Twemoji colour emoji in xterm, and
# fbterm with Unifont (monochrome emoji) as the text console on tty2-6. --no-gui stays
# in VGA text mode (IBM-437 characters only).
#
#   --all                 build the named builds in turn, release and debug:
#                           --dev-env --release, --dev-env, --bash-env --release, --bash-env
#                         Settings and build options you add apply to each, e.g.
#                         ./build.sh --all --dry-run   or   ./build.sh --all IMAGE_DIR=/mnt/c/vm
#                         (--release/--debug don't go with it: it builds both). A failed
#                         image doesn't stop the rest; a summary comes at the end.
#
# Extras (any environment)
#   --no-SDL3             leave SDL3 out (default: SDL3 + headers, X11 backend in GUI images)
#   --SDL3                keep SDL3 (the default; undoes an earlier --no-SDL3)
#   --SDL3-net            add SDL3_net 3.2.0 (networking for SDL3; implies --SDL3)
#   --release             leave out the test programs (selftest, devcheck, soundtest, unitest,
#                         emojitest, symboltest)
#   --debug               keep the test programs (the default). One or the other: giving both
#                         is an error
#   --man                 man pages: every package's pages + the Linux man-pages (C library,
#                         system calls), read with mandoc's man/apropos/whatis and less.
#                         Default: on, except --min (adds ~15 MB of RAM)
#   --no-man              leave the man pages out
#   --no-gcc              leave out the on-target compiler: gcc, g++, binutils, pkgconf
#                         and the C/C++ headers (the biggest part of every image).
#                         Programs are then built elsewhere; SDL3 stays a runtime library
#   --gcc                 keep the compiler (the default; undoes an earlier --no-gcc)
#   --no-mlterm           desktop images without mlterm (the terminal that joins emoji
#                         sequences) and what only it needs: cairo and HarfBuzz (~2.3 MB).
#                         xterm stays. No effect on --no-gui, which never has mlterm
#   --mlterm              keep mlterm (the default; undoes an earlier --no-mlterm)
#   --no-adie             desktop images without Adie, FOX's programmer's editor (~0.4 MB:
#                         syntax highlighting for C, shell, make...; reuses XFE's toolkit)
#   --adie                keep Adie (the default; undoes an earlier --no-adie)
#   --no-shellcheck       leave ShellCheck out (the shell script checker). Nobody publishes
#                         32-bit x86 binaries of it, so build.sh uses, in this order:
#                           1. the file shellcheck-*-i386 next to build.sh, made by
#                              build-shellcheck.sh (~18 MB; ~4 MB with upx installed), or
#                           2. Debian's i386 package (56 MB: costs 56 MB of RAM)
#   --shellcheck          keep ShellCheck (the default; undoes an earlier --no-shellcheck)
#   --tcc                 add TinyCC for Linux programs: tcc, a C compiler ~20x faster than
#                         gcc (C only, ~0.4 MB; uses gcc's C headers, so not with --no-gcc)
#   --no-tcc              leave it out (the default; undoes an earlier --tcc)
#   --wincc               add TinyCC's Windows cross compilers: wincc hello.c -o hello.exe
#                         makes a 64-bit Windows program, wincc -m32 a 32-bit one (C only, ~2 MB)
#   --no-wincc            leave them out (the default; undoes an earlier --wincc)
#   --upx                 add UPX, the program packer: upx prog shrinks a Linux program or a
#                         Windows .exe/.dll, which unpacks itself when run (~2.6 MB)
#   --bash-dev            shell-script development kit: GNU sed, awk (gawk), grep, find/xargs,
#                         coreutils (sort, date, stat...) and diffutils instead of BusyBox's
#                         look-alikes; bash-completion, a script-friendly prompt/history/inputrc,
#                         strace, ltrace, shfmt (formatter), bats (tests), jq, tree, bc, curl, nc,
#                         lsof, ps/top (procps), time, patch, tmux and a `newscript` helper
#   --bashdb              add bashdb 5.2-1.2.0, a debugger for bash scripts: bashdb script.sh
#                         (only bash scripts, under 1 MB)
#   --no-bashdb           leave it out (the default)
#   --gdb                 add gdb and gdbserver (about 12 MB installed in Alpine; with python
#                         off); programs built on the target with gcc -g can then be debugged
#   --no-gdb              leave it out (the default)
#   --no-bash-dev         leave it out (the default, except --bash-env; undoes --bash-dev)
#   --no-upx              leave it out (the default, except --dev-env; undoes an earlier --upx)
#   --xz                  compress kernel + rootfs with xz instead of zstd: ~12% smaller for
#                         small images, up to ~35% for images with gcc, but 7-11x slower to
#                         unpack at every boot (seconds for small images, much more for big
#                         ones). Rootfs: -9e, x86 filter, 32 MB dictionary (the kernel holds
#                         it in RAM while unpacking). Best for the smallest images.
#   --zstd                zstd (the default; undoes an earlier --xz)
#   --no-apic             kernel without the local/IO APIC (the default): interrupts through
#                         the classic 8259 PIC and the PIT timer (like DSL's 2.4 kernel). Also
#                         drops the perf "msr" probe of MSR 0x64e, which v86's debug build stops on.
#   --apic                keep the local/IO APIC (undoes the default --no-apic)
#   Last one wins: --no-SDL3 also drops SDL3_net, --SDL3-net brings SDL3 back.
#   These add a suffix to the image name (e.g. v86-qemu-sdl3net-release-*); man pages
#   only when not the default: v86-min-man-*, v86-qemu-noman-*; -nogcc for --no-gcc,
#   -nomlterm for --no-mlterm, -noadie for --no-adie, -noshellcheck for --no-shellcheck, -tcc for --tcc, -wincc for --wincc, -upx for --upx,
#   -xz for --xz, -apic for --apic.
#   Smallest image: --no-gui --no-SDL3 --no-man --no-gcc --no-shellcheck --release --xz
#
# Caches (kept between runs; downloads are always kept unless you clean them)
#   --no-cache            build without the host-tool snapshot and ccache (same as
#                         HOST_CACHE=0 USE_CCACHE=0): slow, but nothing cached is used or saved
#   --clean               delete the host-tool snapshots, ccache, the build folder and the
#                         dry runs (keeps downloads, Buildroot's source and logs). On its own
#                         it only cleans; with an environment it cleans, then builds:
#                         ./build.sh --clean        ./build.sh --clean --qemu
#   --clean-downloads     like --clean, and also empties the download cache (DL_DIR)
#
# Options
#   --dry-run             generate + check everything, download nothing, build nothing.
#                         With a Buildroot tree (or its cached tarball) present it also
#                         runs Kconfig and lists which sources are not cached yet.
#   (automatic reuse)     building the same image again with the same packages and
#                         settings reuses the previous build folder: only the overlay,
#                         post-build and image packing run again (about a minute). Any
#                         change to packages, the defconfig or the kernel config gives a
#                         fresh build. Changed overlay files are always picked up.
#   --fresh               always build from scratch (the caches still help)
#   --incremental         force-reuse the previous build folder of this image, even if its
#                         packages or settings changed (Buildroot won't rebuild those)
#   -j N                  parallel jobs (default: all cores)
#   -h, --help
#
# Settings (environment or NAME=value arguments)
#   WORK=~/test               where Buildroot is unpacked and built (Linux FS, not /mnt/*)
#   IMAGE_DIR=/mnt/c/Projects where the finished bzImage + rootfs are copied
#                             (default: /mnt/c/Projects under WSL, else <script>/images;
#                             the old name ART_DIR still works)
#   IMAGE_URL=/images/        the images' folder on your web site, for the V86 settings printed
#                             at the end of a build
#   DL_DIR=<script>/downloads download cache (source tarballs), shared by all builds
#   HOST_CACHE=1              host-tool cache (default on; 0 turns it off), kept outside the
#                             build folder so a fresh build never removes it:
#                             the toolchain + all host tools are saved after a fresh build and
#                             restored by ANY later build (any image) with identical toolchain
#                             settings and host packages. All images build in one folder,
#                             $WORK/build, so snapshots are shared; the newest 4 are kept.
#                             Logs: $WORK/logs/<image>.log
#   HOST_CACHE_DIR=~/test/host-cache   where the host-tool snapshots are kept
#   USE_CCACHE=1              compiler cache in $WORK/ccache (default on; 0 turns it off).
#                             The first build fills it; later fresh builds reuse it.
#   USER_OVERLAY=~/test/overlay   extra files copied into the image, if the folder exists
#   BR_SRC=<dir>              use an existing Buildroot 2026.08 tree
#   DL_RETRIES=6              download retries (DNS failures, flaky mirrors)
#   ALLOW_ROOT=1              allow building as root
#
# Examples
#   ./build.sh --min
#   ./build.sh --vesa 1024x768x32
#   ./build.sh --dry-run --qemu
#
set -Eeuo pipefail

BR_VER=2026.08
SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)

C1='' C2='' C3='' C4='' C0=''; [[ -t 1 ]] && C1=$'\033[1;32m' C2=$'\033[1;33m' C3=$'\033[1;31m' C4=$'\033[1;36m' C0=$'\033[0m'
msg()  { printf '%s[build]%s %s\n' "$C1" "$C0" "$*"; }
warn() { printf '%s[warn ]%s %s\n' "$C2" "$C0" "$*" >&2; }
die()  { printf '%s[fatal]%s %s\n' "$C3" "$C0" "$*" >&2; exit 1; }
step() { printf '\n%s━━ %s ━━%s\n' "$C4" "$*" "$C0"; }
have() { command -v "$1" >/dev/null 2>&1; }
usage() { sed -n '2,/^set -E/{/^set -E/d;s/^# \{0,1\}//;p}' "${BASH_SOURCE[0]}"; }
trap 'die "aborted at line $LINENO"' ERR

# ══════════════════════════════════════════════════════════════════════════════
#  Arguments
# ══════════════════════════════════════════════════════════════════════════════

MODE="" W=0 H=0 BPP=0 BOOT_BPP=0 MIN=0 DRY=0 INCR=0 VID_MODE="" SDL=1 SDLNET=0 RELEASE=0 PICKED="" MLTERM=1 ADIE=1 SHELLCHECK=1 TCC=0 WINCC=0 UPX=0 BASHDEV=0 BASHDB=0 GDB=0 ALL=0 MAN="" GCC=1 XZ=0 APIC=0 CLEAN=0 FRESH=0 REUSE=0
ARGS=("$@")                                     # for --all: passed on to every image
set_mode() { [[ -z "$MODE" ]] || die "choose only one of --no-gui / --qemu / --vesa / --min"; MODE=$1; }
parse_vesa() {
    [[ "$1" =~ ^([0-9]+)x([0-9]+)x([0-9]+)$ ]] || die "--vesa wants WxHxBPP, e.g. 800x600x16 (got '$1')"
    W=${BASH_REMATCH[1]} H=${BASH_REMATCH[2]} BPP=${BASH_REMATCH[3]}
    [[ "${W}x$H" == 800x600 || "${W}x$H" == 1024x768 ]] \
        || die "--vesa resolution must be 800x600 or 1024x768 (got ${W}x$H$([[ $W == 1028 ]] && echo ' — did you mean 1024?'))"
    [[ "$BPP" =~ ^(16|24|32)$ ]] || die "--vesa depth must be 16, 24 or 32 bpp (got $BPP)"
    # VBE mode numbers from v86's own BIOS (bios/vgabios.bin = SeaVGABIOS, bochs table).
    # The kernel's boot code takes 0x200 + the VBE number.
    # 24 bpp: Xorg 21.1 can't drive packed 24 bpp (the vesa driver refuses it when 32 bpp
    # modes exist, fbdev asks vesafb for 32 bpp), so those images run Xvesa (TinyX, what
    # DSL uses), which sets the 24 bpp VBE mode itself. vesafb boots at 32 bpp so Xorg's
    # fbdev driver is a working fallback.
    BOOT_BPP=$BPP; ((BPP == 24)) && BOOT_BPP=32
    local vbe
    case "${W}x${H}x$BOOT_BPP" in
        800x600x16)  vbe=0x114 ;;  800x600x24)  vbe=0x115 ;;  800x600x32)  vbe=0x143 ;;
        1024x768x16) vbe=0x117 ;;  1024x768x24) vbe=0x118 ;;  1024x768x32) vbe=0x144 ;;
    esac
    VID_MODE=$(printf '0x%x' $((vbe + 0x200)))
}

while (($#)); do
    case "$1" in
        --no-gui|--nogui) set_mode nogui ;;
        --qemu|--bochs)   set_mode qemu ;;
        --vesa)           set_mode vesa; parse_vesa "${2:-}"; shift ;;
        --vesa=*)         set_mode vesa; parse_vesa "${1#*=}" ;;
        --min)            set_mode vesa; parse_vesa 800x600x16; MIN=1 ;;
        # named builds: the options they stand for, in their place (later options still apply)
        --dev-env)        shift; set -- --min --SDL3-net --man --no-shellcheck --upx "$@"; continue ;;
        --bash-env)       shift; set -- --min --no-SDL3 --man --tcc --wincc --bash-dev "$@"; continue ;;
        --smoke)          set_mode smoke ;;
        --all)            ALL=1 ;;
        --no-SDL3|--no-sdl3)   SDL=0 SDLNET=0 ;;   # last one wins
        --SDL3|--sdl3)         SDL=1 ;;
        --SDL3-net|--sdl3-net) SDLNET=1 SDL=1 ;;
        --release)        [[ "${PICKED:-}" == debug ]] && die "--release and --debug: pick one"
                          RELEASE=1 PICKED=release ;;
        --debug)          [[ "${PICKED:-}" == release ]] && die "--release and --debug: pick one"
                          RELEASE=0 PICKED=debug ;;
        --man)            MAN=1 ;;
        --no-man)         MAN=0 ;;
        --no-gcc)         GCC=0 ;;
        --no-mlterm)      MLTERM=0 ;;
        --no-adie)        ADIE=0 ;;
        --adie)           ADIE=1 ;;
        --mlterm)         MLTERM=1 ;;
        --no-shellcheck)  SHELLCHECK=0 ;;
        --shellcheck)     SHELLCHECK=1 ;;
        --tcc)            TCC=1 ;;
        --no-tcc)         TCC=0 ;;
        --wincc)          WINCC=1 ;;
        --no-wincc)       WINCC=0 ;;
        --upx)            UPX=1 ;;
        --bash-dev)       BASHDEV=1 ;;
        --bashdb)         BASHDB=1 ;;
        --no-bashdb)      BASHDB=0 ;;
        --gdb)            GDB=1 ;;
        --no-gdb)         GDB=0 ;;
        --no-bash-dev)    BASHDEV=0 ;;
        --no-upx)         UPX=0 ;;
        --gcc)            GCC=1 ;;
        --xz)             XZ=1 ;;
        --zstd)           XZ=0 ;;
        --no-apic)        APIC=0 ;;
        --apic)           APIC=1 ;;
        --dry-run)        DRY=1 ;;
        --no-cache)       HOST_CACHE=0 USE_CCACHE=0 ;;
        --clean)          ((CLEAN)) || CLEAN=1 ;;
        --clean-downloads) CLEAN=2 ;;
        --incremental)    INCR=1 ;;
        --fresh)          FRESH=1 ;;
        -j)               JOBS=${2:?-j needs a number}; shift ;;
        -j*)              JOBS=${1#-j} ;;
        -h|--help)        usage; exit 0 ;;
        [A-Z_]*=*)        [[ "${1%%=*}" =~ ^(WORK|IMAGE_DIR|IMAGE_URL|ART_DIR|DL_DIR|USE_CCACHE|HOST_CACHE|HOST_CACHE_DIR|USER_OVERLAY|BR_SRC|DL_RETRIES|ALLOW_ROOT|JOBS)$ ]] \
                              || die "unknown setting: ${1%%=*}"
                          declare -g "${1%%=*}=${1#*=}" ;;
        *)                die "unknown option: $1 (try --help)" ;;
    esac
    shift
done
# ── --clean / --clean-downloads ──────────────────────────────────────────────
clean_caches() {
    local w hc dl t sz lock targets=()
    w=$(readlink -m "${WORK:-$HOME/test}")
    hc=$(readlink -m "${HOST_CACHE_DIR:-$w/host-cache}")
    dl=$(readlink -m "${DL_DIR:-$SCRIPT_DIR/downloads}")
    targets=("$hc" "$w/ccache" "$w/build" "$w/dry-run")
    ((CLEAN == 2)) && targets+=("$dl")
    # never under a running build
    if have flock && [[ -e "$w/build.lock" ]]; then
        exec {lock}>"$w/build.lock"
        flock -n "$lock" || die "a build is running in $w: wait for it to finish, then clean"
    fi
    step "Cleaning caches"
    for t in "${targets[@]}"; do
        case "$t" in ""|/|"$HOME"|"$w"|"$SCRIPT_DIR") die "refusing to delete '$t'" ;; esac
        if [[ -e "$t" ]]; then
            sz=$(du -sh "$t" 2>/dev/null | cut -f1 || true)
            rm -rf -- "$t"
            msg "removed ${sz:-?}  $t"
        else
            msg "already empty  $t"
        fi
    done
    ((CLEAN == 2)) || msg "kept downloads: $dl (--clean-downloads empties them too)"
    [[ -n "${lock:-}" ]] && exec {lock}>&-          # the build below takes its own lock
    return 0
}
if ((CLEAN)); then
    clean_caches
    [[ -n "$MODE" ]] || ((ALL)) || exit 0           # only cleaning was asked for
fi

# ── --all: run this script once per image, then summarise ───────────────────
ALL_TARGETS=("--dev-env --release" "--dev-env" "--bash-env --release" "--bash-env")
if ((ALL)); then
    [[ -z "$MODE" ]] || die "--all builds dev-env and bash-env: leave out --no-gui / --qemu / --vesa / --min / --smoke / --dev-env / --bash-env"
    [[ -z "${PICKED:-}" ]] || die "--all builds both release and debug images: leave out --$PICKED"
    pass=() i=0
    while ((i < ${#ARGS[@]})); do               # everything except --all is passed on
        a=${ARGS[i]}
        case "$a" in --all|--clean|--clean-downloads) ;; *) pass+=("$a") ;; esac   # cleaned once, above
        i=$((i + 1))
    done
    results=() failed=0 t_all=$SECONDS
    # each build adds the image files it delivers here ("bytes path"), for the list at the end
    V86_ALL_REPORT=$(mktemp "${TMPDIR:-/tmp}/v86-all.XXXXXX"); export V86_ALL_REPORT
    for t in "${ALL_TARGETS[@]}"; do
        printf '\n%s════ build.sh %s %s ════%s\n' "$C4" "$t" "${pass[*]}" "$C0"
        t0=$SECONDS
        # through bash: works even if build.sh lost its execute bit (fresh download)
        # shellcheck disable=SC2086
        if bash "${BASH_SOURCE[0]}" $t ${pass[@]+"${pass[@]}"}; then status=OK; else status=FAILED; failed=$((failed + 1)); fi
        results+=("$(printf '%-7s %-30s %3dm%02ds' "$status" "$t ${pass[*]}" $(((SECONDS - t0) / 60)) $(((SECONDS - t0) % 60)))")
    done
    printf '\n%s════ --all summary (%dm%02ds) ════%s\n' "$C4" $(((SECONDS - t_all) / 60)) $(((SECONDS - t_all) % 60)) "$C0"
    printf '  %s\n' "${results[@]}"
    if [[ -s "$V86_ALL_REPORT" ]]; then
        printf '\n%s════ images (size in bytes) ════%s\n' "$C4" "$C0"
        while read -r bytes f; do
            printf '  %12s  %s\n' "$bytes" "$f"
        done < "$V86_ALL_REPORT"
    else
        msg "no images were made$( ((DRY)) && echo ' (dry run)')"
    fi
    rm -f "$V86_ALL_REPORT"
    ((failed == 0)) && msg "all ${#ALL_TARGETS[@]} images OK" || warn "$failed of ${#ALL_TARGETS[@]} failed: logs are in ${WORK:-$HOME/test}/logs/<image>.log"
    exit "$failed"
fi
[[ -n "$MODE" ]] || { usage; die "pick an environment: --no-gui, --qemu, --vesa WxHxBPP, --min or --all"; }

GUI=1; [[ "$MODE" == nogui || "$MODE" == smoke ]] && GUI=0
case "$MODE" in
    nogui) NAME=nogui ;;
    smoke) NAME=smoke ;;
    qemu)  NAME=qemu; W=1024 H=768 BPP=32 ;;     # bochs-drm: 32 bpp framebuffer, X depth 24
    vesa)  NAME="vesa-${W}x${H}x${BPP}"; ((MIN)) && NAME=min ;;
esac
# man pages: on by default, except --min (RAM) and the smoke test
MAN_DEFAULT=1
if [[ "$MODE" == smoke ]] || ((MIN)); then MAN_DEFAULT=0; fi
[[ -n "$MAN" ]] || MAN=$MAN_DEFAULT
[[ "$MODE" == smoke ]] && MAN=0 SHELLCHECK=0 TCC=0 WINCC=0 UPX=0 BASHDEV=0 BASHDB=0 GDB=0
((TCC && !GCC)) && die "--tcc uses gcc's C headers and crt files: leave out --no-gcc, or use only --wincc"
if [[ "$MODE" != smoke ]]; then                  # extras get their own name: no collisions
    ((SDL)) || NAME+="-nosdl3"
    ((SDLNET)) && NAME+="-sdl3net"
    ((GCC)) || NAME+="-nogcc"
    ((GUI && !MLTERM)) && NAME+="-nomlterm"
    ((GUI && !ADIE)) && NAME+="-noadie"
    ((SHELLCHECK)) || NAME+="-noshellcheck"
    ((TCC)) && NAME+="-tcc"
    ((WINCC)) && NAME+="-wincc"
    ((UPX)) && NAME+="-upx"
    ((BASHDEV)) && NAME+="-bashdev"
    ((BASHDB)) && NAME+="-bashdb"
    ((GDB)) && NAME+="-gdb"
    ((MAN == MAN_DEFAULT)) || NAME+=$( ((MAN)) && echo "-man" || echo "-noman")
    ((RELEASE)) && NAME+="-release"
    ((XZ)) && NAME+="-xz"
    ((APIC)) && NAME+="-apic"
    # named builds: exactly these options (with --dev-env / --bash-env or spelled out) get a
    # short name, debug or --release (then -release on the end, so the two never overwrite
    # each other's images); anything else added gives the usual long name
    case "${NAME%-release}" in
        min-sdl3net-noshellcheck-upx-man) NAME=dev-env${NAME#min-sdl3net-noshellcheck-upx-man} ;;
        min-nosdl3-tcc-wincc-bashdev-man)     NAME=bash-env${NAME#min-nosdl3-tcc-wincc-bashdev-man} ;;
    esac
fi
[[ "$MODE" == smoke ]] && XZ=0
ROOTFS_EXT=$( ((XZ)) && echo xz || echo zst )     # what Buildroot writes: rootfs.cpio.<ext>

IMAGE_DIR=${IMAGE_DIR:-${ART_DIR:-}}            # ART_DIR: the setting's old name
# remember which settings came from you (environment or NAME=value) for the summary
declare -A SET_BY
for v in WORK IMAGE_DIR DL_DIR BR_SRC USER_OVERLAY JOBS USE_CCACHE HOST_CACHE HOST_CACHE_DIR DL_RETRIES; do
    if [[ -n "${!v:-}" ]]; then SET_BY[$v]="set"; else SET_BY[$v]="default"; fi
done

WORK=$(readlink -m "${WORK:-$HOME/test}")
DL_DIR=$(readlink -m "${DL_DIR:-$SCRIPT_DIR/downloads}")
BR_SRC=$(readlink -m "${BR_SRC:-$WORK/buildroot-$BR_VER}")
USER_OVERLAY=$(readlink -m "${USER_OVERLAY:-$WORK/overlay}")
HOST_CACHE_DIR=$(readlink -m "${HOST_CACHE_DIR:-$WORK/host-cache}")
if [[ -z "$IMAGE_DIR" ]]; then
    if [[ -d /mnt/c ]]; then IMAGE_DIR=/mnt/c/Projects; else IMAGE_DIR=$SCRIPT_DIR/images; fi
fi
JOBS=${JOBS:-$(nproc 2>/dev/null || echo 4)}
IMAGE_URL=${IMAGE_URL-/images/}                 # where the page finds the images (printed config)
USE_CCACHE=${USE_CCACHE:-1}
HOST_CACHE=${HOST_CACHE:-1}
DL_RETRIES=${DL_RETRIES:-6}
BR_TARBALL="buildroot-$BR_VER.tar.gz"
BR_URL="https://buildroot.org/downloads/$BR_TARBALL"

# per-environment build dir: ext/ (generated BR2_EXTERNAL tree) + output/
# Real builds of every image share one folder: Buildroot bakes the output path into the
# host tools, so a shared path is what lets one host-tool snapshot serve all images.
# Builds are fresh anyway; images go to IMAGE_DIR, logs to $WORK/logs/<image>.log.
if ((DRY)); then BUILD="$WORK/dry-run/$NAME"; else BUILD="$WORK/build"; fi
BUILD_LOG="$WORK/logs/$NAME.log"
# the build folder is wiped by every fresh build: no cache may live inside it
for v in HOST_CACHE_DIR DL_DIR; do
    case "${!v}/" in "$WORK/build/"*) die "$v=${!v} is inside $WORK/build, which every fresh build wipes; pick another folder" ;; esac
done
case "$WORK/ccache/" in "$WORK/build/"*) die "ccache would be wiped" ;; esac
EXT="$BUILD/ext"
OUT="$BUILD/output"
OVL="$EXT/overlay"

[[ "$JOBS" =~ ^[1-9][0-9]*$ ]] || die "invalid job count: $JOBS"
[[ "$DL_RETRIES" =~ ^[1-9][0-9]*$ ]] || die "invalid DL_RETRIES: $DL_RETRIES"
for v in WORK DL_DIR BR_SRC USER_OVERLAY IMAGE_DIR HOST_CACHE_DIR; do
    [[ "${!v}" != *[[:space:]]* ]] || die "$v contains whitespace: '${!v}'"
done

# Run a Buildroot make target for this environment (out-of-tree: the Buildroot
# source stays pristine and is shared by all environments).
br() { BR2_DL_DIR="$DL_DIR" make --no-print-directory -C "$BR_SRC" O="$OUT" BR2_EXTERNAL="$EXT" "$@"; }

# retry N cmd... : up to N attempts, backing off 10s, 20s, 40s ... (max 5 min)
retry() {
    local tries=$1 n delay=10; shift
    for ((n = 1; ; n++)); do
        "$@" && return 0
        ((n < tries)) || return 1
        warn "attempt $n/$tries failed — retrying in ${delay}s (finished downloads are kept)"
        sleep "$delay"; ((delay = delay * 2 > 300 ? 300 : delay * 2))
    done
}

# Buildroot's default "wget -t 3" treats DNS errors ("unable to resolve host
# address") as fatal; --retry-on-host-error (wget >= 1.20) makes them retryable.
WGET_OPTS=(-nd -t 5 --waitretry=5 --retry-connrefused --connect-timeout=20 --read-timeout=60)
[[ "$(wget --help 2>/dev/null)" == *--retry-on-host-error* ]] && WGET_OPTS+=(--retry-on-host-error)

# write stdin to a file (creating its folder); putx also makes it executable
put()  { mkdir -p "$(dirname "$1")"; cat > "$1"; }
putx() { put "$1"; chmod +x "$1"; }


# ══════════════════════════════════════════════════════════════════════════════
#  Preflight
# ══════════════════════════════════════════════════════════════════════════════

# Every setting in effect, default or not, and what each folder is for.
show_settings() {
    local what run="fresh, or reuse of the last $NAME build if nothing changed" files size cc
    case "$MODE" in
        nogui) what="console only" ;;
        smoke) what="smoke test: prebuilt toolchain + BusyBox" ;;
        qemu)  what="Xorg modesetting on bochs-drm, 1024x768, 32 bpp" ;;
        vesa)  if ((BPP != BOOT_BPP)); then what="Xvesa ${W}x${H} ${BPP} bpp (boot framebuffer ${BOOT_BPP} bpp, Xorg fallback)"
               else what="vesafb + Xorg fbdev ${W}x${H} ${BPP} bpp"; fi
               ((MIN)) && what+=", outline window moves" ;;
    esac
    ((FRESH)) && run="fresh (--fresh)"
    ((INCR)) && run="incremental (reuses output/)"
    ((DRY)) && run="dry run (no downloads, no build)"
    files=$(find "$DL_DIR" -type f 2>/dev/null | wc -l || true); size=$(du -sh "$DL_DIR" 2>/dev/null | cut -f1 || true)
    if [[ "$USE_CCACHE" == 1 ]]; then
        cc=$(du -sh "$WORK/ccache" 2>/dev/null | cut -f1 || true); cc="$WORK/ccache  - compiler cache: ${cc:-empty}, max 15G"
    else cc="off - compiler cache disabled"; fi
    msg "environment   $NAME: $what"
    msg "run           $run, JOBS=$JOBS (${SET_BY[JOBS]})"
    msg "extras        SDL3 $( ((SDL)) && echo on || echo off), SDL3_net $( ((SDLNET)) && echo on || echo off), test programs $( ((RELEASE)) && echo "off (--release)" || echo on), man pages $( ((MAN)) && echo on || echo off), gcc/g++ $( ((GCC)) && echo on || echo "off (--no-gcc)"), $( ((GUI)) && echo "mlterm $( ((MLTERM)) && echo on || echo "off (--no-mlterm)"), adie $( ((ADIE)) && echo on || echo "off (--no-adie)"), ")shellcheck $( ((SHELLCHECK)) && echo on || echo "off (--no-shellcheck)"), tcc $( ((TCC)) && echo "on (--tcc)" || echo off), wincc $( ((WINCC)) && echo "on (--wincc)" || echo off), upx $( ((UPX)) && echo "on (--upx)" || echo off), compression $( ((XZ)) && echo "xz (--xz)" || echo zstd), APIC $( ((APIC)) && echo "on (--apic)" || echo off)"
    printf '%s[cache]%s DL_DIR        %s  - download cache: %s files, %s (%s)\n' "$C1" "$C0" "$DL_DIR" "$files" "${size:-0}" "${SET_BY[DL_DIR]}"
    printf '%s[cache]%s ccache        %s (USE_CCACHE=%s, %s)\n' "$C1" "$C0" "$cc" "$USE_CCACHE" "${SET_BY[USE_CCACHE]}"
    local hc="off - host-tool cache disabled"
    if [[ "$HOST_CACHE" == 1 ]]; then
        hc=$(du -sh "$HOST_CACHE_DIR" 2>/dev/null | cut -f1 || true)
        hc="$HOST_CACHE_DIR  - host-tool cache: ${hc:-empty}"
        local n; n=$(find "$HOST_CACHE_DIR" -mindepth 1 -maxdepth 1 -type d -regex '.*/[0-9a-f]\{16\}' 2>/dev/null | wc -l || true)
        hc+=", $n snapshot(s) shared by all images"
    fi
    printf '%s[cache]%s host tools    %s (HOST_CACHE=%s, %s; HOST_CACHE_DIR %s)\n' "$C1" "$C0" "$hc" "$HOST_CACHE" "${SET_BY[HOST_CACHE]}" "${SET_BY[HOST_CACHE_DIR]}"
    printf '%s[dirs ]%s WORK          %s  - build folder: %s, log: %s (%s)\n' "$C1" "$C0" "$WORK" "${BUILD#"$WORK"/}" "${BUILD_LOG#"$WORK"/}" "${SET_BY[WORK]}"
    printf '%s[dirs ]%s BR_SRC        %s  - Buildroot %s source (%s)\n' "$C1" "$C0" "$BR_SRC" "$BR_VER" "${SET_BY[BR_SRC]}"
    printf '%s[dirs ]%s IMAGE_DIR     %s  - finished images: v86-%s-* (%s)\n' "$C1" "$C0" "$IMAGE_DIR" "$NAME" "${SET_BY[IMAGE_DIR]}"
    printf '%s[dirs ]%s USER_OVERLAY  %s  - %s (%s)\n' "$C1" "$C0" "$USER_OVERLAY" \
        "$([[ -d "$USER_OVERLAY" ]] && echo "your extra files: included" || echo "not present: skipped")" "${SET_BY[USER_OVERLAY]}"
    msg "DL_RETRIES=$DL_RETRIES (${SET_BY[DL_RETRIES]})"
}

preflight() {
    step "Preflight: $NAME"
    # WSL appends Windows PATH entries with spaces; Buildroot refuses them.
    local d p="" IFS=:
    for d in $PATH; do [[ -n "$d" && "$d" != *[[:space:]]* ]] && p+="${p:+:}$d"; done
    export PATH="$p"; unset LD_LIBRARY_PATH; IFS=$' \t\n'

    case "$WORK" in /mnt/*) die "WORK=$WORK is on a Windows drive; use the Linux filesystem (e.g. ~/test)" ;; esac
    case "$DL_DIR" in /mnt/*) warn "DL_DIR is on a Windows drive ($DL_DIR): works, but slower" ;; esac
    if ((!DRY)) && [[ $EUID -eq 0 && "${ALLOW_ROOT:-0}" != 1 ]]; then
        die "don't build as root (set ALLOW_ROOT=1 to override)"
    fi

    local tools=(wget curl tar gzip make gcc g++ cpio unzip rsync bc gawk file python3 perl patch bzip2 bison flex) missing=() c
    ((SHELLCHECK)) && tools+=(ar readelf xz)     # unpacking Debian's shellcheck package
    for c in "${tools[@]}"; do have "$c" || missing+=("$c"); done
    if ((${#missing[@]})); then
        ((DRY)) && { warn "missing host tools (needed for a real build): ${missing[*]}"; return 0; }
        have apt-get || die "missing host tools: ${missing[*]} — install them with your package manager"
        local SUDO=""; [[ $EUID -eq 0 ]] || SUDO=sudo
        $SUDO apt-get update
        $SUDO apt-get install -y build-essential wget curl ca-certificates tar gzip bzip2 xz-utils cpio unzip \
            rsync bc gawk file python3 perl patch bison flex libncurses-dev || die "apt-get install failed"
    fi
    mkdir -p "$WORK" "$DL_DIR"
    if ((!DRY)); then
        mkdir -p "$IMAGE_DIR" 2>/dev/null && [[ -w "$IMAGE_DIR" ]] || die "IMAGE_DIR '$IMAGE_DIR' is not writable — set IMAGE_DIR=/some/path"
        local free_kb; free_kb=$(df -Pk "$WORK" | awk 'NR==2{print $4}')
        ((free_kb > 20 * 1024 * 1024)) || warn "less than 20 GB free in $WORK"
    fi
    show_settings
}


# ══════════════════════════════════════════════════════════════════════════════
#  Buildroot source: tarball cached in DL_DIR, unpacked once, never modified
#  (all builds are out-of-tree with O=), so it is safe to share.
# ══════════════════════════════════════════════════════════════════════════════

get_buildroot() {
    [[ -f "$BR_SRC/Makefile" ]] && { msg "Buildroot: $BR_SRC"; return 0; }
    local tb="$DL_DIR/$BR_TARBALL"
    if [[ ! -f "$tb" ]]; then
        ((DRY)) && return 1                       # dry-run never downloads
        msg "downloading $BR_URL"
        retry "$DL_RETRIES" wget "${WGET_OPTS[@]}" --progress=dot:giga -O "$tb.part" "$BR_URL" \
            || { rm -f "$tb.part"; die "could not download Buildroot"; }
        mv "$tb.part" "$tb"
    fi
    gzip -t "$tb" 2>/dev/null || { rm -f "$tb"; die "cached $tb was corrupt (deleted) — run again"; }
    msg "unpacking $tb"
    rm -rf "$BR_SRC.part"; mkdir -p "$BR_SRC.part"
    tar -C "$BR_SRC.part" --strip-components=1 -xf "$tb"
    mv "$BR_SRC.part" "$BR_SRC"
}

# ── ShellCheck ────────────────────────────────────────────────────────────────
# ShellCheck is written in Haskell: Buildroot can't build it, and upstream releases have
# no 32-bit x86 binary (v86 runs i686 only). Debian builds it for i386, so the image gets
# the binary from Debian's package, checked against the sha256 in Debian's package index
# (fetched over HTTPS). The .deb stays in DL_DIR: later builds don't need the network.
# Its libraries (GMP, libffi) come from Buildroot; the image's glibc is newer than
# Debian's, which is what a binary built against an older glibc needs.
SC_MIRROR=https://deb.debian.org/debian
SC_SUITES=(trixie bookworm)          # newest first
SC_DIR="$DL_DIR/shellcheck"
SC_BIN=""                            # the extracted binary, once there is one
SC_PKGS="GMP LIBFFI"                 # Buildroot packages for its libraries (read from the binary)

get_shellcheck() {
    ((SHELLCHECK)) || return 0
    local debs=() deb="" suite idx para file="" sum="" ver="" member lib own=()
    # our own build (build-shellcheck.sh): native code, a third of Debian's size
    shopt -s nullglob; own=("$SCRIPT_DIR"/shellcheck-*-i386); shopt -u nullglob
    if ((${#own[@]})); then
        SC_BIN=${own[-1]}
        [[ "$(readelf -h "$SC_BIN" 2>/dev/null)" =~ Machine:[^$'\n']*80386 ]] \
            || die "ShellCheck: $SC_BIN is not a 32-bit x86 program — delete it, or rebuild it with build-shellcheck.sh"
        sc_libs
        msg "ShellCheck: ${SC_BIN##*/} (build-shellcheck.sh), $(( $(stat -c %s "$SC_BIN") / 1048576 )) MB, libraries from Buildroot: ${SC_PKGS:-none}"
        return 0
    fi
    shopt -s nullglob; debs=("$SC_DIR"/shellcheck_*_i386.deb); shopt -u nullglob
    ((${#debs[@]})) && deb=${debs[-1]}
    if [[ -n "$deb" && -f "$deb.sha256" ]] && (cd "$SC_DIR" && sha256sum -c --status "${deb##*/}.sha256"); then
        msg "ShellCheck: cached ${deb##*/}"
    else
        if ((DRY)); then
            warn "ShellCheck: not downloaded yet (a real build fetches Debian's i386 package)"
            return 0
        fi
        mkdir -p "$SC_DIR"
        for suite in "${SC_SUITES[@]}"; do
            idx="$SC_DIR/Packages-$suite.xz"
            if ! retry "$DL_RETRIES" wget "${WGET_OPTS[@]}" -q -O "$idx.part" \
                    "$SC_MIRROR/dists/$suite/main/binary-i386/Packages.xz"; then
                rm -f "$idx.part"; warn "ShellCheck: could not read Debian $suite's package index"; continue
            fi
            para=$(xz -dc "$idx.part" | awk 'BEGIN { RS = "" } !found && /^Package: shellcheck\n/ { found = 1; print }') || para=""
            rm -f "$idx.part"
            file=$(sed -n 's/^Filename: //p' <<<"$para")
            sum=$(sed -n 's/^SHA256: //p' <<<"$para")
            ver=$(sed -n 's/^Version: //p' <<<"$para")
            [[ "$file" == pool/*/shellcheck_*_i386.deb && "$sum" =~ ^[0-9a-f]{64}$ ]] && break
            file="" sum=""
        done
        [[ -n "$file" ]] || die "ShellCheck: no i386 package found in Debian ${SC_SUITES[*]} — build with --no-shellcheck"
        deb="$SC_DIR/${file##*/}"
        msg "ShellCheck $ver: downloading Debian $suite's i386 package"
        retry "$DL_RETRIES" wget "${WGET_OPTS[@]}" -q -O "$deb.part" "$SC_MIRROR/$file" \
            || { rm -f "$deb.part"; die "ShellCheck: download failed — re-run, or build with --no-shellcheck"; }
        [[ "$(sha256sum < "$deb.part" | cut -d' ' -f1)" == "$sum" ]] \
            || { rm -f "$deb.part"; die "ShellCheck: ${deb##*/} doesn't match the sha256 in Debian's index (deleted) — re-run"; }
        rm -f "$SC_DIR"/shellcheck_*_i386.deb "$SC_DIR"/shellcheck_*_i386.deb.sha256
        mv "$deb.part" "$deb"
        echo "$sum  ${deb##*/}" > "$deb.sha256"
    fi
    # the binary: data.tar.* inside the .deb (ar archive)
    rm -rf "$SC_DIR/x"; mkdir -p "$SC_DIR/x" "$SC_DIR/bin"
    member=$(ar t "$deb" | grep -m1 '^data\.tar') || die "ShellCheck: ${deb##*/} has no data archive"
    case "$member" in
        *.xz)  ar p "$deb" "$member" | tar -xJ -C "$SC_DIR/x" --wildcards '*usr/bin/shellcheck' ;;
        *.zst) ar p "$deb" "$member" | zstd -dc | tar -x -C "$SC_DIR/x" --wildcards '*usr/bin/shellcheck' ;;
        *.gz)  ar p "$deb" "$member" | tar -xz -C "$SC_DIR/x" --wildcards '*usr/bin/shellcheck' ;;
        *)     die "ShellCheck: unknown archive $member in ${deb##*/}" ;;
    esac || die "ShellCheck: could not unpack ${deb##*/}"
    mv "$SC_DIR/x/usr/bin/shellcheck" "$SC_DIR/bin/shellcheck" && rm -rf "$SC_DIR/x"
    SC_BIN="$SC_DIR/bin/shellcheck"
    sc_libs
    msg "ShellCheck: ${deb##*/} (Debian), $(( $(stat -c %s "$SC_BIN") / 1048576 )) MB, libraries from Buildroot: ${SC_PKGS:-none}"
    msg "ShellCheck: Debian's build is 56 MB of RAM; sudo ./build-shellcheck.sh makes a ~18 MB one (~4 MB with upx)"
}

# SC_BIN's shared libraries -> the Buildroot packages that provide them (SC_PKGS)
sc_libs() {
    local lib
    SC_PKGS=""
    for lib in $(readelf -d "$SC_BIN" | sed -n 's/.*(NEEDED).*\[\(.*\)\]/\1/p'); do
        case "$lib" in
            libc.so.6|libm.so.6|libpthread.so.0|librt.so.1|libdl.so.2|libutil.so.1|ld-linux.so.2) ;;   # glibc
            libgmp.so.10) SC_PKGS+=" GMP" ;;
            libffi.so.8)  SC_PKGS+=" LIBFFI" ;;
            libnuma.so.1) SC_PKGS+=" NUMACTL" ;;
            *) die "ShellCheck: ${SC_BIN##*/} needs $lib, which build.sh doesn't know how to add — build with --no-shellcheck" ;;
        esac
    done
    SC_PKGS=${SC_PKGS# }
}

# The binary for the image: packed with UPX when upx is installed (it unpacks itself into
# RAM only while it runs: ~0.3 s more per run in v86). Packed copies are cached by sha256.
sc_image_binary() {
    local sum packed
    have upx || { echo "$SC_BIN"; return 0; }
    sum=$(sha256sum < "$SC_BIN" | cut -c1-16)
    packed="$SC_DIR/upx/shellcheck-$sum"
    if [[ ! -s "$packed" ]]; then
        mkdir -p "$SC_DIR/upx"
        cp "$SC_BIN" "$packed.part"
        upx -q --best "$packed.part" >/dev/null 2>&1 && mv "$packed.part" "$packed" \
            || { rm -f "$packed.part"; warn "ShellCheck: upx failed, using it unpacked"; echo "$SC_BIN"; return 0; }
    fi
    echo "$packed"
}


# ══════════════════════════════════════════════════════════════════════════════
#  External tree: custom packages
# ══════════════════════════════════════════════════════════════════════════════

write_packages() {
    put "$EXT/external.desc" <<'EOF'
name: V86
desc: v86 emulator development images
EOF
    put "$EXT/Config.in" <<'EOF'
source "$BR2_EXTERNAL_V86_PATH/package/gcc-target/Config.in"
source "$BR2_EXTERNAL_V86_PATH/package/shfmt/Config.in"
source "$BR2_EXTERNAL_V86_PATH/package/bashdb/Config.in"
source "$BR2_EXTERNAL_V86_PATH/package/fox/Config.in"
source "$BR2_EXTERNAL_V86_PATH/package/xfe/Config.in"
source "$BR2_EXTERNAL_V86_PATH/package/tinyx/Config.in"
source "$BR2_EXTERNAL_V86_PATH/package/sdl3_net/Config.in"
source "$BR2_EXTERNAL_V86_PATH/package/mandoc/Config.in"
source "$BR2_EXTERNAL_V86_PATH/package/man-pages/Config.in"
source "$BR2_EXTERNAL_V86_PATH/package/unifont/Config.in"
source "$BR2_EXTERNAL_V86_PATH/package/twemoji/Config.in"
source "$BR2_EXTERNAL_V86_PATH/package/mlterm/Config.in"
source "$BR2_EXTERNAL_V86_PATH/package/tinycc/Config.in"
source "$BR2_EXTERNAL_V86_PATH/package/upx-target/Config.in"
EOF
    # Buildroot only includes external.mk; it registers our packages and tweaks.
    put "$EXT/external.mk" <<'EOF'
include $(sort $(wildcard $(BR2_EXTERNAL_V86_PATH)/package/*/*.mk))
EOF
    # --xz: Buildroot's rootfs command is plain "xz -9". -9e with the x86 filter (BCJ, which
    # the kernel's initramfs unpacker supports) is ~4% smaller; the 32 MB dictionary caps
    # what the kernel must allocate while unpacking (sizes measured: no loss vs 64 MB for
    # small images). external.mk is read after fs/common.mk, and the recipe expands this
    # variable when it runs, so the override wins.
    if ((XZ)); then cat >> "$EXT/external.mk" <<'EOF'

ROOTFS_CPIO_COMPRESS_CMD = xz --x86 --lzma2=preset=9e,dict=32MiB -C crc32 -c -T $(PARALLEL_JOBS)
EOF
    fi
    # --man: Buildroot deletes usr/share/man in target-finalize; save it just before
    # (TARGET_FINALIZE_HOOKS run first). post-build.sh puts it back and indexes it.
    if ((MAN)); then cat >> "$EXT/external.mk" <<'EOF'

define V86_KEEP_MAN_PAGES
	rm -rf $(BUILD_DIR)/v86-man
	if [ -d $(TARGET_DIR)/usr/share/man ]; then cp -a $(TARGET_DIR)/usr/share/man $(BUILD_DIR)/v86-man; fi
endef
TARGET_FINALIZE_HOOKS += V86_KEEP_MAN_PAGES
EOF
    fi
    # VESA: v86 runs the kernel's real-mode setup with the BIOS alive, but forces the
    # boot header's vid_mode to "normal VGA" (src/kernel.js), so vga= can't work. Make the
    # setup code use our VBE mode instead; it then sets it via the BIOS (exactly what a
    # bootloader's vga= does under DSL) and vesafb drives the screen. If the mode were
    # rejected the kernel would show its mode menu: verify_kernel guards that.
    if [[ -n "$VID_MODE" ]]; then cat >> "$EXT/external.mk" <<EOF

define V86_FORCE_VID_MODE
	\$(SED) 's/u16 mode = boot_params.hdr.vid_mode;/u16 mode = $VID_MODE; \/* v86: VESA mode forced by build.sh *\//' \$(@D)/arch/x86/boot/video.c
	grep -q 'u16 mode = $VID_MODE;' \$(@D)/arch/x86/boot/video.c
endef
LINUX_POST_PATCH_HOOKS += V86_FORCE_VID_MODE

# xmode: Xorg's vesa driver calls the VGA BIOS through the int10 module. x86emu needs
# no kernel vm86 support (the i386 default backend does).
XSERVER_XORG_SERVER_CONF_OPTS += --with-int10=x86emu

# xf86-video-vesa 2.6.0 refuses to run while any /dev/fb* exists ("Refusing to run,
# Framebuffer or dri device present"), i.e. always next to our vesafb. Keep only the
# DRM half of that check: X takes the screen over and gives it back on exit.
define V86_VESA_ALLOW_FB
	\$(SED) 's@VESAFileExistsPrefix("/dev", "fb") ||@0 /* v86: vesafb may stay */ ||@' \$(@D)/src/vesa.c
	grep -q 'v86: vesafb may stay' \$(@D)/src/vesa.c
endef
XDRIVER_XF86_VIDEO_VESA_POST_PATCH_HOOKS += V86_VESA_ALLOW_FB
EOF
    fi

    # libXft 2.3.9: colour fonts get an ARGB32 glyph set, but their plain greyscale
    # glyphs (Twemoji's .notdef, which xterm loads whenever it checks Twemoji for a
    # character it lacks, e.g. CJK) are uploaded as A8 data: RenderAddGlyphs fails with
    # BadLength and xterm exits. Send those glyphs empty.
    mkdir -p "$EXT/patches"
    put "$EXT/patches/xft-color-plain-glyphs.patch" <<'EOF'
--- a/src/xftglyphs.c
+++ b/src/xftglyphs.c
@@ -824,6 +824,18 @@
 
 	FT_Library_SetLcdFilter( _XftFTlibrary, FT_LCD_FILTER_NONE );
 
+	/*
+	 * A colour font's glyph set is ARGB32, but the font can still have
+	 * plain greyscale glyphs (e.g. .notdef in Twemoji's COLR font).
+	 * Uploading one as A8 data makes RenderAddGlyphs fail with BadLength,
+	 * which kills the client (xterm). Send such a glyph as an empty one.
+	 */
+	if (font->info.color && glyphslot->bitmap.pixel_mode != FT_PIXEL_MODE_BGRA)
+	{
+	    glyphslot->bitmap.width = 0;
+	    glyphslot->bitmap.rows = 0;
+	}
+
 	if (font->info.spacing >= FC_MONO)
 	{
 	    if (transform)
EOF
    cat >> "$EXT/external.mk" <<'EOF'

define V86_XFT_COLOR_PLAIN_GLYPHS
	patch -d $(@D) -p1 < $(BR2_EXTERNAL_V86_PATH)/patches/xft-color-plain-glyphs.patch
	grep -q 'Send such a glyph as an empty one' $(@D)/src/xftglyphs.c
endef
XLIB_LIBXFT_POST_PATCH_HOOKS += V86_XFT_COLOR_PLAIN_GLYPHS
EOF

    # BusyBox less shows byte 0x9b (8-bit CSI) as an inverse "{". In UTF-8 that byte is a
    # continuation byte, so it broke every character ending in it (U+201B, U+221B,
    # U+1F31B...: one in 64 code points). Found with symboltest.
    put "$EXT/patches/busybox-less-utf8.patch" <<'EOF'
--- a/miscutils/less.c
+++ b/miscutils/less.c
@@ -791,7 +791,8 @@
 	/* NUL: never encountered; TAB: not converted */
 	/**/"\x01\x02\x03\x04\x05\x06\x07\x08"  "\x0a\x0b\x0c\x0d\x0e\x0f"
 	"\x10\x11\x12\x13\x14\x15\x16\x17\x18\x19\x1a\x1b\x1c\x1d\x1e\x1f"
-	"\x7f\x9b"; /* DEL and infamous Meta-ESC :( */
+	"\x7f"; /* DEL. Not 0x9b (8-bit CSI): in UTF-8 text it is a continuation
+	 * byte (U+201B, U+219B, U+1F31B...), and a UTF-8 terminal never reads it as CSI */
 static const char ctrlconv[] ALIGN1 =
 	/* why 40 instead of 4a below? - it is a replacement for '\n'.
 	 * '\n' is a former NUL - we subst it with @, not J */
EOF
    cat >> "$EXT/external.mk" <<'EOF'

define V86_BUSYBOX_LESS_UTF8
	patch -d $(@D) -p1 < $(BR2_EXTERNAL_V86_PATH)/patches/busybox-less-utf8.patch
	grep -q 'Not 0x9b (8-bit CSI)' $(@D)/miscutils/less.c
endef
BUSYBOX_POST_PATCH_HOOKS += V86_BUSYBOX_LESS_UTF8
EOF

    # ── gcc-target: a NATIVE i686 gcc/g++ for the target ────────────────────
    # Buildroot has no on-target compiler; this cross-builds one from the same
    # gcc source/version/hash as the cross toolchain.
    put "$EXT/package/gcc-target/Config.in" <<'EOF'
config BR2_PACKAGE_GCC_TARGET
	bool "gcc (target)"
	depends on BR2_TOOLCHAIN_BUILDROOT
	depends on BR2_USE_WCHAR && BR2_TOOLCHAIN_HAS_THREADS && BR2_USE_MMU
	depends on !BR2_STATIC_LIBS
	select BR2_PACKAGE_BINUTILS
	select BR2_PACKAGE_BINUTILS_TARGET
	select BR2_PACKAGE_GMP
	select BR2_PACKAGE_MPFR
	select BR2_PACKAGE_MPC
	help
	  Native C/C++ compiler for the target.
EOF
    put "$EXT/package/gcc-target/gcc-target.mk" <<'EOF'
GCC_TARGET_VERSION = $(GCC_VERSION)
GCC_TARGET_SITE = $(GCC_SITE)
GCC_TARGET_SOURCE = $(GCC_SOURCE)
GCC_TARGET_DL_SUBDIR = gcc
GCC_TARGET_LANGS ?= c,c++
GCC_TARGET_SANITIZER ?= y
GCC_TARGET_LICENSE = GPL-3.0-with-GCC-exception
GCC_TARGET_LICENSE_FILES = COPYING.RUNTIME
GCC_TARGET_DEPENDENCIES = binutils gmp mpfr mpc
GCC_TARGET_INSTALL_STAGING = YES
GCC_TARGET_CONF_OPTS = $(HOST_GCC_COMMON_CONF_OPTS) \
	--with-sysroot=/ --with-build-sysroot=$(STAGING_DIR) \
	--enable-__cxa_atexit \
	--with-gmp=$(STAGING_DIR) --with-mpc=$(STAGING_DIR) --with-mpfr=$(STAGING_DIR) \
	--disable-libquadmath --disable-libgomp \
	--disable-plugin --disable-lto --disable-bootstrap --disable-nls \
	--disable-gcov --disable-werror --enable-languages=$(GCC_TARGET_LANGS) \
	$(if $(filter y,$(GCC_TARGET_SANITIZER)),--enable-libsanitizer,--disable-libsanitizer)
GCC_TARGET_CONF_ENV = $(HOST_GCC_COMMON_CONF_ENV)
GCC_TARGET_MAKE_OPTS += $(HOST_GCC_COMMON_MAKE_OPTS)
define GCC_TARGET_RM_PREFIXED
	rm -f $(TARGET_DIR)/usr/bin/$(GNU_TARGET_NAME)-gcc-* $(TARGET_DIR)/usr/bin/$(GNU_TARGET_NAME)-g++-*
endef
GCC_TARGET_POST_INSTALL_TARGET_HOOKS += GCC_TARGET_RM_PREFIXED
$(eval $(autotools-package))
EOF
    cp "$BR_SRC/package/gcc/gcc.hash" "$EXT/package/gcc-target/gcc-target.hash" 2>/dev/null \
        || : > "$EXT/package/gcc-target/gcc-target.hash"   # dry-run without a Buildroot tree

    # ── shfmt (--bash-dev): the shell formatter, a Go program; Buildroot has no package ──
    put "$EXT/package/shfmt/Config.in" <<'EOF'
config BR2_PACKAGE_SHFMT
	bool "shfmt"
	depends on BR2_PACKAGE_HOST_GO_TARGET_ARCH_SUPPORTS
	help
	  shfmt: format shell programs (bash, POSIX sh, mksh).
EOF
    # github.com is not reachable from every network, and Buildroot's golang-package fetches
    # the source from there; this takes the source and its modules from the Go module proxy.
    put "$EXT/package/shfmt/shfmt.mk" <<'EOF'
SHFMT_VERSION = 3.12.0
SHFMT_SITE = https://proxy.golang.org/mvdan.cc/sh/v3/@v
SHFMT_SOURCE = v$(SHFMT_VERSION).zip
SHFMT_LICENSE = BSD-3-Clause
SHFMT_LICENSE_FILES = LICENSE
SHFMT_DEPENDENCIES = host-go
define SHFMT_EXTRACT_CMDS
	rm -rf $(@D)/.unzip && mkdir -p $(@D)/.unzip
	$(UNZIP) -q -d $(@D)/.unzip $(SHFMT_DL_DIR)/$(SHFMT_SOURCE)
	mv $(@D)/.unzip/mvdan.cc/sh/v3@v$(SHFMT_VERSION)/* $(@D)/
	rm -rf $(@D)/.unzip
endef
define SHFMT_BUILD_CMDS
	cd $(@D) && $(HOST_GO_TARGET_ENV) GOFLAGS=-mod=mod GOPROXY=https://proxy.golang.org \
		GONOSUMDB='*' GONOSUMCHECK=1 \
		$(GO_BIN) build -trimpath -ldflags '-s -w' -o shfmt ./cmd/shfmt
endef
define SHFMT_INSTALL_TARGET_CMDS
	$(INSTALL) -D -m 0755 $(@D)/shfmt $(TARGET_DIR)/usr/bin/shfmt
endef
$(eval $(generic-package))
EOF
    printf '%s  %s\n' e5c1949ada636589ba1d0621274bd6d2e7b46f9def4181cc4fb0e97b7a8779ef v3.12.0.zip > "$EXT/package/shfmt/shfmt.hash"

    # ── bashdb (--bashdb): the bash script debugger; Buildroot has no package ──
    put "$EXT/package/bashdb/Config.in" <<'EOF'
config BR2_PACKAGE_BASHDB
	bool "bashdb"
	select BR2_PACKAGE_BASH
	help
	  bashdb: a debugger for bash scripts (breakpoints, stepping, watch).
EOF
    put "$EXT/package/bashdb/bashdb.mk" <<'EOF'
BASHDB_VERSION = 5.2-1.2.0
BASHDB_SOURCE = bashdb-$(BASHDB_VERSION).tar.gz
BASHDB_SITE = https://downloads.sourceforge.net/project/bashdb/bashdb/$(BASHDB_VERSION)
BASHDB_LICENSE = GPL-3.0+
BASHDB_LICENSE_FILES = COPYING
# configure runs the shell it is given to check its version, so it gets the build host's
# bash; the installed scripts start with /bin/bash, which is the target's bash
BASHDB_CONF_OPTS = --with-POSIXSHELL=/bin/bash
BASHDB_DEPENDENCIES = bash
$(eval $(autotools-package))
EOF
    printf '%s  %s\n' c6015c625d0d05bceb19d33bec89b5faa4e964fd3db2380ac03ec16dc5e20c6b bashdb-5.2-1.2.0.tar.gz \
        > "$EXT/package/bashdb/bashdb.hash"

    # ── SDL3 X11 build order ─────────────────────────────────────────────────
    # SDL3's CMake hard-fails ("Couldn't find dependency package for XCURSOR") if it
    # configures before these Xlib extensions are in staging. Appending to
    # SDL3_DEPENDENCIES here is too late to order the build (the rule already
    # exists), so an explicit order-only prerequisite is added.
    put "$EXT/package/sdl3/sdl3.mk" <<'EOF'
ifeq ($(BR2_PACKAGE_SDL3_X11),y)
SDL3_X11_EXTRA_DEPS = xlib_libXScrnSaver xlib_libXcursor xlib_libXfixes xlib_libXi \
	xlib_libXrandr xlib_libXrender xlib_libXtst xorgproto
SDL3_DEPENDENCIES += $(SDL3_X11_EXTRA_DEPS)
$(SDL3_TARGET_CONFIGURE): | $(SDL3_X11_EXTRA_DEPS)
endif
EOF

    # ── FOX 1.6 toolkit + XFE file manager (not in Buildroot) ────────────────
    # Sources from Ubuntu's archive, pinned by sha256. Cross-compile traps handled:
    # FOX hardcodes -I/usr/include/freetype2 (host path) and would regenerate icons
    # with a target binary (reswrap); XFE probes the host's pkexec, needs X paths,
    # never generates .desktop files with --disable-nls, and ships an old ltmain.sh
    # that Buildroot's libtool patch can't apply to.
    put "$EXT/package/fox/Config.in" <<'EOF'
config BR2_PACKAGE_FOX
	bool "fox"
	depends on BR2_INSTALL_LIBSTDCPP && BR2_PACKAGE_XORG7
	select BR2_PACKAGE_FONTCONFIG
	select BR2_PACKAGE_FREETYPE
	select BR2_PACKAGE_JPEG
	select BR2_PACKAGE_LIBPNG
	select BR2_PACKAGE_XLIB_LIBX11
	select BR2_PACKAGE_XLIB_LIBXCURSOR
	select BR2_PACKAGE_XLIB_LIBXEXT
	select BR2_PACKAGE_XLIB_LIBXFT
	select BR2_PACKAGE_XLIB_LIBXRANDR
	select BR2_PACKAGE_XLIB_LIBXRENDER
	select BR2_PACKAGE_ZLIB
	help
	  FOX 1.6 C++ GUI toolkit (required by XFE).

config BR2_PACKAGE_FOX_ADIE
	bool "adie"
	depends on BR2_PACKAGE_FOX
	help
	  Adie: FOX's programmer's editor (syntax highlighting).
EOF
    # Adie's shell mode only knows comments, keywords and operators, and only by a
    # "#!/bin/sh" first line: this one also matches *.sh names and env bash, and colours
    # strings, $variables, $(...) and builtins. Swapped into Adie.stx at install.
    put "$EXT/package/fox/adie-shell.stx" <<'EOF'
language "Shell"
  filesmatch "*.sh,*.bash,.bashrc,.bash_profile,.profile"
  contentsmatch "\A#!\s*(/usr)?/bin/(env\s+)?(ba)?sh"
  delimiters "~.,/\`'!@#$%^&*()-=+{}|[]\":;<>?"
  rule "Comment"
    pattern    "(^|[ \t;])#.*?$"
  end
  rule "String"
    openpattern   "\""
    closepattern  "\"|$"
    rule "ControlEscape"
      pattern     "\\."
    end
    rule "Variable"
      pattern     "\$(\{[^}]*\}|\([^)]*\)|[A-Za-z_][A-Za-z0-9_]*|[0-9#?@*$!-])"
    end
  end
  rule "String"
    openpattern   "'"
    closepattern  "'"
  end
  rule "Variable"
    pattern     "\$(\{[^}]*\}|\(|[A-Za-z_][A-Za-z0-9_]*|[0-9#?@*$!-])"
  end
  rule "Keyword"
    pattern     "\<(for|in|do|done|if|then|else|elif|fi|case|esac|while|until|function|select|time|return|break|continue)\>"
  end
  rule "Builtin"
    pattern     "\<(echo|printf|read|cd|pwd|export|local|declare|readonly|shift|exit|set|unset|trap|eval|exec|source|test|true|false|wait|kill|alias|let|getopts|mapfile|command|type)\>"
  end
  rule "Operator"
    pattern     "(=|;;|;|!|\{|\}|\[\[|\]\]|&&|\|\||\|)"
  end
end
EOF
    put "$EXT/package/fox/fox.mk" <<'EOF'
FOX_VERSION = 1.6.57
FOX_SOURCE = fox1.6_$(FOX_VERSION).orig.tar.gz
FOX_SITE = https://archive.ubuntu.com/ubuntu/pool/universe/f/fox1.6
FOX_LICENSE = LGPL-2.1+ with exceptions
FOX_LICENSE_FILES = LICENSE
FOX_INSTALL_STAGING = YES
FOX_CONFIG_SCRIPTS = fox-config
FOX_DEPENDENCIES = host-pkgconf freetype fontconfig libpng jpeg zlib \
	xlib_libX11 xlib_libXext xlib_libXft xlib_libXcursor xlib_libXrender xlib_libXrandr
FOX_CONF_OPTS = \
	--with-x --x-includes=$(STAGING_DIR)/usr/include --x-libraries=$(STAGING_DIR)/usr/lib \
	--with-xft --with-xcursor --with-xrender --with-xrandr \
	--disable-tiff --disable-bz2lib --with-opengl=no
# Adie (FOX's editor) only when asked: its icons are prebuilt like the library's, and
# it looks for its syntax file in /usr/share/adie first (upstream: only along $PATH)
FOX_SUBDIRS = include src $(if $(BR2_PACKAGE_FOX_ADIE),adie)
FOX_MAKE_OPTS = SUBDIRS="$(FOX_SUBDIRS)"
FOX_INSTALL_STAGING_OPTS = DESTDIR=$(STAGING_DIR) SUBDIRS="include src" install
FOX_INSTALL_TARGET_OPTS = DESTDIR=$(TARGET_DIR) SUBDIRS="$(FOX_SUBDIRS)" install
ifeq ($(BR2_PACKAGE_FOX_ADIE),y)
define FOX_ADIE_SYNTAX_PATH
	$(SED) 's|FXPath::search(FXSystem::getExecPath(),"Adie.stx")|FXPath::search("/usr/share/adie:"+FXSystem::getExecPath(),"Adie.stx")|' $(@D)/adie/Adie.cpp
	grep -q '"/usr/share/adie:"' $(@D)/adie/Adie.cpp
endef
FOX_POST_PATCH_HOOKS += FOX_ADIE_SYNTAX_PATH
define FOX_ADIE_INSTALL_SYNTAX
	mkdir -p $(TARGET_DIR)/usr/share/adie
	awk 'FNR == NR { shell = shell $$0 "\n"; next } /^language "Shell"$$/ { printf "%s", shell; skip = 1; next } skip && /^end$$/ { skip = 0; next } !skip' \
		$(FOX_PKGDIR)/adie-shell.stx $(@D)/adie/Adie.stx > $(TARGET_DIR)/usr/share/adie/Adie.stx
	grep -q 'rule "Builtin"' $(TARGET_DIR)/usr/share/adie/Adie.stx
	rm -f $(TARGET_DIR)/usr/bin/Adie.stx
endef
FOX_POST_INSTALL_TARGET_HOOKS += FOX_ADIE_INSTALL_SYNTAX
endif
define FOX_FIX_HOST_PATHS
	$(SED) 's,XFTCFLAGS="-I/usr/include/freetype2",XFTCFLAGS="-I$(STAGING_DIR)/usr/include/freetype2",' $(@D)/configure
endef
FOX_PRE_CONFIGURE_HOOKS += FOX_FIX_HOST_PATHS
define FOX_KEEP_PREBUILT_ICONS
	touch $(@D)/adie/icons.h $(@D)/adie/icons.cpp
	touch $(@D)/src/icons.h $(@D)/src/icons.cpp
endef
FOX_PRE_BUILD_HOOKS += FOX_KEEP_PREBUILT_ICONS
$(eval $(autotools-package))
EOF
    put "$EXT/package/fox/fox.hash" <<'EOF'
sha256  812066cd54f120eff5bacf61b5c5a1e9269bb2505e819c9d16610006a860d235  fox1.6_1.6.57.orig.tar.gz
EOF

    put "$EXT/package/xfe/Config.in" <<'EOF'
config BR2_PACKAGE_XFE
	bool "xfe"
	depends on BR2_INSTALL_LIBSTDCPP && BR2_PACKAGE_XORG7
	select BR2_PACKAGE_FONTCONFIG
	select BR2_PACKAGE_FOX
	select BR2_PACKAGE_FREETYPE
	select BR2_PACKAGE_LIBPNG
	select BR2_PACKAGE_XLIB_LIBX11
	select BR2_PACKAGE_XLIB_LIBXFT
	select BR2_PACKAGE_XLIB_LIBXRANDR
	help
	  X File Explorer: lightweight graphical file manager.
EOF
    put "$EXT/package/xfe/xfe.mk" <<'EOF'
XFE_VERSION = 2.1.7
XFE_SOURCE = xfe_$(XFE_VERSION).orig.tar.xz
XFE_SITE = https://archive.ubuntu.com/ubuntu/pool/universe/x/xfe
XFE_LICENSE = GPL-2.0+
XFE_LICENSE_FILES = COPYING
XFE_DEPENDENCIES = host-pkgconf fox freetype fontconfig libpng \
	xlib_libX11 xlib_libXft xlib_libXrandr
XFE_LIBTOOL_PATCH = NO
XFE_CONF_ENV = ac_cv_prog_FOX_CONFIG=$(STAGING_DIR)/usr/bin/fox-config
XFE_CONF_OPTS = \
	--with-x --x-includes=$(STAGING_DIR)/usr/include --x-libraries=$(STAGING_DIR)/usr/lib \
	--disable-nls --disable-sn --disable-debug
define XFE_NO_HOST_PKEXEC
	$(SED) 's/`pkexec --version/`false/' $(@D)/configure
endef
XFE_PRE_CONFIGURE_HOOKS += XFE_NO_HOST_PKEXEC
define XFE_PREPARE_DESKTOP_FILES
	for f in xfe xfa xfi xfp xfw; do cp $(@D)/$$f.desktop.in $(@D)/$$f.desktop; done
endef
XFE_POST_CONFIGURE_HOOKS += XFE_PREPARE_DESKTOP_FILES
$(eval $(autotools-package))
EOF
    put "$EXT/package/xfe/xfe.hash" <<'EOF'
sha256  180514e5e8d9682bc372e94d642b15a89322ff39346068e2e3560f26e0696ff4  xfe_2.1.7.orig.tar.xz
EOF

    # ── SDL3_net: networking library for SDL3 (not in Buildroot) ──────────────
    # Release 3.2.0, pinned by commit and fetched with Buildroot's git method (its
    # reproducible tarball is what the hash covers). CMake; finds SDL3 via its CMake files.
    put "$EXT/package/sdl3_net/Config.in" <<'EOF'
config BR2_PACKAGE_SDL3_NET
	bool "sdl3_net"
	depends on BR2_PACKAGE_SDL3
	help
	  SDL_net 3.x: portable networking for SDL3.
	  https://github.com/libsdl-org/SDL_net
EOF
    put "$EXT/package/sdl3_net/sdl3_net.mk" <<'EOF'
SDL3_NET_VERSION = 1a84a2a6b9663572f77e2eb5348d42845bac0053
SDL3_NET_SITE = https://github.com/libsdl-org/SDL_net.git
SDL3_NET_SITE_METHOD = git
SDL3_NET_LICENSE = Zlib
SDL3_NET_LICENSE_FILES = LICENSE.txt
SDL3_NET_INSTALL_STAGING = YES
SDL3_NET_DEPENDENCIES = sdl3
SDL3_NET_CONF_OPTS = -DSDLNET_SAMPLES=OFF -DSDLNET_INSTALL=ON -DSDLNET_WERROR=OFF
$(eval $(cmake-package))
EOF
    put "$EXT/package/sdl3_net/sdl3_net.hash" <<'EOF'
sha256  040e215ec647cb769eba5f22fa35b52d23a914c16fb65dba3bf54a122d0220e5  sdl3_net-1a84a2a6b9663572f77e2eb5348d42845bac0053-git4.tar.gz
EOF

    # ── TinyX: Xvesa, the X server DSL uses ──────────────────────────────────
    # Xorg dropped Xvesa in 2008; Tiny Core Linux maintains it as TinyX. It calls the VGA
    # BIOS itself through vm86 (real 16-bit code, like DSL), so it can set v86's packed
    # 24 bpp VBE modes that Xorg 21.1 refuses. Only Xvesa is built (no Xfbdev).
    put "$EXT/package/tinyx/Config.in" <<'EOF'
config BR2_PACKAGE_TINYX
	bool "tinyx (Xvesa)"
	depends on BR2_i386
	depends on BR2_PACKAGE_XORG7
	select BR2_PACKAGE_XLIB_LIBXFONT
	select BR2_PACKAGE_XLIB_LIBFONTENC
	select BR2_PACKAGE_XLIB_LIBXEXT
	select BR2_PACKAGE_XLIB_LIBXTST
	select BR2_PACKAGE_XLIB_LIBXAU
	select BR2_PACKAGE_XLIB_LIBXDMCP
	select BR2_PACKAGE_XLIB_XTRANS
	select BR2_PACKAGE_XORGPROTO
	select BR2_PACKAGE_XUTIL_UTIL_MACROS
	select BR2_PACKAGE_XFONT_FONT_MISC_MISC
	select BR2_PACKAGE_XFONT_FONT_CURSOR_MISC
	help
	  Xvesa X server from TinyX (Tiny Core Linux).
EOF
    put "$EXT/package/tinyx/tinyx.mk" <<'EOF'
TINYX_VERSION = feab72ca891bc04b18763763e15ee4e532369cdf
TINYX_SITE = https://github.com/tinycorelinux/tinyx.git
TINYX_SITE_METHOD = git
TINYX_LICENSE = MIT
TINYX_LICENSE_FILES = COPYING
TINYX_AUTORECONF = YES
# It also borrows client-library headers: dpms.h/sync.h/XShm.h (libXext), XTest.h
# (libXtst), Xauth.h (libXau), Xdmcp.h (libXdmcp).
TINYX_DEPENDENCIES = host-pkgconf xutil_util-macros xlib_libXfont xlib_libfontenc xlib_xtrans xorgproto \
	xlib_libXext xlib_libXtst xlib_libXau xlib_libXdmcp
# - LTO off: TinyX enables -flto whenever the compiler accepts it, but packs objects into
#   static libs with plain ar (no LTO plugin set up) -> undefined symbols at link time.
#   It then falls back to -ffunction-sections + --gc-sections.
# - Old C: K&R definitions and "()" prototypes break under gcc 15's C23 default
#   ("too many arguments", implicit int). C17 + -fpermissive builds it cleanly
#   (verified with gcc 14 -std=gnu23 rules: 9 errors before, none after).
TINYX_CONF_ENV = ax_cv_check_cflags___flto=no CFLAGS="$(TARGET_CFLAGS) -std=gnu17 -fpermissive"
TINYX_CONF_OPTS = \
	--disable-xfbdev --disable-xdmcp --disable-xdm-auth-1 \
	--with-fontdir=/usr/share/fonts/X11
$(eval $(autotools-package))
EOF
    put "$EXT/package/tinyx/tinyx.hash" <<'EOF'
sha256  a250fba82aa582651b907f45085343339a13af75e744b309849f61a020ea72d9  tinyx-feab72ca891bc04b18763763e15ee4e532369cdf-git4.tar.gz
EOF

    # ── mandoc: man, apropos, whatis, makewhatis (not in Buildroot) ──────────
    # Small, no groff needed. Its configure RUNS test programs, impossible when
    # cross-compiling, so the answers for glibc are preset (configure.local; checked
    # against a native glibc run; NEED_GNU_SOURCE: glibc hides getsubopt, strptime,
    # wcwidth... without it, and undeclared they crash). HAVE_FTS=1: mandoc's own fts fallback crashes with
    # glibc. The host build (always on, so the host-tool cache is the same for every
    # image) indexes the pages at build time: mandoc.db, for apropos/whatis and quiet man.
    put "$EXT/package/mandoc/Config.in" <<'EOF'
config BR2_PACKAGE_MANDOC
	bool "mandoc"
	depends on BR2_PACKAGE_BUSYBOX_SHOW_OTHERS
	select BR2_PACKAGE_ZLIB
	select BR2_PACKAGE_LESS
	help
	  mandoc: man page formatter and man/apropos/whatis.

config BR2_PACKAGE_HOST_MANDOC
	bool "host mandoc"
	help
	  makewhatis for the build host (indexes the target's man pages).
EOF
    put "$EXT/package/mandoc/mandoc.mk" <<'EOF'
MANDOC_VERSION = 1.14.6
MANDOC_SOURCE = mdocml_$(MANDOC_VERSION).orig.tar.gz
MANDOC_SITE = https://archive.ubuntu.com/ubuntu/pool/universe/m/mdocml
MANDOC_LICENSE = ISC
MANDOC_LICENSE_FILES = LICENSE
MANDOC_DEPENDENCIES = zlib
HOST_MANDOC_DEPENDENCIES = host-zlib
define MANDOC_CONFIGURE_CMDS
	(echo 'CC="$(TARGET_CC)"'; echo 'AR="$(TARGET_AR)"'; \
	 echo 'CFLAGS="$(TARGET_CFLAGS)"'; echo 'LDFLAGS="$(TARGET_LDFLAGS)"'; \
	 cat $(MANDOC_PKGDIR)/configure.local) > $(@D)/configure.local
	cd $(@D) && $(TARGET_MAKE_ENV) ./configure
endef
define MANDOC_BUILD_CMDS
	$(TARGET_MAKE_ENV) $(MAKE) -C $(@D)
endef
define MANDOC_INSTALL_TARGET_CMDS
	$(TARGET_MAKE_ENV) $(MAKE) -C $(@D) DESTDIR=$(TARGET_DIR) install
endef
define HOST_MANDOC_CONFIGURE_CMDS
	(echo 'CC="$(HOSTCC)"'; echo 'CFLAGS="$(HOST_CFLAGS)"'; echo 'LDFLAGS="$(HOST_LDFLAGS)"'; \
	 echo 'PREFIX="$(HOST_DIR)"'; echo 'MANDIR="$(HOST_DIR)/share/man"'; \
	 echo 'HAVE_LESS_T=0'; echo 'UTF8_LOCALE=C') > $(@D)/configure.local
	cd $(@D) && $(HOST_MAKE_ENV) ./configure
endef
define HOST_MANDOC_BUILD_CMDS
	$(HOST_MAKE_ENV) $(MAKE) -C $(@D)
endef
define HOST_MANDOC_INSTALL_CMDS
	$(HOST_MAKE_ENV) $(MAKE) -C $(@D) install
endef
$(eval $(generic-package))
$(eval $(host-generic-package))
EOF
    put "$EXT/package/mandoc/configure.local" <<'EOF'
NEED_GNU_SOURCE=1
OSENUM=MANDOC_OS_OTHER
PREFIX=/usr
MANDIR=/usr/share/man
MANPATH_DEFAULT=/usr/share/man
MANPATH_BASE=/usr/share/man
BINM_PAGER=less
UTF8_LOCALE=C.UTF-8
HAVE_WCHAR=1
HAVE_LESS_T=1
HAVE_ATTRIBUTE=1
HAVE_CMSG=1
HAVE_DIRENT_NAMLEN=0
HAVE_EFTYPE=0
HAVE_ENDIAN=1
HAVE_SYS_ENDIAN=0
HAVE_ERR=1
HAVE_FTS=1
HAVE_FTS_COMPARE_CONST=0
HAVE_GETLINE=1
HAVE_GETSUBOPT=1
HAVE_ISBLANK=1
HAVE_MKDTEMP=1
HAVE_MKSTEMPS=1
HAVE_NANOSLEEP=1
HAVE_NTOHL=1
HAVE_O_DIRECTORY=1
HAVE_OHASH=0
HAVE_PATH_MAX=1
HAVE_PLEDGE=0
HAVE_PROGNAME=0
HAVE_REALLOCARRAY=1
HAVE_RECALLOCARRAY=0
HAVE_RECVMSG=1
HAVE_REWB_BSD=0
HAVE_REWB_SYSV=1
HAVE_SANDBOX_INIT=0
HAVE_STRCASESTR=1
HAVE_STRINGLIST=0
HAVE_STRLCAT=1
HAVE_STRLCPY=1
HAVE_STRNDUP=1
HAVE_STRPTIME=1
HAVE_STRSEP=1
HAVE_STRTONUM=0
HAVE_VASPRINTF=1
HAVE_WFLAG=1
EOF
    put "$EXT/package/mandoc/mandoc.hash" <<'EOF'
sha256  8bf0d570f01e70a6e124884088870cbed7537f36328d512909eb10cd53179d9c  mdocml_1.14.6.orig.tar.gz
EOF

    # ── man-pages: C library + system call + file format pages (not in Buildroot) ──
    # Sub-sections (man3type, man2const...) go into their main section (man3, man2), as
    # Debian does: "man size_t" then finds size_t.3type.
    put "$EXT/package/man-pages/Config.in" <<'EOF'
config BR2_PACKAGE_MAN_PAGES
	bool "man-pages"
	help
	  Linux man-pages: sections 2-7 (system calls, C library, ...).
EOF
    put "$EXT/package/man-pages/man-pages.mk" <<'EOF'
MAN_PAGES_VERSION = 6.19
MAN_PAGES_SOURCE = manpages_$(MAN_PAGES_VERSION).orig.tar.xz
MAN_PAGES_SITE = https://archive.ubuntu.com/ubuntu/pool/main/m/manpages
MAN_PAGES_LICENSE = Linux-man-pages-copyleft, GPL-2.0+, BSD, MIT (per page)
define MAN_PAGES_INSTALL_TARGET_CMDS
	for d in $(@D)/man/man[1-9]*; do \
		s=$$(basename $$d | cut -c4); \
		mkdir -p $(TARGET_DIR)/usr/share/man/man$$s; \
		cp -a $$d/. $(TARGET_DIR)/usr/share/man/man$$s/; \
	done
endef
$(eval $(generic-package))
EOF
    put "$EXT/package/man-pages/man-pages.hash" <<'EOF'
sha256  88a7c42ad2e03d8b96dc72d95e451f2d875ff0f43103a8eb8ac8242133bdcb05  manpages_6.19.orig.tar.xz
EOF

    # ── UPX for the target (--upx): Buildroot's own upx package is only a host tool ──
    # Same CMake build as Buildroot's host-upx; the source is Ubuntu's copy of UPX
    # 4.2.4 (its sha256 is also Debian's). Brings its compression libraries (vendor/);
    # needs only libstdc++ at run time.
    put "$EXT/package/upx-target/Config.in" <<'EOF'
config BR2_PACKAGE_UPX_TARGET
	bool "upx (target)"
	depends on BR2_INSTALL_LIBSTDCPP
	help
	  UPX: packs Linux programs and Windows .exe/.dll files.
EOF
    put "$EXT/package/upx-target/upx-target.mk" <<'EOF'
UPX_TARGET_VERSION = 4.2.4
UPX_TARGET_SOURCE = upx-ucl_$(UPX_TARGET_VERSION).orig.tar.xz
UPX_TARGET_SITE = https://archive.ubuntu.com/ubuntu/pool/universe/u/upx-ucl
UPX_TARGET_LICENSE = GPL-2.0+
UPX_TARGET_LICENSE_FILES = COPYING
UPX_TARGET_SUPPORTS_IN_SOURCE_BUILD = NO
UPX_TARGET_CONF_OPTS = -DUPX_CONFIG_DISABLE_GITREV=ON -DUPX_CONFIG_DISABLE_WERROR=ON
$(eval $(cmake-package))
EOF
    put "$EXT/package/upx-target/upx-target.hash" <<'EOF'
sha256  5ed6561607d27fb4ef346fc19f08a93696fa8fa127081e7a7114068306b8e1c4  upx-ucl_4.2.4.orig.tar.xz
EOF

    # ── TinyCC (not in Buildroot): a fast C compiler, for Linux and for Windows .exe ──
    # Debian/Ubuntu's TinyCC snapshot (0.9.27, git 2020-08-14; the 0.9.27 release is from
    # 2017). Three compilers that run in the VM:
    #   tcc                Linux programs, with the image's glibc (needs the C headers and
    #                      crt files that come with gcc: left out with --no-gcc)
    #   i386-win32-tcc     32-bit Windows .exe   } TinyCC's own Windows headers and
    #   x86_64-win32-tcc   64-bit Windows .exe   } import libraries; no separate linker
    # Runtime libraries (libtcc1.a): Linux's is built with the target gcc (TinyCC's
    # "usegcc" option); the Windows ones must be compiled BY the Windows cross compilers,
    # which the build machine can't run from the target build, so the host build (always
    # on: same host-tool cache for every image) makes them. configure would guess library
    # paths by running a test program on the build machine: every path is given instead.
    # The tarball has no top-level folder.
    put "$EXT/package/tinycc/Config.in" <<'EOF'
config BR2_PACKAGE_TINYCC
	bool "tinycc"
	depends on BR2_i386
	select BR2_PACKAGE_HOST_TINYCC
	help
	  TinyCC: tcc (Linux) and/or cross compilers for Windows .exe.

if BR2_PACKAGE_TINYCC
config BR2_PACKAGE_TINYCC_LINUX
	bool "tcc: Linux programs"
	depends on BR2_PACKAGE_GCC_TARGET
	help
	  Uses the C headers and crt files that gcc-target installs.

config BR2_PACKAGE_TINYCC_WIN
	bool "i386-win32-tcc, x86_64-win32-tcc: Windows programs"
endif

config BR2_PACKAGE_HOST_TINYCC
	bool "host tinycc"
	help
	  Builds the Windows runtime libraries (libtcc1.a) for tinycc.
EOF
    put "$EXT/package/tinycc/tinycc.mk" <<'EOF'
TINYCC_VERSION = 0.9.27+git20200814.62c30a4a
TINYCC_SOURCE = tcc_$(TINYCC_VERSION).orig.tar.bz2
TINYCC_SITE = https://archive.ubuntu.com/ubuntu/pool/universe/t/tcc
TINYCC_STRIP_COMPONENTS = 0
TINYCC_LICENSE = LGPL-2.1+
TINYCC_LICENSE_FILES = COPYING
TINYCC_DEPENDENCIES = host-tinycc
TINYCC_TCCDIR = /usr/lib/tcc
# Buildroot's target-finalize deletes every *.a under usr/lib, the runtime libraries
# included: they also go to staging, and post-build puts them back from there.
TINYCC_INSTALL_STAGING = YES
# tcc for Linux programs: needs the C headers and crt files that gcc-target installs. Its
# runtime library is built with the target gcc, without stack protection (gcc's checks
# call a glibc helper that tcc doesn't link: tcc -b failed with __stack_chk_fail_local).
ifeq ($(BR2_PACKAGE_TINYCC_LINUX),y)
TINYCC_DEPENDENCIES += gcc-target
TINYCC_TARGETS += tcc
define TINYCC_BUILD_LINUX_LIB
	$(TARGET_MAKE_ENV) $(MAKE) -C $(@D)/lib i386-libtcc1-usegcc=yes \
		XFLAGS="$(TARGET_CFLAGS) -fPIC -fno-stack-protector -I.."
endef
define TINYCC_INSTALL_LINUX
	$(INSTALL) -D -m 0755 $(@D)/tcc $(TARGET_DIR)/usr/bin/tcc
	mkdir -p $(TARGET_DIR)$(TINYCC_TCCDIR)/include
	cp $(@D)/libtcc1.a $(@D)/bcheck.o $(@D)/bt-exe.o $(@D)/bt-log.o $(TARGET_DIR)$(TINYCC_TCCDIR)/
	cp $(@D)/include/*.h $(@D)/tcclib.h $(TARGET_DIR)$(TINYCC_TCCDIR)/include/
endef
define TINYCC_STAGING_LINUX
	$(INSTALL) -D -m 0644 $(@D)/libtcc1.a $(STAGING_DIR)$(TINYCC_TCCDIR)/libtcc1.a
endef
endif
# Windows cross compilers, with TinyCC's own Windows headers and import libraries; the
# runtime libraries come from the host build. libtcc1.a holds them as ELF objects,
# x86-64 ones for 64-bit .exe (hence the architecture check exclusion).
ifeq ($(BR2_PACKAGE_TINYCC_WIN),y)
TINYCC_TARGETS += i386-win32-tcc x86_64-win32-tcc
TINYCC_BIN_ARCH_EXCLUDE = $(TINYCC_TCCDIR)/win32
define TINYCC_INSTALL_WIN
	$(INSTALL) -D -m 0755 $(@D)/i386-win32-tcc $(TARGET_DIR)/usr/bin/i386-win32-tcc
	$(INSTALL) -D -m 0755 $(@D)/x86_64-win32-tcc $(TARGET_DIR)/usr/bin/x86_64-win32-tcc
	ln -sf x86_64-win32-tcc $(TARGET_DIR)/usr/bin/wincc
	mkdir -p $(TARGET_DIR)$(TINYCC_TCCDIR)/win32/lib $(TARGET_DIR)$(TINYCC_TCCDIR)/win32/include
	cp $(@D)/win32/lib/*.def $(HOST_DIR)/share/tinycc/*-libtcc1.a $(TARGET_DIR)$(TINYCC_TCCDIR)/win32/lib/
	cp -r $(@D)/win32/include/. $(TARGET_DIR)$(TINYCC_TCCDIR)/win32/include/
	cp $(@D)/include/*.h $(@D)/tcclib.h $(TARGET_DIR)$(TINYCC_TCCDIR)/win32/include/
	cp -r $(@D)/win32/examples $(TARGET_DIR)$(TINYCC_TCCDIR)/win32/
endef
define TINYCC_STAGING_WIN
	mkdir -p $(STAGING_DIR)$(TINYCC_TCCDIR)/win32/lib
	cp $(HOST_DIR)/share/tinycc/*-libtcc1.a $(STAGING_DIR)$(TINYCC_TCCDIR)/win32/lib/
endef
endif
define HOST_TINYCC_CONFIGURE_CMDS
	cd $(@D) && $(HOST_MAKE_ENV) ./configure --prefix=$(HOST_DIR) --cc="$(HOSTCC)"
endef
define HOST_TINYCC_BUILD_CMDS
	$(HOST_MAKE_ENV) $(MAKE) -C $(@D) cross-i386-win32 cross-x86_64-win32
endef
define HOST_TINYCC_INSTALL_CMDS
	mkdir -p $(HOST_DIR)/share/tinycc
	cp $(@D)/i386-win32-libtcc1.a $(@D)/x86_64-win32-libtcc1.a $(HOST_DIR)/share/tinycc/
endef
define TINYCC_CONFIGURE_CMDS
	cd $(@D) && $(TARGET_MAKE_ENV) ./configure --prefix=/usr --cc="$(TARGET_CC)" --ar="$(TARGET_AR)" \
		--cpu=i386 --extra-cflags="$(TARGET_CFLAGS)" --extra-ldflags="$(TARGET_LDFLAGS)" \
		--sysincludepaths="{B}/include:/usr/local/include:/usr/include" \
		--libpaths="{B}:/usr/lib:/lib:/usr/local/lib" --crtprefix=/usr/lib --elfinterp=/lib/ld-linux.so.2
endef
# the compilers, built with the target gcc
define TINYCC_BUILD_CMDS
	$(TARGET_MAKE_ENV) $(MAKE) -C $(@D) $(TINYCC_TARGETS)
	$(TINYCC_BUILD_LINUX_LIB)
endef
# Same layout as TinyCC's own "make install": $(tccdir) and, for Windows, $(tccdir)/win32
define TINYCC_INSTALL_TARGET_CMDS
	rm -rf $(TARGET_DIR)$(TINYCC_TCCDIR)
	$(TINYCC_INSTALL_LINUX)
	$(TINYCC_INSTALL_WIN)
endef
define TINYCC_INSTALL_STAGING_CMDS
	rm -rf $(STAGING_DIR)$(TINYCC_TCCDIR)
	$(TINYCC_STAGING_LINUX)
	$(TINYCC_STAGING_WIN)
endef
$(eval $(generic-package))
$(eval $(host-generic-package))
EOF
    put "$EXT/package/tinycc/tinycc.hash" <<'EOF'
sha256  d63c113bcdd657f51c8ee197a0b0a24203603313e19a52b5f678d40ab7f5df35  tcc_0.9.27+git20200814.62c30a4a.orig.tar.bz2
EOF

    # ── GNU Unifont (not in Buildroot): a glyph for every Unicode character ──
    # unifont.otf: the Basic Multilingual Plane (all modern scripts, CJK, symbols, box
    # drawing) PLUS the emoji blocks U+1F000-1FAFF from Unifont Upper (2,816 glyphs),
    # built here with Unifont's own hex2otf. One OpenType font holds at most 65,535
    # glyphs, so all of Unifont (114,000) can't be one font, but this subset (59,902)
    # can: xterm, which draws every wide character from a single font, then shows CJK
    # and (monochrome) emoji together. unifont_upper.otf: planes 1+ as released (other
    # historic scripts, math alphanumerics...), for fontconfig fallback and fbterm.
    put "$EXT/package/unifont/Config.in" <<'EOF'
config BR2_PACKAGE_UNIFONT
	bool "unifont"
	help
	  GNU Unifont, OpenType: Unifont (BMP + emoji) and Unifont Upper.
EOF
    put "$EXT/package/unifont/unifont.mk" <<'EOF'
UNIFONT_VERSION = 18.0.01
UNIFONT_SITE = https://ftp.gnu.org/gnu/unifont/unifont-$(UNIFONT_VERSION)
UNIFONT_LICENSE = OFL-1.1 or GPL-2.0+ with font embedding exception
UNIFONT_LICENSE_FILES = OFL-1.1.txt COPYING
UNIFONT_HEX = $(@D)/font/precompiled
# Invisible characters (variation selectors, ZWJ/ZWNJ, direction marks, word joiner, BOM)
# as blank zero-width glyphs: Unifont draws them as labelled dashed boxes, and xterm shows a
# box for a VS16 (❤️, ⭐️) or a ZWJ that its font has no glyph for.
UNIFONT_BLANK = FE0.|200[B-F]|206[0-4]|FEFF
# hex: plane 0 (invisible characters blanked, see above) then the emoji
# blocks of Unifont Upper (01F000-01FAFF: 6 hex digits there); both files are already in code
# order. Combining-mark offsets only for glyphs that are in the font.
define UNIFONT_BUILD_CMDS
	$(HOSTCC) -std=c99 -O2 -o $(@D)/hex2otf $(@D)/src/hex2otf.c
	sed -E 's/^($(UNIFONT_BLANK)):.*/\1:00000000000000000000000000000000/' \
		$(UNIFONT_HEX)/unifont-$(UNIFONT_VERSION).hex > $(@D)/v86.hex
	grep -E '^01F[0-9A][0-9A-F][0-9A-F]:' $(UNIFONT_HEX)/unifont_upper-$(UNIFONT_VERSION).hex >> $(@D)/v86.hex
	cut -d: -f1 $(@D)/v86.hex > $(@D)/v86.codes
	cat $(@D)/font/plane00/plane00-combining.txt $(@D)/font/plane01/plane01-combining.txt | \
		awk -F: 'NR == FNR { have[$$1] = 1; next } have[$$1]' $(@D)/v86.codes - > $(@D)/v86.combining
	grep -E '^($(UNIFONT_BLANK)):' $(@D)/v86.hex | sed 's/:.*/:0/' >> $(@D)/v86.combining
	$(@D)/hex2otf hex=$(@D)/v86.hex pos=$(@D)/v86.combining format=cff,gpos,gsub \
		1="Unifont" 2="Regular" 4="Unifont" 6="Unifont" \
		5="Version $(UNIFONT_VERSION), plane 0 + emoji (v86)" out=$(@D)/unifont.otf
endef
define UNIFONT_INSTALL_TARGET_CMDS
	$(INSTALL) -D -m 0644 $(@D)/unifont.otf $(TARGET_DIR)/usr/share/fonts/unifont/unifont.otf
	$(INSTALL) -D -m 0644 $(UNIFONT_HEX)/unifont_upper-$(UNIFONT_VERSION).otf \
		$(TARGET_DIR)/usr/share/fonts/unifont/unifont_upper.otf
endef
$(eval $(generic-package))
EOF
    put "$EXT/package/unifont/unifont.hash" <<'EOF'
sha256  eab60847aac34c8768765cecc7821faf50de2636187b452b9b5fa50a12b00bc3  unifont-18.0.01.tar.gz
EOF

    # ── mlterm (not in Buildroot): a second terminal, with HarfBuzz shaping ──
    # xterm draws every character separately; mlterm shapes runs with HarfBuzz, so flags
    # (🇫🇷), skin tones (👍🏽) and ZWJ sequences (🏳️‍🌈) combine. Drawn with cairo: only
    # mlterm's cairo engine falls back to other fonts for missing characters. Left out:
    # input methods, GTK config tools, images, ssh, and (removed after install) the
    # legacy-encoding converters and Indic keyboard plugins (UTF-8 + HarfBuzz cover
    # those scripts). ~0.9 MB + HarfBuzz 0.9 MB + cairo 0.85 MB.
    put "$EXT/package/mlterm/Config.in" <<'EOF'
config BR2_PACKAGE_MLTERM
	bool "mlterm"
	depends on BR2_PACKAGE_XORG7
	select BR2_PACKAGE_XLIB_LIBX11
	select BR2_PACKAGE_FONTCONFIG
	select BR2_PACKAGE_FREETYPE
	select BR2_PACKAGE_HARFBUZZ
	select BR2_PACKAGE_CAIRO
	help
	  mlterm: multilingual X11 terminal with OpenType shaping (HarfBuzz).
EOF
    put "$EXT/package/mlterm/mlterm.mk" <<'EOF'
MLTERM_VERSION = 3.9.4
MLTERM_SITE = https://downloads.sourceforge.net/project/mlterm/01release/mlterm-$(MLTERM_VERSION)
MLTERM_LICENSE = BSD-3-Clause
MLTERM_LICENSE_FILES = LICENCE
MLTERM_DEPENDENCIES = host-pkgconf xlib_libX11 fontconfig freetype harfbuzz cairo
MLTERM_CONF_OPTS = --with-gui=xlib --with-type-engines=cairo --enable-otl \
	--disable-fribidi --disable-image --disable-ssh2 --disable-kbd --disable-uim \
	--disable-iiimf --disable-m17nlib --disable-ibus --disable-fcitx --disable-scim \
	--disable-canna --disable-wnn --disable-skk --with-tools=mlclient \
	--with-scrollbars=sample --with-utmp=none
# its Makefiles add -I/usr/local/include -L/usr/local/lib (host paths: the cross
# compiler refuses them)
define MLTERM_NO_HOST_PATHS
	find $(@D) -name Makefile.in -exec $(SED) 's@-I/usr/local/include@@g; s@-L/usr/local/lib@@g' {} +
endef
MLTERM_POST_PATCH_HOOKS += MLTERM_NO_HOST_PATHS
define MLTERM_TRIM
	rm -rf $(TARGET_DIR)/usr/lib/mef $(TARGET_DIR)/usr/lib/mlterm/libind_* \
		$(TARGET_DIR)/usr/lib/mlterm/libctl_iscii.so $(TARGET_DIR)/usr/lib/mlterm/libzmodem.so
endef
MLTERM_POST_INSTALL_TARGET_HOOKS += MLTERM_TRIM
$(eval $(autotools-package))
EOF
    # Emoji ZWJ and tag sequences (👩‍💻, families, 🏳️‍🌈, 🏴‍☠️, England's flag) in one cell, so
    # that HarfBuzz gets them in one piece; only between emoji (a joiner in Indic or Arabic
    # text is left alone). Buildroot applies the package's *.patch files, in name order.
    put "$EXT/package/mlterm/0001-emoji-zwj-and-tag-sequences.patch" <<'EOF'
--- a/vtemu/vt_parser.c
+++ b/vtemu/vt_parser.c
@@ -925,6 +925,16 @@
   return 1;
 }
 
+/* Characters that take part in emoji ZWJ sequences: pictographs and emoji-capable symbols */
+static int is_emoji_char(u_int32_t ch) {
+  return (0x1f000 <= ch && ch <= 0x1faff) || (0x2600 <= ch && ch <= 0x27bf) ||
+         (0x2300 <= ch && ch <= 0x23ff) || (0x2b00 <= ch && ch <= 0x2bff) ||
+         (0x2190 <= ch && ch <= 0x21ff) || (0x25a0 <= ch && ch <= 0x25ff) ||
+         ch == 0x00a9 || ch == 0x00ae || ch == 0x203c || ch == 0x2049 || ch == 0x2122 ||
+         ch == 0x2139 || ch == 0x24c2 || ch == 0x2934 || ch == 0x2935 || ch == 0x3030 ||
+         ch == 0x303d || ch == 0x3297 || ch == 0x3299;
+}
+
 static void put_char(vt_parser_t *vt_parser, u_int32_t ch, ef_charset_t cs,
                      ef_property_t prop) {
   vt_color_t fg_color;
@@ -973,6 +983,48 @@
     is_comb = 0;
   }
 
+  /*
+   * Emoji ZWJ sequences (U+1F469 U+200D U+1F4BB, families, U+1F3F3 U+FE0F U+200D U+1F308)
+   * and tag sequences (flag of England): the joiner, the emoji after it and the tag
+   * characters are combined to the previous character, so that the sequence is in one
+   * cell and OpenType layout shapes it in one piece. Otherwise the joiner is a cell of
+   * its own, drawn with another font, and the sequence is split into its parts.
+   * Only between emoji: a joiner between letters (Indic scripts, Arabic) stays as it was.
+   * If the cell is full (MAX_COMB_SIZE), the character is appended as usual.
+   */
+  if (cs == ISO10646_UCS4_1 && !is_comb && vt_parser->use_char_combining &&
+      (ch == 0x200d || is_emoji_char(ch) || (0xe0020 <= ch && ch <= 0xe007f))) {
+    vt_char_t *prev_ch;
+
+    if (vt_parser->w_buf.filled_len > 0) {
+      prev_ch = &vt_parser->w_buf.chars[vt_parser->w_buf.filled_len - 1];
+    } else {
+      prev_ch = vt_screen_get_n_prev_char(vt_parser->screen, 1);
+    }
+
+    if (prev_ch) {
+      vt_char_t *comb;
+      u_int num;
+      u_int32_t last;
+
+      comb = vt_get_combining_chars(prev_ch, &num);
+      last = num > 0 ? vt_char_code(comb + num - 1) : vt_char_code(prev_ch);
+
+      if (ch == 0x200d) {
+        /* after an emoji (or its U+FE0F / skin tone / tag) */
+        is_comb = is_emoji_char(vt_char_code(prev_ch)) &&
+                  (is_emoji_char(last) || last == 0xfe0f || (0xe0020 <= last && last <= 0xe007f));
+      } else if (0xe0020 <= ch && ch <= 0xe007f) {
+        /* tags: after U+1F3F4 WAVING BLACK FLAG or a tag */
+        is_comb = vt_char_code(prev_ch) == 0x1f3f4 &&
+                  (num == 0 || (0xe0020 <= last && last <= 0xe007f));
+      } else {
+        /* an emoji right after a joiner */
+        is_comb = (last == 0x200d);
+      }
+    }
+  }
+
   fg_color = vt_parser->fg_color;
   bg_color = vt_parser->bg_color;
   is_italic = vt_parser->is_italic ? 1 : 0;
EOF
    # Keycap emoji (#️⃣ 1️⃣: # or a digit, U+FE0F, U+20E3): the base is ASCII, so mlterm
    # would use the text font, which has no keycap. Once U+20E3 is combined, the base
    # moves to the emoji font's unicode area (aafont's U+1F000-1FAFF line).
    put "$EXT/package/mlterm/0002-keycap-emoji-use-the-emoji-font.patch" <<'EOF'
--- a/vtemu/vt_char.c
+++ b/vtemu/vt_char.c
@@ -519,6 +519,39 @@
   vt_char_set(comb, code, cs, is_fullwidth, is_awidth, is_comb, fg_color, bg_color,
               is_bold, is_italic, line_style, is_blinking, is_protected);
 
+  /*
+   * Keycap emoji: # * or 0-9, U+FE0F, U+20E3. The base is ASCII, so the cell would be
+   * shaped and drawn with the text font, which has no keycap glyph. Once U+20E3 is
+   * combined, the base goes to the unicode area of the emoji font (the area U+1F51F
+   * KEYCAP TEN is in, e.g. "U+1F000-1FAFF = Twemoji Mozilla" in ~/.mlterm/aafont), like
+   * any emoji, so that shaping and drawing both use that font. No such area: unchanged.
+   * Only with U+FE0F: the emoji font has no keycap for "# U+20E3" alone.
+   */
+  if (code == 0x20e3) {
+    vt_char_t *base = ch->u.multi_ch;
+    u_int32_t base_code = base->u.ch.code;
+    u_int num = get_comb_size(base);
+    u_int count;
+    u_int idx;
+
+    for (count = 1; count <= num; count++) {
+      if (base[count].u.ch.code == 0xfe0f) {
+        break;
+      }
+    }
+
+    if ((base_code == '#' || base_code == '*' || ('0' <= base_code && base_code <= '9')) &&
+        count <= num && !IS_UNICODE_AREA_CS(base->u.ch.attr)) {
+      for (idx = num_unicode_areas; idx > 0; idx--) {
+        if (unicode_areas[idx - 1].min <= 0x1f51f && 0x1f51f <= unicode_areas[idx - 1].max) {
+          base->u.ch.attr &= ~(MAX_CHARSET << CS_SHIFT);
+          base->u.ch.attr |= (idx << CS_SHIFT) | (0x1 << 15); /* see IS_UNICODE_AREA_CS() */
+          break;
+        }
+      }
+    }
+  }
+
 #ifdef USE_NORMALIZE
   normalize(ch, comb_size);
 #endif
EOF
    put "$EXT/package/mlterm/mlterm.hash" <<'EOF'
sha256  171de4c4f3443bc1211cc51df5caa0e082ffcdd33ab3ce261bc0a4cfe85d9b5e  mlterm-3.9.4.tar.gz
EOF

    # ── Twemoji (Mozilla's COLR build): colour emoji for Xft programs (xterm, XFE) ──
    # Vector colour layers (COLRv0): FreeType renders them in colour at any size, 1.5 MB.
    # Noto Color Emoji also works but is 11 MB of PNG bitmaps (barely compressible) that
    # Xft scales down from 128 px. Emoji 15.0. Graphics CC-BY 4.0 (Twitter).
    # fbterm can't draw colour glyphs; it keeps Unifont's monochrome emoji.
    put "$EXT/package/twemoji/Config.in" <<'EOF'
config BR2_PACKAGE_TWEMOJI
	bool "twemoji"
	help
	  Twemoji colour emoji font (COLRv0 build by Mozilla).
EOF
    put "$EXT/package/twemoji/twemoji.mk" <<'EOF'
TWEMOJI_VERSION = 0.7.0
TWEMOJI_SOURCE = Twemoji.Mozilla.ttf
TWEMOJI_SITE = https://github.com/mozilla/twemoji-colr/releases/download/v$(TWEMOJI_VERSION)
TWEMOJI_LICENSE = CC-BY-4.0
define TWEMOJI_EXTRACT_CMDS
	cp $(TWEMOJI_DL_DIR)/$(TWEMOJI_SOURCE) $(@D)/
endef
define TWEMOJI_INSTALL_TARGET_CMDS
	$(INSTALL) -D -m 0644 $(@D)/$(TWEMOJI_SOURCE) $(TARGET_DIR)/usr/share/fonts/twemoji/$(TWEMOJI_SOURCE)
endef
$(eval $(generic-package))
EOF
    put "$EXT/package/twemoji/twemoji.hash" <<'EOF'
sha256  6d90152ee0d29e82fe2a87793af5aa4b7ad13e6538360889e141e81ed299ee8e  Twemoji.Mozilla.ttf
EOF

    # ── BusyBox: UTF-8 (ls, the line editor, etc. show non-ASCII names instead of '?').
    # Unicode is on when $LANG etc. contain ".UTF".
    put "$EXT/busybox.fragment" <<'EOF'
CONFIG_UNICODE_SUPPORT=y
# CONFIG_UNICODE_USING_LOCALE is not set
CONFIG_FEATURE_CHECK_UNICODE_IN_ENV=y
CONFIG_SUBST_WCHAR=63
CONFIG_LAST_SUPPORTED_WCHAR=0
CONFIG_UNICODE_COMBINING_WCHARS=y
CONFIG_UNICODE_WIDE_WCHARS=y
EOF
}


# ══════════════════════════════════════════════════════════════════════════════
#  Kernel fragment (on top of i386_defconfig)
#
#  v86 passes no command line, so it is baked in.
#  Sound: v86 always emulates a NON-PnP Sound Blaster 16 at 0x220, IRQ 5, DMA 1/16-bit 5
#  (src/sb16.js). With PnP support in the kernel, snd-sb16 defaults to isapnp=1, skips
#  the ISA probe and finds no card ("no soundcards"). isapnp=0 + explicit resources fix it.
# ══════════════════════════════════════════════════════════════════════════════

write_kernel_fragment() {
    local cmdline="root=/dev/ram0 rw console=tty0 net.ifnames=0 consoleblank=0"
    cmdline+=" snd_sb16.isapnp=0 snd_sb16.port=0x220 snd_sb16.irq=5 snd_sb16.dma8=1 snd_sb16.dma16=5 snd_sb16.fm_port=0x388"
    [[ "$MODE" == qemu ]] && cmdline+=" vmalloc=512M"     # room to map the bochs framebuffer

    {
        cat <<EOF
CONFIG_CMDLINE_BOOL=y
CONFIG_CMDLINE="$cmdline"
CONFIG_CMDLINE_OVERRIDE=y
EOF
        cat <<'EOF'
# ── trim what v86 doesn't have ──
# CONFIG_SMP is not set
# CONFIG_ACPI is not set
# CONFIG_HYPERVISOR_GUEST is not set
# CONFIG_MODULES is not set
# CONFIG_SUSPEND is not set
# CONFIG_HIBERNATION is not set
# CONFIG_DEBUG_KERNEL is not set
# CONFIG_CFG80211 is not set
# CONFIG_WIRELESS is not set
# CONFIG_EXT4_FS is not set
# CONFIG_XFS_FS is not set
# CONFIG_BTRFS_FS is not set
# CONFIG_LOGO is not set
# v86 boots through its BIOS, never EFI (also drops the EFI framebuffer path)
# CONFIG_EFI is not set
CONFIG_HZ_100=y
CONFIG_NO_HZ_IDLE=y
# ── console, input ──
CONFIG_VT=y
CONFIG_VGA_CONSOLE=y
CONFIG_INPUT_EVDEV=y
CONFIG_INPUT_MOUSEDEV=y
CONFIG_INPUT_MOUSEDEV_PSAUX=y
CONFIG_KEYBOARD_ATKBD=y
CONFIG_MOUSE_PS2=y
CONFIG_SERIO_I8042=y
CONFIG_SERIAL_8250=y
CONFIG_SERIAL_8250_CONSOLE=y
CONFIG_DEVTMPFS=y
CONFIG_DEVTMPFS_MOUNT=y
CONFIG_TMPFS=y
CONFIG_UNIX98_PTYS=y
CONFIG_BLK_DEV_INITRD=y
CONFIG_RD_ZSTD=y
CONFIG_RD_XZ=y
# ── sound: SB16 (ISA, non-PnP) ──
CONFIG_SOUND=y
CONFIG_SND=y
CONFIG_SND_PCM=y
CONFIG_ISA=y
# CONFIG_ISAPNP is not set
# CONFIG_PNPBIOS is not set
CONFIG_SND_ISA=y
CONFIG_SND_SB16=y
# ── network: NE2000 (v86 default) + virtio, 9p host share ──
CONFIG_NETDEVICES=y
CONFIG_ETHERNET=y
# NE2000 lives in the National Semiconductor menu: both menus must stay on
CONFIG_NET_VENDOR_NATSEMI=y
CONFIG_NET_VENDOR_8390=y
CONFIG_NE2K_PCI=y
CONFIG_VIRTIO=y
CONFIG_VIRTIO_PCI=y
CONFIG_VIRTIO_NET=y
CONFIG_NET_9P=y
CONFIG_NET_9P_VIRTIO=y
CONFIG_9P_FS=y
CONFIG_PCI=y
# ── drop hardware v86 never presents ──
# v86 emulates: PIIX3 IDE (ata_piix), NE2000, virtio (net/console/balloon/9p), PS/2,
# 16550 serial, RTC/PIT, SB16, VGA/Bochs. Everything below is i386_defconfig's
# real-PC hardware that only probes and finds nothing (smaller kernel, faster boot).
# USB (host controllers, HID, storage, printer, USB audio/net)
# CONFIG_USB_SUPPORT is not set
# CONFIG_HID_SUPPORT is not set
# other network cards (keeps NATSEMI -> 8390 -> NE2K_PCI) and wireless
# CONFIG_NET_VENDOR_3COM is not set
# CONFIG_NET_VENDOR_ADAPTEC is not set
# CONFIG_NET_VENDOR_AGERE is not set
# CONFIG_NET_VENDOR_ALACRITECH is not set
# CONFIG_NET_VENDOR_AMAZON is not set
# CONFIG_NET_VENDOR_AMD is not set
# CONFIG_NET_VENDOR_AQUANTIA is not set
# CONFIG_NET_VENDOR_ARC is not set
# CONFIG_NET_VENDOR_ASIX is not set
# CONFIG_NET_VENDOR_ATHEROS is not set
# CONFIG_NET_VENDOR_BROADCOM is not set
# CONFIG_NET_VENDOR_CADENCE is not set
# CONFIG_NET_VENDOR_CAVIUM is not set
# CONFIG_NET_VENDOR_CHELSIO is not set
# CONFIG_NET_VENDOR_CIRRUS is not set
# CONFIG_NET_VENDOR_CISCO is not set
# CONFIG_NET_VENDOR_CORTINA is not set
# CONFIG_NET_VENDOR_DAVICOM is not set
# CONFIG_NET_VENDOR_DEC is not set
# CONFIG_NET_VENDOR_DLINK is not set
# CONFIG_NET_VENDOR_EMULEX is not set
# CONFIG_NET_VENDOR_ENGLEDER is not set
# CONFIG_NET_VENDOR_EZCHIP is not set
# CONFIG_NET_VENDOR_FUNGIBLE is not set
# CONFIG_NET_VENDOR_GOOGLE is not set
# CONFIG_NET_VENDOR_HUAWEI is not set
# CONFIG_NET_VENDOR_I825XX is not set
# CONFIG_NET_VENDOR_INTEL is not set
# CONFIG_NET_VENDOR_LITEX is not set
# CONFIG_NET_VENDOR_MARVELL is not set
# CONFIG_NET_VENDOR_MELLANOX is not set
# CONFIG_NET_VENDOR_META is not set
# CONFIG_NET_VENDOR_MICREL is not set
# CONFIG_NET_VENDOR_MICROCHIP is not set
# CONFIG_NET_VENDOR_MICROSEMI is not set
# CONFIG_NET_VENDOR_MICROSOFT is not set
# CONFIG_NET_VENDOR_MUCSE is not set
# CONFIG_NET_VENDOR_MYRI is not set
# CONFIG_NET_VENDOR_NI is not set
# CONFIG_NET_VENDOR_NETRONOME is not set
# CONFIG_NET_VENDOR_NVIDIA is not set
# CONFIG_NET_VENDOR_OKI is not set
# CONFIG_NET_VENDOR_PENSANDO is not set
# CONFIG_NET_VENDOR_QLOGIC is not set
# CONFIG_NET_VENDOR_BROCADE is not set
# CONFIG_NET_VENDOR_QUALCOMM is not set
# CONFIG_NET_VENDOR_RDC is not set
# CONFIG_NET_VENDOR_REALTEK is not set
# CONFIG_NET_VENDOR_RENESAS is not set
# CONFIG_NET_VENDOR_ROCKER is not set
# CONFIG_NET_VENDOR_SAMSUNG is not set
# CONFIG_NET_VENDOR_SEEQ is not set
# CONFIG_NET_VENDOR_SILAN is not set
# CONFIG_NET_VENDOR_SIS is not set
# CONFIG_NET_VENDOR_SOLARFLARE is not set
# CONFIG_NET_VENDOR_SMSC is not set
# CONFIG_NET_VENDOR_SOCIONEXT is not set
# CONFIG_NET_VENDOR_STMICRO is not set
# CONFIG_NET_VENDOR_SUN is not set
# CONFIG_NET_VENDOR_SYNOPSYS is not set
# CONFIG_NET_VENDOR_TEHUTI is not set
# CONFIG_NET_VENDOR_TI is not set
# CONFIG_NET_VENDOR_VERTEXCOM is not set
# CONFIG_NET_VENDOR_VIA is not set
# CONFIG_NET_VENDOR_WANGXUN is not set
# CONFIG_NET_VENDOR_WIZNET is not set
# CONFIG_NET_VENDOR_XILINX is not set
# CONFIG_NET_VENDOR_XIRCOM is not set
# CONFIG_USB_NET_DRIVERS is not set
# CONFIG_WLAN is not set
# disks: PIIX3 IDE only (no AHCI/SATA, other PATA chips, RAID, device-mapper, virtio-blk)
# CONFIG_SATA_AHCI is not set
# CONFIG_SATA_PMP is not set
# CONFIG_PATA_AMD is not set
# CONFIG_PATA_OLDPIIX is not set
# CONFIG_PATA_SCH is not set
# CONFIG_PATA_MPIIX is not set
# CONFIG_ATA_GENERIC is not set
# CONFIG_MD is not set
# CONFIG_SCSI_LOWLEVEL is not set
# CONFIG_VIRTIO_BLK is not set
# sound: SB16 only (no HD Audio, PCI/PCMCIA cards)
# CONFIG_SND_PCI is not set
# CONFIG_SND_PCMCIA is not set
# CONFIG_SND_X86 is not set
# input: PS/2 keyboard + mouse only
# CONFIG_INPUT_JOYSTICK is not set
# CONFIG_INPUT_TABLET is not set
# CONFIG_INPUT_TOUCHSCREEN is not set
# CONFIG_INPUT_MISC is not set
# CONFIG_INPUT_LEDS is not set
# CONFIG_VIRTIO_INPUT is not set
# serial: one plain 16550
# CONFIG_SERIAL_8250_PCI is not set
# CONFIG_SERIAL_8250_EXAR is not set
# CONFIG_SERIAL_8250_LPSS is not set
# CONFIG_SERIAL_8250_MID is not set
# CONFIG_SERIAL_8250_PERICOM is not set
# CONFIG_SERIAL_8250_EXTENDED is not set
# CONFIG_SERIAL_NONSTANDARD is not set
# real-PC chipset and CPU features
# CONFIG_AGP is not set
# CONFIG_IOMMU_SUPPORT is not set
# CONFIG_X86_MCE is not set
# CONFIG_CPU_FREQ is not set
# CONFIG_PM_DEBUG is not set
# CONFIG_THERMAL is not set
# CONFIG_HWMON is not set
# CONFIG_WATCHDOG is not set
# CONFIG_VIRTUALIZATION is not set
# CONFIG_PERF_EVENTS_INTEL_UNCORE is not set
# CONFIG_PERF_EVENTS_INTEL_RAPL is not set
# CONFIG_PERF_EVENTS_INTEL_CSTATE is not set
# CONFIG_PERF_EVENTS_AMD_UNCORE is not set
# ── drop server/distro features a single-user dev VM doesn't use ──
# networking: plain IPv4 (v86's network backends are IPv4 only)
# CONFIG_IPV6 is not set
# CONFIG_NETFILTER is not set
# CONFIG_NET_SCHED is not set
# CONFIG_NETLABEL is not set
# CONFIG_XFRM_USER is not set
# CONFIG_IP_ADVANCED_ROUTER is not set
# CONFIG_IP_PNP is not set
# CONFIG_TCP_CONG_ADVANCED is not set
# CONFIG_NETCONSOLE is not set
# file systems: keep ISO9660, FAT, loop, tmpfs, 9p
# CONFIG_NFS_FS is not set
# CONFIG_AUTOFS_FS is not set
# CONFIG_QUOTA is not set
# CONFIG_HUGETLBFS is not set
# CONFIG_PROC_KCORE is not set
# security, containers, tracing
# CONFIG_SECURITY is not set
# CONFIG_AUDIT is not set
# CONFIG_INTEGRITY is not set
# CONFIG_SYSTEM_TRUSTED_KEYRING is not set
# CONFIG_KEYS is not set
# CONFIG_CGROUPS is not set
# CONFIG_KPROBES is not set
# CONFIG_FTRACE is not set
# CONFIG_BLK_DEV_IO_TRACE is not set
# CONFIG_PROFILING is not set
EOF
        case "$MODE" in
            qemu) cat <<'EOF'
# ── graphics: bochs-drm (KMS) for Xorg modesetting ──
CONFIG_DRM=y
CONFIG_DRM_BOCHS=y
CONFIG_DRM_FBDEV_EMULATION=y
CONFIG_FB=y
CONFIG_FRAMEBUFFER_CONSOLE=y
# CONFIG_DRM_I915 is not set
# CONFIG_DRM_RADEON is not set
# CONFIG_DRM_NOUVEAU is not set
EOF
            ;;
            vesa) cat <<'EOF'
# ── graphics: vesafb on the VBE mode set at boot (Xorg fbdev driver) ──
# FB_VESA also turns on BOOT_VESA_SUPPORT, without which the boot code ignores VESA
# graphics modes. No DRM: bochs-drm would take the card away from vesafb, and
# simplefb/simpledrm would replace vesafb.
# CONFIG_DRM is not set
# CONFIG_SYSFB_SIMPLEFB is not set
CONFIG_FB=y
CONFIG_FB_VESA=y
CONFIG_FRAMEBUFFER_CONSOLE=y
# xmode (Xorg vesa driver): int10 needs port I/O and the BIOS area via /dev/mem
CONFIG_X86_IOPL_IOPERM=y
CONFIG_DEVMEM=y
# CONFIG_STRICT_DEVMEM is not set
# Xvesa (24 bpp) runs the BIOS in real vm86 mode, with the low 1 MB mapped at address 0
CONFIG_X86_LEGACY_VM86=y
CONFIG_DEFAULT_MMAP_MIN_ADDR=0
EOF
            ;;
            nogui) cat <<'EOF'
# ── text console only: no DRM, no framebuffer, so v86 stays in VGA text mode ──
# CONFIG_DRM is not set
# CONFIG_FB is not set
# CONFIG_SYSFB_SIMPLEFB is not set
# CONFIG_FRAMEBUFFER_CONSOLE is not set
EOF
            ;;
        esac
        # --no-apic (the default): no local APIC / IO-APIC (PCI_MSI needs them, v86's devices don't use MSI).
        # Interrupts go through the 8259 PIC, the timer is the PIT. arch/x86/events/msr.c is
        # only built with X86_LOCAL_APIC, so its probe of MSR 0x64e (MSR_PPERF) goes too.
        if ((!APIC)); then cat <<'EOF'
# ── no APIC: 8259 PIC + PIT ──
# CONFIG_PCI_MSI is not set
# CONFIG_X86_UP_APIC is not set
EOF
        fi
    } | put "$EXT/linux.fragment"
}


# ══════════════════════════════════════════════════════════════════════════════
#  Root filesystem overlay
# ══════════════════════════════════════════════════════════════════════════════

write_overlay() {
    # ── init: mounts, rcS (udev), then the console/X session ─────────────────
    {
        cat <<'EOF'
::sysinit:/bin/mount -t proc proc /proc
::sysinit:/bin/mount -t sysfs sysfs /sys
::sysinit:/bin/sh -c 'mountpoint -q /dev || mount -t devtmpfs devtmpfs /dev'
::sysinit:/bin/mkdir -p /dev/pts /dev/shm /run /tmp /mnt/9p
::sysinit:/bin/mount -t devpts devpts /dev/pts
::sysinit:/bin/mount -t tmpfs -o mode=0755 tmpfs /run
::sysinit:/bin/hostname -F /etc/hostname
::sysinit:/etc/init.d/rcS
EOF
        if ((GUI)); then
            echo 'tty1::respawn:/usr/bin/xstart'
        else
            echo 'tty1::respawn:-/bin/bash'
        fi
        cat <<'EOF'
tty2::respawn:/sbin/getty -L tty2 0 linux
ttyS0::respawn:/sbin/getty -L -n -l /usr/bin/autologin ttyS0 115200 vt100
::shutdown:/bin/umount -a -r
EOF
    } | put "$OVL/etc/inittab"

    # COM1 (the page's serial console) logs in as root by itself: getty -n -l runs this
    # instead of asking for a name. login -f skips the password and still shows /etc/motd.
    putx "$OVL/usr/bin/autologin" <<'EOF'
#!/bin/sh
exec /bin/login -f root
EOF
    # root's password is "root" (BR2_TARGET_GENERIC_ROOT_PASSWD): say so after each login
    printf '%s\n' "v86 $NAME" 'The root password is "root" (change it with: passwd)' '' | put "$OVL/etc/motd"

    put "$OVL/etc/profile.d/v86.sh" <<'EOF'
case ":$PATH:" in *:/usr/local/bin:*) ;; *) PATH="$PATH:/usr/local/bin" ;; esac
# busybox init starts the tty1 shell in / with HOME=/ : use root's home instead
if [ "$(id -u)" = 0 ] && [ "${HOME:-/}" = / ]; then HOME=/root; fi
[ "$PWD" = / ] && cd "$HOME"
export PATH TERM=${TERM:-linux}
# UTF-8 everywhere (C.UTF-8 is generated at build time: BR2_GENERATE_LOCALE)
export LANG=${LANG:-C.UTF-8}
# Unicode on the text console: the kernel console holds only 512 glyphs, so logins on a
# framebuffer console (tty2-6 of the GUI images) run fbterm with GNU Unifont instead.
# "touch ~/.nofbterm" turns it off; exiting fbterm logs out.
case "$(tty 2>/dev/null)" in
    /dev/tty[1-9]*)
        if [ -z "$FBTERM_RUNNING" ] && [ -c /dev/fb0 ] && [ ! -e "$HOME/.nofbterm" ] &&
           command -v fbterm >/dev/null; then
            export FBTERM_RUNNING=1
            FONTCONFIG_FILE=/etc/fonts/fbterm.conf fbterm && exit   # if it can't start: plain console
        fi ;;
esac
EOF

    if ((BASHDEV)); then
        put "$OVL/etc/inputrc" <<'EOF'
set show-all-if-ambiguous on
set completion-ignore-case on
set colored-stats on
set mark-symlinked-directories on
"\e[A": history-search-backward
"\e[B": history-search-forward
EOF
        put "$OVL/etc/profile.d/v86-bashdev.sh" <<'EOF'
# shell-script development helpers (--bash-dev); interactive bash only
[ -n "$BASH_VERSION" ] || return 0
case $- in *i*) ;; *) return 0 ;; esac
export HISTSIZE=5000 HISTCONTROL=ignoreboth:erasedups HISTFILE=$HOME/.bash_history
export INPUTRC=/etc/inputrc LESS=-R PAGER=less
shopt -s histappend checkwinsize globstar extglob
# prompt: exit status of the last command when it failed, then user@host:dir
PS1='$(s=$?; [ $s -ne 0 ] && printf "[%s] " $s)\u@\h:\w\$ '
# "bash -x" / set -x traces show file:line and function
export PS4='+ ${BASH_SOURCE##*/}:${LINENO}${FUNCNAME:+ ${FUNCNAME}()}: '
alias ll='ls -lah --color=auto' ls='ls --color=auto' grep='grep --color=auto'
alias sc='shellcheck -x' fmt='shfmt -i 2 -ci -w'
# newscript NAME: a strict-mode script skeleton, made executable
newscript() {
    [ -n "$1" ] && [ ! -e "$1" ] || { echo "usage: newscript NEWFILE" >&2; return 1; }
    cat >"$1" <<'SKEL'
#!/usr/bin/env bash
set -Eeuo pipefail
trap 'echo "error: line $LINENO: $BASH_COMMAND" >&2' ERR
IFS=$'\n\t'

main() {
    echo "hello"
}

main "$@"
SKEL
    chmod +x "$1"; ${EDITOR:-nano} "$1"
}
EOF
    fi

    # fbterm: Unifont, 8x16 cells (without font-width, fbterm sizes every cell to
    # Unifont's widest glyph, 16 px: letters spread apart). fbterm 1.7 keeps characters
    # in 16 bits, so it shows the Basic Multilingual Plane only: emoji above U+FFFF
    # (most of them) appear as boxes; ❤ ✈ ☕ ⚡ and all scripts incl. CJK are fine.
    if ((GUI)); then
        put "$OVL/root/.fbtermrc" <<'EOF'
font-names=Unifont,Unifont Upper
font-size=16
font-width=8
font-height=16
color-foreground=7
color-background=0
history-lines=1000
cursor-shape=0
cursor-interval=500
EOF
    fi

    # ── nano: darker interface (syntax colors are toned down in post-build) ──
    put "$OVL/root/.nanorc" <<'EOF'
include "/usr/share/nano/*.nanorc"
set constantshow
set softwrap
set titlecolor white,blue
set statuscolor white,blue
set promptcolor white,blue
set keycolor cyan
set functioncolor white
set numbercolor cyan
set selectedcolor black,cyan
EOF

    # ── helpers ──────────────────────────────────────────────────────────────
    putx "$OVL/usr/bin/netup" <<'EOF'
#!/bin/sh
# netup: bring up the network cards (v86: net_device.relay_url, or net_devices for two)
#   netup                  every card at once: DHCP (relay_url "fetch", wisp or a websocket
#                          relay such as wss://relay.widgetry.org/); a card without an answer
#                          gets an automatic in-browser address 10.86.x.y/16 from its MAC
#                          (relay_url "inbrowser" links v86 instances in one browser but has
#                          no DHCP), so each instance gets its own. Which card is eth0 depends
#                          on the drivers (virtio before NE2000), not on v86's card order.
#   netup 10.0.0.1         fixed address (/24 unless given) on the first card
#   netup eth1 [ADDRESS]   one card: DHCP, or the fixed address
ip link set lo up
cards=""
for i in /sys/class/net/*; do [ "${i##*/}" = lo ] || cards="$cards ${i##*/}"; done
first=$(echo $cards | cut -d' ' -f1)
[ -n "$first" ] || { echo "netup: no network interface"; exit 1; }
fixed() {   # card address
    case "$2" in */*) cidr=$2 ;; *) cidr=$2/24 ;; esac
    ip link set "$1" up
    ip addr flush dev "$1"
    ip addr add "$cidr" dev "$1" || { echo "netup: bad address $2"; return 1; }
    echo "netup: $1 is $cidr"
}
dhcp() {    # card tries
    ip link set "$1" up
    n=$2
    while [ "$n" -gt 0 ]; do
        udhcpc -i "$1" -q -n -t 3 -T 3 >/dev/null 2>&1 && { echo "netup: $1 is $(ip -4 addr show dev "$1" | awk '/inet /{print $2}') (DHCP)"; return 0; }
        n=$((n - 1)); sleep 1
    done
    return 1
}
auto() {    # card: in-browser address from the MAC's last two bytes
    mac=$(cat "/sys/class/net/$1/address")
    a=$((0x$(echo "$mac" | cut -d: -f5))) b=$((0x$(echo "$mac" | cut -d: -f6)))
    [ "$b" -eq 0 ] && b=1; [ "$b" -eq 255 ] && b=254
    fixed "$1" "10.86.$a.$b/16" >/dev/null && echo "netup: $1 is 10.86.$a.$b/16 (in-browser, from its MAC)"
}
case "$1" in
    "")         for c in $cards; do { dhcp "$c" 2 || auto "$c"; } > "/tmp/netup.$c" 2>&1 & done
                wait
                up=0
                for c in $cards; do cat "/tmp/netup.$c"; grep -q " is " "/tmp/netup.$c" && up=1; rm -f "/tmp/netup.$c"; done
                [ "$up" = 1 ] ;;
    [0-9]*)     fixed "$first" "$1" ;;
    *)          [ -e "/sys/class/net/$1" ] || { echo "netup: no card $1 (cards:$cards)"; exit 1; }
                if [ -n "$2" ]; then fixed "$1" "$2"
                else dhcp "$1" 5 || { echo "netup: DHCP failed on $1"; exit 1; }; fi ;;
esac
EOF
    putx "$OVL/usr/bin/netdown" <<'EOF'
#!/bin/sh
# netdown [CARD]: take one card down, or all of them
killall udhcpc 2>/dev/null
for i in /sys/class/net/*; do
    c=${i##*/}
    [ "$c" = lo ] && continue
    [ -n "$1" ] && [ "$c" != "$1" ] && continue
    ip addr flush dev "$c"; ip link set "$c" down
    echo "netdown: $c is down"
done
EOF
    putx "$OVL/etc/init.d/S50mount9p" <<'EOF'
#!/bin/sh
# v86 'filesystem: {}' shares a host folder over virtio-9p (tag host9p)
[ "$1" = start ] || exit 0
mount -t 9p -o trans=virtio,version=9p2000.L,msize=131072 host9p /mnt/9p 2>/dev/null || true
EOF

    # ALSA default device: v86's SB16 DMA buffer holds at most ~17800 frames, but most
    # programs (speaker-test's defaults, SDL...) ask for more and fail ("buffer to small").
    # dmix fixes the hardware buffer at a size the SB16 takes, at its native 44.1 kHz,
    # and lets several programs play at once; plug converts whatever they send.
    put "$OVL/etc/asound.conf" <<'EOF'
pcm.!default {
    type plug
    slave.pcm "sb16mix"
}
pcm.sb16mix {
    type dmix
    ipc_key 5316
    ipc_perm 0666
    slave {
        pcm "hw:0,0"
        format S16_LE
        channels 2
        rate 44100
        period_size 1024
        buffer_size 8192
    }
}
ctl.!default {
    type hw
    card 0
}
EOF

    # ALSA brings the SB16 up muted at volume 0 after every boot: set a usable level
    putx "$OVL/etc/init.d/S31sound" <<'EOF'
#!/bin/sh
[ "$1" = start ] || exit 0
for c in Master PCM; do amixer -q sset "$c" 80% unmute 2>/dev/null; done
exit 0
EOF

    putx "$OVL/usr/bin/soundtest" <<'EOF'
#!/bin/sh
# soundtest: show the sound card, unmute it and play a 440 Hz tone
echo "=== sound cards ==="
cat /proc/asound/cards 2>/dev/null
if ! grep -q '^ *[0-9]' /proc/asound/cards 2>/dev/null; then
    echo "no card. Kernel messages:"; dmesg | grep -i -E 'sb16|sound|snd' | tail -n 10
    echo "(v86 emulates an SB16 at 0x220 irq 5 dma 1/5; the kernel cmdline must have snd_sb16.isapnp=0)"
    exit 1
fi
for c in Master PCM; do amixer -q sset "$c" 80% unmute 2>/dev/null; done
echo "=== tone (in the browser, click the page first so audio is allowed) ==="
# The SB16's DMA buffer holds at most ~17800 frames; speaker-test's default (4 x 12000)
# doesn't fit ("buffer to small"). Ask for 300 ms in 4 periods at 44.1 kHz (SB16's max
# rate, so nothing has to be resampled).
speaker-test -c 2 -r 44100 -F S16_LE -b 300000 -p 75000 -t sine -f 440 -l 1 && exit 0
echo "speaker-test failed; trying 2 s of noise with aplay instead"
aplay -q -f S16_LE -r 22050 -c 1 -d 2 /dev/urandom
EOF

    # selftest: the same quick checks for every image (PASS/FAIL, exit status = failures)
    putx "$OVL/usr/bin/selftest" <<'EOF'
#!/bin/sh
# selftest: quick checks of this image; run it once the desktop (or prompt) is up
pass=0 fail=0
chk() {
    d=$1; shift
    if "$@" >/dev/null 2>&1; then echo "PASS  $d"; pass=$((pass + 1))
    else echo "FAIL  $d"; fail=$((fail + 1)); fi
}
echo "=== $(head -n1 /etc/issue 2>/dev/null | sed 's/\\[a-zA-Z]//g') | kernel $(uname -r) | $(awk '/MemTotal/ {print int($2/1024)" MB RAM"}' /proc/meminfo)"
chk "initramfs unpacked completely"   sh -c '! dmesg | grep -qi "initramfs unpacking failed"'
chk "sound card: SB16"                grep -q SB16 /proc/asound/cards
chk "speaker-test + amixer present"   sh -c 'command -v speaker-test && command -v amixer'
if command -v gcc >/dev/null; then
if [ -f /usr/include/SDL3/SDL.h ]; then
cat > /tmp/st.c <<'C'
#include <stdio.h>
#include <SDL3/SDL.h>
int main(void) { printf("ok\n"); return SDL_GetVersion() <= 0; }
C
sdlflags='$(pkg-config --cflags --libs sdl3)'; what="stdio.h + SDL3"
else
cat > /tmp/st.c <<'C'
#include <stdio.h>
int main(void) { printf("ok\n"); return 0; }
C
sdlflags=""; what="stdio.h"
fi
cat > /tmp/st.cc <<'CC'
#include <iostream>
int main() { std::cout << "ok" << std::endl; }
CC
chk "gcc: $what compile, link, run" sh -c "gcc -o /tmp/st /tmp/st.c $sdlflags && /tmp/st"
if [ -f /usr/include/SDL3_net/SDL_net.h ]; then
    printf '%s\n' '#include <SDL3_net/SDL_net.h>' 'int main(void) { return !NET_Init(); }' > /tmp/stn.c
    chk "gcc: SDL3_net compile, link, init" sh -c 'gcc -o /tmp/stn /tmp/stn.c $(pkg-config --cflags --libs sdl3-net) && /tmp/stn'
fi
chk "g++: iostream compile, link, run"       sh -c 'g++ -o /tmp/stcc /tmp/st.cc && /tmp/stcc'
# atexit() and C++ global destructors need __dso_handle from gcc's crtbegin (only
# there when gcc is built with --enable-__cxa_atexit)
cat > /tmp/stx.cc <<'CC'
#include <cstdio>
#include <cstdlib>
struct G { ~G() { std::puts("dtor"); } } g;
static void bye() { std::puts("atexit"); }
int main() { return std::atexit(bye); }
CC
chk "atexit + C++ global destructor"  sh -c 'g++ -o /tmp/stx /tmp/stx.cc && [ "$(/tmp/stx | tr "\n" " ")" = "atexit dtor " ]'
else
    echo "info  no compiler in this image (--no-gcc): compile tests skipped"
fi
chk "network interface present"       sh -c 'ls /sys/class/net | grep -qv "^lo$"'
chk "nano installed"                  command -v nano
if command -v tcc >/dev/null; then
    cat > /tmp/t.c <<'C'
#include <stdio.h>
#include <math.h>
#include <pthread.h>
static void *twice(void *p) { return (void *)(2 * (long)p); }
int main(void) { pthread_t t; void *r; pthread_create(&t, 0, twice, (void *)21); pthread_join(t, &r);
                 printf("%ld %.1f\n", (long)r, sqrt(2.25)); return 0; }
C
    chk "tcc: compile, link, run (stdio, math, threads)" sh -c 'tcc -o /tmp/tt /tmp/t.c -lm -lpthread && [ "$(/tmp/tt)" = "42 1.5" ]'
    chk "tcc -run"                    sh -c '[ "$(tcc -run -lm -lpthread /tmp/t.c)" = "42 1.5" ]'
    chk "tcc -b: bounds checking catches an overflow" sh -c 'printf "int main(void) { int a[4]; volatile int i = 4; return a[i]; }\n" > /tmp/tb.c && tcc -b -o /tmp/tb /tmp/tb.c && ! /tmp/tb 2>/dev/null'
fi
if command -v upx >/dev/null; then
    rm -f /tmp/nano.upx
    chk "upx: packs nano, the packed copy runs" sh -c 'upx -q -o /tmp/nano.upx /usr/bin/nano && [ "$(wc -c < /tmp/nano.upx)" -lt "$(wc -c < /usr/bin/nano)" ] && /tmp/nano.upx --version | grep -q nano'
    chk "upx -t: the packed copy tests OK" upx -q -t /tmp/nano.upx
fi
if command -v wincc >/dev/null; then
    cat > /tmp/w.c <<'C'
#include <stdio.h>
#include <windows.h>
int main(void) { printf("process %lu\n", GetCurrentProcessId()); return 0; }
C
    # a Windows program: "MZ", then "PE\0\0" where offset 60 points, then the machine type
    cat > /tmp/pe.sh <<'P'
[ "$(head -c 2 "$1")" = MZ ] || exit 1
o=$(od -An -t u4 -j 60 -N 4 "$1" | tr -d ' ')
[ "$(od -An -t x1 -j "$o" -N 6 "$1" | tr -d ' ')" = "50450000$2" ]
P
    chk "wincc: 64-bit Windows .exe"     sh -c 'wincc /tmp/w.c -o /tmp/w64.exe && sh /tmp/pe.sh /tmp/w64.exe 6486'
    chk "wincc -m32: 32-bit Windows .exe" sh -c 'wincc -m32 /tmp/w.c -o /tmp/w32.exe && sh /tmp/pe.sh /tmp/w32.exe 4c01'
fi
if command -v shellcheck >/dev/null; then
    printf '#!/bin/sh\necho $1\n' > /tmp/sc.sh
    chk "shellcheck: finds SC2086 in a test script" sh -c 'shellcheck /tmp/sc.sh | grep -q SC2086'
fi
if command -v mandoc >/dev/null; then
    chk "man: man 3 printf"           sh -c 'man -T ascii 3 printf | grep -q fprintf'
    chk "man: index (apropos socket)" sh -c 'apropos socket | grep -q "socket(2)"'
fi
chk "nano syntax files (/usr/share/nano/*.nanorc)" sh -c 'ls /usr/share/nano/*.nanorc'
if [ -x /usr/bin/xstart ]; then
    chk "X server running"            pidof Xorg Xvesa
    chk "Fluxbox running"             pidof fluxbox
    chk "no failed xmode attempt"     sh -c '[ ! -s /var/log/Xorg.xmode-failed.log ]'
    [ -e /sys/class/graphics/fb0 ] && echo "info  framebuffer $(tr , x < /sys/class/graphics/fb0/virtual_size)x$(cat /sys/class/graphics/fb0/bits_per_pixel)"
    command -v xmode >/dev/null && echo "info  xmode: $(xmode | head -n1)"
else
    chk "text mode: no framebuffer/DRM" sh -c '[ ! -e /dev/fb0 ] && [ ! -e /dev/dri ]'
fi
mountpoint -q /mnt/9p 2>/dev/null && echo "info  9p host share mounted at /mnt/9p"
echo "=== $pass passed, $fail failed"
exit "$fail"
EOF

    # unitest: UTF-8 and font checks (PASS/FAIL); emojitest is the look-at-it companion
    putx "$OVL/usr/bin/unitest" <<'EOF'
#!/bin/bash
# unitest: check that Unicode works end to end: locale, shell, BusyBox, console, fonts.
# Exit status = number of failures. In X, run emojitest afterwards to see the result.
pass=0 fail=0
chk() {
    d=$1; shift
    if "$@" >/dev/null 2>&1; then echo "PASS  $d"; pass=$((pass + 1))
    else echo "FAIL  $d"; fail=$((fail + 1)); fi
}
# readable BG FG...: every FG has at least 4.5:1 contrast on BG (WCAG, #rrggbb colours).
# Colour 0 (ANSI black) is left out: it is meant to be close to the background.
readable() {
    [ -n "$1" ] && [ $# -ge 2 ] || return 1
    awk -v list="$*" '
        function v(h, i) { return index("0123456789abcdef", tolower(substr(h, i, 1))) - 1 }
        # sRGB to linear, without "^" (BusyBox awk has no math library): y^2.4 = y^2 * t,
        # t^5 = y^2 solved by Newton
        function lin(c,   y, t, t4, i) {
            c /= 255
            if (c <= 0.04045) return c / 12.92
            y = (c + 0.055) / 1.055; t = 1
            for (i = 0; i < 40; i++) { t4 = t * t * t * t; t -= (t4 * t - y * y) / (5 * t4) }
            return y * y * t }
        function lum(h) { sub(/^#/, "", h)
            return 0.2126 * lin(16 * v(h, 1) + v(h, 2)) + 0.7152 * lin(16 * v(h, 3) + v(h, 4)) + 0.0722 * lin(16 * v(h, 5) + v(h, 6)) }
        BEGIN { n = split(list, c, " "); bg = lum(c[1]); bad = 0
            for (i = 2; i <= n; i++) { l = lum(c[i]); if (l > bg) r = (l + 0.05) / (bg + 0.05); else r = (bg + 0.05) / (l + 0.05)
                if (r < 4.5) { printf "%s on %s: %.1f:1\n", c[i], c[1], r; bad = 1 } }
            exit bad }'
}
echo "=== Unicode: LANG=${LANG:-unset}"
chk "LANG is a UTF-8 locale"               sh -c 'case "$LANG" in *.UTF-8|*.utf8) ;; *) exit 1 ;; esac'
s='aé€😀'
chk "bash counts characters, not bytes"    test "${#s}" = 4
chk "glibc decodes UTF-8 (\${s:3:1} = 😀)"  test "${s:3:1}" = '😀'
chk "BusyBox wc -m counts characters"      test "$(printf 'é€😀' | wc -m)" = 3
d=/tmp/unitest.$$; mkdir -p "$d" && : > "$d/ünï-😀.txt"
chk "BusyBox ls shows a UTF-8 file name"   sh -c "ls '$d' | grep -qx 'ünï-😀.txt'"
rm -rf "$d"
chk "kernel console in UTF-8 mode"         grep -qx 1 /sys/module/vt/parameters/default_utf8
if [ -x /usr/bin/xstart ]; then
    chk "fontconfig: Unifont installed"           sh -c 'fc-list | grep -q "/unifont.otf"'
    chk "Unifont has CJK and emoji (one font)"    sh -c 'fc-list ":charset=4f60 1face" file | grep -q "/unifont.otf"'
    if [ -e /etc/fonts/conf.d/66-v86-colour-emoji.conf ]; then
        chk "colour mode: ❤ not from misc-fixed/Unifont" sh -c '! fc-list ":charset=2764" family | grep -qE "^(Fixed|Unifont)"'
    fi
    chk "fbterm's own font config sees Unifont"   sh -c 'FONTCONFIG_FILE=/etc/fonts/fbterm.conf fc-list ":charset=2764" family | grep -q Unifont'
    chk "fontconfig: Unifont Upper installed"     sh -c 'fc-list | grep -q "/unifont_upper.otf"'
    chk "Twemoji (colour emoji font) installed"   test -f /usr/share/fonts/twemoji/Twemoji.Mozilla.ttf
    chk "Unifont: all symboltest symbols + letters" sh -c 'f=/usr/share/fonts/unifont/v86-symbols.txt; export FONTCONFIG_FILE=/etc/fonts/fbterm.conf
        fc-list ":charset=$(sed -n 1p $f)" file | grep -q /unifont.otf && fc-list ":charset=$(sed -n 2p $f)" file | grep -q /unifont_upper.otf'
    chk "Unifont has all of Twemoji's code points" sh -c 'FONTCONFIG_FILE=/etc/fonts/fbterm.conf fc-list ":charset=$(cat /usr/share/fonts/twemoji/v86-ranges.txt)" file | grep -q /unifont.otf'
    chk "xterm text font is misc-fixed 6x13"      sh -c 'fc-match "Fixed:foundry=Misc:pixelsize=13:width=87:weight=medium" file | grep -q "/6x13.pcf.gz"'
    chk "xterm resources: UTF-8 + fonts"          sh -c 'grep -q "^XTerm\*utf8: *1" ~/.Xdefaults && grep -q "^XTerm\*faceName:" ~/.Xdefaults'
    chk "xterm text colours >= 4.5:1"            readable "$(sed -n 's/^XTerm\*background: *//p' ~/.Xdefaults)" \
        $(sed -n 's/^XTerm\*\(foreground\|color[1-9]\|color1[0-5]\): *//p' ~/.Xdefaults)
    if [ -f ~/.mlterm/main ]; then
        chk "mlterm text colours >= 4.5:1"        readable "$(sed -n 's/^bg_color = //p' ~/.mlterm/main)" \
            $(sed -n 's/^fg_color = //p' ~/.mlterm/main) $(sed -n 's/^\([1-9]\|1[0-5]\) = //p' ~/.mlterm/color)
    fi
    if [ -f /etc/foxrc/Adie ]; then
        chk "Adie text colours >= 4.5:1"          readable "$(sed -n 's/^textbackground=//p' /etc/foxrc/Adie)" \
            $(sed -n -e 's/^textforeground=//p' -e '/^\[STYLE\]/,$s/^[^=]*=\(#[0-9a-f]\{6\}\),.*/\1/p' /etc/foxrc/Adie | sort -u)
        chk "Adie line numbers >= 4.5:1"          readable "$(sed -n 's/^textnumberbackground=//p' /etc/foxrc/Adie)" \
            "$(sed -n 's/^textnumberforeground=//p' /etc/foxrc/Adie)"
        # styles with a background of their own (#if 0 blocks): text on that background
        for s in $(sed -n '/^\[STYLE\]/,$s/^\([^=]*\)=\(#[0-9a-f]\{6\}\),\(#[0-9a-f]\{6\}\),.*/\1:\3:\2/p' /etc/foxrc/Adie); do
            n=${s%%:*} c=${s##*:} b=${s#*:}; b=${b%:*}
            chk "Adie $n text on its background >= 4.5:1" readable "$b" "$c"
        done
        chk "Adie syntax file: C and shell rules" sh -c 'grep -q "^language \"C\"$" /usr/share/adie/Adie.stx && grep -q "rule \"Builtin\"" /usr/share/adie/Adie.stx'
    fi
    # every sample must be in at least one installed font (U+ code, what it is)
    while read -r cp what; do
        chk "glyph U+$cp $what"                   sh -c "fc-list ':charset=$cp' file | grep -q ."
    done <<'CP'
00E9 Latin é
0428 Cyrillic Ш
03A9 Greek Ω
05E9 Hebrew ש
0645 Arabic م
0928 Devanagari न
0E2A Thai ส
4F60 CJK 你
3053 Hiragana こ
C548 Hangul 안
2554 box drawing ╔
2211 math ∑
20AC euro €
2764 heart ❤
1F600 emoji 😀
1F680 emoji 🚀
1F3FD skin tone 🏽
1FAE0 Emoji 14 🫠
1FACE Emoji 15 🫎
1D400 math bold 𝐀
CP
    if [ "$(emojistyle)" = colour ]; then
        echo "--- emojistyle colour"
        chk "fontconfig 'emoji' family is Twemoji"    sh -c 'fc-match emoji family | grep -q Twemoji'
        chk "emoji 😀 comes from the colour font"     sh -c 'fc-list ":charset=1f600:color=true" family | grep -q Twemoji'
    else
        # mono must never draw in colour: no colour font left for any program to fall back to
        echo "--- emojistyle mono"
        chk "mono: no colour font visible"            sh -c '! fc-list :color=true | grep -q .'
        chk "mono: Unifont draws every Twemoji emoji" sh -c 'fc-list ":charset=$(cat /usr/share/fonts/twemoji/v86-ranges.txt)" file | grep -q /unifont.otf'
        chk "mono: xterm has no Twemoji fallback"     sh -c '! grep "^XTerm\*faceName:" ~/.Xdefaults | grep -q Twemoji'
        [ -f ~/.mlterm/aafont ] && chk "mono: mlterm has no Twemoji ranges" sh -c '! grep -q Twemoji ~/.mlterm/aafont'
    fi
    if [ -f ~/.mlterm/main ]; then
        chk "mlterm + its config present"         sh -c 'command -v mlterm && grep -q "^use_ot_layout = true" ~/.mlterm/main && [ -f /usr/lib/mlterm/libtype_cairo.so ]'
        # fontconfig matches the program name: a copy of fc-match named mlterm sees mlterm's rules
        chk "mlterm falls back to Unifont, not bitmaps" sh -c 'd=$(mktemp -d) && cp "$(command -v fc-match)" "$d/mlterm" &&
            r=$("$d/mlterm" -s "Twemoji Mozilla:charset=2605" family | grep -v Twemoji | head -n 1); rm -rf "$d"; [ "$r" = Unifont ]'
    else
        echo "      (no mlterm: built with --no-mlterm)"
    fi
    chk "fbterm + its config present"             sh -c 'command -v fbterm && grep -q Unifont ~/.fbtermrc && grep -q "^font-width=8" ~/.fbtermrc'
fi
echo "=== $pass passed, $fail failed"
[ -x /usr/bin/xstart ] && echo "Now look at it: emojitest in xterm/mlterm and on tty2 (fbterm); symboltest pages through every symbol block."
exit "$fail"
EOF

    putx "$OVL/usr/bin/devcheck" <<'EOF'
#!/bin/sh
# devcheck: compile + run a stdio + SDL3 program with the on-target gcc
dmesg | grep -i 'initramfs unpacking failed' && echo "!! the rootfs did not fit in RAM: raise v86 memory_size"
[ -f /usr/include/SDL3/SDL.h ] || { echo "this image has no SDL3 (--no-SDL3); use selftest for gcc"; exit 0; }
for f in /usr/include/stdio.h /usr/include/SDL3/SDL.h /usr/lib/libSDL3.so; do
    [ -e "$f" ] && echo "ok       $f" || echo "MISSING  $f"
done
cat > /tmp/devcheck.c <<'C'
#include <stdio.h>
#include <SDL3/SDL.h>
int main(void) {
    int v = SDL_GetVersion();
    printf("compiled and ran /tmp/devcheck.c: gcc OK, SDL %d.%d.%d linked\n",
           SDL_VERSIONNUM_MAJOR(v), SDL_VERSIONNUM_MINOR(v), SDL_VERSIONNUM_MICRO(v));
    return 0;
}
C
echo "compiling /tmp/devcheck.c (stdio.h + SDL3/SDL.h) ..."
# shellcheck disable=SC2046  # pkg-config output must split into separate flags
gcc -o /tmp/devcheck /tmp/devcheck.c $(pkg-config --cflags --libs sdl3) && /tmp/devcheck
EOF

    if ((RELEASE)); then                     # release images ship without test programs
        rm -f "$OVL/usr/bin/selftest" "$OVL/usr/bin/devcheck" "$OVL/usr/bin/soundtest" "$OVL/usr/bin/unitest"
    fi
    ((GCC)) || rm -f "$OVL/usr/bin/devcheck"  # devcheck is a compiler test
    ((GUI)) && write_gui_overlay
    return 0
}

# xmode: change resolution/depth while running, like DSL (its Xvesa sets the VESA mode
# through the BIOS when X starts). vesafb's boot mode is fixed, so xmode switches X to
# Xorg's vesa driver, which calls the VGA BIOS (int10/x86emu) and restarts X. v86's
# display follows the new mode on its own.
write_xmode() {
    cp "$OVL/etc/X11/xorg.conf" "$OVL/etc/X11/xorg.conf.boot"
    putx "$OVL/usr/bin/xmode" <<'XMODE'
#!/bin/sh
# xmode WxHxBPP   restart X in that VESA mode, e.g. xmode 1024x768x24 (BPP 16, 24 or 32;
#                 24 runs Xvesa with packed pixels, 16/32 run Xorg's vesa driver)
# xmode boot      back to the boot framebuffer mode (vesafb + fbdev driver)
# xmode           show the current setting
CONF=${XMODE_CONF:-/etc/X11/xorg.conf}
XV=${CONF%/*}/xvesa.args       # present = xstart runs Xvesa with these arguments
usage() { sed -n '2,5s/^# //p' "$0"; exit 1; }
case "$1" in
    "")
        # what is set, and what is running: WxHxBPP + which X server drives it
        if [ -f "$XV" ]; then
            case "$(cat "$XV")" in
                *0x115*) set_mode="800x600x24" ;;  *0x118*) set_mode="1024x768x24" ;;  *) set_mode="?" ;;
            esac
            if pidof Xvesa >/dev/null; then echo "$set_mode (Xvesa, packed 24 bpp: 3 bytes per pixel)"
            else echo "$set_mode (Xvesa) is set; X isn't running at the moment"; fi
        elif grep -q '^# xmode' "$CONF"; then
            set_mode=$(sed -n 's/^# xmode \([0-9x]*\).*/\1/p' "$CONF" | head -n1)
            if pidof Xorg >/dev/null; then echo "$set_mode (Xorg vesa driver)"
            else echo "$set_mode (Xorg vesa driver) is set; X isn't running at the moment"; fi
        else
            echo "$(cat /sys/class/graphics/fb0/virtual_size 2>/dev/null | tr , x)x$(cat /sys/class/graphics/fb0/bits_per_pixel 2>/dev/null) (boot framebuffer, Xorg fbdev driver)"
        fi
        [ -s /var/log/Xorg.xmode-failed.log ] && \
            echo "note: an earlier xmode setting failed and X fell back to the boot mode: xlog shows why"
        exit 0 ;;
    boot)
        rm -f "$XV"; cp "$CONF.boot" "$CONF" ;;
    *x*x*)
        w=${1%%x*}; r=${1#*x}; h=${r%%x*}; bpp=${r#*x}
        case "$w$h$bpp" in ""|*[!0-9]*) usage ;; esac
        case "$bpp" in
            16) depth=16 ;;
            32) depth=24 ;;
            24) # packed 24 bpp: Xvesa with v86's 24 bpp VBE mode (Xorg can't drive it)
                case "${w}x${h}" in
                    800x600) m=0x115 ;;  1024x768) m=0x118 ;;
                    *) echo "xmode: 24 bpp (Xvesa) is available at 800x600 and 1024x768"; exit 1 ;;
                esac
                [ -n "$XMODE_CONF" ] || [ -x /usr/bin/Xvesa ] || { echo "xmode: Xvesa isn't in this image"; exit 1; }
                echo "-mode $m -shadow -s 0 -mouse /dev/input/mice,5 -nolisten tcp" > "$XV" ;;
            *)  echo "xmode: BPP must be 16, 24 or 32"; exit 1 ;;
        esac
        [ "$bpp" = 24 ] || { rm -f "$XV"; cat > "$CONF"; } <<EOF
# xmode ${w}x${h}x${bpp}: Xorg vesa driver (sets the mode through the VGA BIOS)
Section "Monitor"
    Identifier  "mon"
    HorizSync   28-80
    VertRefresh 48-75
EndSection
Section "Device"
    Identifier "gfx"
    Driver     "vesa"
    Option     "ShadowFB" "true"
EndSection
Section "Screen"
    Identifier   "screen"
    Device       "gfx"
    Monitor      "mon"
    DefaultDepth $depth
    DefaultFbBpp $bpp
    SubSection "Display"
        Depth   $depth
        FbBpp   $bpp
        Modes   "${w}x${h}"
        Virtual $w $h
    EndSubSection
EndSection
Section "ServerLayout"
    Identifier "layout"
    Screen     "screen"
    Option     "BlankTime"   "0"
    Option     "StandbyTime" "0"
    Option     "SuspendTime" "0"
    Option     "OffTime"     "0"
EndSection
EOF
        ;;
    *) usage ;;
esac
[ -n "$XMODE_CONF" ] && exit 0          # writing a config elsewhere (image build): no restart
if pidof Xorg Xvesa >/dev/null; then
    echo "xmode: restarting X (if the mode doesn't work, X comes back in the boot mode)"
    (sleep 1; killall Xorg Xvesa) >/dev/null 2>&1 &
elif ps | grep -q "[/]usr/bin/xstart"; then
    echo "xmode: X isn't running right now; xstart on tty1 starts it with this mode in a few seconds"
else
    echo "xmode: used the next time X starts"
fi
XMODE
    # 24 bpp images start Xvesa at the requested mode (the boot framebuffer is 32 bpp);
    # if that fails, xstart falls back to Xorg on the boot framebuffer.
    if ((BPP != BOOT_BPP)); then
        XMODE_CONF="$OVL/etc/X11/xorg.conf" sh "$OVL/usr/bin/xmode" "${W}x${H}x${BPP}" >/dev/null
    fi
}

write_gui_overlay() {
    # ── Xorg config for the chosen driver ────────────────────────────────────
    local depth
    if [[ "$MODE" == qemu ]]; then
        put "$OVL/etc/X11/xorg.conf" <<'EOF'
# bochs-drm via KMS: 32 bpp framebuffer only, so X runs at depth 24
Section "Device"
    Identifier "gfx"
    Driver     "modesetting"
EndSection
Section "Screen"
    Identifier   "screen"
    Device       "gfx"
    DefaultDepth 24
    SubSection "Display"
        Depth 24
        Modes "1024x768"
    EndSubSection
EndSection
EOF
    else
        depth=$(( BOOT_BPP == 16 ? 16 : 24 ))    # a 32 bpp framebuffer is X depth 24
        put "$OVL/etc/X11/xorg.conf" <<EOF
# vesafb, ${W}x${H} at ${BOOT_BPP} bpp (VBE mode set by the kernel at boot; X depth ${depth}).
# The fbdev driver uses the framebuffer's mode as is.
Section "Device"
    Identifier "gfx"
    Driver     "fbdev"
    Option     "fbdev" "/dev/fb0"
    Option     "ShadowFB" "true"
EndSection
Section "Screen"
    Identifier   "screen"
    Device       "gfx"
    DefaultDepth ${depth}
    SubSection "Display"
        Depth ${depth}
    EndSubSection
EndSection
EOF
    fi
    # no screen saver / power-saving blank (the screensaver command turns it back on)
    cat >> "$OVL/etc/X11/xorg.conf" <<'EOF'
Section "ServerLayout"
    Identifier "layout"
    Screen     "screen"
    Option     "BlankTime"   "0"
    Option     "StandbyTime" "0"
    Option     "SuspendTime" "0"
    Option     "OffTime"     "0"
EndSection
EOF
    [[ "$MODE" == vesa ]] && write_xmode

    # xstart (tty1): wait for the KMS device when using modesetting, run X, log it
    putx "$OVL/usr/bin/xstart" <<'EOF'
#!/bin/sh
export HOME=/root DISPLAY=:0 PATH="$PATH:/usr/local/bin" LANG=${LANG:-C.UTF-8}
LOG=/var/log/xstart.log
cd "$HOME" || exit 1
dev=/dev/fb0; grep -q modesetting /etc/X11/xorg.conf && dev=/dev/dri/card0
for _ in $(seq 1 30); do [ -e "$dev" ] && break; sleep 0.5; done
[ -e "$dev" ] || echo "xstart: $dev never appeared (see dmesg)" >>"$LOG"
pidof Xorg Xvesa >/dev/null || rm -f /tmp/.X0-lock /tmp/.X11-unix/X0
XV=/etc/X11/xvesa.args
t0=$(date +%s)
if [ -f "$XV" ] && [ -x /usr/bin/Xvesa ]; then
    echo "xstart: $(date) starting Xvesa $(cat "$XV")" >>"$LOG"
    # No "vt1": Xvesa reads the keyboard from its console, and tty1 is the controlling
    # terminal of this session; xinit runs the server in its own process group, so the
    # kernel refuses those reads (EIO): mouse works, keyboard dead. Without a vt option
    # Xvesa takes a free console (VT_OPENQRY), like DSL and Tiny Core do.
    # shellcheck disable=SC2046  # xvesa.args holds several arguments
    xinit "$HOME/.xinitrc" -- /usr/bin/Xvesa :0 $(cat "$XV") >>"$LOG" 2>&1
else
    echo "xstart: $(date) starting X" >>"$LOG"
    startx -- :0 vt1 -nolisten tcp >>"$LOG" 2>&1
fi
rc=$?
echo "xstart: X exited ($rc), see /var/log/Xorg.0.log" >>"$LOG"
# say so on tty1 too, or a failing X just looks like a frozen screen
[ "$rc" = 0 ] || echo "xstart: X failed to start, retrying. Log in on COM1 or tty2 (Alt+F2) and run 'xlog'."
# a mode picked with 'xmode' that X can't use: go back to the boot mode
if [ -f "$XV" ] && [ "$rc" != 0 ] && [ $(( $(date +%s) - t0 )) -lt 30 ]; then
    tail -n 40 "$LOG" > /var/log/Xorg.xmode-failed.log
    rm -f "$XV"; cp /etc/X11/xorg.conf.boot /etc/X11/xorg.conf
    echo "xstart: Xvesa failed (log: /var/log/Xorg.xmode-failed.log), back to the boot mode" | tee -a "$LOG"
elif [ -f /etc/X11/xorg.conf.boot ] && grep -q '^# xmode' /etc/X11/xorg.conf \
        && grep -q 'Fatal server error' /var/log/Xorg.0.log 2>/dev/null; then
    cp /var/log/Xorg.0.log /var/log/Xorg.xmode-failed.log
    cp /etc/X11/xorg.conf.boot /etc/X11/xorg.conf
    echo "xstart: the xmode setting failed (log: /var/log/Xorg.xmode-failed.log), back to the boot mode" | tee -a "$LOG"
fi
sleep 3
EOF
    putx "$OVL/usr/bin/screensaver" <<'EOF'
#!/bin/sh
# screensaver          show whether the screen saver / blanking is on
# screensaver on|off   switch it (remembered across X restarts, e.g. by xmode)
# Off by default: the screen never goes black. Works with Xorg and Xvesa.
STATE=/root/.screensaver-on
apply() {
    if [ -f "$STATE" ]; then
        xset s 600 600 s blank 2>/dev/null; xset +dpms 2>/dev/null
    else
        xset s off s noblank 2>/dev/null; xset -dpms 2>/dev/null   # Xvesa has no DPMS: fine
    fi
    return 0
}
case "$1" in
    on)    touch "$STATE"; apply; echo "screen saver on: blanks after 10 minutes idle" ;;
    off)   rm -f "$STATE"; apply; echo "screen saver off: the screen stays on" ;;
    apply) apply ;;
    "")    [ -f "$STATE" ] && echo "screen saver on (10 minutes)" || echo "screen saver off" ;;
    *)     sed -n '2,4s/^# //p' "$0"; exit 1 ;;
esac
EOF
    putx "$OVL/root/.xinitrc" <<'EOF'
#!/bin/sh
xsetroot -solid '#232831'
screensaver apply                     # off unless "screensaver on" was chosen
xfe-fit                               # keep XFE's remembered size within this screen
xterm -geometry 80x24+30+30 &
while :; do fluxbox -log /var/log/fluxbox.log; sleep 1; done
EOF

    # ── xterm: dark palette (each color >= 4.5:1 contrast on the background) ─
    put "$OVL/root/.Xdefaults" <<'EOF'
XTerm*termName:    xterm-256color
XTerm*locale:      false
XTerm*utf8:        1
XTerm*utf8Title:   true
! Text: xterm's classic 6x13 "fixed" font (misc-fixed, ~4,100 characters), drawn through
! Xft so other fonts fill in what it lacks: emoji in colour from Twemoji (listed second, so
! it is tried first), CJK and the rest from Unifont. Colour mode (emojistyle) also hides
! Twemoji's characters from misc-fixed and Unifont, so ❤ ☀ ✈ come out in colour too.
! emojiWidth: an emoji followed by VS16 (❤️) takes two cells; useClipping off lets the
! colour glyph use both. "emojistyle mono" = everything from Unifont, monochrome.
XTerm*faceName:    Fixed:foundry=Misc:pixelsize=13:width=87:weight=medium,Twemoji Mozilla:pixelsize=13
XTerm*emojiWidth:  true
XTerm*useClipping: false
! near-black, not Nord's #2e3440: Twemoji draws ♠ ✔ ✖ ⬛ 🖤 🕷... in #292f33, which vanishes
! on slate (82 of its 1361 glyphs hard to see; here 14, and text contrast 7.5:1)
XTerm*background:  #0a0b0e
XTerm*foreground:  #e5e9f0
XTerm*cursorColor: #e5e9f0
XTerm*color0:  #3b4252
XTerm*color1:  #e5848e
XTerm*color2:  #a3be8c
XTerm*color3:  #ebcb8b
XTerm*color4:  #8fb0d4
XTerm*color5:  #c39dbf
XTerm*color6:  #88c0d0
XTerm*color7:  #e5e9f0
XTerm*color8:  #95a2ba
XTerm*color9:  #ee939c
XTerm*color10: #b4cd9d
XTerm*color11: #f2d79f
XTerm*color12: #a2c0e0
XTerm*color13: #d0a9cc
XTerm*color14: #9bcbcb
XTerm*color15: #eceff4
EOF

    # fontconfig rejects bitmap fonts (70-no-bitmaps-except-emoji.conf); let xterm's
    # misc-fixed through, so Xft can draw the same 6x13 glyphs as the core "fixed" font.
    if ((GUI)); then
        put "$OVL/etc/fonts/conf.d/71-v86-misc-fixed.conf" <<'EOF'
<?xml version="1.0"?>
<!DOCTYPE fontconfig SYSTEM "fonts.dtd">
<!-- v86: accept xterm's 6x13 font (the ISO 10646 files only, not the 8-bit code pages).
     fontconfig 2.17 names it family "Fixed", foundry "Misc" -->
<fontconfig>
  <selectfont>
    <acceptfont>
      <pattern><patelt name="family"><string>Fixed</string></patelt><patelt name="foundry"><string>Misc</string></patelt></pattern>
    </acceptfont>
    <rejectfont>
      <glob>/usr/share/fonts/X11/misc/*-ISO8859-*</glob>
      <glob>/usr/share/fonts/X11/misc/*-KOI8-*</glob>
    </rejectfont>
  </selectfont>
</fontconfig>
EOF
        # mlterm's fallback (cairo): for a character its font lacks it takes the first family
        # in its search pattern that has it, and otherwise appends every installed family in
        # fontconfig's list order. That can pick misc-fixed (xterm's bitmap font), which
        # cairo can't draw: boxes, depending on what was drawn before. Unifont first, for
        # mlterm only (fontconfig matches the program name).
        if ((MLTERM)); then
        put "$OVL/etc/fonts/conf.d/72-v86-mlterm-fallback.conf" <<'EOF'
<?xml version="1.0"?>
<!DOCTYPE fontconfig SYSTEM "fonts.dtd">
<!-- v86, mlterm only: characters its font lacks come from Unifont (every character),
     never from misc-fixed (a bitmap font mlterm's cairo engine can't draw) -->
<fontconfig>
  <match target="pattern">
    <test name="prgname"><string>mlterm</string></test>
    <edit name="family" mode="append" binding="strong">
      <string>Unifont</string>
      <string>Unifont Upper</string>
    </edit>
  </match>
</fontconfig>
EOF
        fi
        # Colour-emoji rule: hide Twemoji's emoji from misc-fixed (xterm's text font) and
        # Unifont, which xterm would otherwise use first for ❤ ☀ ☕... (black and white).
        # Arrows, punctuation, geometric shapes and CJK signs stay text. Scan-time rule:
        # fontconfig builds its cache at the first use after boot (no cache in the image);
        # emojistyle clears it when switching. Ranges: Twemoji 0.7.0's code points.
        local cs="" rr twe
        twe=$(cat <<'RANGES'
    231A-231B 2328 23CF 23E9-23F3 23F8-23FA 2600-2604 260E 2611 2614-2615 2618 261D
    2620 2622-2623 2626 262A 262E-262F 2638-263A 2640 2642 2648-2653 265F-2660 2663
    2665-2666 2668 267B 267E-267F 2692-2697 2699 269B-269C 26A0-26A1 26A7 26AA-26AB
    26B0-26B1 26BD-26BE 26C4-26C5 26C8 26CE-26CF 26D1 26D3-26D4 26E9-26EA 26F0-26F5
    26F7-26FA 26FD 2702 2705 2708-270D 270F 2712 2714 2716 271D 2721 2728 2733-2734
    2744 2747 274C 274E 2753-2755 2757 2763-2764 2795-2797 27A1 27B0 27BF 2B1B-2B1C
    2B50 2B55 1F004 1F0CF 1F170-1F171 1F17E-1F17F 1F18E 1F191-1F19A 1F1E6-1F1FF
    1F201-1F202 1F21A 1F22F 1F232-1F23A 1F250-1F251 1F300-1F321 1F324-1F393
    1F396-1F397 1F399-1F39B 1F39E-1F3F0 1F3F3-1F3F5 1F3F7-1F3FA 1F400-1F4FD
    1F4FF-1F53D 1F549-1F54E 1F550-1F567 1F56F-1F570 1F573-1F57A 1F587 1F58A-1F58D
    1F590 1F595-1F596 1F5A4-1F5A5 1F5A8 1F5B1-1F5B2 1F5BC 1F5C2-1F5C4 1F5D1-1F5D3
    1F5DC-1F5DE 1F5E1 1F5E3 1F5E8 1F5EF 1F5F3 1F5FA-1F64F 1F680-1F6C5 1F6CB-1F6D2
    1F6D5-1F6D7 1F6DD-1F6E5 1F6E9 1F6EB-1F6EC 1F6F0 1F6F3-1F6FC 1F7E0-1F7EB 1F7F0
    1F90C-1F93A 1F93C-1F945 1F947-1F9FF 1FA70-1FA74 1FA78-1FA7C 1FA80-1FA86
    1FA90-1FAAC 1FAB0-1FABA 1FAC0-1FAC5 1FAD0-1FAD9 1FAE0-1FAE7 1FAF0-1FAF6
RANGES
        )
        for rr in $twe; do cs+=$(printf '\n      <range><int>%d</int><int>%d</int></range>' "$((16#${rr%-*}))" "$((16#${rr#*-}))"); done
        mkdir -p "$OVL/etc/fonts/conf.avail" "$OVL/etc/fonts/conf.d"
        {
            printf '<?xml version="1.0"?>\n<!DOCTYPE fontconfig SYSTEM "fonts.dtd">\n'
            printf '<!-- v86: colour emoji first (emojistyle colour); off = emojistyle mono -->\n<fontconfig>\n'
            for rr in 'compare="eq"><string>Fixed' 'compare="contains"><string>Unifont'; do
                printf '  <match target="scan">\n    <test name="family" %s</string></test>\n' "$rr"
                printf '    <edit name="charset" mode="assign"><minus><name>charset</name><charset>%s\n    </charset></minus></edit>\n  </match>\n' "$cs"
            done
            printf '</fontconfig>\n'
        } | put "$OVL/etc/fonts/conf.avail/66-v86-colour-emoji.conf"
        ln -sf ../conf.avail/66-v86-colour-emoji.conf "$OVL/etc/fonts/conf.d/66-v86-colour-emoji.conf"
        # Mono rule (emojistyle mono links it into conf.d): reject every colour font, so no
        # program can fall back to Twemoji. Unifont has a glyph for each of Twemoji's code
        # points (unitest checks), so nothing turns into a box either.
        put "$OVL/etc/fonts/conf.avail/65-v86-mono-emoji.conf" <<'EOF'
<?xml version="1.0"?>
<!DOCTYPE fontconfig SYSTEM "fonts.dtd">
<!-- v86: monochrome emoji (emojistyle mono): no colour fonts at all -->
<fontconfig>
  <selectfont>
    <rejectfont>
      <pattern><patelt name="color"><bool>true</bool></patelt></pattern>
    </rejectfont>
  </selectfont>
</fontconfig>
EOF
        ((RELEASE)) || printf '%s\n' $twe | put "$OVL/usr/share/fonts/twemoji/v86-ranges.txt"   # for unitest
        # fbterm: monochrome only, so it keeps Unifont's emoji: own config without the rule
        put "$OVL/etc/fonts/fbterm.conf" <<'EOF'
<?xml version="1.0"?>
<!DOCTYPE fontconfig SYSTEM "fonts.dtd">
<!-- fbterm (tty2-6): Unifont only, no colour-emoji rule, own cache -->
<fontconfig>
  <dir>/usr/share/fonts/unifont</dir>
  <cachedir>/var/cache/fontconfig-fbterm</cachedir>
</fontconfig>
EOF

        # emojitest (test program: not in --release): colour emoji, wide characters and other scripts
        if ((!RELEASE)); then
        putx "$OVL/usr/bin/emojitest" <<'EOF'
#!/bin/sh
# emojitest: run in xterm (colour emoji) or in fbterm on tty2-6 (monochrome; fbterm
# shows only U+0000-FFFF, so emoji above that are boxes there).
# "emojistyle mono" makes new X programs draw emoji in monochrome.
# Each emoji should take exactly two cells: the | marks must line up.
cat <<'T'
Faces    |😀|😂|😍|🤔|😎|😭|🥳|😴|
Hands    |👍|👎|👋|🙏|👏|💪|✌️|🤞|
Hearts   |❤️|🧡|💛|💚|💙|💜|🖤|💔|
Animals  |🐶|🐱|🦊|🐼|🐸|🐙|🦄|🐝|
Food     |🍎|🍕|🍔|🍩|🍣|☕|🍺|🎂|
Travel   |🚀|✈️|🚗|🚲|⛵|🏠|🌋|🗽|
Symbols  |⭐|🔥|💧|⚡|🌈|❄️|✅|❌|
Newer    |🫠|🫶|  Emoji 14
Newest   |🪿|🫎|🩷|🪼|  Emoji 15: not in Twemoji 0.7, monochrome from Unifont
Text/emoji ❤ ❤️  ☀ ☀️  ✈ ✈️  (with VS16 = 2 cells; colour mode draws both forms in colour)
Sequences: 👍🏽 (skin tone)  👨‍👩‍👧 (ZWJ family)  🇫🇷 (flag)
  xterm draws sequences as their separate parts: that is expected.

Other scripts (not emoji):
Greek    Καλημέρα κόσμε          Cyrillic  Здравствуй, мир
Hebrew   שלום עולם               Arabic    مرحبا بالعالم
CJK      你好，世界  こんにちは  안녕하세요   (two cells each)
Thai     สวัสดีชาวโลก               Hindi     नमस्ते दुनिया
Box      ┌─┬─┐ ╔═╦═╗ ░▒▓█  Math ∀x∈ℝ: x²≥0  ∑∫√∞≠≈
Arrows   ← ↑ → ↓ ↔ ⇒ ⇔      Currency € £ ¥ ₹ ₿
T
EOF
        fi
        # symboltest (test program): every symbol and emoji block, 16 per row, paged with less.
        # Generated here (bash, any host locale): u8 puts the UTF-8 escapes of code point $1 in U.
        if ((!RELEASE)); then
        local U
        u8() {
            local c=$1
            if ((c < 0x800)); then printf -v U '\\x%02x\\x%02x' $((0xC0 | c >> 6)) $((0x80 | (c & 63)))
            elif ((c < 0x10000)); then printf -v U '\\x%02x\\x%02x\\x%02x' $((0xE0 | c >> 12)) $((0x80 | (c >> 6 & 63))) $((0x80 | (c & 63)))
            else printf -v U '\\x%02x\\x%02x\\x%02x\\x%02x' $((0xF0 | c >> 18)) $((0x80 | (c >> 12 & 63))) $((0x80 | (c >> 6 & 63))) $((0x80 | (c & 63))); fi
        }
        # Code points of these blocks not assigned in Unicode 18 (Unifont 18's version): shown
        # as "·", so any box on screen is a real missing glyph. Unifont draws unassigned code
        # points as dashed boxes with the number inside, which looks like a font problem.
        local -A unas=()
        local c
        for rr in $(cat <<'UNASSIGNED'
    2072-2073 20C5-20CF 218C-218F 242A-243F 244B-245F 2B74-2B75 321F 1F02C-1F02F 1F094-1F09F
    1F0AF-1F0B0 1F0C0 1F0D0 1F0F6-1F0FF 1F1AF-1F1E5 1F203-1F20F 1F23C-1F23F 1F249-1F24F 1F252-1F25F
    1F266-1F2FF 1F6DA-1F6DB 1F6ED-1F6EF 1F6FD-1F6FF 1F7DC-1F7DF 1F7EC-1F7EF 1F80C-1F80F 1F848-1F84F
    1F85A-1F85F 1F888-1F88F 1F8AE-1F8AF 1F8BC-1F8BF 1F8C2-1F8CF 1F8D9-1F8FF 1FA58-1FA5F 1FA6E-1FA6F
    1FA7D-1FA7F 1FAC7 1FAC9-1FACB 1FADE 1FAEC-1FAEE 1FAFB-1FAFF 1FB93 1FBFB-1FBFF FE1A-FE1F FE53
    FE67 FE6C-FE6F FFBF-FFC1 FFC8-FFC9 FFD0-FFD1 FFD8-FFD9 FFDD-FFDF FFE7 FFEF 1D357-1D35F
    1D379-1D37F 1D455 1D49D 1D4A0-1D4A1 1D4A3-1D4A4 1D4A7-1D4A8 1D4AD 1D4BA 1D4BC 1D4C4 1D506
    1D50B-1D50C 1D515 1D51D 1D53A 1D53F 1D545 1D547-1D549 1D551 1D6A7 1D7CC-1D7CD 1EE04 1EE20 1EE23
    1EE25-1EE26 1EE28 1EE33 1EE38 1EE3A 1EE3C-1EE41 1EE43-1EE46 1EE48 1EE4A 1EE4C 1EE50 1EE53
    1EE55-1EE56 1EE58 1EE5A 1EE5C 1EE5E 1EE60 1EE63 1EE65-1EE66 1EE6B 1EE73 1EE78 1EE7D 1EE7F 1EE8A
    1EE9C-1EEA0 1EEA4 1EEAA 1EEBC-1EEEF 1EEF2-1EEFF
UNASSIGNED
        ); do for ((c = 16#${rr%-*}; c <= 16#${rr#*-}; c++)); do unas[$c]=1; done; done
        # cov_add: collect the assigned code points shown, as fontconfig charset ranges:
        # cov[0] = in v86's Unifont (plane 0 + U+1F000-1FAFF), cov[1] = in Unifont Upper
        local -a cov=("" "") cs0=(-2 -2) ce0=(-2 -2)
        cov_add() {
            local c=$1 k=1
            ((c < 0x10000 || (c >= 0x1F000 && c <= 0x1FAFF))) && k=0
            if ((c == ce0[k] + 1)); then ce0[k]=$c; return; fi
            cov_flush "$k"; cs0[k]=$c; ce0[k]=$c
        }
        cov_flush() {
            local k=$1
            ((cs0[k] < 0)) && return
            if ((cs0[k] == ce0[k])); then cov[k]+=$(printf ' %x' "${cs0[k]}")
            else cov[k]+=$(printf ' %x-%x' "${cs0[k]}" "${ce0[k]}"); fi
            cs0[k]=-2; ce0[k]=-2
        }
        # sym_block FIRST LAST NAME: one block, rows aligned to 16
        sym_block() {
            local a=$((16#$1)) z=$((16#$2)) name=$3 r c line
            printf '\n== U+%04X-%04X  %s ==\n' "$a" "$z" "$name"
            for ((r = a & ~15; r <= z; r += 16)); do
                line=""
                for ((c = r; c < r + 16; c++)); do
                    if ((c < a || c > z)); then line+="  "
                    elif [[ ${unas[$c]:-} ]]; then line+="· "
                    else u8 "$c"; line+="$U "; cov_add "$c"; fi
                done
                printf 'U+%05X  %b\n' "$r" "$line"
            done
        }
        {
            cat <<'EOF'
#!/bin/sh
# symboltest: every symbol and emoji block (16 per row), paged with less (space = next
# page, b = back, q = quit; /U+1F6 jumps to a row). For checking fonts page by page:
# after "emojistyle mono" nothing here may be in colour, in xterm or mlterm.
# · = not assigned in Unicode 18. Any box (xterm) or hex box (mlterm) is a missing glyph.
# Then a row of letters from each writing system (Arabic and Hebrew unshaped and left to
# right in xterm: it has no shaping or bidi), the emoji forms and sequences.
{ cat <<'SYMBOLS'
EOF
            echo "All symbol and emoji blocks, 16 per row; · = unassigned. Any box = a missing glyph."
            echo "After 'emojistyle mono' nothing may be in colour. Writing systems follow the symbols."
            while read -r a z name; do sym_block "$a" "$z" "$name"; done <<'BLOCKS'
00A1 00BF Latin-1 Supplement Symbols
02B0 02FF Spacing Modifier Letters
2010 2027 General Punctuation
2030 205E General Punctuation
2070 209F Superscripts and Subscripts
20A0 20CF Currency Symbols
2100 214F Letterlike Symbols
2150 218F Number Forms
2190 21FF Arrows
2200 22FF Mathematical Operators
2300 23FF Miscellaneous Technical
2400 243F Control Pictures
2440 245F Optical Character Recognition
2460 24FF Enclosed Alphanumerics
2500 257F Box Drawing
2580 259F Block Elements
25A0 25FF Geometric Shapes
2600 26FF Miscellaneous Symbols
2700 27BF Dingbats
27C0 27EF Miscellaneous Mathematical Symbols-A
27F0 27FF Supplemental Arrows-A
2800 28FF Braille Patterns
2900 297F Supplemental Arrows-B
2980 29FF Miscellaneous Mathematical Symbols-B
2A00 2AFF Supplemental Mathematical Operators
2B00 2BFF Miscellaneous Symbols and Arrows
3000 3029 CJK Symbols and Punctuation
3030 303F CJK Symbols and Punctuation
3200 32FF Enclosed CJK Letters and Months
3300 33FF CJK Compatibility
4DC0 4DFF Yijing Hexagram Symbols
FE10 FE1F Vertical Forms
FE30 FE4F CJK Compatibility Forms
FE50 FE6F Small Form Variants
FF01 FF60 Fullwidth ASCII
FF61 FFEF Halfwidth and Fullwidth Forms
FFFC FFFD Specials
1D100 1D1FF Musical Symbols
1D300 1D35F Tai Xuan Jing Symbols
1D360 1D37F Counting Rod Numerals
1D400 1D7FF Mathematical Alphanumeric Symbols
1EE00 1EEFF Arabic Mathematical Alphabetic Symbols
1F000 1F02F Mahjong Tiles
1F030 1F09F Domino Tiles
1F0A0 1F0FF Playing Cards
1F100 1F1FF Enclosed Alphanumeric Supplement
1F200 1F2FF Enclosed Ideographic Supplement
1F300 1F5FF Miscellaneous Symbols and Pictographs
1F600 1F64F Emoticons
1F650 1F67F Ornamental Dingbats
1F680 1F6FF Transport and Map Symbols
1F700 1F77F Alchemical Symbols
1F780 1F7FF Geometric Shapes Extended
1F800 1F8FF Supplemental Arrows-C
1F900 1F9FF Supplemental Symbols and Pictographs
1FA00 1FA6F Chess Symbols
1FA70 1FAFF Symbols and Pictographs Extended-A
1FB00 1FBFF Symbols for Legacy Computing
BLOCKS
            # text characters that have an emoji form, each followed by VS16 (U+FE0F)
            # a row of letters from each writing system (all in Unifont / Unifont Upper)
            printf '\n== Writing systems: 24 letters each ==\n'
            local name line c2
            while read -r rr name; do
                line=""
                for c in ${rr//,/ }; do
                    for ((c2 = 16#${c%-*}; c2 <= 16#${c#*-}; c2++)); do u8 "$c2"; line+="$U "; cov_add "$c2"; done
                done
                printf '%-26s %b\n' "$name" "$line"
            done <<'SCRIPTS'
100-117 Latin Extended-A
250-267 IPA Extensions
370-377,37A-37D,37F,384-386,388-38A,38C,38E-391 Greek
400-417 Cyrillic
531-548 Armenian
5D0-5E7 Hebrew
627-63E Arabic
710,712-728 Syriac
780-797 Thaana
7C0-7D7 NKo
905-91C Devanagari
985-98C,98F-990,993-9A0 Bengali
A05-A0A,A0F-A10,A13-A22 Gurmukhi
A85-A8D,A8F-A91,A93-A9E Gujarati
B05-B0C,B0F-B10,B13-B20 Oriya
B85-B8A,B8E-B90,B92-B95,B99-B9A,B9C,B9E-B9F,BA3-BA4,BA8-BAA,BAE Tamil
C05-C0C,C0E-C10,C12-C1E Telugu
C85-C8C,C8E-C90,C92-C9E Kannada
D05-D0C,D0E-D10,D12-D1E Malayalam
D85-D96,D9A-D9F Sinhala
E01-E18 Thai
E81-E82,E84,E86-E8A,E8C-E9B Lao
F40-F47,F49-F58 Tibetan
1000-1017 Myanmar
10A0-10B7 Georgian
1100-1117 Hangul Jamo
1200-1217 Ethiopic
13A0-13B7 Cherokee
1401-1418 Canadian Syllabics
1681-1698 Ogham
16A0-16B7 Runic
1780-1797 Khmer
1820-1837 Mongolian
1E00-1E17 Latin Extended Additional
1F00-1F15,1F18-1F19 Greek Extended
3041-3058 Hiragana
30A1-30B8 Katakana
3105-311C Bopomofo
4E00-4E17 CJK Unified Ideographs
A000-A017 Yi
AC00-AC17 Hangul Syllables
A6A0-A6B7 Bamum
10000-1000B,1000D-10018 Linear B
10300-10317 Old Italic
10330-10347 Gothic
10380-10397 Ugaritic
10900-10917 Phoenician
1D400-1D417 Math Alphanumerics
1D100-1D117 Musical Symbols
SCRIPTS
            # text characters that have an emoji form, each + VS16 (U+FE0F); label = row's first
            printf '\n== Emoji presentation: text characters + VS16 (U+FE0F), 16 per row ==\n'
            local cps=() c n line
            for rr in 00A9 00AE 203C 2049 2122 2139 2194-2199 21A9-21AA 24C2 25AA-25AB 25B6 25C0 \
                      25FB-25FE 2934-2935 2B05-2B07 3030 303D 3297 3299 $twe; do
                for ((c = 16#${rr%-*}; c <= 16#${rr#*-} && c < 0x1F000; c++)); do cps+=("$c"); done
            done
            for ((n = 0; n < ${#cps[@]}; n += 16)); do
                line=""
                for c in "${cps[@]:n:16}"; do u8 "$c"; line+="$U\xef\xb8\x8f "; done
                printf 'U+%05X  %b\n' "${cps[n]}" "$line"
            done
            cat <<'EOF'

== Sequences (mlterm combines them; xterm shows the parts) ==
Keycaps    #️⃣ *️⃣ 0️⃣ 1️⃣ 2️⃣ 3️⃣ 4️⃣ 5️⃣ 6️⃣ 7️⃣ 8️⃣ 9️⃣
Flags      🇫🇷 🇩🇪 🇯🇵 🇺🇸 🇬🇧 🇧🇷 🇮🇳 🇺🇦 🏳️‍🌈 🏴‍☠️
Skin tones 👍🏻 👍🏼 👍🏽 👍🏾 👍🏿 👋🏽 🙏🏾
ZWJ        👨‍👩‍👧 👩‍💻 🧑‍🚀 ❤️‍🔥 🐻‍❄️
-- end --
SYMBOLS
} | if [ -t 1 ]; then less; else cat; fi
EOF
            # what unitest checks Unifont against: line 1 Unifont, line 2 Unifont Upper
            cov_flush 0; cov_flush 1
            printf '%s\n' "${cov[0]# }" "${cov[1]# }" | put "$OVL/usr/share/fonts/unifont/v86-symbols.txt"
        } | putx "$OVL/usr/bin/symboltest"
        unset -f u8 sym_block cov_add cov_flush
        fi

        # mlterm: Liberation Mono (Courier New metrics, like the browser's xterm.js),
        # Twemoji for emoji, Unifont for scripts Liberation lacks. cairo falls back to
        # other fonts by itself, but would pick the misc-fixed bitmap font (which it
        # can't draw) for some ranges: those are pinned to Unifont. Same palette as xterm.
        if ((MLTERM)); then
        mkdir -p "$OVL/root/.mlterm"
        put "$OVL/root/.mlterm/main" <<'EOF'
type_engine = cairo
termtype = xterm-256color
use_anti_alias = true
use_ot_layout = true
fontsize = 13
fg_color = #e5e9f0
bg_color = #0a0b0e
cursor_fg_color = #0a0b0e
cursor_bg_color = #e5e9f0
scrollbar_mode = none
geometry = 80x24
EOF
        put "$OVL/root/.mlterm/aafont" <<'EOF'
DEFAULT = Liberation Mono
U+0600-06FF = Unifont
U+0750-077F = Unifont
U+FB50-FDFF = Unifont
U+FE70-FEFF = Unifont
U+20A0-20CF = Unifont
U+2100-214F = Unifont
U+2190-21FF = Unifont
U+2200-22FF = Unifont
U+1F000-1FAFF = Twemoji Mozilla
U+2600-27BF = Twemoji Mozilla
U+2300-23FF = Twemoji Mozilla
U+2B1B-2B55 = Twemoji Mozilla
EOF
        put "$OVL/root/.mlterm/color" <<'EOF'
0 = #3b4252
1 = #e5848e
2 = #a3be8c
3 = #ebcb8b
4 = #8fb0d4
5 = #c39dbf
6 = #88c0d0
7 = #e5e9f0
8 = #95a2ba
9 = #ee939c
10 = #b4cd9d
11 = #f2d79f
12 = #a2c0e0
13 = #d0a9cc
14 = #9bcbcb
15 = #eceff4
EOF
        fi

        # emojistyle: colour emoji (Twemoji, default) or monochrome emoji (Unifont) in xterm
        putx "$OVL/usr/bin/emojistyle" <<'EOF'
#!/bin/sh
# emojistyle [colour|mono]: how emoji look in X programs (xterm, XFE...) started after it
#   colour  Twemoji colour emoji, also for ❤ ☀ ✈ (the default); emoji newer than
#           Twemoji 0.7 (Emoji 15) in monochrome from Unifont
#   mono    all emoji monochrome from Unifont (built with the emoji blocks added); colour
#           fonts are hidden from every program (65-v86-mono-emoji.conf)
# Also switches mlterm's emoji font. fbterm (tty2-6) is always monochrome and has its own
# font setup (/etc/fonts/fbterm.conf).
f=$HOME/.Xdefaults
ml=$HOME/.mlterm/aafont
rule=/etc/fonts/conf.d/66-v86-colour-emoji.conf
nocolour=/etc/fonts/conf.d/65-v86-mono-emoji.conf
fresh() { rm -rf /var/cache/fontconfig/* "$HOME/.cache/fontconfig"; }   # rescan with the new rule
case "$1" in
    colour|color)  ln -sf ../conf.avail/66-v86-colour-emoji.conf "$rule"; rm -f "$nocolour"
                   ps=$(sed -n 's/^XTerm\*faceName:[^,]*pixelsize=\([0-9]*\).*/\1/p' "$f" | head -n 1)
                   sed -i "/^XTerm\\*faceName:/{/Twemoji Mozilla/!s/\$/,Twemoji Mozilla:pixelsize=${ps:-13}/}" "$f"
                   [ -f "$ml" ] && sed -i '/^U+1F000-1FAFF\|^U+2600-27BF\|^U+2300-23FF\|^U+2B1B-2B55/s/= .*/= Twemoji Mozilla/' "$ml"
                   sed -i '/^XTerm\*faceNameDoublesize:/d' "$f"; fresh
                   echo "emojistyle: colour (programs started from now on)" ;;
    mono)          rm -f "$rule"; ln -sf ../conf.avail/65-v86-mono-emoji.conf "$nocolour"
                   sed -i '/^XTerm\*faceName:/s/,Twemoji Mozilla[^,]*$//' "$f"
                   [ -f "$ml" ] && sed -i '/^U+1F000-1FAFF\|^U+2600-27BF\|^U+2300-23FF\|^U+2B1B-2B55/s/= .*/= Unifont/' "$ml"
                   sed -i '/^XTerm\*faceNameDoublesize:/d' "$f"; fresh
                   echo "emojistyle: mono (programs started from now on)" ;;
    "")            if [ -e "$rule" ]; then echo colour; else echo mono; fi ;;
    *)             echo "usage: emojistyle [colour|mono]"; exit 1 ;;
esac
EOF

    fi

    # ── --min: fast graphics for a slow host ─────────────────────────────────
    # Fluxbox already moves windows as an outline and draws its own text without
    # antialiasing (see its init below). This turns antialiasing off for every Xft /
    # FreeType program too (XFE, xterm with Xft fonts, SDL_ttf...): monochrome glyphs need
    # no alpha blending, the costliest part of text drawing in an emulated CPU.
    if ((MIN)); then
        put "$OVL/etc/fonts/conf.d/99-v86-min.conf" <<'EOF'
<?xml version="1.0"?>
<!DOCTYPE fontconfig SYSTEM "fonts.dtd">
<!-- v86 "min" image: no antialiasing (cheaper text drawing on a slow host) -->
<fontconfig>
  <match target="font">
    <edit name="antialias" mode="assign"><bool>false</bool></edit>
    <edit name="rgba" mode="assign"><const>none</const></edit>
    <edit name="hinting" mode="assign"><bool>true</bool></edit>
    <edit name="hintstyle" mode="assign"><const>hintfull</const></edit>
  </match>
</fontconfig>
EOF
        printf 'Xft.antialias: 0\nXft.rgba: none\nXft.hinting: 1\nXft.hintstyle: hintfull\n' >> "$OVL/root/.Xdefaults"
    fi

    # ── Fluxbox ──────────────────────────────────────────────────────────────
    put "$OVL/root/.fluxbox/menu" <<'EOF'
[begin] (Fluxbox)
  [submenu] (Terminal)
    [exec] (xterm) {xterm}
    [exec] (mlterm) {mlterm}
  [end]
  [exec] (XFE File Manager) {xfe}
  [submenu] (Editor)
    [exec] (nano) {xterm -e nano}
    [exec] (vi) {xterm -e vi}
    [exec] (Adie) {adie}
  [end]
  [submenu] (Network)
    [exec] (Start) {xterm -hold -e netup}
    [exec] (Stop) {xterm -hold -e netdown}
  [end]
  [submenu] (Emoji)
    [exec] (Colour) {emojistyle colour}
    [exec] (Monochrome) {emojistyle mono}
    [exec] (Show emoji test) {xterm -hold -e emojitest}
    [exec] (Show all symbols, paged) {xterm -T symboltest -e symboltest}
  [end]
  [submenu] (Window Manager)
    [config] (Configure)
    [reconfig] (Reload Config)
    [restart] (Restart)
    [exit] (Exit)
  [end]
  [workspaces] (Workspace List)
  [submenu] (Fluxbox Styles)
    [stylesdir] (/usr/share/fluxbox/styles)
  [end]
[end]
EOF
    # --release: no test programs, so the Emoji menu only switches the emoji font
    if ((RELEASE)); then
        sed -i -e '/(Show emoji test)/d' -e '/(Show all symbols, paged)/d' "$OVL/root/.fluxbox/menu"
    fi
    # --no-mlterm: one terminal, so no Terminal submenu: xterm goes first in the menu
    ((MLTERM)) || sed -i '/^  \[submenu\] (Terminal)$/,/^  \[end\]$/c\  [exec] (xterm) {xterm}' "$OVL/root/.fluxbox/menu"
    ((ADIE)) || sed -i '/{adie}$/d' "$OVL/root/.fluxbox/menu"      # --no-adie
    {
        cat <<'EOF'
session.menuFile: ~/.fluxbox/menu
session.styleFile: /usr/share/fluxbox/styles/V86-Dark
session.screen0.toolbar.visible: true
session.screen0.toolbar.tools: prevworkspace, workspacename, nextworkspace, iconbar, clock
EOF
        # --min: move/resize shows an outline + its position instead of redrawing the
        # window on every mouse step, and Fluxbox's own text is drawn without antialiasing
        if ((MIN)); then cat <<'EOF'
session.screen0.workspaces: 1
session.screen0.opaqueMove: false
session.screen0.showwindowposition: true
session.screen0.antialias: false
EOF
        else printf 'session.screen0.workspaces: 2\nsession.screen0.opaqueMove: true\n'; fi
    } | put "$OVL/root/.fluxbox/init"

    # DejaVu fonts (Fluxbox segfaults with no usable font); 'background: unset' stops
    # styles from repainting the desktop (xinitrc sets it once).
    put "$OVL/root/.fluxbox/overlay" <<'EOF'
*.font:          DejaVu Sans-9
menu.title.font: DejaVu Sans Bold-9
background:      unset
EOF

    # The dark style lives next to the stock ones, so "Fluxbox Styles" switches
    # between V86-Dark and the original look (bloe = Fluxbox's default).
    put "$OVL/usr/share/fluxbox/styles/V86-Dark" <<'EOF'
! V86 Dark: flat dark slate, high-contrast text
menu.borderColor:                #1b1f27
menu.borderWidth:                1
menu.bullet:                     triangle
menu.bullet.position:            right
menu.title:                      flat
menu.title.color:                #3b4252
menu.title.textColor:            #ffffff
menu.title.justify:              center
menu.frame:                      flat
menu.frame.color:                #2e3440
menu.frame.textColor:            #eceff4
menu.frame.disableColor:         #8f9aae
menu.hilite:                     flat
menu.hilite.color:               #3f6b9b
menu.hilite.textColor:           #ffffff
toolbar:                         flat
toolbar.color:                   #2e3440
toolbar.borderColor:             #1b1f27
toolbar.borderWidth:             1
toolbar.height:                  22
toolbar.clock:                   flat
toolbar.clock.color:             #2e3440
toolbar.clock.textColor:         #eceff4
toolbar.workspace:               flat
toolbar.workspace.color:         #2e3440
toolbar.workspace.textColor:     #eceff4
toolbar.button:                  flat
toolbar.button.color:            #3b4252
toolbar.button.picColor:         #eceff4
toolbar.button.pressed:          flat
toolbar.button.pressed.color:    #4c566a
toolbar.iconbar.empty:           flat
toolbar.iconbar.empty.color:     #2e3440
toolbar.iconbar.focused:         flat
toolbar.iconbar.focused.color:   #3d5a80
toolbar.iconbar.focused.textColor: #ffffff
toolbar.iconbar.unfocused:       flat
toolbar.iconbar.unfocused.color: #3b4252
toolbar.iconbar.unfocused.textColor: #c3cad8
window.borderColor:              #1b1f27
window.borderWidth:              1
window.bevelWidth:               0
window.title.height:             20
window.justify:                  center
window.title.focus:              flat
window.title.focus.color:        #3d5a80
window.title.unfocus:            flat
window.title.unfocus.color:      #3b4252
window.label.focus:              flat
window.label.focus.color:        #3d5a80
window.label.focus.textColor:    #ffffff
window.label.unfocus:            flat
window.label.unfocus.color:      #3b4252
window.label.unfocus.textColor:  #c3cad8
window.button.focus:             flat
window.button.focus.color:       #3d5a80
window.button.focus.picColor:    #ffffff
window.button.unfocus:           flat
window.button.unfocus.color:     #3b4252
window.button.unfocus.picColor:  #c3cad8
window.button.pressed:           flat
window.button.pressed.color:     #4c566a
window.handle.focus:             flat
window.handle.focus.color:       #3d5a80
window.handle.unfocus:           flat
window.handle.unfocus.color:     #3b4252
window.handleWidth:              3
window.grip.focus:               flat
window.grip.focus.color:         #5e81ac
window.grip.unfocus:             flat
window.grip.unfocus.color:       #4c566a
slit:                            flat
slit.color:                      #2e3440
EOF

    # XFE's built-in window size is 1024x768 (xfedefs.h): too big for 800x600. Size it to
    # the screen minus room for the toolbar/title bar; applied to its xferc in post-build.
    printf '%s %s\n' $((W - 60)) $((H - 100)) | put "$OVL/usr/share/v86/xfe-size"

    # XFE dark colors (XFE's own "Dark" theme); merged into its global xferc in post-build.
    put "$OVL/usr/share/v86/xfe-colors" <<'EOF'
basecolor=#343434
bordercolor=#696969
backcolor=#242424
forecolor=#d7d7d7
selbackcolor=#31567c
selforecolor=#ffffff
listbackcolor=#242424
listforecolor=#d7d7d7
highlightcolor=#454545
pbarcolor=#31567c
attentioncolor=#ff7b7b
scrollbarcolor=#484848
hilitecolor=#3e3e3e
shadowcolor=#242424
EOF

    # Adie: FOX's system-wide settings for it (the user's changes go to ~/.foxrc/Adie and
    # win). XFE's dark colours for the window, xterm's background and palette for the
    # text (each colour >= 4.5:1, checked by unitest), monospace font, syntax colouring,
    # line numbers and a marked current line. [STYLE]: one entry per rule name in
    # Adie.stx (shared by all languages): foreground, background, then 5 colours that
    # #00000000 leaves at the editor's own, then flags (4 = bold). Rules without an entry
    # use the text colour. Code in #if 0 gets a faint band: dimmed text alone looked like
    # a comment.
    if ((ADIE)); then
        {
            echo '[SETTINGS]'
            grep -E '^(basecolor|bordercolor|backcolor|forecolor|selbackcolor|selforecolor|hilitecolor|shadowcolor)=' \
                "$OVL/usr/share/v86/xfe-colors"
            cat <<'EOF'
textfont=DejaVu Sans Mono,90
textbackground=#0a0b0e
textforeground=#e5e9f0
textcursor=#e5e9f0
textselbackground=#31567c
textselforeground=#ffffff
texthilitebackground=#4c566a
texthiliteforeground=#ffffff
textactivebackground=#15181e
textnumberbackground=#15181e
textnumberforeground=#8a95a8
browserbackground=#242424
browserforeground=#d7d7d7
browserselbackground=#31567c
browserselforeground=#ffffff
browserlines=#696969
colorize=1
showactive=1
barcols=4
autoindent=1
searchpath=/usr/include:/usr/lib/tcc/include
EOF
            printf 'width=%s\nheight=%s\nx=40\ny=20\n' $((W - 160)) $((H - 160))
            echo '[STYLE]'
            local z='#00000000,#00000000,#00000000,#00000000,#00000000' r c b f n
            while read -r c b f r; do
                [ "$b" = - ] && b='#00000000'
                for n in $r; do echo "$n=$c,$b,$z,$f"; done
            done <<'EOF'
#95a2ba - 0 Comment CComment CPPComment CDocComment CommentContinuation AssignmentComment
#ebcb8b - 4 FIXME
#a3be8c - 0 String Char TripleString
#ebcb8b - 0 ControlEscape OctalEscape HexEscape CharEscape Escape Format
#8fb0d4 - 4 Keyword Statement DependencyLine
#8fb0d4 - 0 Builtin DependencyContinuation
#88c0d0 - 0 Type Variable Assignment AssignmentContinuation
#d0a9cc - 0 Number Value
#e5848e - 0 Preprocessor PreprocessorContinuation
#8a95a8 #1c1f26 0 PPBlocked PPSubblock
EOF
        } | put "$OVL/etc/foxrc/Adie"
    fi

    # XFE remembers its last window size, so a size that fit 1024x768 overflows after
    # 'xmode 800x600x..'. Fit it to the current screen (xrandr) each time X starts:
    # a size that is too big shrinks, and grows back when the screen does (xmode up).
    # The default size follows the screen; a size you picked by resizing XFE is kept
    # (only shrunk while it doesn't fit). ~/.config/xfe/fit remembers which is which.
    putx "$OVL/usr/bin/xfe-fit" <<'EOF'
#!/bin/sh
# Xorg (RandR 1.2+) prints "current W x H,"; Xvesa (RandR 1.1) prints a size table
# with "*" on the current size: "*0    800 x 600    ( 211mm x 158mm ) ...".
# shellcheck disable=SC2046  # split "W H" into $1 $2
set -- $(xrandr --current 2>/dev/null | sed -n -e 's/.* current \([0-9]*\) x \([0-9]*\),.*/\1 \2/p' \
    -e 's/^\*[0-9]* *\([0-9]*\) x \([0-9]*\) .*/\1 \2/p' | head -n1)
[ -n "$2" ] || exit 0
maxw=$(($1 - 60)) maxh=$(($2 - 100))
# Adie remembers its size and position too (~/.foxrc/Adie): bring a window that no
# longer fits back onto the screen
if [ -f "$HOME/.foxrc/Adie" ]; then
    awk -v mw="$maxw" -v mh="$maxh" -v sw="$1" -v sh="$2" '
        /^\[/ { sec = $0 }
        sec == "[SETTINGS]" && /^(width|height|x|y)=/ { split($0, a, "="); v[a[1]] = a[2] + 0 }
        { line[NR] = $0; s[NR] = sec }
        END {
            if (v["width"] > mw) v["width"] = mw; if (v["height"] > mh) v["height"] = mh
            if (v["x"] + v["width"] > sw || v["x"] < 0) v["x"] = 0
            if (v["y"] + v["height"] > sh || v["y"] < 0) v["y"] = 0
            for (i = 1; i <= NR; i++) {
                if (s[i] == "[SETTINGS]" && line[i] ~ /^(width|height|x|y)=/) { split(line[i], a, "="); line[i] = a[1] "=" v[a[1]] }
                print line[i] }
        }' "$HOME/.foxrc/Adie" > "$HOME/.foxrc/Adie.tmp" && mv "$HOME/.foxrc/Adie.tmp" "$HOME/.foxrc/Adie"
fi
f=$HOME/.config/xfe/xferc state=$HOME/.config/xfe/fit
if [ ! -f "$f" ]; then
    [ -f /usr/share/xfe/xferc ] || exit 0
    mkdir -p "${f%/*}" && cp /usr/share/xfe/xferc "$f"
fi
# shellcheck disable=SC2046
set -- $(awk '/^\[/ { sec = $0 } sec == "[OPTIONS]" && /^(width|height)=/ { split($0, a, "="); v[a[1]] = a[2] }
    END { print v["width"] + 0, v["height"] + 0 }' "$f")
curw=$1 curh=$2
# state: "WANTW WANTH WROTEW WROTEH"; WANT is a size or "fit fit"
wantw=$curw wanth=$curh
if [ -f "$state" ] && read -r sw sh lw lh < "$state" && [ "$curw $curh" = "$lw $lh" ]; then
    wantw=$sw wanth=$sh                  # unchanged since we set it: keep what was wanted
elif [ ! -f "$state" ] && [ "$curw $curh" = "$(cat /usr/share/v86/xfe-size 2>/dev/null)" ]; then
    wantw=fit wanth=fit                  # the image's default size: follow the screen
fi
if [ "$wantw" = fit ]; then neww=$maxw newh=$maxh
else neww=$((wantw < maxw ? wantw : maxw)) newh=$((wanth < maxh ? wanth : maxh)); fi
awk -v w="$neww" -v h="$newh" '
    /^\[/ { sec = $0 }
    sec == "[OPTIONS]" && /^width=/  { $0 = "width=" w }
    sec == "[OPTIONS]" && /^height=/ { $0 = "height=" h }
    { print }
' "$f" > "$f.tmp" && mv "$f.tmp" "$f"
echo "$wantw $wanth $neww $newh" > "$state"
EOF

    putx "$OVL/usr/bin/xlog" <<'EOF'
#!/bin/sh
tail -n 15 /var/log/xstart.log; tail -n 40 /var/log/Xorg.0.log
[ -f /var/log/Xorg.xmode-failed.log ] && { echo "=== last failed xmode attempt ==="; grep -E '\((EE|WW)\)|Xvesa|vm86|Fatal|rror' /var/log/Xorg.xmode-failed.log | tail -n 25; }
EOF
}


# ══════════════════════════════════════════════════════════════════════════════
#  post-build: runs after Buildroot's target-finalize (which deletes headers and
#  static libs from the target), just before the rootfs is packed.
# ══════════════════════════════════════════════════════════════════════════════

write_post_build() {
    putx "$EXT/post-build.sh" <<'EOF'
#!/bin/sh
set -e
TARGET="$1"
# Buildroot exports the real staging path. Not output/staging: that symlink is only
# created by staging-finalize, which runs AFTER this hook (missing on fresh builds).
STAGING="${STAGING_DIR:?post-build: STAGING_DIR not set (run from Buildroot)}"
[ -f "$STAGING/usr/include/stdio.h" ] || { echo "post-build: no libc headers in $STAGING"; exit 1; }

grep -q '^root:\$' "$TARGET/etc/shadow" || { echo "post-build: root has no password hash"; exit 1; }
rm -f "$TARGET/etc/init.d/S40xorg"        # would start a second X server on :0

# Xlib's UTF-8 locale: Buildroot's locale purge (whitelist "C en_US") deletes the
# en_US.UTF-8 folder and locale.alias, so Xlib can't use LANG=C.UTF-8 ("locale not
# supported by C library"). Put back the UTF-8 locale and the C.UTF-8 alias.
X11L=/usr/share/X11/locale
if [ -d "$STAGING$X11L/en_US.UTF-8" ] && [ -d "$TARGET$X11L" ]; then
    cp -a "$STAGING$X11L/en_US.UTF-8" "$TARGET$X11L/"
    printf '%s\n' 'C/XLC_LOCALE: C' 'en_US.UTF-8/XLC_LOCALE: en_US.UTF-8' > "$TARGET$X11L/locale.dir"
    printf '%s\n' 'C.UTF-8: en_US.UTF-8' 'C.utf8: en_US.UTF-8' 'en_US.utf8: en_US.UTF-8' > "$TARGET$X11L/locale.alias"
    printf '%s\n' 'en_US.UTF-8/Compose: en_US.UTF-8' > "$TARGET$X11L/compose.dir"
fi

# nano: slightly darker syntax colors (bright -> normal; blue/black stay bright to stay readable)
for f in "$TARGET"/usr/share/nano/*.nanorc "$TARGET"/usr/share/nano/extra/*.nanorc; do
    [ -f "$f" ] && sed -i -E 's/(^|[ ,])(bright|light)(red|green|yellow|magenta|cyan|white)\b/\1\3/g' "$f"
done

# XFE: no "running as root" nag, dark colors, drop the 3 extra icon themes
XFERC="$TARGET/usr/share/xfe/xferc"
if [ -f "$XFERC" ]; then
    grep -q '^basecolor=' "$XFERC" || sed -i "/^\[SETTINGS\]/r $TARGET/usr/share/v86/xfe-colors" "$XFERC"
    # [OPTIONS]: no root nag, main window sized to the screen (XFE's default is 1024x768),
    # and the tools + panel toolbars on their own rows: side by side with the general
    # toolbar they set a ~600 px minimum width. Value = shown;side;galley with FOX's
    # LAYOUT_SIDE_TOP=0 and LAYOUT_DOCK_NEXT=64 (same keys in XFE 2.1.7 and later).
    XW=760 XH=500
    [ -f "$TARGET/usr/share/v86/xfe-size" ] && read -r XW XH < "$TARGET/usr/share/v86/xfe-size"
    awk -v w="$XW" -v h="$XH" '
        function opts() {
            print "root_warn=0"; print "width=" w; print "height=" h
            print "toolstoolbar=1;0;64"; print "paneltoolbar=1;0;64"; done = 1
        }
        /^\[/ { sec = $0 }
        sec == "[OPTIONS]" && /^(width|height|root_warn|toolstoolbar|paneltoolbar)=/ { next }
        { print }
        $0 == "[OPTIONS]" && !done { opts() }
        END { if (!done) { print "[OPTIONS]"; opts() } }
    ' "$XFERC" > "$XFERC.tmp" && mv "$XFERC.tmp" "$XFERC"
    rm -rf "$TARGET"/usr/share/xfe/icons/gnome-theme "$TARGET"/usr/share/xfe/icons/kde-theme \
           "$TARGET"/usr/share/xfe/icons/xfce-theme
    # Adie (not with --no-adie) as XFE's text editor: Open and Edit on .c, .h, .sh... use it
    if [ -x "$TARGET/usr/bin/adie" ]; then
        awk '
            /^\[/ { if (sec == "[PROGS]" && !done) { print "txteditor=adie"; done = 1 } sec = $0 }
            sec == "[PROGS]" && /^txteditor=/ { next }
            { print }
            END { if (!done) { if (sec != "[PROGS]") print "[PROGS]"; print "txteditor=adie" } }
        ' "$XFERC" > "$XFERC.tmp" && mv "$XFERC.tmp" "$XFERC"
    fi
fi

# On-target compiler: put headers and dev files back (not with --no-gcc).
# Headers are MERGED into usr/include (copying the folder into an existing one
# would nest it as usr/include/include). Libraries: only what the target lacks
# (crt*.o, libc.so script, .so links, .a, pkgconfig); the stripped .so files stay.
if [ -x "$TARGET/usr/bin/gcc" ]; then
mkdir -p "$TARGET/usr/include"
cp -a "$STAGING/usr/include/." "$TARGET/usr/include/"
for d in usr/lib lib usr/share/pkgconfig; do
    [ -d "$STAGING/$d" ] || continue
    mkdir -p "$TARGET/$d"
    rsync -a --ignore-existing --exclude='*.la' "$STAGING/$d/" "$TARGET/$d/"
done
STRIP=$(ls "${HOST_DIR:-$TARGET/../host}"/bin/*-linux-*-strip 2>/dev/null | head -n1)
[ -n "$STRIP" ] && find "$TARGET/usr/lib" "$TARGET/lib/" -name '*.a' -type f -not -path '*/usr/lib/tcc/*' \
    -exec "$STRIP" --strip-debug {} + 2>/dev/null || true

for t in ar as ld nm objcopy objdump ranlib readelf strip; do
    [ -e "$TARGET/usr/bin/$t" ] && continue
    c=$(ls "$TARGET"/usr/*/bin/$t 2>/dev/null | head -n1)
    [ -n "$c" ] && ln -sf "${c#$TARGET}" "$TARGET/usr/bin/$t"
done
ln -sf gcc "$TARGET/usr/bin/cc"; ln -sf g++ "$TARGET/usr/bin/c++"
fi

# TinyCC (--tcc, --wincc): Buildroot deleted its runtime libraries (libtcc1.a) along with
# the target's static libraries; put them back from staging. Not stripped: the Windows
# ones are the 64-bit and PE runtime, which the target's strip doesn't know.
if [ -d "$TARGET/usr/lib/tcc" ] && [ -d "$STAGING/usr/lib/tcc" ]; then
    (cd "$STAGING/usr/lib/tcc" && find . -name '*.a' -type f) | while read -r f; do
        cp "$STAGING/usr/lib/tcc/$f" "$TARGET/usr/lib/tcc/$f"
    done
fi

# --man: put back the pages Buildroot removed (saved by V86_KEEP_MAN_PAGES), turn the
# one-line ".so manN/page.N" redirects into symlinks (each file costs a 4 KB page in
# the RAM filesystem, a symlink almost nothing), then index them for man/apropos/whatis.
if [ -x "$TARGET/usr/bin/mandoc" ]; then
    saved="${BUILD_DIR:?}/v86-man"
    [ -d "$saved" ] || { echo "post-build: ERROR man pages were not saved ($saved)"; exit 1; }
    MAN="$TARGET/usr/share/man"
    rm -rf "$MAN"; mkdir -p "$TARGET/usr/share"; cp -a "$saved" "$MAN"
    find "$MAN" -type f -size -200c | while IFS= read -r f; do
        [ "$(wc -l < "$f")" -le 1 ] || continue
        l=$(head -n1 "$f")
        case "$l" in .so\ *) ;; *) continue ;; esac
        t=$(printf '%s\n' "${l#.so }" | sed -E 's|^man([0-9])[a-z]*/|man\1/|')
        [ -f "$MAN/$t" ] || continue
        ln -sf "../$t" "$f"
    done
    "${HOST_DIR:?}/sbin/makewhatis" "$MAN"
    echo "post-build: man pages: $(find "$MAN" -type f -name '*.[0-9]*' | wc -l) files + $(find "$MAN" -type l | wc -l) links, $(du -sm "$MAN" | cut -f1) MB, indexed"
fi

need=""
if [ -x "$TARGET/usr/bin/gcc" ]; then
    need="usr/include/stdio.h usr/bin/gcc"
    ls "$TARGET"/usr/lib/libSDL3.so* >/dev/null 2>&1 && need="$need usr/include/SDL3/SDL.h"
    ls "$TARGET"/usr/lib/libSDL3_net.so* >/dev/null 2>&1 && need="$need usr/include/SDL3_net/SDL_net.h"
else
    rm -rf "$TARGET/usr/include"              # --no-gcc: no compiler, no headers
    # file's magic database (~10 MB) and the file command are developer tools. nano keeps
    # libmagic (its colours depend on it in Buildroot) and falls back to file extensions
    # when the database is missing.
    rm -f "$TARGET/usr/share/misc/magic.mgc" "$TARGET/usr/bin/file"
    # libstdc++ (~1.9 MB) is installed with every C++ toolchain; without a compiler it is
    # only needed by C++ programs in the image. Drop it if no ELF file links to it.
    READELF=$(ls "${HOST_DIR:?}"/bin/*-linux-*-readelf 2>/dev/null | head -n1)
    if [ -n "$READELF" ] && ls "$TARGET"/usr/lib/libstdc++.so* >/dev/null 2>&1; then
        users=$(find "$TARGET" -type f \( -perm -u+x -o -name '*.so*' \) ! -name 'libstdc++.so*' \
                -exec "$READELF" -d {} + 2>/dev/null | grep -c 'NEEDED.*\[libstdc++\.so' || true)
        if [ "$users" = 0 ]; then
            rm -f "$TARGET"/usr/lib/libstdc++.so*
            echo "post-build: removed libstdc++ (no compiler, nothing links to it)"
        fi
    fi
fi
for f in $need; do
    [ -e "$TARGET/$f" ] || { echo "post-build: ERROR /$f is missing"; exit 1; }
done
echo "post-build: target is $(du -sm "$TARGET" | cut -f1) MB unpacked"
EOF
}


# ══════════════════════════════════════════════════════════════════════════════
#  Defconfig
# ══════════════════════════════════════════════════════════════════════════════

write_defconfig() {
    if [[ "$MODE" == smoke ]]; then
        # prebuilt toolchain, BusyBox only, no kernel/overlay/post-build
        {
            cat <<EOF
BR2_x86_i686=y
BR2_TOOLCHAIN_EXTERNAL=y
BR2_TOOLCHAIN_EXTERNAL_BOOTLIN=y
BR2_TOOLCHAIN_EXTERNAL_BOOTLIN_X86_I686_GLIBC_STABLE=y
BR2_RELRO_PARTIAL=y
BR2_DL_DIR="$DL_DIR"
BR2_WGET="$EXT/v86-fetch"
BR2_PRIMARY_SITE="https://sources.buildroot.net"
BR2_TARGET_ROOTFS_CPIO=y
BR2_TARGET_ROOTFS_CPIO_ZSTD=y
# BR2_TARGET_ROOTFS_TAR is not set
EOF
            [[ "$USE_CCACHE" == 1 ]] && printf 'BR2_CCACHE=y\nBR2_CCACHE_DIR="%s"\nBR2_CCACHE_INITIAL_SETUP="--max-size=15G"\n' "$WORK/ccache"
        } | put "$EXT/configs/v86_defconfig"
        return 0
    fi
    local overlays="\$(BR2_EXTERNAL_V86_PATH)/overlay"
    [[ -d "$USER_OVERLAY" ]] && overlays+=" $USER_OVERLAY"
    {
        cat <<EOF
BR2_x86_i686=y
BR2_TOOLCHAIN_BUILDROOT_GLIBC=y
BR2_TOOLCHAIN_BUILDROOT_CXX=y
# gcc 15 (Buildroot 2026.08's default) pinned, for the cross compiler and the native one
# in the image alike. Only TinyX needs old-C flags for it (see tinyx.mk).
BR2_GCC_VERSION_15_X=y
BR2_OPTIMIZE_S=y
# Full RELRO links with -z now; Xorg drivers use symbols from helper modules that are
# loaded after them (fbdev -> fbdevhw, vesa -> int10/vbe) and then fail to load with
# "undefined symbol: fbdevHWSave". Partial RELRO keeps lazy binding.
BR2_RELRO_PARTIAL=y
BR2_DL_DIR="$DL_DIR"
BR2_WGET="$EXT/v86-fetch"
# Buildroot's own mirror first (same files, checked against the same sha256 hashes);
# upstream sites like xorg.freedesktop.org are only tried if the mirror lacks a file.
BR2_PRIMARY_SITE="https://sources.buildroot.net"
BR2_TARGET_GENERIC_HOSTNAME="v86"
BR2_TARGET_GENERIC_ISSUE="v86 $NAME"
BR2_TARGET_GENERIC_ROOT_PASSWD="root"
BR2_SYSTEM_DHCP=""
BR2_ROOTFS_DEVICE_CREATION_DYNAMIC_EUDEV=y
# udev's hardware database (/lib/udev/hwdb.bin, ~9.5 MB): USB/PCI vendor and model names
# for real hardware. Nothing in a v86 guest needs it; also drops the host-eudev build.
# BR2_PACKAGE_EUDEV_ENABLE_HWDB is not set
BR2_ROOTFS_OVERLAY="$overlays"
BR2_ROOTFS_POST_BUILD_SCRIPT="\$(BR2_EXTERNAL_V86_PATH)/post-build.sh"
BR2_LINUX_KERNEL=y
BR2_LINUX_KERNEL_LATEST_VERSION=y
BR2_LINUX_KERNEL_DEFCONFIG="i386"
BR2_LINUX_KERNEL_CONFIG_FRAGMENT_FILES="\$(BR2_EXTERNAL_V86_PATH)/linux.fragment"
BR2_LINUX_KERNEL_$( ((XZ)) && echo XZ || echo ZSTD )=y
BR2_TARGET_ROOTFS_CPIO=y
BR2_TARGET_ROOTFS_CPIO_$( ((XZ)) && echo XZ || echo ZSTD )=y
BR2_PACKAGE_BUSYBOX_SHOW_OTHERS=y
BR2_PACKAGE_BUSYBOX_CONFIG_FRAGMENT_FILES="\$(BR2_EXTERNAL_V86_PATH)/busybox.fragment"
BR2_SYSTEM_BIN_SH_BASH=y
BR2_PACKAGE_BASH=y
BR2_PACKAGE_NANO=y
# nano's "optimize for size" defaults to y and drops colour, nanorc and all syntax files
# BR2_PACKAGE_NANO_TINY is not set
# file: Buildroot builds nano's colours and syntax files only together with it (libmagic);
# --no-gcc images drop just its 10 MB magic database and the file command (post-build)
BR2_PACKAGE_FILE=y
BR2_PACKAGE_NCURSES_WCHAR=y
# glibc has no built-in C.UTF-8: generate it (LANG=C.UTF-8 in every image)
BR2_GENERATE_LOCALE="C.UTF-8"
BR2_PACKAGE_ALSA_LIB=y
BR2_PACKAGE_ALSA_UTILS=y
BR2_PACKAGE_ALSA_UTILS_AMIXER=y
BR2_PACKAGE_ALSA_UTILS_APLAY=y
BR2_PACKAGE_ALSA_UTILS_SPEAKER_TEST=y
EOF
        if ((SDL)); then echo 'BR2_PACKAGE_SDL3=y'; fi
        if ((SDLNET)); then echo 'BR2_PACKAGE_SDL3_NET=y'; fi
        echo 'BR2_PACKAGE_HOST_MANDOC=y'              # every image: same host-tool cache
        echo 'BR2_PACKAGE_HOST_TINYCC=y'              # (the same reason)
        if ((TCC || WINCC)); then echo 'BR2_PACKAGE_TINYCC=y'; fi
        if ((TCC)); then echo 'BR2_PACKAGE_TINYCC_LINUX=y'; fi
        if ((WINCC)); then echo 'BR2_PACKAGE_TINYCC_WIN=y'; fi
        if ((UPX)); then echo 'BR2_PACKAGE_UPX_TARGET=y'; fi
        if ((BASHDB)); then echo 'BR2_PACKAGE_BASHDB=y'; fi
        if ((GDB)); then printf '%s\n' BR2_PACKAGE_GDB=y BR2_PACKAGE_GDB_SERVER=y BR2_PACKAGE_GDB_DEBUGGER=y; fi
        if ((BASHDEV)); then printf '%s\n' BR2_PACKAGE_COREUTILS=y BR2_PACKAGE_GAWK=y BR2_PACKAGE_GREP=y \
            BR2_PACKAGE_SED=y BR2_PACKAGE_FINDUTILS=y BR2_PACKAGE_DIFFUTILS=y BR2_PACKAGE_PATCH=y \
            BR2_PACKAGE_BASH_COMPLETION=y BR2_PACKAGE_STRACE=y BR2_PACKAGE_LTRACE=y BR2_PACKAGE_JQ=y \
            BR2_PACKAGE_BATS_CORE=y BR2_PACKAGE_BATS_SUPPORT=y BR2_PACKAGE_BATS_ASSERT=y BR2_PACKAGE_BATS_FILE=y \
            BR2_PACKAGE_TREE=y BR2_PACKAGE_BC=y BR2_PACKAGE_NETCAT=y BR2_PACKAGE_LSOF=y BR2_PACKAGE_PROCPS_NG=y \
            BR2_PACKAGE_TIME=y BR2_PACKAGE_TMUX=y BR2_PACKAGE_LIBCURL=y BR2_PACKAGE_LIBCURL_CURL=y \
            BR2_PACKAGE_OPENSSL=y BR2_PACKAGE_LIBOPENSSL=y BR2_PACKAGE_CA_CERTIFICATES=y BR2_PACKAGE_SHFMT=y; fi
        if ((GCC)); then printf '%s\n' 'BR2_PACKAGE_GCC_TARGET=y' 'BR2_PACKAGE_PKGCONF=y'; fi
        if ((MAN)); then printf '%s\n' 'BR2_PACKAGE_MANDOC=y' 'BR2_PACKAGE_MAN_PAGES=y' 'BR2_PACKAGE_LESS=y'; fi
        if ((GUI && MLTERM)); then echo 'BR2_PACKAGE_MLTERM=y'; fi   # brings cairo + HarfBuzz
        if ((GUI && ADIE)); then echo 'BR2_PACKAGE_FOX_ADIE=y'; fi
        if ((SHELLCHECK)); then for p in $SC_PKGS; do echo "BR2_PACKAGE_$p=y"; done; fi   # Debian's shellcheck's libraries
        if ((GUI)); then cat <<'EOF'
BR2_PACKAGE_XORG7=y
BR2_PACKAGE_XSERVER_XORG_SERVER=y
BR2_PACKAGE_XSERVER_XORG_SERVER_MODULAR=y
BR2_PACKAGE_XDRIVER_XF86_INPUT_EVDEV=y
BR2_PACKAGE_LIBPCIACCESS=y
BR2_PACKAGE_XKEYBOARD_CONFIG=y
BR2_PACKAGE_XAPP_XINIT=y
BR2_PACKAGE_XAPP_XAUTH=y
BR2_PACKAGE_XAPP_XSETROOT=y
BR2_PACKAGE_XAPP_XSET=y
BR2_PACKAGE_XAPP_XRANDR=y
BR2_PACKAGE_XTERM=y
BR2_PACKAGE_XFE=y
BR2_PACKAGE_FLUXBOX=y
BR2_PACKAGE_XLIB_LIBXFT=y
BR2_PACKAGE_FREETYPE=y
BR2_PACKAGE_FONTCONFIG=y
BR2_PACKAGE_DEJAVU=y
BR2_PACKAGE_UNIFONT=y
BR2_PACKAGE_TWEMOJI=y
BR2_PACKAGE_FBTERM=y
BR2_PACKAGE_XORGPROTO=y
EOF
            # SDL3's X11 backend and the Xlib extensions its CMake insists on
            if ((SDL)); then cat <<'EOF'
BR2_PACKAGE_SDL3_X11=y
BR2_PACKAGE_XLIB_LIBXSCRNSAVER=y
BR2_PACKAGE_XLIB_LIBXCURSOR=y
BR2_PACKAGE_XLIB_LIBXFIXES=y
BR2_PACKAGE_XLIB_LIBXI=y
BR2_PACKAGE_XLIB_LIBXRANDR=y
BR2_PACKAGE_XLIB_LIBXRENDER=y
BR2_PACKAGE_XLIB_LIBXTST=y
EOF
            fi
        fi
        case "$MODE" in
            qemu) echo 'BR2_PACKAGE_LIBDRM=y' ;;                       # modesetting driver
            vesa) printf 'BR2_PACKAGE_XDRIVER_XF86_VIDEO_FBDEV=y\nBR2_PACKAGE_XDRIVER_XF86_VIDEO_VESA=y\nBR2_PACKAGE_TINYX=y\n' ;;
        esac
        if [[ "$USE_CCACHE" == 1 ]]; then
            printf 'BR2_CCACHE=y\nBR2_CCACHE_DIR="%s"\nBR2_CCACHE_INITIAL_SETUP="--max-size=15G"\n' "$WORK/ccache"
        fi
    } | put "$EXT/configs/v86_defconfig"
}

# Buildroot's download command (BR2_WGET). Called as: v86-fetch [-q] [opts] -O <file> <url>
# wget first; if that fails, curl (OpenSSL — Debian/Ubuntu wget uses GnuTLS, and a TLS
# handshake that one library fails often works with the other); X.org files last from a
# public X.org mirror. Buildroot checks every file's sha256, so a mirror can't sneak in
# a different file.
write_fetch() {
    { printf '#!/bin/sh\nWGET_OPTS="%s"\n' "${WGET_OPTS[*]}"; cat <<'EOF'
out="" url="" prev=""
for a in "$@"; do [ "$prev" = "-O" ] && out=$a; prev=$a; url=$a; done
wget $WGET_OPTS "$@" && exit 0
[ -n "$out" ] && [ -n "$url" ] || exit 1
echo "v86-fetch: wget failed, trying curl: $url" >&2
curl -fL --retry 3 --connect-timeout 20 -o "$out" "$url" && exit 0
case "$url" in
    *://xorg.freedesktop.org/*/individual/*|*://www.x.org/*/individual/*)
        alt="https://www.mirrorservice.org/sites/ftp.x.org/pub/individual/${url#*/individual/}"
        echo "v86-fetch: trying X.org mirror: $alt" >&2
        curl -fL --retry 3 --connect-timeout 20 -o "$out" "$alt" && exit 0 ;;
esac
rm -f "$out"; exit 1
EOF
    } | putx "$EXT/v86-fetch"
}

write_tree() {
    step "Generating $EXT"
    rm -rf "$EXT"
    write_fetch
    write_packages
    write_kernel_fragment
    write_overlay
    if ((SHELLCHECK)) && [[ -n "$SC_BIN" ]]; then
        install -D -m 755 "$(sc_image_binary)" "$OVL/usr/bin/shellcheck"
    fi
    write_post_build
    write_defconfig
}


# ══════════════════════════════════════════════════════════════════════════════
#  Checks that need no Buildroot: generated scripts, configs, menu
# ══════════════════════════════════════════════════════════════════════════════

lint_tree() {
    step "Checking generated files"
    local f bad=0 dup
    while IFS= read -r f; do
        head -c2 "$f" | grep -q '^#!' || continue
        if head -n1 "$f" | grep -q bash; then bash -n "$f"; else sh -n "$f"; fi \
            || { warn "shell syntax error in ${f#"$EXT"/}"; bad=1; }
        [[ -x "$f" ]] || { warn "not executable: ${f#"$EXT"/}"; bad=1; }
    done < <(find "$EXT" -type f)

    for f in "$EXT/configs/v86_defconfig" "$EXT/linux.fragment"; do
        dup=$(grep -oE '^(# )?(BR2|CONFIG)_[A-Za-z0-9_]+' "$f" | sed 's/^# //' | sort | uniq -d)
        [[ -z "$dup" ]] || { warn "set twice in ${f##*/}: $dup"; bad=1; }
    done

    if ((GUI)); then
        local want=modesetting conf="$OVL/etc/X11/xorg.conf"
        if [[ "$MODE" == vesa ]]; then
            want=fbdev conf="$OVL/etc/X11/xorg.conf.boot"
            if ((BPP != BOOT_BPP)); then
                grep -q -- "-mode 0x11[58] " "$OVL/etc/X11/xvesa.args" 2>/dev/null \
                    || { warn "24 bpp image doesn't start Xvesa with a 24 bpp VBE mode"; bad=1; }
            fi
        fi
        grep -q "Driver *\"$want\"" "$conf" || { warn "${conf##*/} doesn't use the $want driver"; bad=1; }
        # the Terminal submenu (xterm, mlterm) only with mlterm; without it, xterm comes first
        local first; first=$(sed -n '2p' "$OVL/root/.fluxbox/menu")
        if ((MLTERM)); then
            [[ "$first" == '  [submenu] (Terminal)' ]] && grep -q '^    \[exec\] (mlterm) {mlterm}$' "$OVL/root/.fluxbox/menu" \
                || { warn "menu: no Terminal submenu with mlterm"; bad=1; }
        else
            [[ "$first" == '  [exec] (xterm) {xterm}' ]] && ! grep -q '(Terminal)\|mlterm' "$OVL/root/.fluxbox/menu" \
                || { warn "menu: --no-mlterm should put xterm first, with no Terminal submenu"; bad=1; }
        fi
        # every command the Fluxbox menu runs must exist in the image
        local cmd known=" xterm xfe nano vi mlterm adie "
        while read -r cmd; do
            [[ "$known" == *" $cmd "* || -x "$OVL/usr/bin/$cmd" ]] || { warn "menu runs unknown command: $cmd"; bad=1; }
        done < <(sed -n 's/.*\[exec\][^{]*{\([^}]*\)}.*/\1/p' "$OVL/root/.fluxbox/menu" \
                 | awk '{ print ($1 == "xterm" && $NF != "xterm") ? $NF : $1 }')
        grep -q '^session.styleFile: /usr/share/fluxbox/styles/V86-Dark$' "$OVL/root/.fluxbox/init" \
            && [[ -f "$OVL/usr/share/fluxbox/styles/V86-Dark" ]] || { warn "dark style not wired up"; bad=1; }
        if ((MIN)); then
            grep -q 'opaqueMove: false' "$OVL/root/.fluxbox/init" || { warn "--min without outline moving"; bad=1; }
            grep -q '<bool>false</bool>' "$OVL/etc/fonts/conf.d/99-v86-min.conf" 2>/dev/null \
                && python3 -I -c 'import sys, xml.dom.minidom as m; m.parse(sys.argv[1])' "$OVL/etc/fonts/conf.d/99-v86-min.conf" \
                || { warn "--min fontconfig rule missing or not valid XML"; bad=1; }
        fi
    fi
    grep -q 'snd_sb16.isapnp=0' "$EXT/linux.fragment" || { warn "SB16 isapnp=0 missing from cmdline"; bad=1; }
    if [[ "$MODE" == vesa ]]; then
        grep -q "u16 mode = $VID_MODE;" "$EXT/external.mk" && grep -qx 'CONFIG_FB_VESA=y' "$EXT/linux.fragment" \
            || { warn "VESA boot mode $VID_MODE not wired up"; bad=1; }
    fi
    ((bad == 0)) || die "generated files have problems (see above)"
    msg "scripts, defconfig, kernel fragment$( ((GUI)) && echo ', xorg.conf, Fluxbox menu') OK"
}


# ══════════════════════════════════════════════════════════════════════════════
#  Configure + verify (Kconfig only: compiles the small kconfig tool, nothing else)
# ══════════════════════════════════════════════════════════════════════════════

configure() {
    step "Configuring ($NAME)"
    mkdir -p "$OUT"
    rm -f "$OUT/.config" "$OUT/.config.old"            # never reuse an old config
    local log="$BUILD/defconfig.log"
    br v86_defconfig >"$log" 2>&1 || { cat "$log"; die "defconfig failed"; }
    grep -iE -A4 'unmet direct dependencies|^warning:' "$log" >&2 && warn "Kconfig reported problems (above)"

    # Kconfig silently drops symbols with unmet dependencies or that no longer exist.
    local line sym dropped=()
    while IFS= read -r line; do
        [[ "$line" =~ ^(BR2_[A-Za-z0-9_]+)=y$ ]] || continue
        sym=${BASH_REMATCH[1]}
        grep -qx "$sym=y" "$OUT/.config" || dropped+=("$sym")
    done < "$EXT/configs/v86_defconfig"
    ((${#dropped[@]} == 0)) || die "these options did not survive Kconfig: ${dropped[*]}"
    # ...and every "is not set" must really be off (some options default to y)
    local forced=()
    while IFS= read -r line; do
        [[ "$line" =~ ^#\ (BR2_[A-Za-z0-9_]+)\ is\ not\ set$ ]] || continue
        grep -qx "${BASH_REMATCH[1]}=y" "$OUT/.config" && forced+=("${BASH_REMATCH[1]}")
    done < "$EXT/configs/v86_defconfig"
    ((${#forced[@]} == 0)) || die "these options are still on although the defconfig turns them off: ${forced[*]}"
    grep -qx 'BR2_LEGACY=y' "$OUT/.config" && die "BR2_LEGACY=y: the defconfig uses a removed option"
    grep -qx 'BR2_RELRO_FULL=y' "$OUT/.config" && die "full RELRO is on: Xorg drivers would fail to load"

    # Package makefiles parse, and our tweaks are really in effect
    local vars
    vars=$(br -s printvars VARS="GCC_TARGET_VERSION XFE_VERSION LINUX_POST_PATCH_HOOKS XSERVER_XORG_SERVER_CONF_OPTS XDRIVER_XF86_VIDEO_VESA_POST_PATCH_HOOKS XLIB_LIBXFT_POST_PATCH_HOOKS BUSYBOX_POST_PATCH_HOOKS" 2>&1) \
        || { printf '%s\n' "$vars" | tail -n 20; die "Buildroot makefiles failed to parse"; }
    [[ "$MODE" == smoke ]] || grep -q '^GCC_TARGET_VERSION=[0-9]' <<<"$vars" || die "gcc-target package not registered"
    grep -q '^BUSYBOX_POST_PATCH_HOOKS=.*V86_BUSYBOX_LESS_UTF8' <<<"$vars" || die "BusyBox less UTF-8 patch hook not registered"
    if [[ "$MODE" == vesa ]]; then
        grep -q '^LINUX_POST_PATCH_HOOKS=.*V86_FORCE_VID_MODE' <<<"$vars" || die "VESA boot-mode kernel hook not registered"
        grep -q -- '--with-int10=x86emu' <<<"$vars" || die "xorg int10 backend override not applied"
        grep -q '^XDRIVER_XF86_VIDEO_VESA_POST_PATCH_HOOKS=.*V86_VESA_ALLOW_FB' <<<"$vars" || die "vesa driver framebuffer patch not registered"
    fi
    if ((GUI)); then
        grep -q '^XLIB_LIBXFT_POST_PATCH_HOOKS=.*V86_XFT_COLOR_PLAIN_GLYPHS' <<<"$vars" || die "libXft colour-font patch hook not registered"
    fi
    if ((GUI && SDL)); then
        br -pn sdl3-configure 2>/dev/null >"$BUILD/sdl3.pn" || true
        grep -qE '/sdl3-[^ /]+/\.stamp_configured: .*\|.*xlib_libXcursor' "$BUILD/sdl3.pn" \
            || die "SDL3 -> libXcursor build-order rule not in effect"
        rm -f "$BUILD/sdl3.pn"
    fi
    msg "config OK: $(grep -c '=y$' "$OUT/.config") options, all requested symbols present"
}

# What would be downloaded (no network): 'show-info' lists every package's sources.
report_downloads() {
    br -s show-info > "$BUILD/show-info.json" 2>/dev/null || { warn "show-info failed"; return 0; }
    python3 -I - "$BUILD/show-info.json" "$DL_DIR" <<'PY'
import json, os, sys
info = json.load(open(sys.argv[1])); dl = sys.argv[2]
files, missing = 0, []
for name, pkg in sorted(info.items()):
    for d in pkg.get("downloads", []):
        src = d.get("source")
        if not src or pkg.get("virtual"):
            continue
        files += 1
        sub = pkg.get("dl_dir", name)
        if not os.path.exists(os.path.join(dl, sub, src)):
            missing.append(f"{sub}/{src}")
print(f"[build] {len(info)} packages, {files} source files, {files - len(missing)} already in {dl}")
if missing:
    print(f"[build] a real build will download {len(missing)}:", ", ".join(missing[:12]) + (" ..." if len(missing) > 12 else ""))
PY
}


# ══════════════════════════════════════════════════════════════════════════════
#  Build
# ══════════════════════════════════════════════════════════════════════════════

verify_kernel() {
    local kc; kc=$(ls -t "$OUT"/build/linux-[0-9]*/.config 2>/dev/null | head -n1)
    [[ -n "$kc" ]] || die "no kernel .config found"
    local need=(CMDLINE_BOOL CMDLINE_OVERRIDE BLK_DEV_INITRD RD_ZSTD RD_XZ SND_SB16 VGA_CONSOLE
                NE2K_PCI ATA_PIIX KEYBOARD_ATKBD MOUSE_PS2 SERIO_I8042 VIRTIO_NET NET_9P_VIRTIO 9P_FS) s bad=()
    case "$MODE" in
        qemu) need+=(DRM_BOCHS) ;;
        nogui) local g
               for g in DRM FB FB_VESA FB_EFI SYSFB_SIMPLEFB DRM_SIMPLEDRM FRAMEBUFFER_CONSOLE; do
                   grep -qx "CONFIG_$g=y" "$kc" && die "--no-gui kernel has graphics (CONFIG_$g=y): v86 would leave text mode"
               done ;;
        vesa) need+=(FB_VESA BOOT_VESA_SUPPORT FRAMEBUFFER_CONSOLE X86_IOPL_IOPERM DEVMEM X86_LEGACY_VM86 INPUT_MOUSEDEV) ;;
    esac
    for s in "${need[@]}"; do grep -qx "CONFIG_$s=y" "$kc" || bad+=("$s"); done
    ((APIC)) || ! grep -qx 'CONFIG_X86_LOCAL_APIC=y' "$kc" || die "--no-apic, but the kernel still has CONFIG_X86_LOCAL_APIC=y"
    ((${#bad[@]} == 0)) || die "kernel is missing: ${bad[*]}"
    if [[ "$MODE" == vesa ]]; then
        grep -qx 'CONFIG_DRM_BOCHS=y' "$kc" && die "bochs-drm is built in: it would take the screen from vesafb"
        grep -q "u16 mode = $VID_MODE;" "$(dirname "$kc")/arch/x86/boot/video.c" \
            || die "kernel boot code was not patched to VESA mode $VID_MODE"
    fi
    grep -qx 'CONFIG_SMP=y' "$kc" && warn "CONFIG_SMP=y — v86 is single-CPU"
    msg "kernel config OK"
}

# The image's shellcheck (checked on its unpacked original): its libraries and glibc version are there, and (when
# this host can run 32-bit x86 programs, as WSL2 can) it runs and finds a bug.
verify_shellcheck() {
    local t="$OUT/target" bin="$SC_BIN" lib need out ok=1
    [[ -x "$t/usr/bin/shellcheck" ]] || { warn "shellcheck: not in the target"; return 1; }
    [[ -x "$bin" ]] || { warn "shellcheck: $bin is gone"; return 1; }   # the unpacked original
    for lib in $(readelf -d "$bin" | sed -n 's/.*(NEEDED).*\[\(.*\)\]/\1/p'); do
        [[ -e "$t/lib/$lib" || -e "$t/usr/lib/$lib" ]] || { warn "shellcheck: needs $lib, which isn't in the image"; ok=0; }
    done
    need=$(readelf -V "$bin" | grep -o 'GLIBC_[0-9.]*' | sort -uV | tail -n1)
    # (the whole output first: with pipefail, grep -q stopping early fails readelf with SIGPIPE)
    if [[ -n "$need" && " $(readelf -V "$t/lib/libc.so.6" | grep -o 'GLIBC_[0-9.]*' | tr '\n' ' ') " != *" $need "* ]]; then
        warn "shellcheck: needs $need, newer than the image's glibc"; ok=0
    fi
    ((ok)) || return 1
    printf '#!/bin/sh\necho $1\n' > "$OUT/sc-test.sh"
    if out=$("$t/lib/ld-linux.so.2" --library-path "$t/lib:$t/usr/lib" "$bin" "$OUT/sc-test.sh" 2>&1); [[ "$out" == *SC2086* ]]; then
        msg "shellcheck: runs and finds SC2086 in a test script ($("$t/lib/ld-linux.so.2" --library-path "$t/lib:$t/usr/lib" "$bin" --version | sed -n 's/^version: //p'))"
    else
        msg "shellcheck: libraries OK; not run here (this host can't run 32-bit programs, or: ${out:0:120})"
    fi
    rm -f "$OUT/sc-test.sh"
}

verify_image() {
    local img="$OUT/images/rootfs.cpio.$ROOTFS_EXT" z list f ok=1 mb
    z="$OUT/host/bin/$( ((XZ)) && echo xz || echo zstd)"; [[ -x "$z" ]] || z=${z##*/}
    list=$("$z" -dc "$img" | cpio -it 2>/dev/null) || die "cannot read $img"
    local want=('usr/bin/nano' 'usr/share/nano/c\.nanorc' 'usr/bin/speaker-test' 'usr/bin/amixer'
                'etc/asound\.conf' 'etc/init\.d/S31sound')
    ((GCC)) && want+=('usr/bin/gcc' 'usr/libexec/gcc/[^/]+/[^/]+/cc1' 'usr/include/stdio\.h' 'usr/lib/crt1\.o' 'usr/lib/libc\.so')
    ((SDL)) && want+=('usr/lib/libSDL3\.so')
    ((SDL && GCC)) && want+=('usr/include/SDL3/SDL\.h')
    ((MIN)) && want+=('etc/fonts/conf\.d/99-v86-min\.conf')
    ((SDLNET)) && want+=('usr/lib/libSDL3_net\.so')
    ((SDLNET && GCC)) && want+=('usr/include/SDL3_net/SDL_net\.h')
    ((MAN)) && want+=('usr/bin/mandoc' 'usr/bin/man' 'usr/bin/apropos' 'usr/bin/less'
                      'usr/share/man/mandoc\.db' 'usr/share/man/man3/printf\.3' 'usr/share/man/man2/open\.2')
    ((RELEASE)) || want+=('usr/bin/selftest' 'usr/bin/soundtest' 'usr/bin/unitest')
    ((RELEASE || !GUI)) || want+=('usr/bin/emojitest' 'usr/bin/symboltest' 'usr/share/fonts/twemoji/v86-ranges\.txt' 'usr/share/fonts/unifont/v86-symbols\.txt')
    ((RELEASE || !GCC)) || want+=('usr/bin/devcheck')
    ((GUI && ADIE)) && want+=('usr/bin/adie' 'usr/share/adie/Adie\.stx' 'etc/foxrc/Adie')
    ((GUI && MLTERM)) && want+=('usr/bin/mlterm' 'usr/lib/mlterm/libtype_cairo\.so' 'usr/lib/mlterm/libotl\.so' 'root/\.mlterm/aafont'
                                'etc/fonts/conf\.d/72-v86-mlterm-fallback\.conf')
    ((GUI)) && want+=('usr/bin/fbterm' 'root/\.fbtermrc' 'usr/share/fonts/unifont/unifont\.otf' 'usr/share/fonts/unifont/unifont_upper\.otf'
                      'usr/share/fonts/twemoji/Twemoji\.Mozilla\.ttf'
                      'usr/bin/emojistyle' 'etc/fonts/conf\.d/71-v86-misc-fixed\.conf'
                      'etc/fonts/conf\.avail/66-v86-colour-emoji\.conf' 'etc/fonts/conf\.avail/65-v86-mono-emoji\.conf'
                      'etc/fonts/fbterm\.conf')
    ((SHELLCHECK)) && want+=('usr/bin/shellcheck')
    ((TCC)) && want+=('usr/bin/tcc' 'usr/lib/tcc/libtcc1\.a' 'usr/lib/tcc/bt-exe\.o' 'usr/lib/tcc/include/stdarg\.h')
    ((UPX)) && want+=('usr/bin/upx')
    ((WINCC)) && want+=('usr/bin/i386-win32-tcc' 'usr/bin/x86_64-win32-tcc' 'usr/bin/wincc'
                        'usr/lib/tcc/win32/lib/i386-win32-libtcc1\.a' 'usr/lib/tcc/win32/lib/x86_64-win32-libtcc1\.a'
                        'usr/lib/tcc/win32/lib/kernel32\.def' 'usr/lib/tcc/win32/include/winapi/windows\.h' 'usr/lib/tcc/win32/include/stdarg\.h')
    ((GUI)) && want+=('usr/bin/Xorg' 'usr/bin/fluxbox' 'usr/bin/xterm' 'usr/bin/xfe' 'usr/share/fluxbox/styles/V86-Dark'
                      "usr/lib/xorg/modules/drivers/$([[ $MODE == qemu ]] && echo modesetting || echo fbdev)_drv\.so")
    [[ "$MODE" == vesa ]] && want+=('usr/lib/xorg/modules/drivers/vesa_drv\.so' 'usr/bin/xmode' 'usr/bin/Xvesa')
    for f in "${want[@]}"; do
        grep -qE "(^|/)$f\$" <<<"$list" || { warn "missing from the image: /${f//\\/}"; ok=0; }
    done
    grep -qE '(^|/)etc/init\.d/S40xorg$' <<<"$list" && { warn "S40xorg is in the image"; ok=0; }
    for f in UNICODE_SUPPORT FEATURE_CHECK_UNICODE_IN_ENV; do
        grep -qx "CONFIG_$f=y" "$OUT"/build/busybox-*/.config || { warn "BusyBox built without CONFIG_$f"; ok=0; }
    done
    ((RELEASE)) && grep -qE '(^|/)usr/bin/(selftest|devcheck|soundtest|unitest|emojitest|symboltest)$' <<<"$list" && { warn "--release image contains test programs"; ok=0; }
    ((GUI && !MLTERM)) && grep -qE '(^|/)(usr/bin/mlterm|usr/lib/libcairo\.so[.0-9]*|usr/lib/libharfbuzz\.so[.0-9]*)$' <<<"$list" && { warn "--no-mlterm image contains mlterm, cairo or HarfBuzz"; ok=0; }
    ((SDL)) || ! grep -qE '(^|/)usr/lib/libSDL3\.so' <<<"$list" || { warn "--no-SDL3 image contains SDL3"; ok=0; }
    ((MAN)) || ! grep -qE '(^|/)usr/share/man/' <<<"$list" || { warn "--no-man image contains man pages"; ok=0; }
    ((GCC)) || ! grep -qE '(^|/)usr/(bin/(gcc|g\+\+|as|ld)|include/stdio\.h)$' <<<"$list" || { warn "--no-gcc image contains the compiler or headers"; ok=0; }
    ((GCC)) || ! grep -qE '(^|/)usr/share/misc/magic\.mgc$' <<<"$list" || { warn "--no-gcc image still has file's magic database"; ok=0; }
    ((SHELLCHECK)) || ! grep -qE '(^|/)usr/bin/shellcheck$' <<<"$list" || { warn "--no-shellcheck image contains shellcheck"; ok=0; }
    ((SHELLCHECK)) && { verify_shellcheck || ok=0; }
    ((TCC)) || ! grep -qE '(^|/)usr/bin/tcc$' <<<"$list" || { warn "image without --tcc contains tcc"; ok=0; }
    ((WINCC)) || ! grep -qE '(^|/)usr/bin/(wincc|i386-win32-tcc|x86_64-win32-tcc)$' <<<"$list" || { warn "image without --wincc contains wincc"; ok=0; }
    ((UPX)) || ! grep -qE '(^|/)usr/bin/upx$' <<<"$list" || { warn "image without --upx contains upx"; ok=0; }
    ((GUI && ADIE)) || ! grep -qE '(^|/)usr/bin/adie$' <<<"$list" || { warn "image without Adie contains adie"; ok=0; }
    ((ok)) || die "the packed image is incomplete (see above)"
    mb=$(( $("$z" -dc "$img" | wc -c) / 1048576 ))
    UNPACKED_MB=$mb
    ((mb < 640)) || warn "rootfs unpacks to $mb MB: give v86 more than 1024 MB memory_size"
    msg "image OK: $mb MB unpacked, $(du -h "$img" | cut -f1) compressed"
}

# ccache's own statistics add up over every run; report just this one by keeping the
# previous totals next to the cache.
ccache_report() {
    local cc="$OUT/host/bin/ccache" f="$WORK/ccache/.v86-last-stats" stats h m ph=0 pm=0 dh dm
    [[ "$USE_CCACHE" == 1 ]] || { warn "ccache is off (USE_CCACHE=$USE_CCACHE)"; return 0; }
    [[ -x "$cc" ]] || return 0
    stats=$(CCACHE_DIR="$WORK/ccache" "$cc" --print-stats 2>/dev/null) || return 0
    h=$(awk '$1 == "direct_cache_hit" || $1 == "preprocessed_cache_hit" { s += $2 } END { print s + 0 }' <<<"$stats")
    m=$(awk '$1 == "cache_miss" { s += $2 } END { print s + 0 }' <<<"$stats")
    [[ -f "$f" ]] && read -r ph pm < "$f"
    printf '%s %s\n' "$h" "$m" > "$f"
    dh=$((h - ph)) dm=$((m - pm))
    ((dh >= 0 && dm >= 0)) || { dh=$h dm=$m; }          # stats were reset in between
    msg "ccache this run: $dh hits, $dm misses ($(( dh + dm ? dh * 100 / (dh + dm) : 0 ))% hit rate), cache $(du -sh "$WORK/ccache" 2>/dev/null | cut -f1)"
}

# ── host-tool cache ──────────────────────────────────────────────────────────
# Stage 0 builds the toolchain and every host package (and whatever they depend on)
# before any target package. That state is saved, and restored on the next fresh build
# of the same environment if nothing it was built from changed: the key covers each of
# those packages' name + version, every non-package option (toolchain, optimisation,
# hardening, ccache...) plus those packages' own options, and the output path
# (Buildroot bakes absolute paths into host/). Kept: host/ (tools, cross gcc, staging),
# target/ (skeleton + C library at that point) and the build stamps of those packages.

# prints: <key> <space-separated stage-0 package names>
host_stage_info() {
    br -s show-info > "$BUILD/show-info.json" 2>/dev/null || return 1
    python3 -I - "$BUILD/show-info.json" "$OUT/.config" "$BR_VER" "$OUT" <<'PYEOF'
import hashlib, json, re, sys
info = json.load(open(sys.argv[1]))
roots = sorted(n for n, p in info.items() if p.get("type") == "host")
if "toolchain" in info:
    roots.append("toolchain")
seen, todo = set(), list(roots)
while todo:
    n = todo.pop()
    if n in seen or n not in info:
        continue
    seen.add(n)
    todo += info[n].get("dependencies", [])
own = tuple("BR2_PACKAGE_" + n.upper().replace("-", "_") for n in seen)
later = re.compile(r"BR2_(DL_DIR|WGET|PRIMARY_SITE|BACKUP_SITE|ROOTFS_|TARGET_GENERIC_|TARGET_ROOTFS_|"
                   r"LINUX_KERNEL|JLEVEL|CCACHE_DIR|DEFCONFIG|EXTERNAL)")
opts = []
for line in open(sys.argv[2]).read().splitlines():
    if not line.startswith("BR2_"):
        continue
    if line.startswith("BR2_PACKAGE_"):
        if line.split("=", 1)[0].startswith(own):
            opts.append(line)
    elif not later.match(line):
        opts.append(line)
h = hashlib.sha256()
for x in ["v86-host-cache-1", sys.argv[3], sys.argv[4]] + \
         sorted(f"{n}={info[n].get('version')}" for n in seen) + sorted(opts):
    h.update((x + "\n").encode())
print(h.hexdigest()[:16], " ".join(roots))
PYEOF
}

host_stage() {
    local key pkgs snap
    read -r key pkgs < <(host_stage_info) || { warn "could not list host packages; skipping the host-tool cache"; return 0; }
    snap="$HOST_CACHE_DIR/$key"
    if [[ "$HOST_CACHE" == 1 && "$INCR" != 1 && -d "$snap" ]]; then
        msg "host-tool cache hit ($key): restoring toolchain + host tools from $snap"
        cp -a "$snap/." "$OUT/"
        touch "$snap"                            # recently used: kept longest
    fi
    local note=""
    [[ "$HOST_CACHE" == 1 && -d "$snap" ]] && note=" (from the host-tool cache)"
    step "Stage 0: toolchain + $(wc -w <<<"$pkgs") host packages$note"
    # shellcheck disable=SC2086
    br -j"$JOBS" $pkgs
    if [[ "$HOST_CACHE" == 1 && ! -d "$snap" ]]; then
        msg "saving toolchain + host tools to the host-tool cache ($key)"
        rm -rf "$snap.part"; mkdir -p "$snap.part/build"
        cp -a "$OUT/host" "$OUT/target" "$snap.part/"
        rsync -a -m --include='*/' --include='.stamp_*' --include='.files-list*.txt' --exclude='*' \
            "$OUT/build/" "$snap.part/build/"
        mv "$snap.part" "$snap"
        # keep the 4 most recently used snapshots; drop older ones and the per-image
        # snapshots of earlier script versions (built in other folders: never reusable)
        local old keep=0
        while IFS= read -r old; do
            [[ -d "$old" ]] || continue
            if [[ "${old##*/}" =~ ^[0-9a-f]{16}$ ]] && ((keep < 4)); then keep=$((keep + 1)); continue; fi
            rm -rf -- "$old"
        done < <(ls -1dt "$HOST_CACHE_DIR"/*/ 2>/dev/null | sed 's|/$||')
        msg "host-tool cache: saved $(du -sh "$snap" | cut -f1); shared by every image with the same toolchain + host tools"
    fi
}

build() {
    step "Downloading sources into $DL_DIR"
    retry "$DL_RETRIES" br source || die "downloads keep failing — check DNS/network and re-run (finished files are kept)"

    host_stage

    if [[ "$MODE" == smoke ]]; then
        step "Smoke build: prebuilt toolchain + BusyBox"
        local t0=$SECONDS
        br -j"$JOBS"
        [[ -s "$OUT/images/rootfs.cpio.zst" ]] || die "smoke build produced no rootfs"
        msg "smoke build OK in $((SECONDS - t0))s: $(du -h "$OUT/images/rootfs.cpio.zst" | cut -f1) rootfs"
        msg "downloads cached in $DL_DIR: $(find "$DL_DIR" -type f | wc -l) files, $(du -sh "$DL_DIR" | cut -f1)"
        ccache_report
        msg "run './build.sh --smoke' again: downloads are skipped and most compiles become cache hits"
        return 0
    fi

    step "Stage 1/2: toolchain + kernel"
    local early=(linux)
    [[ "$MODE" == vesa ]] && early+=(tinyx)                  # TinyX fails fast here, not hours in
    br -j"$JOBS" "${early[@]}"
    verify_kernel

    step "Stage 2/2: everything else (native gcc is the long part)"
    br -j"$JOBS"
    verify_image
    ccache_report
}

deliver() {
    local img="$OUT/images" vram=8 ksize rsize
    ((GUI)) && vram=16                  # room for xmode up to 1024x768x32
    cp -f "$img/bzImage"         "$IMAGE_DIR/v86-$NAME-bzImage"
    # Delivered as .zstd, not .zst: v86 decompresses any URL ending in ".zst" in the
    # browser and needs its unpacked size; without one, the guest gets an empty initrd
    # ("VFS: Unable to mount root fs"). The kernel unpacks zstd itself, so v86 shouldn't.
    # xz: .cpio.xz, which v86 passes through untouched.
    local rootfs="v86-$NAME-rootfs.cpio.zstd"
    ((XZ)) && rootfs="v86-$NAME-rootfs.cpio.xz"
    rm -f "$IMAGE_DIR/v86-$NAME-rootfs.cpio.zst"
    cp -f "$img/rootfs.cpio.$ROOTFS_EXT" "$IMAGE_DIR/$rootfs"
    if [[ -n "${V86_ALL_REPORT:-}" ]]; then         # run by --all: report the files for its list
        stat -c '%s %n' "$IMAGE_DIR/v86-$NAME-bzImage" "$IMAGE_DIR/$rootfs" >> "$V86_ALL_REPORT"
    fi
    # RAM: v86 loads the initrd at 64 MB; while unpacking, the kernel holds the compressed
    # file, the unpacked rootfs (it stays in RAM) and the decompressor's window (xz 32 MB,
    # zstd 8 MB). Then room for programs: 64 MB console, 128 MB with X. Rounded up to 64 MB.
    local comp mem
    comp=$(( ($(stat -c %s "$img/rootfs.cpio.$ROOTFS_EXT") + 1048575) / 1048576 ))
    mem=$(( ${UNPACKED_MB:-200} + comp + 24 + ( XZ ? 32 : 8 ) + ( GUI ? 128 : 64 ) ))
    (( mem < 64 + comp + 32 )) && mem=$(( 64 + comp + 32 ))
    mem=$(( (mem + 63) / 64 * 64 ))
    step "Done in $((SECONDS / 60))m$((SECONDS % 60))s"
    ls -lh "$IMAGE_DIR/v86-$NAME-bzImage" "$IMAGE_DIR/$rootfs"
    # The page's V86 settings for this image, with each file's size in bytes: v86 uses it
    # for its download progress when the server doesn't send one. Only printed: IMAGE_DIR
    # gets just the two images.
    ksize=$(stat -c %s "$IMAGE_DIR/v86-$NAME-bzImage") rsize=$(stat -c %s "$IMAGE_DIR/$rootfs")
    echo
    cat <<EOF
      var emulator = window.emulator = new V86({
          wasm_path: "/v86.wasm",
          memory_size: $mem * 1024 * 1024,      // suggested from the image size; more for big programs
          vga_memory_size: $vram * 1024 * 1024,      //8-16MB
          screen_container: document.getElementById("screen_container"),
          bios:     { url: "/bios/seabios.bin" },
          vga_bios: { url: "/bios/vgabios.bin" },   // required: the VESA boot mode comes from this BIOS
          bzimage:  { url: "${IMAGE_URL}v86-$NAME-bzImage", size: $ksize },
          initrd:   { url: "${IMAGE_URL}$rootfs", size: $rsize },$( ((XZ)) || echo '   // keep .zstd, not .zst: see build.sh')
          net_devices: [
              { type: "ne2k",   relay_url: "wss://relay.widgetry.org/" },  // internet (DHCP from the relay)
              { type: "virtio", relay_url: "inbrowser" },                  // other v86 pages in this browser
          ],
          filesystem: {
              basefs: "",
              baseurl: "",
          },
          serial_console: {
              type: "xtermjs",
              container: document.getElementById("serial_terminal"),
              xterm_lib: V86GraphemeTerminal,
          },
          autostart: false,
      });
EOF
    msg "sizes are in bytes: update them if you rebuild the files"
    cat <<EOF

In the guest: $( ((RELEASE)) || echo 'selftest · soundtest · unitest · ')$( ((RELEASE || !GUI)) || echo 'emojitest · symboltest · ')$( ((RELEASE || !GCC)) || echo 'devcheck · ')netup/netdown$( ((SHELLCHECK)) && echo ' · shellcheck')$( ((TCC)) && echo ' · tcc')$( ((WINCC)) && echo ' · wincc')$( ((UPX)) && echo ' · upx')$( ((GUI)) && echo ' · xlog · xmode')$( ((MAN)) && echo ' · man/apropos')
$( ((SDL && GCC)) && echo 'SDL3: #include <SDL3/SDL.h>   gcc app.c $(pkg-config --cflags --libs sdl3)')$( ((SDLNET && GCC)) && echo '
SDL3_net: #include <SDL3_net/SDL_net.h>   gcc app.c $(pkg-config --cflags --libs sdl3-net)')
EOF
}


# ══════════════════════════════════════════════════════════════════════════════
#  Main
# ══════════════════════════════════════════════════════════════════════════════

preflight
if ((DRY)); then
    rm -rf "$BUILD"; mkdir -p "$BUILD"
    have_br=0; get_buildroot && have_br=1
    get_shellcheck                                # uses a cached package; never downloads
    write_tree
    lint_tree
    if ((have_br)); then
        configure
        report_downloads
    else
        warn "no Buildroot tree or cached tarball: skipped the Kconfig checks (set BR_SRC, or run a real build once)"
    fi
    step "Dry run OK: $NAME"
    msg "generated files: $EXT"
    exit 0
fi

# one build at a time in the shared folder
mkdir -p "$WORK" "$WORK/logs"
if have flock; then
    exec 9>"$WORK/build.lock"
    flock -n 9 || die "another build.sh is running in $WORK/build (wait for it, or use another WORK=)"
fi
last=$(cat "$BUILD/.v86-image" 2>/dev/null || true)
if ((INCR)); then
    [[ "$last" == "$NAME" ]] || die "--incremental: the last build in $BUILD was '${last:-none}', not '$NAME'; build without --incremental"
fi
: > "$BUILD_LOG"
exec > >(tee -a "$BUILD_LOG") 2>&1
get_buildroot
get_shellcheck

# Automatic reuse: same image, previous build finished, and the generated package/config
# files identical to last time -> keep the build folder. Buildroot then only reruns
# target-finalize (overlay, post-build) and the image packing. The old generated files
# are kept with their timestamps (the kernel reconfigures when its fragment looks newer);
# only the overlay and post-build script are taken from this run.
tree_fingerprint() {
    (cd "$1" && find configs package patches external.mk external.desc Config.in linux.fragment busybox.fragment -type f 2>/dev/null |
        LC_ALL=C sort | xargs sha256sum; echo "$BR_VER $NAME") | sha256sum | cut -c1-16
}
if ((!INCR && !FRESH)) && [[ "$last" == "$NAME" && -f "$BUILD/.v86-complete" && -d "$EXT" && "$MODE" != smoke ]]; then
    rm -rf "$EXT.prev"; mv "$EXT" "$EXT.prev"
    write_tree
    if [[ "$(tree_fingerprint "$EXT")" == "$(tree_fingerprint "$EXT.prev")" ]]; then
        # overlay files that existed last time but not now: remove them from the target
        (cd "$EXT.prev/overlay" && find . -type f) | while IFS= read -r f; do
            [[ -e "$EXT/overlay/$f" ]] || rm -f "$OUT/target/$f"
        done
        rm -rf "$EXT.prev/overlay" "$EXT.prev/post-build.sh"
        mv "$EXT/overlay" "$EXT/post-build.sh" "$EXT.prev/"
        rm -rf "$EXT"; mv "$EXT.prev" "$EXT"
        REUSE=1 INCR=1
        msg "reusing the previous $NAME build: same packages and settings (--fresh rebuilds everything)"
    else
        msg "packages or settings changed since the last $NAME build: building fresh"
        rm -rf "$EXT.prev"
    fi
fi
((INCR)) || rm -rf "$BUILD"                     # fresh: only the caches survive
mkdir -p "$BUILD"
rm -f "$BUILD/.v86-complete"
echo "$NAME" > "$BUILD/.v86-image"
((REUSE)) || write_tree
lint_tree
configure
build
[[ "$MODE" == smoke ]] || deliver
touch "$BUILD/.v86-complete"
