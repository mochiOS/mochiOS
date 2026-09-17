#!/usr/bin/env bash
set -euo pipefail
[[ $# -eq 2 ]] || exit 2
root=$1; mmake_out=$2; image=$mmake_out/image/esp.img; temporary=$image.new
size=$(sed -n 's/^IMAGE_ESP_SIZE_MB=//p' "$root/.config" | tr -d '"' | tail -n1)
export MTOOLS_SKIP_CHECK=1
if [[ ! -f $image || $(stat -c %s "$image") -ne $((size * 1024 * 1024)) ]]; then
    rm -f "$temporary"
    truncate -s "${size}M" "$temporary"
    mkfs.fat -F 32 -n EFI "$temporary" >/dev/null
    mmd -i "$temporary" ::/EFI ::/EFI/BOOT ::/system
    mv "$temporary" "$image"
    rm -f "$image".*.stamp
fi

update_file() {
    local source=$1 destination=$2 name=$3
    local stamp state="$image.$name.stamp" previous=""
    stamp=$(stat -c '%d:%i:%s:%y' "$source")
    [[ -f $state ]] && previous=$(cat "$state")
    [[ $stamp == "$previous" ]] && return
    mcopy -o -i "$image" "$source" "$destination"
    printf '%s\n' "$stamp" > "$state.new"
    mv "$state.new" "$state"
}

update_file "$mmake_out/components/BOOTX64.EFI" ::/EFI/BOOT/BOOTX64.EFI bootloader
update_file "$mmake_out/components/kernel.elf" ::/system/kernel.elf kernel
update_file "$mmake_out/components/kernel.meta" ::/system/kernel.meta kernel-meta
update_file "$mmake_out/image/initfs.img" ::/system/initfs.img initfs
