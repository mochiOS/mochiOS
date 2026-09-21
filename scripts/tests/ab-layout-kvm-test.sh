#!/usr/bin/env bash
set -euo pipefail
[[ $# -ge 2 && $# -le 4 ]] || { echo "usage: $0 <root> <ab-image> [A|B] [stable|trial:0|trial:1|trial:2]" >&2; exit 2; }
root=$1
image=$2
slot=${3:-A}
expected=${4:-stable}
case "$slot" in
    A) part_index=1 ;;
    B) part_index=2 ;;
    *) echo "slot must be A or B" >&2; exit 2 ;;
esac
command -v sfdisk >/dev/null || { echo "sfdisk is required" >&2; exit 1; }
command -v jq >/dev/null || { echo "jq is required" >&2; exit 1; }
partition_table=$(sfdisk -J "$image")
rootfs_start=$(jq -r --argjson index "$part_index" '.partitiontable.partitions[$index].start' <<< "$partition_table")
rootfs_size=$(jq -r --argjson index "$part_index" '.partitiontable.partitions[$index].size' <<< "$partition_table")
data_start=$(jq -r '.partitiontable.partitions[3].start' <<< "$partition_table")
data_size=$(jq -r '.partitiontable.partitions[3].size' <<< "$partition_table")
[[ $rootfs_start =~ ^[1-9][0-9]*$ && $rootfs_size =~ ^[1-9][0-9]*$ ]] || {
    echo "could not determine system $slot partition boundaries" >&2; exit 1;
}
[[ $data_start =~ ^[1-9][0-9]*$ && $data_size =~ ^[1-9][0-9]*$ ]] || {
    echo "could not determine data partition boundaries" >&2; exit 1;
}
artifact_dir=$(mktemp -d "$root/out/mmake/ab-layout-artifacts.XXXXXX")
trap 'rm -rf -- "$artifact_dir"' EXIT
ln -s "$image" "$artifact_dir/disk.img"
run_log="$artifact_dir/smoke-output.log"

if ! ARTIFACT_DIR="$artifact_dir" QEMU_ACCELERATOR=kvm SMOKE_PROGRESS=1 \
    SMOKE_ROOTFS_START_SECTOR="$rootfs_start" \
    SMOKE_ROOTFS_SIZE_SECTORS="$rootfs_size" \
    SMOKE_DATA_START_SECTOR="$data_start" \
    SMOKE_DATA_SIZE_SECTORS="$data_size" \
    "$root/scripts/smoke-test.sh" 2>&1 | tee "$run_log"; then
    exit 1
fi
serial=$(sed -n 's/^\[done\] serial log: //p' "$run_log" | tail -n 1)
[[ -n $serial && -f $serial ]] || { echo "smoke test did not report a serial log" >&2; exit 1; }
case "$expected" in
    stable) boot_marker="A/B boot state found; booting system $slot" ;;
    trial:[012]) boot_marker="A/B trial boot: system $slot attempts remaining=${expected#trial:}" ;;
    *) echo "invalid expected boot state: $expected" >&2; exit 2 ;;
esac
grep -Fq "$boot_marker" "$serial" || {
    echo "bootloader did not validate the A/B boot-state partition: $serial" >&2
    exit 1
}
grep -Fq 'A/B boot ESP identity: available' "$serial" || {
    echo "bootloader did not identify the booted ESP partition: $serial" >&2
    exit 1
}
grep -Fq "A/B boot assets: slot $slot" "$serial" || {
    echo "bootloader did not load slot $slot boot assets: $serial" >&2
    exit 1
}
grep -Fq "ext2.cext: mounted system $slot" "$serial" || {
    echo "ext2 did not mount the selected system $slot partition: $serial" >&2
    exit 1
}
echo "A/B layout booted system $slot under KVM; serial log: $serial"
if [[ ${KEEP_SMOKE_ARTIFACTS:-0} == 1 ]]; then
    disk=${serial%/serial.log}/disk.img
    [[ -f $disk ]] || { echo "retained smoke disk not found: $disk" >&2; exit 1; }
    echo "[done] smoke disk: $disk"
fi
