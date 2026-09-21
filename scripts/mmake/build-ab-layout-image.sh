#!/usr/bin/env bash
set -euo pipefail
[[ $# -eq 4 ]] || { echo "usage: $0 <root> <mmake-out> <seed-tool> <system-image-sign>" >&2; exit 2; }
root=$1
mmake_out=$2
seed_tool=$3
sign_tool=$4
image_dir=$mmake_out/image
image=$image_dir/ab-layout.img
temporary=$image.new
state_image=$image_dir/ab-state.img.new
data_image=$image_dir/ab-data.img.new
esp_image=$image_dir/ab-esp.img
system_source_image=$image_dir/rootfs.img
system_image=$image_dir/ab-system.img.new
rootfs_stage=$image_dir/rootfs
data_stage=$image_dir/ab-data-stage.new
system_stage=$image_dir/ab-system-stage.new
data_mb=128
state_mb=1

esp_bytes=$(stat -c %s "$esp_image")
(( esp_bytes > 0 && esp_bytes % 1048576 == 0 )) || {
    echo "ESP image must have a nonzero MiB-aligned size" >&2; exit 1;
}
esp_mb=$((esp_bytes / 1048576))
system_bytes=$(stat -c %s "$system_source_image")
(( system_bytes > 0 && system_bytes % 1048576 == 0 )) || {
    echo "system image must have a nonzero MiB-aligned size" >&2; exit 1;
}
system_mb=$((system_bytes / 1048576))

# All boundaries are MiB-aligned. The trailing MiB reserves GPT backup space.
esp_start=1
a_start=$((esp_start + esp_mb))
b_start=$((a_start + system_mb))
data_start=$((b_start + system_mb))
state_start=$((data_start + data_mb))
disk_mb=$((state_start + state_mb + 1))
system_type=6d6f6368-694f-5300-8000-6d5061727401
data_type=6d6f6368-694f-5300-8000-6d5061727402
state_type=6d6f6368-694f-5300-8000-6d5061727403

mkdir -p "$image_dir"
rm -f -- "$temporary" "$state_image" "$data_image" "$system_image"
rm -rf -- "$data_stage" "$system_stage"
trap 'rm -f -- "$temporary" "$state_image" "$data_image" "$data_image.state.json" "$system_image"; rm -rf -- "$data_stage" "$system_stage"' EXIT
truncate -s "${disk_mb}M" "$temporary"
{
    printf 'label: gpt\nunit: sectors\nfirst-lba: 2048\nsector-size: 512\n\n'
    printf 'start=%s, size=%s, type=U, name="mochiOS ESP"\n' "$((esp_start * 2048))" "$((esp_mb * 2048))"
    printf 'start=%s, size=%s, type=%s, name="mochiOS System A"\n' "$((a_start * 2048))" "$((system_mb * 2048))" "$system_type"
    printf 'start=%s, size=%s, type=%s, name="mochiOS System B"\n' "$((b_start * 2048))" "$((system_mb * 2048))" "$system_type"
    printf 'start=%s, size=%s, type=%s, name="mochiOS Data"\n' "$((data_start * 2048))" "$((data_mb * 2048))" "$data_type"
    printf 'start=%s, size=%s, type=%s, name="mochiOS Boot State"\n' "$((state_start * 2048))" "$((state_mb * 2048))" "$state_type"
} | sfdisk "$temporary" >/dev/null

[[ -d $rootfs_stage/system && -d $rootfs_stage/home && -d $rootfs_stage/var && -f $rootfs_stage/var/lib/accounts/users.db ]] || {
    echo "rootfs data directories are missing" >&2; exit 1;
}
mkdir -p "$system_stage/system" "$data_stage"
cp -a "$rootfs_stage/system/." "$system_stage/system/"
truncate -s "$system_bytes" "$system_image"
fakeroot -- sh -c 'stage=$1; image=$2; chown -R 0:0 "$stage"; exec mke2fs -q -t ext2 -b 4096 -d "$stage" -F -L MOCHI_SYSTEM "$image"' \
    mmake-ab-system "$system_stage" "$system_image"

if grep -qx 'DEVELOPMENT_SYSTEM_SIGNATURES=y' "$root/.config"; then
    signing_key=$root/tools/devkit/fixtures/development/root.key
    public_key='k0Ja3inoDQGAO74BWDx4pIZsCSDB/hdIt7iaspNKL/Q='
else
    signing_key=${MOCHIOS_SYSTEM_SIGNING_KEY:-}
    public_key='7Gh+xoUEQsOF3HoZXK+y4OtZcx9xa/oWhnK6+JP7Hbg='
    [[ -n $signing_key ]] || { echo "MOCHIOS_SYSTEM_SIGNING_KEY is required for production System images" >&2; exit 1; }
fi
manifest=$image_dir/system.manifest.new
"$sign_tool" "$system_image" "$manifest" "$signing_key" \
    "${MOCHIOS_VERSION:-26.0.0}" "${MOCHIOS_BUILD_NUMBER:-0}" x86_64 "$public_key"
export MTOOLS_SKIP_CHECK=1
for slot in A B; do
    mcopy -o -i "$esp_image" "$manifest" "::/slots/$slot/system.manifest"
done
rm -f -- "$manifest"

truncate -s "${data_mb}M" "$data_image"
cp -a "$rootfs_stage/bin" "$rootfs_stage/applications" "$rootfs_stage/libraries" \
    "$rootfs_stage/home" "$rootfs_stage/var" "$rootfs_stage/tmp" "$data_stage/"
if [[ -f $rootfs_stage/.mochios-ownership ]]; then
    awk '$1 ~ /^(home|var|tmp)\//' \
        "$rootfs_stage/.mochios-ownership" > "$data_stage/.mochios-ownership"
fi
fakeroot -- sh -c 'stage=$1; image=$2; chown -R 0:0 "$stage"; exec mke2fs -q -t ext2 -b 4096 -d "$stage" -F -L MOCHI_DATA "$image"' \
    mmake-ab-data "$data_stage" "$data_image"
python3 "$root/scripts/mmake/sync-ext2.py" record "$data_stage" "$data_image" "$data_image.state.json"
"$seed_tool" "$state_image"
[[ $(stat -c %s "$state_image") -eq $((state_mb * 1048576)) ]] || {
    echo "invalid boot-state image size" >&2; exit 1;
}

dd if="$esp_image" of="$temporary" bs=1M seek="$esp_start" conv=notrunc,sparse status=none
dd if="$system_image" of="$temporary" bs=1M seek="$a_start" conv=notrunc,sparse status=none
dd if="$system_image" of="$temporary" bs=1M seek="$b_start" conv=notrunc,sparse status=none
dd if="$data_image" of="$temporary" bs=1M seek="$data_start" conv=notrunc,sparse status=none
dd if="$state_image" of="$temporary" bs=1M seek="$state_start" conv=notrunc,sparse status=none
mv -- "$temporary" "$image"
printf 'layout-only disk=%s esp=%s system=%s data=%s state=%s\n' \
    "$disk_mb" "$esp_mb" "$system_mb" "$data_mb" "$state_mb"
