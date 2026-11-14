#!/usr/bin/env bash
# Build installation media in its own tree using the normal disk/boot formats.
set -euo pipefail
[[ $# -eq 2 ]] || { echo "usage: $0 <project-root> <mmake-output>" >&2; exit 2; }
root=$1
mmake_out=$2
work=$mmake_out/installer
scripts=$root/scripts/mmake
[[ -f $work/image/rootfs/.ready ]] || { echo 'Installer rootfs is not staged' >&2; exit 1; }
mkdir -p "$work/components"
for asset in BOOTX64.EFI kernel.elf kernel.meta; do
    install -m 0644 "$mmake_out/components/$asset" "$work/components/$asset"
done
bash "$scripts/make-ext2-image.sh" "$work/image/rootfs" "$work/image/rootfs.img" rootfs "$root/.config"
bash "$scripts/make-data-image.sh" "$root" "$work"
bash "$scripts/stage-initfs.sh" "$root" "$work"
bash "$scripts/make-ext2-image.sh" "$work/image/initfs" "$work/image/initfs.img" initfs "$root/.config"
bash "$scripts/make-esp-image.sh" "$root" "$work"
bash "$scripts/make-disk-image.sh" "$root" "$work"
# Keep normal OS artifacts and their incremental image state untouched.
artifacts=$root/out/installer-artifacts
mkdir -p "$artifacts"
rm -f "$artifacts/disk.img.new"
ln "$work/image/disk.img" "$artifacts/disk.img.new" 2>/dev/null \
    || cp --reflink=auto --sparse=always "$work/image/disk.img" "$artifacts/disk.img.new"
mv -f "$artifacts/disk.img.new" "$artifacts/disk.img"
