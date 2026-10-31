#!/bin/sh
# Fetches the UEFI firmware of the ARM core's virt machine (docs/arm64.md, "UEFI"): QEMU 9.2.0's prebuilt edk2
# ArmVirtQemu (edk2-stable202408), pc-bios/edk2-aarch64-code.fd and edk2-arm-vars.fd, both padded to 64 MiB there.
# v86 keeps the firmware volume (the first 2 MiB) and the variable store (the first 768 KiB); the rest is padding.
set -e
cd "$(dirname "$0")"
base=https://raw.githubusercontent.com/qemu/qemu/v9.2.0/pc-bios
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
curl -sSfL -o "$tmp/code.bz2" $base/edk2-aarch64-code.fd.bz2
curl -sSfL -o "$tmp/vars.bz2" $base/edk2-arm-vars.fd.bz2
curl -sSfL -o AAVMF-LICENSES.txt $base/edk2-licenses.txt
bunzip2 -c "$tmp/code.bz2" | head -c 2097152 > aavmf-code.fd
bunzip2 -c "$tmp/vars.bz2" | head -c 786432 > aavmf-vars.fd
sha256sum -c <<SUMS
49d6f4b3cb4b15b77705b0758422f2cc7e2b98effcf94bfe6a4daa6bef8acb82  aavmf-code.fd
8b634c1e6bd11607850b69111f6c4dbd1583dbbd1460dac72dbacbdc1a4a130a  aavmf-vars.fd
SUMS
