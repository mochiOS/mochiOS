#!/usr/bin/env bash
set -euo pipefail
[[ $# -eq 3 ]] || { echo "usage: $0 <root> <ab-image> <signed-efi>" >&2; exit 2; }
root=$1
source_image=$2
source_efi=$3
test_dir=$(mktemp -d "$root/out/mmake/secure-boot.XXXXXX")
trap 'rm -rf -- "$test_dir"' EXIT
# Derive this negative-test fixture from the imported boot artifact. Never
# depend on a producer's private Cargo directory or modify its signed binary.
unsigned_efi=$test_dir/unsigned.efi
cp "$source_efi" "$unsigned_efi"
sbattach --remove "$unsigned_efi"
python3 - "$unsigned_efi" <<'PY'
from pathlib import Path
import struct,sys
data=Path(sys.argv[1]).read_bytes()
assert data[:2]==b'MZ'
pe=struct.unpack_from('<I',data,0x3c)[0]
assert data[pe:pe+4]==b'PE\0\0'
optional=pe+24
assert struct.unpack_from('<H',data,optional)[0]==0x20b
assert struct.unpack_from('<II',data,optional+112+4*8)==(0,0), 'test EFI is still signed'
PY
image=$test_dir/disk.img
cp --reflink=auto --sparse=always "$source_image" "$image"
table=$(sfdisk -J "$image")
esp_start=$(jq -er '.partitiontable.partitions[] | select(.name == "mochiOS ESP") | .start' <<< "$table")
esp_size=$(jq -er '.partitiontable.partitions[] | select(.name == "mochiOS ESP") | .size' <<< "$table")
system_start=$(jq -er '.partitiontable.partitions[] | select(.name == "mochiOS System A") | .start' <<< "$table")
system_size=$(jq -er '.partitiontable.partitions[] | select(.name == "mochiOS System A") | .size' <<< "$table")
data_start=$(jq -er '.partitiontable.partitions[] | select(.name == "mochiOS Data") | .start' <<< "$table")
data_size=$(jq -er '.partitiontable.partitions[] | select(.name == "mochiOS Data") | .size' <<< "$table")
esp=$test_dir/esp.img
dd if="$image" of="$esp" bs=512 skip="$esp_start" count="$esp_size" status=none
MTOOLS_SKIP_CHECK=1 mcopy -o -i "$esp" "$unsigned_efi" ::/EFI/BOOT/BOOTX64.EFI
dd if="$esp" of="$image" bs=512 seek="$esp_start" conv=notrunc status=none
artifact_dir=$test_dir/artifacts
mkdir -p "$artifact_dir"
ln -s "$image" "$artifact_dir/disk.img"
ARTIFACT_DIR="$artifact_dir" QEMU_ACCELERATOR=kvm QEMU_SECURE_BOOT=1 \
    QEMU_TIMEOUT_SECONDS=12 SMOKE_EXPECT_UEFI_REJECTION=1 SMOKE_PROGRESS=1 \
    SMOKE_ROOTFS_START_SECTOR="$system_start" SMOKE_ROOTFS_SIZE_SECTORS="$system_size" \
    SMOKE_DATA_START_SECTOR="$data_start" SMOKE_DATA_SIZE_SECTORS="$data_size" \
    "$root/scripts/smoke-test.sh"
