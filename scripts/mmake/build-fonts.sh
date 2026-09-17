#!/usr/bin/env bash
set -euo pipefail
[[ $# -eq 1 ]] || exit 2
root=$1
fonts=$root/libraries/fonts
output=$fonts/out/fonts
stamp=$output/.installed
required=(InterVariable.ttf IBMPlexSansJP-Regular.ttf IBMPlexSansJP-Medium.ttf IBMPlexSansJP-SemiBold.ttf)
signature=$(sha256sum "$fonts/fonts.conf" "$fonts/scripts/install-fonts.pl" | sha256sum | awk '{print $1}')
complete=1
for name in "${required[@]}"; do [[ -s $output/$name ]] || complete=0; done
if [[ $complete == 1 ]] && { [[ ! -s $stamp ]] || grep -qx "$signature" "$stamp"; }; then
    for name in UDEVGothic-Regular.ttf UDEVGothic-Bold.ttf; do
        [[ -s $output/$name ]] || printf 'warning: optional font is unavailable: %s\n' "$name" >&2
    done
    printf '%s\n' "$signature" > "$stamp"
    exit 0
fi
mkdir -p "$output" "$fonts/out/cache/fonts" "$fonts/out/build/fonts"
perl "$fonts/scripts/install-fonts.pl" --config "$fonts/fonts.conf" --output "$output" --cache "$fonts/out/cache/fonts" --work "$fonts/out/build/fonts" --curl curl
printf '%s\n' "$signature" > "$stamp.new"
mv "$stamp.new" "$stamp"
