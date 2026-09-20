#!/usr/bin/env bash
set -euo pipefail
[[ $# -eq 2 ]] || { echo "usage: $0 <root> <trial-image>" >&2; exit 2; }
root=$1
current_image=$2
test_dir=$(mktemp -d "$root/out/mmake/ab-trial-rollback.XXXXXX")
finish() {
    result=$?
    if [[ $result -eq 0 ]]; then
        rm -rf -- "$test_dir"
    else
        echo "trial/rollback test output retained: $test_dir" >&2
    fi
}
trap finish EXIT
command -v timeout >/dev/null || { echo "timeout is required" >&2; exit 1; }
smoke_disks=()

# Each smoke run clones its source disk. Feed that retained clone into the
# next boot so the bootloader's durable trial counter survives QEMU restarts.
for remaining in 2 1 0; do
    echo "[trial] boot $((3 - remaining))/4: system B, expected remaining=$remaining"
    log=$test_dir/trial-$remaining.log
    if ! KEEP_SMOKE_ARTIFACTS=1 timeout --signal=TERM --kill-after=10s 240s \
        bash "$root/scripts/tests/ab-layout-kvm-test.sh" \
        "$root" "$current_image" B "trial:$remaining" 2>&1 | tee "$log"; then
        echo "trial boot failed or timed out; see $log" >&2
        exit 1
    fi
    current_image=$(sed -n 's/^\[done\] smoke disk: //p' "$log" | tail -n 1)
    [[ -n $current_image && -f $current_image ]] || { echo "trial smoke disk missing" >&2; exit 1; }
    case "$current_image" in
        "$root"/out/runner/workspace-*/disk.img) smoke_disks+=("$current_image") ;;
        *) echo "unexpected smoke disk path: $current_image" >&2; exit 1 ;;
    esac
done

echo "[trial] boot 4/4: expected automatic rollback to stable A"
log=$test_dir/rollback-a.log
if ! KEEP_SMOKE_ARTIFACTS=1 timeout --signal=TERM --kill-after=10s 240s \
    bash "$root/scripts/tests/ab-layout-kvm-test.sh" \
    "$root" "$current_image" A stable 2>&1 | tee "$log"; then
    echo "rollback boot failed or timed out; see $log" >&2
    exit 1
fi
rollback_disk=$(sed -n 's/^\[done\] smoke disk: //p' "$log" | tail -n 1)
case "$rollback_disk" in
    "$root"/out/runner/workspace-*/disk.img) [[ -f $rollback_disk ]] && smoke_disks+=("$rollback_disk") ;;
    *) echo "unexpected rollback smoke disk path: $rollback_disk" >&2; exit 1 ;;
esac
[[ ${#smoke_disks[@]} -eq 4 ]] || { echo "rollback smoke disk missing" >&2; exit 1; }
# These four copies are test scratch space; serial logs remain for inspection.
rm -f -- "${smoke_disks[@]}"
echo "pending B consumed three attempts, then automatically returned to stable A"
