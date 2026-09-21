#!/usr/bin/env bash
set -euo pipefail
[[ $# -ge 2 && $# -le 3 ]] || { echo "usage: $0 <mmake-out> <seed-tool> [stable|trial]" >&2; exit 2; }
mmake_out=$1
seed_tool=$2
mode=${3:-stable}
case "$mode" in
    stable) image_name=ab-slot-b; seed_mode=B ;;
    trial) image_name=ab-trial-b; seed_mode=trial-B ;;
    *) echo "mode must be stable or trial" >&2; exit 2 ;;
esac
source_image=$mmake_out/image/ab-layout.img
image=$mmake_out/image/$image_name.img
temporary=$image.new
state_image=$mmake_out/image/$image_name-state.img.new

partition_table=$(sfdisk -J "$source_image")
state_start=$(jq -er '.partitiontable.partitions[] |
    select(.name == "mochiOS Boot State" and (.type | ascii_downcase) == "6d6f6368-694f-5300-8000-6d5061727403") | .start' <<< "$partition_table")
state_size=$(jq -er '.partitiontable.partitions[] | select(.name == "mochiOS Boot State") | .size' <<< "$partition_table")
[[ $state_start =~ ^[1-9][0-9]*$ && $state_size -eq 2048 && $((state_start % 2048)) -eq 0 ]] || {
    echo "invalid A/B boot-state partition" >&2; exit 1;
}

rm -f -- "$temporary" "$state_image"
trap 'rm -f -- "$temporary" "$state_image"' EXIT
cp --reflink=auto --sparse=always -- "$source_image" "$temporary"
"$seed_tool" "$state_image" "$seed_mode"
[[ $(stat -c %s "$state_image") -eq 1048576 ]] || {
    echo "invalid $mode B boot-state image" >&2; exit 1;
}
dd if="$state_image" of="$temporary" bs=1M seek="$((state_start / 2048))" conv=notrunc status=none
cmp -s "$state_image" <(dd if="$temporary" bs=1M skip="$((state_start / 2048))" count=1 status=none)
[[ $(stat -c %s "$temporary") -eq $(stat -c %s "$source_image") ]]
mv -- "$temporary" "$image"
echo "$mode B boot fixture: $image"
