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
a_image=$mmake_out/image/ab-layout.img
b_image=$mmake_out/image/$image_name.img
[[ $(stat -c %s "$a_image") -eq $(stat -c %s "$b_image") ]]
temp_dir=$(mktemp -d "$mmake_out/image/$image_name-test.XXXXXX")
trap 'rm -rf -- "$temp_dir"' EXIT
"$seed_tool" "$temp_dir/state-a.img"
"$seed_tool" "$temp_dir/state-b.img" "$seed_mode"
for slot in a b; do
    image=$a_image
    [[ $slot == b ]] && image=$b_image
    state_start=$(sfdisk -J "$image" | jq -er '.partitiontable.partitions[4] |
        select(.name == "mochiOS Boot State" and (.type | ascii_downcase) == "6d6f6368-694f-5300-8000-6d5061727403") | .start')
    [[ $state_start =~ ^[1-9][0-9]*$ && $((state_start % 2048)) -eq 0 ]]
    cmp -s "$temp_dir/state-$slot.img" <(dd if="$image" bs=1M skip="$((state_start / 2048))" count=1 status=none)
done
echo "stable-A and $mode-B boot-state fixtures validated"
