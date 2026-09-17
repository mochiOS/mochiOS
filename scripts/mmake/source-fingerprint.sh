#!/usr/bin/env bash
set -euo pipefail

[[ $# -eq 2 ]] || { echo "usage: $0 <source-root> <output>" >&2; exit 2; }
source_root=$1
output=$2
mkdir -p "$(dirname "$output")"
temporary=$output.new.$$
trap 'rm -f "$temporary"' EXIT HUP INT TERM

if [[ -d $source_root/.git ]] && command -v git >/dev/null 2>&1; then
    {
        git -C "$source_root" rev-parse HEAD
        git -C "$source_root" diff --no-ext-diff --binary HEAD -- .
        while IFS= read -r -d '' path; do
            printf 'untracked:%s\0' "$path"
            sha256sum "$source_root/$path"
        done < <(git -C "$source_root" ls-files --others --exclude-standard -z)
    } | sha256sum | awk '{print $1}' > "$temporary"
else
    find "$source_root" -path "$source_root/.git" -prune -o -type f -print0 | sort -z | xargs -0 sha256sum | sha256sum | awk '{print $1}' > "$temporary"
fi

if [[ -f $output ]] && cmp -s "$temporary" "$output"; then
    rm -f "$temporary"
else
    mv "$temporary" "$output"
fi
trap - EXIT HUP INT TERM
