#!/usr/bin/env bash
set -euo pipefail
[[ $# -eq 3 ]] || { echo "usage: $0 <root> <ab-image> <system-image-sign>" >&2; exit 2; }
root=$1
source_image=$2
sign_tool=$3
test_dir=$(mktemp -d "$root/out/mmake/anti-rollback.XXXXXX")
trap 'rm -rf -- "$test_dir"' EXIT
low=$test_dir/build-1.img
high=$test_dir/build-2.img
cp --reflink=auto --sparse=always "$source_image" "$low"
cp --reflink=auto --sparse=always "$source_image" "$high"
table=$(sfdisk -J "$high")
esp_start=$(jq -er '.partitiontable.partitions[0].start' <<< "$table")
esp_size=$(jq -er '.partitiontable.partitions[0].size' <<< "$table")
system_start=$(jq -er '.partitiontable.partitions[1].start' <<< "$table")
system_size=$(jq -er '.partitiontable.partitions[1].size' <<< "$table")
data_start=$(jq -er '.partitiontable.partitions[3].start' <<< "$table")
data_size=$(jq -er '.partitiontable.partitions[3].size' <<< "$table")
esp=$test_dir/esp.img
system=$test_dir/system.img
manifest=$test_dir/system.manifest
dd if="$high" of="$esp" bs=512 skip="$esp_start" count="$esp_size" status=none
dd if="$high" of="$system" bs=512 skip="$system_start" count="$system_size" status=none
"$sign_tool" "$system" "$root/out/mmake/components/kernel.elf" \
    "$root/out/mmake/components/kernel.meta" "$root/out/mmake/image/initfs.img" \
    "$manifest" "$root/tools/devkit/fixtures/development/root.key" \
    "${MOCHIOS_VERSION:-26.0.0}" 2 x86_64 \
    'k0Ja3inoDQGAO74BWDx4pIZsCSDB/hdIt7iaspNKL/Q='
MTOOLS_SKIP_CHECK=1 mcopy -o -i "$esp" "$manifest" ::/slots/A/system.manifest
dd if="$esp" of="$high" bs=512 seek="$esp_start" conv=notrunc status=none

vars=$test_dir/OVMF_VARS.fd
boot_image() {
    local image=$1 expectation=${2:-0}
    local artifact_dir=$test_dir/artifacts-$expectation
    mkdir -p "$artifact_dir"
    ln -s "$image" "$artifact_dir/disk.img"
    ARTIFACT_DIR="$artifact_dir" OVMF_VARS_FILE="$vars" QEMU_ACCELERATOR=kvm QEMU_SECURE_BOOT=1 \
        SMOKE_PROGRESS=1 SMOKE_EXPECT_ROLLBACK_REJECTION="$expectation" \
        SMOKE_ROOTFS_START_SECTOR="$system_start" SMOKE_ROOTFS_SIZE_SECTORS="$system_size" \
        SMOKE_DATA_START_SECTOR="$data_start" SMOKE_DATA_SIZE_SECTORS="$data_size" \
        "$root/scripts/smoke-test.sh"
}

echo '[anti-rollback] booting and committing signed build 2 floor'
boot_image "$high"
echo '[anti-rollback] attempting signed build 1 with persistent firmware state'
boot_image "$low" 1
