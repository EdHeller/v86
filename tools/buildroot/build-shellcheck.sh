#!/usr/bin/env bash
#
# build-shellcheck.sh — a 32-bit x86 ShellCheck for build.sh's images
#
# ShellCheck is written in Haskell: Buildroot can't build it, and upstream publishes no
# 32-bit x86 binary (v86 runs i686 only). Without this script build.sh puts Debian's i386
# package into the image (56 MB of RAM). This builds ShellCheck itself, smaller, and leaves it
# next to build.sh, where build.sh finds it and uses it instead (packed with upx when upx is
# installed on the host).
#
#   sudo ./build-shellcheck.sh             build (a first run takes a while: see below)
#   sudo ./build-shellcheck.sh --clean     delete the build system it made (keeps the binary)
#
# Writes shellcheck-<version>-i386 next to this script (older ones are deleted: build.sh takes
# the last one it finds). Measured: ShellCheck 0.11.0 (and 0.10.0) with GHC 9.8.4, 18 MB, needing
# only glibc and GMP; build.sh packs it with upx to 4.3 MB when upx is installed (Debian's: 56 MB).
# A first run takes ~30-60 minutes (the Debian system, GHC, ~40 Haskell libraries); later runs reuse
# them.
#
# How: a Debian i386 system in a folder (debootstrap), entered with chroot: an x86_64 Linux
# kernel runs i386 programs directly (WSL2 too), no emulator. In it, GHC's own i386 release
# (downloads.haskell.org, checked against its SHA256SUMS) and cabal build ShellCheck and every
# Haskell library it uses from Hackage's source, all with -split-sections (each function in its
# own section) so the linker keeps only what the program uses, then strip it. That is what makes
# it small. Not Debian's GHC: on i386 it is "unregisterised" (compiles through C, no object
# splitting), and everything built with it, Debian's own package too, stays ~56 MB.
# The C libraries it needs are the ones build.sh adds from Buildroot (GMP, libffi, libnuma);
# Debian 13's glibc (2.41) is older than the image's, which is what a program built elsewhere
# needs. The build system and cabal's cache stay for the next run (SC_WORK, default
# /var/tmp/v86-shellcheck, ~3 GB): only --clean removes them.
#
# Settings (environment):
#   SC_VERSION=0.10.0                     the ShellCheck release on Hackage (default: the newest)
#   SC_GHC=9.8.4                          GHC's release (an i386 Linux build of it must exist on
#                                         downloads.haskell.org; 9.8 is the newest Debian 13's
#                                         cabal 3.10 works with)
#   SC_SUITE=trixie                       the Debian release (its glibc must not be newer than
#                                         the image's: Buildroot 2026.08 has 2.44)
#   SC_MIRROR=https://deb.debian.org/debian
#   SC_WORK=/var/tmp/v86-shellcheck       where the build system lives (Linux filesystem, not /mnt/c)
#   SC_EXTRA_CA=/path/ca.crt              a CA certificate apt must trust (an HTTPS-inspecting proxy);
#                                         http_proxy / https_proxy / no_proxy are passed through.
#                                         (sudo drops them: sudo -E, or sudo env https_proxy=... ./build-shellcheck.sh)
#
# Needs: root (debootstrap, chroot, mount), debootstrap, an x86_64 or i686 Linux host, and
# access to deb.debian.org, downloads.haskell.org and hackage.haskell.org.
#   sudo apt-get install debootstrap
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
SC_SUITE=${SC_SUITE:-trixie}
SC_MIRROR=${SC_MIRROR:-https://deb.debian.org/debian}
SC_WORK=${SC_WORK:-/var/tmp/v86-shellcheck}
SC_GHC=${SC_GHC:-9.8.4}
ROOT="$SC_WORK/$SC_SUITE-i386"

msg() { printf '\033[1;34m==>\033[0m %s\n' "$*"; }
die() { printf '\033[1;31merror:\033[0m %s\n' "$*" >&2; exit 1; }

[[ $EUID -eq 0 ]] || die "run it as root: sudo $0 $*"

if [[ "${1:-}" == --clean ]]; then
    for m in proc sys dev/pts dev; do mountpoint -q "$ROOT/$m" && umount -l "$ROOT/$m"; done
    rm -rf "$SC_WORK"; msg "removed $SC_WORK"; exit 0
fi
[[ $# -eq 0 ]] || die "unknown option $1 (only --clean)"

case "$(uname -m)" in x86_64|i686|i386) ;; *) die "needs an x86 Linux host (this is $(uname -m))" ;; esac
command -v debootstrap >/dev/null || die "debootstrap is missing: sudo apt-get install debootstrap"
case "$SC_WORK" in /mnt/*) die "SC_WORK=$SC_WORK is on a Windows drive; use the Linux filesystem" ;; esac

# ── the Debian i386 system ──────────────────────────────────────────────────────
if [[ ! -x "$ROOT/usr/bin/apt-get" ]]; then
    msg "Debian $SC_SUITE i386 into $ROOT (debootstrap)"
    rm -rf "$ROOT"; mkdir -p "$SC_WORK"
    debootstrap --arch=i386 --variant=minbase "$SC_SUITE" "$ROOT" "$SC_MIRROR"
fi
chroot "$ROOT" /bin/true 2>/dev/null || die "this kernel can't run 32-bit x86 programs (CONFIG_IA32_EMULATION)"

cleanup() { for m in dev/pts dev sys proc; do mountpoint -q "$ROOT/$m" && umount -l "$ROOT/$m"; done; true; }
trap cleanup EXIT
mountpoint -q "$ROOT/proc"    || mount -t proc proc "$ROOT/proc"
mountpoint -q "$ROOT/sys"     || mount -t sysfs sys "$ROOT/sys"
mountpoint -q "$ROOT/dev"     || mount --bind /dev "$ROOT/dev"
mountpoint -q "$ROOT/dev/pts" || mount --bind /dev/pts "$ROOT/dev/pts"
cp -f /etc/resolv.conf "$ROOT/etc/resolv.conf" 2>/dev/null || true

# a proxy (http_proxy, https_proxy, no_proxy) and its CA certificate (SC_EXTRA_CA=file.crt) reach apt
proxy_env=()
for v in http_proxy https_proxy no_proxy HTTP_PROXY HTTPS_PROXY NO_PROXY; do
    [[ -n "${!v:-}" ]] && proxy_env+=("$v=${!v}")
done
if [[ -n "${SC_EXTRA_CA:-}" ]]; then
    [[ -f "$SC_EXTRA_CA" ]] || die "SC_EXTRA_CA=$SC_EXTRA_CA: no such file"
    mkdir -p "$ROOT/usr/local/share/ca-certificates"
    cp -f "$SC_EXTRA_CA" "$ROOT/usr/local/share/ca-certificates/v86-extra-ca.crt"
    chroot "$ROOT" /usr/sbin/update-ca-certificates >/dev/null
fi
in_root() { chroot "$ROOT" /usr/bin/env -i HOME=/root LANG=C.UTF-8 PATH=/usr/sbin:/usr/bin:/sbin:/bin \
    DEBIAN_FRONTEND=noninteractive "${proxy_env[@]}" /bin/bash -euo pipefail -c "$1"; }

msg "cabal (Debian) and the C libraries the Haskell ones bind to"
in_root "
echo 'deb $SC_MIRROR $SC_SUITE main' > /etc/apt/sources.list
apt-get update -q
apt-get install -y -q --no-install-recommends cabal-install libgmp-dev libffi-dev libnuma-dev \
    zlib1g-dev libtinfo6 ca-certificates curl xz-utils make gcc g++ libc6-dev binutils file
"

# ── GHC: its own i386 release (registerised: real machine code, split sections) ─
if ! in_root "[ -x /opt/ghc-$SC_GHC/bin/ghc ]" 2>/dev/null; then
    msg "GHC $SC_GHC (i386 release from downloads.haskell.org)"
    in_root "
mkdir -p /opt/src && cd /opt/src
base=https://downloads.haskell.org/~ghc/$SC_GHC
curl -fsSL -o SHA256SUMS \$base/SHA256SUMS
f=''
for d in deb10 deb9; do
    grep -q \"ghc-$SC_GHC-i386-\$d-linux.tar.xz\$\" SHA256SUMS && { f=ghc-$SC_GHC-i386-\$d-linux.tar.xz; break; }
done
[ -n \"\$f\" ] || { echo 'no i386 Linux build of GHC $SC_GHC (SC_GHC=...)' >&2; exit 1; }
curl -fsSL -o \$f \$base/\$f
grep \" ./\$f\$\\| \$f\$\" SHA256SUMS | sed 's| \./| |' | sha256sum -c -
find . -mindepth 1 -maxdepth 1 -type d -exec rm -rf {} + && tar xJf \$f
cd \$(find . -maxdepth 1 -type d -name 'ghc-$SC_GHC*' | head -n1)
./configure --prefix=/opt/ghc-$SC_GHC >/dev/null
make install >/dev/null
cd / && rm -rf /opt/src
"
fi
# (registerised: native code generator, -split-sections works; Debian's i386 GHC isn't)
in_root "/opt/ghc-$SC_GHC/bin/ghc --info | grep -q '\"Unregisterised\",\"NO\"'" \
    || die "GHC $SC_GHC is unregisterised: -split-sections won't work and the program would stay big"

# ── ShellCheck and its libraries from Hackage, all with -split-sections ────────
msg "ShellCheck${SC_VERSION:+ $SC_VERSION} and its libraries from Hackage (a first run builds ~40 libraries)"
in_root "
export PATH=/opt/ghc-$SC_GHC/bin:\$PATH
cabal update -v1
rm -rf /build && mkdir -p /build && cd /build
cabal get -v1 ShellCheck${SC_VERSION:+-$SC_VERSION}
cd \$(find . -maxdepth 1 -type d -name 'ShellCheck-*' | head -n1)
# for every package, the libraries too (a command-line flag would reach only ShellCheck itself)
printf '%s\n' 'package *' '  split-sections: True' '  executable-stripping: True' \
    '  library-profiling: False' '  documentation: False' '  optimization: 1' \
    '  ghc-options: -split-sections' 'tests: False' 'benchmarks: False' > cabal.project.local
cabal build -v1 -w /opt/ghc-$SC_GHC/bin/ghc exe:shellcheck --ghc-options='-optl-Wl,--gc-sections'
strip --strip-all \$(cabal list-bin -w /opt/ghc-$SC_GHC/bin/ghc exe:shellcheck) -o /build/shellcheck
/build/shellcheck --version
file /build/shellcheck
"

# ── check it, copy it next to build.sh ─────────────────────────────────────────
ver=$(chroot "$ROOT" /build/shellcheck --version | sed -n 's/^version: *//p')
[[ "$ver" =~ ^[0-9][0-9.]*$ ]] || die "the new shellcheck doesn't report a version"
# a test: one warning (SC2086, unquoted variable) in a two-line script
# (it exits with 1 when it reports something: expected here)
printf '#!/bin/sh\necho $1\n' > "$ROOT/build/t.sh"
report=$(chroot "$ROOT" /build/shellcheck -f gcc /build/t.sh || true)
grep -q SC2086 <<<"$report" || die "the new shellcheck doesn't find the expected warning: $report"
# its libraries: glibc, and what build.sh adds from Buildroot (sc_libs in build.sh)
bad=$(readelf -d "$ROOT/build/shellcheck" | sed -n 's/.*(NEEDED).*\[\(.*\)\]/\1/p' \
    | grep -vxE 'libc\.so\.6|libm\.so\.6|libpthread\.so\.0|librt\.so\.1|libdl\.so\.2|libutil\.so\.1|ld-linux\.so\.2|libgmp\.so\.10|libffi\.so\.8|libnuma\.so\.1' || true)
[[ -z "$bad" ]] || die "the new shellcheck needs libraries build.sh can't add: $bad"

out="$SCRIPT_DIR/shellcheck-$ver-i386"
rm -f "$SCRIPT_DIR"/shellcheck-*-i386
cp "$ROOT/build/shellcheck" "$out"; chmod 755 "$out"
[[ -n "${SUDO_UID:-}" ]] && chown "$SUDO_UID:${SUDO_GID:-$SUDO_UID}" "$out"
msg "$out: ShellCheck $ver, $(( $(stat -c %s "$out") / 1048576 )) MB (Debian's package: 56 MB)"
msg "libraries: $(readelf -d "$out" | sed -n 's/.*(NEEDED).*\[\(.*\)\]/\1/p' | tr '\n' ' ')"
msg "build.sh uses it from now on; the build system stays in $SC_WORK (sudo $0 --clean removes it)"
