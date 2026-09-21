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
system_start=$(jq -er '.partitiontable.partitions[1].start' <<< "$table")
system_size=$(jq -er '.partitiontable.partitions[1].size' <<< "$table")
esp_start=$(jq -er '.partitiontable.partitions[0].start' <<< "$table")
esp_size=$(jq -er '.partitiontable.partitions[0].size' <<< "$table")
data_start=$(jq -er '.partitiontable.partitions[3].start' <<< "$table")
data_size=$(jq -er '.partitiontable.partitions[3].size' <<< "$table")
case "$target" in
    system)
        printf '\x5a' | dd of="$image" bs=1 seek="$((system_start * 512 + 1048576))" conv=notrunc status=none
        ;;
    kernel|initfs)
        esp=$test_dir/esp.img
        asset=$test_dir/$target
        dd if="$image" of="$esp" bs=512 skip="$esp_start" count="$esp_size" status=none
        MTOOLS_SKIP_CHECK=1 mcopy -i "$esp" "::/slots/A/${target}.elf" "$asset" 2>/dev/null || \
            MTOOLS_SKIP_CHECK=1 mcopy -i "$esp" "::/slots/A/${target}.img" "$asset"
        printf '\x5a' | dd of="$asset" bs=1 seek=4096 conv=notrunc status=none
        if [[ $target == kernel ]]; then path='::/slots/A/kernel.elf'; else path='::/slots/A/initfs.img'; fi
        MTOOLS_SKIP_CHECK=1 mcopy -o -i "$esp" "$asset" "$path"
        dd if="$esp" of="$image" bs=512 seek="$esp_start" conv=notrunc status=none
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
