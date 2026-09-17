#!/usr/bin/env bash
set -euo pipefail
[[ $# -eq 2 ]] || exit 2
root=$1; mmake_out=$2; artifacts=$root/out/artifacts
mkdir -p "$artifacts"; rm -f "$artifacts/disk.img"
ln "$mmake_out/image/disk.img" "$artifacts/disk.img" 2>/dev/null || cp --reflink=auto --sparse=always "$mmake_out/image/disk.img" "$artifacts/disk.img"
install -m 0644 "$mmake_out/image/initfs.img" "$artifacts/initfs.img"
install -m 0644 "$mmake_out/components/kernel.elf" "$artifacts/kernel.elf"
install -m 0644 "$mmake_out/components/kernel.debug" "$artifacts/kernel.debug"
install -m 0644 "$mmake_out/components/kernel.meta" "$artifacts/kernel.meta"
install -m 0644 "$mmake_out/components/BOOTX64.EFI" "$artifacts/BOOTX64.EFI"
install -m 0644 "$root/version.toml" "$artifacts/version.toml"
(
    cd "$artifacts"
    find . -maxdepth 1 -type f ! -name SHA256SUMS -printf '%P\0' | sort -z | xargs -0 sha256sum > SHA256SUMS.new
    mv SHA256SUMS.new SHA256SUMS
)
