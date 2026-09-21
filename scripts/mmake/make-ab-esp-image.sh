#!/usr/bin/env bash
set -euo pipefail
[[ $# -eq 2 ]] || { echo "usage: $0 <root> <mmake-out>" >&2; exit 2; }
root=$1
mmake_out=$2
image=$mmake_out/image/ab-esp.img
temporary=$image.new
bootloader=$mmake_out/components/BOOTX64.EFI
# Some UEFI implementations refuse undersized FAT32 volumes even when the
# filesystem itself is readable. Keep a standards-friendly 64 MiB loader ESP.
size_mb=64
mkdir -p "$(dirname "$image")"
rm -f -- "$temporary"
trap 'rm -f -- "$temporary"' EXIT
truncate -s "${size_mb}M" "$temporary"
mkfs.fat -F 32 -n EFI "$temporary" >/dev/null
export MTOOLS_SKIP_CHECK=1
mmd -i "$temporary" ::/EFI ::/EFI/BOOT
mcopy -o -i "$temporary" "$bootloader" ::/EFI/BOOT/BOOTX64.EFI
mv -- "$temporary" "$image"
