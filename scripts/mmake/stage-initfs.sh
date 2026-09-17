#!/usr/bin/env bash
set -euo pipefail
[[ $# -eq 2 ]] || exit 2
root=$1; mmake_out=$2; stage=$mmake_out/image/initfs
rm -rf "$stage.new"; mkdir -p "$stage.new/config"
install -m 0755 "$root/out/rust-std/target/x86_64-unknown-mochios/release/core" "$stage.new/init"
install -m 0644 "$root/boot/config/kernel.conf" "$stage.new/config/kernel.conf"
for bundle in disk ext2; do mkdir -p "$stage.new/$bundle.cext"; install -m 0644 "$mmake_out/cexts/$bundle.cext/entry" "$stage.new/$bundle.cext/entry"; install -m 0644 "$mmake_out/cexts/$bundle.cext/manifest.toml" "$stage.new/$bundle.cext/manifest.toml"; done
mkdir -p "$stage.new/install"
sha256sum "$mmake_out/image/rootfs.img" | awk '{print $1}' | xxd -r -p > "$stage.new/install/rootfs.sha256"
chmod 0644 "$stage.new/install/rootfs.sha256"
touch "$stage.new/.ready"; rm -rf "$stage"; mv "$stage.new" "$stage"
