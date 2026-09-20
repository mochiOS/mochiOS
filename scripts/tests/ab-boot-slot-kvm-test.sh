#!/usr/bin/env bash
set -euo pipefail
[[ $# -eq 2 ]] || { echo "usage: $0 <root> <stable-b-image>" >&2; exit 2; }
root=$1
image=$2
test_dir=$(mktemp -d "$root/out/mmake/ab-boot-slot.XXXXXX")
finish() {
    result=$?
    if [[ $result -eq 0 ]]; then rm -rf -- "$test_dir";
    else echo "boot-slot test output retained: $test_dir" >&2; fi
}
trap finish EXIT

log=$test_dir/smoke.log
if ! KEEP_SMOKE_ARTIFACTS=1 QEMU_NETWORK_SETTLE_SECONDS=10 \
    bash "$root/scripts/tests/ab-layout-kvm-test.sh" \
    "$root" "$image" B stable 2>&1 | tee "$log"; then
    exit 1
fi
disk=$(sed -n 's/^\[done\] smoke disk: //p' "$log" | tail -n 1)
case "$disk" in
    "$root"/out/runner/workspace-*/disk.img) [[ -f $disk ]] ;;
    *) echo "unexpected smoke disk path: $disk" >&2; exit 1 ;;
esac
table=$(sfdisk -J "$disk")
system_start=$(jq -er '.partitiontable.partitions[2] |
    select(.name == "mochiOS System B" and (.type | ascii_downcase) == "6d6f6368-694f-5300-8000-6d5061727401") | .start' <<< "$table")
system_size=$(jq -er '.partitiontable.partitions[2].size' <<< "$table")
[[ $system_start =~ ^[1-9][0-9]*$ && $system_size =~ ^[1-9][0-9]*$ ]] || {
    echo "invalid system B partition" >&2; exit 1;
}
rootfs=$test_dir/system-b.img
dd if="$disk" of="$rootfs" bs=1M iflag=skip_bytes,count_bytes \
    skip="$((system_start * 512))" count="$((system_size * 512))" status=none
[[ $(stat -c %s "$rootfs") -eq $((system_size * 512)) ]] || {
    echo "incomplete system B extraction" >&2; exit 1;
}
debugfs -R 'cat /system/logs/services/update.log' "$rootfs" >"$test_dir/update.log" 2>"$test_dir/debugfs.log"
grep -Fq 'update.service: boot system slot=B; install_enabled=false' "$test_dir/update.log" || {
    echo "update.service did not observe boot slot B: $test_dir/update.log" >&2; exit 1;
}
rm -f -- "$disk"
echo "update.service observed system B through read-only boot-slot syscall"
