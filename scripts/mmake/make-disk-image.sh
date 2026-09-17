#!/usr/bin/env bash
set -euo pipefail
[[ $# -eq 2 ]] || exit 2
root=$1; mmake_out=$2; config=$root/.config; image=$mmake_out/image/disk.img; temporary=$image.new
layout_state=$image.layout
value() { sed -n "s/^$1=//p" "$config" | tr -d '"' | tail -n1; }
disk=$(value IMAGE_DISK_SIZE_MB); esp=$(value IMAGE_ESP_SIZE_MB); rootfs=$((disk - esp - 2)); esp_sectors=$((esp * 2048)); root_start=$((2048 + esp_sectors)); root_sectors=$((rootfs * 2048))
layout="disk=$disk esp=$esp rootfs=$rootfs"

disk_layout_matches() {
    [[ -f $image ]] || return 1
    [[ $(stat -c %s "$image") -eq $((disk * 1024 * 1024)) ]] || return 1
    [[ $(sfdisk --part-start "$image" 1 2>/dev/null) -eq 2048 ]] || return 1
    [[ $(sfdisk --part-size "$image" 1 2>/dev/null) -eq $esp_sectors ]] || return 1
    [[ $(sfdisk --part-start "$image" 2 2>/dev/null) -eq $root_start ]] || return 1
    [[ $(sfdisk --part-size "$image" 2 2>/dev/null) -eq $root_sectors ]] || return 1
    [[ ! -f $layout_state || $(cat "$layout_state") == "$layout" ]]
}

if ! disk_layout_matches; then
    rm -f "$temporary"
    truncate -s "${disk}M" "$temporary"
    printf 'label: gpt\nunit: sectors\nfirst-lba: 2048\nsector-size: 512\n\n2048,%s,U,*\n%s,%s,L\n' "$esp_sectors" "$root_start" "$root_sectors" | sfdisk "$temporary" >/dev/null
    mv "$temporary" "$image"
    rm -f "$image.esp.state.json" "$image.rootfs.state.json"
fi

python3 "$root/scripts/mmake/patch-disk-partition.py" \
    "$mmake_out/image/esp.img" "$image" 1 "$esp" "$image.esp.state.json" \
    "$mmake_out/image/esp.img.dirty.json"
python3 "$root/scripts/mmake/patch-disk-partition.py" \
    "$mmake_out/image/rootfs.img" "$image" $((1 + esp)) "$rootfs" "$image.rootfs.state.json" \
    "$mmake_out/image/rootfs.img.dirty.json"

printf '%s\n' "$layout" > "$layout_state.new"
mv "$layout_state.new" "$layout_state"
