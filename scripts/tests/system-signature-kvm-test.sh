#!/usr/bin/env bash
set -euo pipefail
[[ $# -eq 2 ]] || { echo "usage: $0 <root> <ab-image>" >&2; exit 2; }
root=$1
source_image=$2
test_dir=$(mktemp -d "$root/out/mmake/system-signature.XXXXXX")
trap 'rm -rf -- "$test_dir"' EXIT
image=$test_dir/disk.img
cp --reflink=auto --sparse=always "$source_image" "$image"
table=$(sfdisk -J "$image")
system_start=$(jq -er '.partitiontable.partitions[1].start' <<< "$table")
system_size=$(jq -er '.partitiontable.partitions[1].size' <<< "$table")
data_start=$(jq -er '.partitiontable.partitions[3].start' <<< "$table")
data_size=$(jq -er '.partitiontable.partitions[3].size' <<< "$table")
printf '\x5a' | dd of="$image" bs=1 seek="$((system_start * 512 + 1048576))" conv=notrunc status=none
artifact_dir=$test_dir/artifacts
mkdir -p "$artifact_dir"
ln -s "$image" "$artifact_dir/disk.img"
ARTIFACT_DIR="$artifact_dir" QEMU_ACCELERATOR=kvm SMOKE_PROGRESS=1 \
    SMOKE_EXPECT_SYSTEM_REJECTION=1 \
    SMOKE_ROOTFS_START_SECTOR="$system_start" SMOKE_ROOTFS_SIZE_SECTORS="$system_size" \
    SMOKE_DATA_START_SECTOR="$data_start" SMOKE_DATA_SIZE_SECTORS="$data_size" \
    "$root/scripts/smoke-test.sh"
