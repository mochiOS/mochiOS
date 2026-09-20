#!/usr/bin/env bash
set -euo pipefail
[[ $# -eq 8 ]] || { echo "usage: $0 <ab-image> <esp-image> <system-image> <seed-tool> <kernel> <kernel-meta> <initfs> <gpt-probe>" >&2; exit 2; }
image=$1
esp_image=$2
system_image=$3
seed_tool=$4
kernel=$5
kernel_meta=$6
initfs=$7
gpt_probe=$8
temp_dir=$(mktemp -d "$(dirname "$image")/ab-layout-test.XXXXXX")
trap 'rm -rf -- "$temp_dir"' EXIT

esp_mb=$(( $(stat -c %s "$esp_image") / 1048576 ))
system_mb=$(( $(stat -c %s "$system_image") / 1048576 ))
data_mb=128
state_mb=1
esp_start=1
a_start=$((esp_start + esp_mb))
b_start=$((a_start + system_mb))
data_start=$((b_start + system_mb))
state_start=$((data_start + data_mb))
disk_mb=$((state_start + state_mb + 1))
[[ $(stat -c %s "$image") -eq $((disk_mb * 1048576)) ]]

sfdisk -J "$image" | jq -e \
    --argjson esp_start "$((esp_start * 2048))" --argjson esp_size "$((esp_mb * 2048))" \
    --argjson a_start "$((a_start * 2048))" --argjson b_start "$((b_start * 2048))" \
    --argjson system_size "$((system_mb * 2048))" \
    --argjson data_start "$((data_start * 2048))" --argjson data_size "$((data_mb * 2048))" \
    --argjson state_start "$((state_start * 2048))" --argjson state_size "$((state_mb * 2048))" '
    .partitiontable.label == "gpt" and .partitiontable.sectorsize == 512 and
    (.partitiontable.partitions | length) == 5 and
    (.partitiontable.partitions[0] | .start == $esp_start and .size == $esp_size and .name == "mochiOS ESP") and
    (.partitiontable.partitions[1] | .start == $a_start and .size == $system_size and .name == "mochiOS System A" and (.type | ascii_downcase) == "6d6f6368-694f-5300-8000-6d5061727401") and
    (.partitiontable.partitions[2] | .start == $b_start and .size == $system_size and .name == "mochiOS System B" and (.type | ascii_downcase) == "6d6f6368-694f-5300-8000-6d5061727401") and
    (.partitiontable.partitions[3] | .start == $data_start and .size == $data_size and .name == "mochiOS Data" and (.type | ascii_downcase) == "6d6f6368-694f-5300-8000-6d5061727402") and
    (.partitiontable.partitions[4] | .start == $state_start and .size == $state_size and .name == "mochiOS Boot State" and (.type | ascii_downcase) == "6d6f6368-694f-5300-8000-6d5061727403")
    ' >/dev/null

esp_guid=$(sfdisk -J "$image" | jq -er '.partitiontable.partitions[0].uuid')
"$gpt_probe" "$image" "$esp_guid" "$((esp_start * 2048))" "$((esp_start * 2048 + esp_mb * 2048 - 1))"
if "$gpt_probe" "$image" "00000000-0000-0000-0000-000000000001" \
    "$((esp_start * 2048))" "$((esp_start * 2048 + esp_mb * 2048 - 1))" >/dev/null 2>&1; then
    echo "GPT identity probe accepted the wrong ESP GUID" >&2
    exit 1
fi

cmp -s "$esp_image" <(dd if="$image" bs=1M skip="$esp_start" count="$esp_mb" status=none)
initfs_image=$(dirname "$image")/initfs.img
mmake_out=$(dirname "$(dirname "$image")")
for bundle in disk ext2; do
    manifest="$mmake_out/cexts/$bundle.cext/manifest.toml"
    expected_abi=$(sed -n 's/^abi = \([0-9][0-9]*\)$/\1/p' "$manifest")
    [[ $expected_abi =~ ^[1-9][0-9]*$ ]] || { echo "invalid $bundle CEXT ABI" >&2; exit 1; }
    debugfs -R "dump /$bundle.cext/entry $temp_dir/$bundle.entry" "$initfs_image" >/dev/null 2>&1
    [[ -s "$temp_dir/$bundle.entry" ]] || { echo "missing $bundle CEXT in initfs" >&2; exit 1; }
    cmp -s "$manifest" <(debugfs -R "cat /$bundle.cext/manifest.toml" "$initfs_image" 2>/dev/null)
    [[ $(dd if="$temp_dir/$bundle.entry" bs=1 count=4 status=none) == MCEX ]] || {
        echo "invalid $bundle CEXT package magic" >&2; exit 1;
    }
    read -r abi_low abi_high < <(od -An -tu1 -j4 -N2 "$temp_dir/$bundle.entry")
    [[ $((abi_low + abi_high * 256)) -eq $expected_abi ]] || {
        echo "$bundle CEXT package ABI does not match its manifest" >&2; exit 1;
    }
done
for slot in A B; do
    mcopy -i "$esp_image" "::/slots/$slot/kernel.elf" "$temp_dir/$slot.kernel.elf"
    mcopy -i "$esp_image" "::/slots/$slot/kernel.meta" "$temp_dir/$slot.kernel.meta"
    mcopy -i "$esp_image" "::/slots/$slot/initfs.img" "$temp_dir/$slot.initfs.img"
    cmp -s "$kernel" "$temp_dir/$slot.kernel.elf"
    cmp -s "$kernel_meta" "$temp_dir/$slot.kernel.meta"
    cmp -s "$initfs" "$temp_dir/$slot.initfs.img"
done
cmp -s "$system_image" <(dd if="$image" bs=1M skip="$a_start" count="$system_mb" status=none)
cmp -s "$system_image" <(dd if="$image" bs=1M skip="$b_start" count="$system_mb" status=none)
"$seed_tool" "$temp_dir/expected-state.img"
cmp -s "$temp_dir/expected-state.img" <(dd if="$image" bs=1M skip="$state_start" count="$state_mb" status=none)

magic=$(dd if="$image" bs=1 skip=$((data_start * 1048576 + 1080)) count=2 status=none | od -An -tx1 | tr -d ' \n')
[[ $magic == 53ef ]] || { echo "data partition has no ext2 superblock" >&2; exit 1; }
echo "A/B layout validated: per-slot boot assets, two identical system slots, empty data, initialized boot state"
