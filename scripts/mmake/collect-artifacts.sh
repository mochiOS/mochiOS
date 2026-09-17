#!/usr/bin/env bash
set -euo pipefail
[[ $# -eq 2 ]] || exit 2
root=$1; mmake_out=$2; artifacts=$root/out/artifacts
link_or_clone() {
    local source=$1 destination=$2
    rm -f "$destination"
    ln "$source" "$destination" 2>/dev/null || cp --reflink=auto --sparse=always "$source" "$destination"
    chmod 0644 "$destination"
}

mkdir -p "$artifacts"
link_or_clone "$mmake_out/image/disk.img" "$artifacts/disk.img"
link_or_clone "$mmake_out/image/initfs.img" "$artifacts/initfs.img"
install -m 0644 "$mmake_out/components/kernel.elf" "$artifacts/kernel.elf"
install -m 0644 "$mmake_out/components/kernel.debug" "$artifacts/kernel.debug"
install -m 0644 "$mmake_out/components/kernel.meta" "$artifacts/kernel.meta"
install -m 0644 "$mmake_out/components/BOOTX64.EFI" "$artifacts/BOOTX64.EFI"
install -m 0644 "$root/version.toml" "$artifacts/version.toml"
(
    cd "$artifacts"
    # disk.img is a sparse development image whose authenticated rootfs hash is
    # embedded in initfs. Hashing its full logical size on every incremental
    # build defeats the block-level image updater.
    find . -maxdepth 1 -type f ! -name SHA256SUMS ! -name disk.img -printf '%P\0' | sort -z | xargs -0 sha256sum > SHA256SUMS.new
    mv SHA256SUMS.new SHA256SUMS
)
