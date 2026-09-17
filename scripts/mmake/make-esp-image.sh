#!/usr/bin/env bash
set -euo pipefail
[[ $# -eq 2 ]] || exit 2
root=$1; mmake_out=$2; image=$mmake_out/image/esp.img; temporary=$image.new
size=$(sed -n 's/^IMAGE_ESP_SIZE_MB=//p' "$root/.config" | tr -d '"' | tail -n1)
rm -f "$temporary"; truncate -s "${size}M" "$temporary"; mkfs.fat -F 32 -n EFI "$temporary" >/dev/null
export MTOOLS_SKIP_CHECK=1
mmd -i "$temporary" ::/EFI ::/EFI/BOOT ::/system
mcopy -i "$temporary" "$mmake_out/components/BOOTX64.EFI" ::/EFI/BOOT/BOOTX64.EFI
mcopy -i "$temporary" "$mmake_out/components/kernel.elf" ::/system/kernel.elf
mcopy -i "$temporary" "$mmake_out/components/kernel.meta" ::/system/kernel.meta
mcopy -i "$temporary" "$mmake_out/image/initfs.img" ::/system/initfs.img
mv "$temporary" "$image"
