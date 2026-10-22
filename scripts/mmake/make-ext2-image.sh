#!/usr/bin/env bash
set -euo pipefail
[[ $# -eq 4 ]] || exit 2
stage=$1; image=$2; kind=$3; config=$4
value() { sed -n "s/^$1=//p" "$config" | tr -d '"' | tail -n1; }
if [[ $kind == initfs ]]; then
    size=$(value IMAGE_INITFS_SIZE_MB)
    block=1024
else
    disk=$(value IMAGE_DISK_SIZE_MB)
    esp=$(value IMAGE_ESP_SIZE_MB)
    data=$(value IMAGE_DATA_SIZE_MB)
    size=$((disk - esp - data - 2))
    block=4096
    (( size > 0 )) || { echo "disk image is too small for ESP and Data partitions" >&2; exit 1; }
fi
state=$image.state.json
mkdir -p "$(dirname "$image")"
image_matches_size() {
    if [[ $kind == initfs ]]; then
        perl "$(dirname "$0")/pack-boot-module.pl" check "$image" "$((size * 1024 * 1024))"
    else
        [[ $(stat -c %s "$image") -eq $((size * 1024 * 1024)) ]]
    fi
}
if [[ -f $image && -f $state ]] && image_matches_size; then
    python3 "$(dirname "$0")/sync-ext2.py" sync "$stage" "$image" "$state"
else
    temporary=$image.new; rm -f "$temporary"; truncate -s "${size}M" "$temporary"
    fakeroot -- sh -c 'stage=$1; image=$2; block=$3; chown -R 0:0 "$stage"; exec mke2fs -q -t ext2 -b "$block" -d "$stage" -F "$image"' mmake-ext2 "$stage" "$temporary" "$block"
    mv "$temporary" "$image"
    python3 "$(dirname "$0")/sync-ext2.py" record "$stage" "$image" "$state"
fi
if [[ $kind == initfs ]]; then
    options=(-)
    [[ ! -f $stage/config/kernel.conf ]] || options=("$stage/config/kernel.conf")
    options+=("$stage")
    platform_data=-
    [[ ! -f $stage/install/rootfs.sha256 ]] || platform_data=$stage/install/rootfs.sha256
    options+=("$platform_data")
    perl "$(dirname "$0")/pack-boot-module.pl" pack "$image" "$((size * 1024 * 1024))" "$stage/init" "${options[@]}"
fi
