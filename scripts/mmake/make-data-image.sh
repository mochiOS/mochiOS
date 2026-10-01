#!/usr/bin/env bash
set -euo pipefail

[[ $# -eq 2 ]] || { echo "usage: $0 <project-root> <mmake-output>" >&2; exit 2; }
root=$1
mmake_out=$2
config=$root/.config
source_stage=$mmake_out/image/rootfs
stage=$mmake_out/image/data-stage
stage_new=$stage.new
image=$mmake_out/image/data.img
state=$image.state.json
temporary=$image.new
value() { sed -n "s/^$1=//p" "$config" | tr -d '"' | tail -n1; }
size=$(value IMAGE_DATA_SIZE_MB)
cleanup() {
    rm -rf -- "$stage_new"
    rm -f -- "$temporary"
}
trap cleanup EXIT

rm -rf -- "$stage_new"
mkdir -p "$stage_new"
for directory in bin applications libraries home var tmp; do
    [[ -d $source_stage/$directory ]] || {
        echo "Data directory is missing from rootfs stage: $directory" >&2
        exit 1
    }
    cp -a "$source_stage/$directory" "$stage_new/"
done
if [[ -f $source_stage/.mochios-ownership ]]; then
    awk '$1 ~ /^(bin|applications|libraries|home|var|tmp)(\/|$)/' \
        "$source_stage/.mochios-ownership" > "$stage_new/.mochios-ownership"
fi
rm -rf -- "$stage"
mv -- "$stage_new" "$stage"

if [[ -f $image && -f $state && $(stat -c %s "$image") -eq $((size * 1024 * 1024)) ]]; then
    python3 "$root/scripts/mmake/sync-ext2.py" sync "$stage" "$image" "$state"
else
    rm -f -- "$temporary"
    truncate -s "${size}M" "$temporary"
    fakeroot -- sh -c 'stage=$1; image=$2; chown -R 0:0 "$stage"; exec mke2fs -q -t ext4 -O ^has_journal -b 4096 -d "$stage" -F -L MOCHI_DATA "$image"' \
        mmake-data "$stage" "$temporary"
    mv -- "$temporary" "$image"
    python3 "$root/scripts/mmake/sync-ext2.py" record "$stage" "$image" "$state"
fi
trap - EXIT
