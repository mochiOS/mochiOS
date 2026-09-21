#!/usr/bin/env bash
set -euo pipefail
[[ $# -eq 3 ]] || { echo "usage: $0 <root> <trial-image> <confirm-tool>" >&2; exit 2; }
root=$1
trial_image=$2
confirm_tool=$3
test_dir=$(mktemp -d "$root/out/mmake/ab-trial-confirm.XXXXXX")
finish() {
    result=$?
    if [[ $result -eq 0 ]]; then rm -rf -- "$test_dir";
    else echo "trial/confirm test output retained: $test_dir" >&2; fi
}
trap finish EXIT
command -v timeout >/dev/null || { echo "timeout is required" >&2; exit 1; }

echo "[trial] boot 1/2: system B as a pending trial"
log=$test_dir/trial-b.log
if ! KEEP_SMOKE_ARTIFACTS=1 timeout --signal=TERM --kill-after=10s 240s \
    bash "$root/scripts/tests/ab-layout-kvm-test.sh" "$root" "$trial_image" B trial:2 \
    2>&1 | tee "$log"; then
    echo "trial B boot failed or timed out; see $log" >&2; exit 1
fi
trial_disk=$(sed -n 's/^\[done\] smoke disk: //p' "$log" | tail -n 1)
case "$trial_disk" in
    "$root"/out/runner/workspace-*/disk.img) [[ -f $trial_disk ]] ;;
    *) echo "unexpected trial smoke disk path: $trial_disk" >&2; exit 1 ;;
esac

table=$(sfdisk -J "$trial_disk")
state_start=$(jq -er '.partitiontable.partitions[] |
    select(.name == "mochiOS Boot State" and (.type | ascii_downcase) == "6d6f6368-694f-5300-8000-6d5061727403") | .start' <<< "$table")
state_size=$(jq -er '.partitiontable.partitions[] | select(.name == "mochiOS Boot State") | .size' <<< "$table")
[[ $state_start =~ ^[1-9][0-9]*$ && $state_size -eq 2048 && $((state_start % 2048)) -eq 0 ]] || {
    echo "invalid trial boot-state partition" >&2; exit 1;
}
state_image=$test_dir/attempted-state.img
dd if="$trial_disk" of="$state_image" bs=1M skip="$((state_start / 2048))" count=1 status=none
[[ $(stat -c %s "$state_image") -eq 1048576 ]] || { echo "incomplete boot-state extraction" >&2; exit 1; }
"$confirm_tool" "$state_image"
dd if="$state_image" of="$trial_disk" bs=1M seek="$((state_start / 2048))" conv=notrunc,fsync status=none
cmp -s "$state_image" <(dd if="$trial_disk" bs=1M skip="$((state_start / 2048))" count=1 status=none)

echo "[trial] boot 2/2: confirmed system B must remain stable"
log=$test_dir/stable-b.log
if ! KEEP_SMOKE_ARTIFACTS=1 timeout --signal=TERM --kill-after=10s 240s \
    bash "$root/scripts/tests/ab-layout-kvm-test.sh" "$root" "$trial_disk" B stable \
    2>&1 | tee "$log"; then
    echo "confirmed B boot failed or timed out; see $log" >&2; exit 1
fi
confirmed_disk=$(sed -n 's/^\[done\] smoke disk: //p' "$log" | tail -n 1)
case "$confirmed_disk" in
    "$root"/out/runner/workspace-*/disk.img) [[ -f $confirmed_disk ]] ;;
    *) echo "unexpected confirmed smoke disk path: $confirmed_disk" >&2; exit 1 ;;
esac
rm -f -- "$trial_disk" "$confirmed_disk"
echo "attempted B was confirmed offline after a successful smoke boot and remained stable B"
