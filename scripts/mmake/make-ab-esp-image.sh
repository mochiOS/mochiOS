#!/usr/bin/env bash
set -euo pipefail
[[ $# -eq 2 ]] || { echo "usage: $0 <root> <mmake-out>" >&2; exit 2; }
root=$1
mmake_out=$2
image=$mmake_out/image/ab-esp.img
temporary=$image.new
initfs=$mmake_out/image/initfs.img
kernel=$mmake_out/components/kernel.elf
kernel_meta=$mmake_out/components/kernel.meta
bootloader=$mmake_out/components/BOOTX64.EFI
size_mb=256

[[ $(stat -c %s "$initfs") -le $((110 * 1048576)) ]] || {
    echo "A/B ESP cannot hold two initfs images larger than 110 MiB" >&2; exit 1;
}
mkdir -p "$(dirname "$image")"
rm -f -- "$temporary"
trap 'rm -f -- "$temporary"' EXIT
truncate -s "${size_mb}M" "$temporary"
mkfs.fat -F 32 -n EFI "$temporary" >/dev/null
export MTOOLS_SKIP_CHECK=1
mmd -i "$temporary" ::/EFI ::/EFI/BOOT ::/slots ::/slots/A ::/slots/B
mcopy -o -i "$temporary" "$bootloader" ::/EFI/BOOT/BOOTX64.EFI
for slot in A B; do
    mcopy -o -i "$temporary" "$kernel" "::/slots/$slot/kernel.elf"
    mcopy -o -i "$temporary" "$kernel_meta" "::/slots/$slot/kernel.meta"
    mcopy -o -i "$temporary" "$initfs" "::/slots/$slot/initfs.img"
done
mv -- "$temporary" "$image"
