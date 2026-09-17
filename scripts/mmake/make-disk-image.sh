#!/usr/bin/env bash
set -euo pipefail
[[ $# -eq 2 ]] || exit 2
root=$1; mmake_out=$2; config=$root/.config; image=$mmake_out/image/disk.img; temporary=$image.new
value() { sed -n "s/^$1=//p" "$config" | tr -d '"' | tail -n1; }
disk=$(value IMAGE_DISK_SIZE_MB); esp=$(value IMAGE_ESP_SIZE_MB); rootfs=$((disk - esp - 2)); esp_sectors=$((esp * 2048)); root_start=$((2048 + esp_sectors)); root_sectors=$((rootfs * 2048))
rm -f "$temporary"; truncate -s "${disk}M" "$temporary"
printf 'label: gpt\nunit: sectors\nfirst-lba: 2048\nsector-size: 512\n\n2048,%s,U,*\n%s,%s,L\n' "$esp_sectors" "$root_start" "$root_sectors" | sfdisk "$temporary" >/dev/null

copy_sparse_partition() {
    local source=$1 offset_mib=$2
    if ! command -v filefrag >/dev/null 2>&1; then
        dd if="$source" of="$temporary" bs=1M seek="$offset_mib" conv=notrunc,sparse status=none
        return
    fi

    local offset_blocks=$((offset_mib * 256)) logical blocks
    while read -r logical blocks; do
        dd if="$source" of="$temporary" bs=4096 skip="$logical" \
            seek=$((offset_blocks + logical)) count="$blocks" \
            iflag=fullblock conv=notrunc status=none
    done < <(
        filefrag -e -b4096 "$source" |
            awk '$1 ~ /^[0-9]+:$/ && $0 !~ /unwritten/ {
                gsub(/\.\./, "", $2)
                gsub(/:/, "", $6)
                print $2, $6
            }'
    )
}

copy_sparse_partition "$mmake_out/image/esp.img" 1
copy_sparse_partition "$mmake_out/image/rootfs.img" $((1 + esp))
mv "$temporary" "$image"
