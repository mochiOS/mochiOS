#!/usr/bin/env bash
set -euo pipefail
[[ $# -ge 2 && $# -le 3 ]] || { echo "usage: $0 <root> <ab-image> [system|kernel|initfs]" >&2; exit 2; }
root=$1
source_image=$2
target=${3:-system}
test_dir=$(mktemp -d "$root/out/mmake/system-signature.XXXXXX")
trap 'rm -rf -- "$test_dir"' EXIT
image=$test_dir/disk.img
cp --reflink=auto --sparse=always "$source_image" "$image"
table=$(sfdisk -J "$image")
system_start=$(jq -er '.partitiontable.partitions[] | select(.name == "mochiOS System A") | .start' <<< "$table")
system_size=$(jq -er '.partitiontable.partitions[] | select(.name == "mochiOS System A") | .size' <<< "$table")
boot_start=$(jq -er '.partitiontable.partitions[] | select(.name == "mochiOS Boot A") | .start' <<< "$table")
data_start=$(jq -er '.partitiontable.partitions[] | select(.name == "mochiOS Data") | .start' <<< "$table")
data_size=$(jq -er '.partitiontable.partitions[] | select(.name == "mochiOS Data") | .size' <<< "$table")
case "$target" in
    system)
        printf '\x5a' | dd of="$image" bs=1 seek="$((system_start * 512 + 1048576))" conv=notrunc status=none
        ;;
    kernel|initfs)
        if [[ $target == kernel ]]; then header_field=40; else header_field=72; fi
        asset_offset=$(od -An -tu8 -j "$((boot_start * 512 + header_field))" -N8 "$image" | tr -d ' ')
        [[ $asset_offset =~ ^[1-9][0-9]*$ ]] || { echo "invalid $target slot offset" >&2; exit 1; }
        printf '\x5a' | dd of="$image" bs=1 seek="$((boot_start * 512 + asset_offset + 4096))" conv=notrunc status=none
        ;;
    *) echo "invalid tamper target: $target" >&2; exit 2 ;;
esac
artifact_dir=$test_dir/artifacts
mkdir -p "$artifact_dir"
ln -s "$image" "$artifact_dir/disk.img"
ARTIFACT_DIR="$artifact_dir" QEMU_ACCELERATOR=kvm SMOKE_PROGRESS=1 \
    SMOKE_EXPECT_SYSTEM_REJECTION=1 \
    SMOKE_ROOTFS_START_SECTOR="$system_start" SMOKE_ROOTFS_SIZE_SECTORS="$system_size" \
    SMOKE_DATA_START_SECTOR="$data_start" SMOKE_DATA_SIZE_SECTORS="$data_size" \
    "$root/scripts/smoke-test.sh"
