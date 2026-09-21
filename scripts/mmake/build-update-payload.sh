#!/usr/bin/env bash
set -euo pipefail

[[ $# -eq 3 ]] || {
    echo "usage: $0 <boot-slot.img> <system-slot.img> <output.moupdate>" >&2
    exit 2
}

boot=$1
system=$2
output=$3
temporary=$output.new

[[ -f $boot && -f $system && $output == *.moupdate ]] || {
    echo "update payload inputs or output name are invalid" >&2
    exit 1
}
boot_size=$(stat -c %s "$boot")
system_size=$(stat -c %s "$system")
(( boot_size > 0 && boot_size % 1048576 == 0 && system_size > 0 && system_size % 512 == 0 )) || {
    echo "update payload components are not correctly aligned" >&2
    exit 1
}

rm -f -- "$temporary"
trap 'rm -f -- "$temporary"' EXIT
cp --reflink=auto --sparse=always "$boot" "$temporary"
dd if="$system" of="$temporary" bs=1M seek="$((boot_size / 1048576))" \
    conv=notrunc,sparse status=none
expected=$((boot_size + system_size))
[[ $(stat -c %s "$temporary") -eq $expected ]] || {
    echo "update payload length mismatch" >&2
    exit 1
}
mv -- "$temporary" "$output"
sha256sum "$output" > "$output.sha256"
printf 'update payload: %s (%s bytes)\n' "$output" "$expected"
