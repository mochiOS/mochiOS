#!/usr/bin/env bash
set -euo pipefail
[[ $# -eq 2 ]] || exit 2
root=$1; mmake_out=$2; config=$root/.config; image=$mmake_out/image/disk.img; temporary=$image.new
layout_state=$image.layout
value() { sed -n "s/^$1=//p" "$config" | tr -d '"' | tail -n1; }
disk=$(value IMAGE_DISK_SIZE_MB); esp=$(value IMAGE_ESP_SIZE_MB); rootfs=$((disk - esp - 2)); esp_sectors=$((esp * 2048)); root_start=$((2048 + esp_sectors)); root_sectors=$((rootfs * 2048))
layout="disk=$disk esp=$esp rootfs=$rootfs"

readonly MOCHIOS_ROOT_PARTITION_TYPE="6d6f6368-694f-5300-8000-6d5061727401"

partition_field() {
    local partition=$1 field=$2
    sfdisk -d "$image" 2>/dev/null | awk -v partition="$partition" -v field="$field" '
        $1 ~ partition "$" {
            for (i = 1; i <= NF; i++) {
                if ($i == field "=" && i < NF) {
                    value = $(i + 1)
                    gsub(/,$/, "", value)
                    print value
                    exit
                }
                if (index($i, field "=") == 1) {
                    value = substr($i, length(field) + 2)
                    gsub(/,$/, "", value)
                    print value
                    exit
                }
            }
        }
    '
}

disk_layout_matches() {
    [[ -f $image ]] || return 1
    [[ $(stat -c %s "$image") -eq $((disk * 1024 * 1024)) ]] || return 1
    [[ $(partition_field 1 start) -eq 2048 ]] || return 1
    [[ $(partition_field 1 size) -eq $esp_sectors ]] || return 1
    [[ $(partition_field 2 start) -eq $root_start ]] || return 1
    [[ $(partition_field 2 size) -eq $root_sectors ]] || return 1
    [[ ! -f $layout_state || $(cat "$layout_state") == "$layout" ]]
}

if ! disk_layout_matches; then
    rm -f "$temporary"
    truncate -s "${disk}M" "$temporary"
    printf 'label: gpt\nunit: sectors\nfirst-lba: 2048\nsector-size: 512\n\n2048,%s,U,*\n%s,%s,%s\n' "$esp_sectors" "$root_start" "$root_sectors" "$MOCHIOS_ROOT_PARTITION_TYPE" | sfdisk "$temporary" >/dev/null
    mv "$temporary" "$image"
    rm -f "$image.esp.state.json" "$image.rootfs.state.json"
fi

root_partition_type=$(partition_field 2 type)
if [[ ${root_partition_type,,} != "$MOCHIOS_ROOT_PARTITION_TYPE" ]]; then
    sfdisk --part-type "$image" 2 "$MOCHIOS_ROOT_PARTITION_TYPE" >/dev/null
fi

python3 "$root/scripts/mmake/patch-disk-partition.py" \
    "$mmake_out/image/esp.img" "$image" 1 "$esp" "$image.esp.state.json" \
    "$mmake_out/image/esp.img.dirty.json"
python3 "$root/scripts/mmake/patch-disk-partition.py" \
    "$mmake_out/image/rootfs.img" "$image" $((1 + esp)) "$rootfs" "$image.rootfs.state.json" \
    "$mmake_out/image/rootfs.img.dirty.json"

printf '%s\n' "$layout" > "$layout_state.new"
mv "$layout_state.new" "$layout_state"
