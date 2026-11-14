#!/usr/bin/env bash
set -euo pipefail
[[ $# -eq 2 ]] || exit 2
root=$1; mmake_out=$2; stage=$mmake_out/image/initfs
rm -rf "$stage.new"; mkdir -p "$stage.new/config"
test -f "$mmake_out/image/mdp-initfs/init"
cp -a "$mmake_out/image/mdp-initfs/." "$stage.new/"
mkdir -p "$stage.new/install"
sha256sum "$mmake_out/image/rootfs.img" | awk '{print $1}' | xxd -r -p > "$stage.new/install/rootfs.sha256"
chmod 0644 "$stage.new/install/rootfs.sha256"
touch "$stage.new/.ready"; rm -rf "$stage"; mv "$stage.new" "$stage"
