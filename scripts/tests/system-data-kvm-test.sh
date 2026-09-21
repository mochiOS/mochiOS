#!/usr/bin/env bash
set -euo pipefail
[[ $# -eq 3 ]] || { echo "usage: $0 <root> <ab-image> <system-image-sign>" >&2; exit 2; }
root=$1
source_image=$2
sign_tool=$3
test_dir=$(mktemp -d "$root/out/mmake/system-data.XXXXXX")
trap 'rm -rf -- "$test_dir"' EXIT
image=$test_dir/disk.img
cp --reflink=auto --sparse=always "$source_image" "$image"
table=$(sfdisk -J "$image")
esp_start=$(jq -er '.partitiontable.partitions[0].start' <<< "$table")
esp_size=$(jq -er '.partitiontable.partitions[0].size' <<< "$table")
system_start=$(jq -er '.partitiontable.partitions[1].start' <<< "$table")
system_size=$(jq -er '.partitiontable.partitions[1].size' <<< "$table")
data_start=$(jq -er '.partitiontable.partitions[3].start' <<< "$table")
data_size=$(jq -er '.partitiontable.partitions[3].size' <<< "$table")
system_image=$test_dir/system-a.img
esp_image=$test_dir/esp.img
dd if="$image" of="$system_image" bs=512 skip="$system_start" count="$system_size" status=none
dd if="$image" of="$esp_image" bs=512 skip="$esp_start" count="$esp_size" status=none
selftest=$root/out/mmake/image/rootfs/system/bin/selftest-system-layout
debugfs -w -R 'rm /system/services/secure-ui.service' "$system_image" >/dev/null 2>&1
debugfs -w -R "write $selftest /system/services/secure-ui.service" "$system_image" >/dev/null 2>&1
debugfs -w -R 'set_inode_field /system/services/secure-ui.service mode 0100755' "$system_image" >/dev/null 2>&1
manifest=$test_dir/system.manifest
"$sign_tool" "$system_image" "$root/out/mmake/components/kernel.elf" \
    "$root/out/mmake/components/kernel.meta" "$root/out/mmake/image/initfs.img" \
    "$manifest" "$root/tools/devkit/fixtures/development/root.key" \
    "${MOCHIOS_VERSION:-26.0.0}" "${MOCHIOS_BUILD_NUMBER:-1}" x86_64 \
    'k0Ja3inoDQGAO74BWDx4pIZsCSDB/hdIt7iaspNKL/Q='
MTOOLS_SKIP_CHECK=1 mcopy -o -i "$esp_image" "$manifest" ::/slots/A/system.manifest
dd if="$system_image" of="$image" bs=512 seek="$system_start" conv=notrunc status=none
dd if="$esp_image" of="$image" bs=512 seek="$esp_start" conv=notrunc status=none
artifact_dir=$test_dir/artifacts
mkdir -p "$artifact_dir"
ln -s "$image" "$artifact_dir/disk.img"
ARTIFACT_DIR="$artifact_dir" QEMU_ACCELERATOR=kvm SMOKE_PROGRESS=1 \
    SMOKE_EXPECT_SYSTEM_LAYOUT_PASS=1 \
    SMOKE_ROOTFS_START_SECTOR="$system_start" SMOKE_ROOTFS_SIZE_SECTORS="$system_size" \
    SMOKE_DATA_START_SECTOR="$data_start" SMOKE_DATA_SIZE_SECTORS="$data_size" \
    "$root/scripts/smoke-test.sh"
