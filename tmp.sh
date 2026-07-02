#!/usr/bin/env bash
set -euo pipefail

cd "$(git rev-parse --show-toplevel 2>/dev/null || pwd)"

find . \
  \( \
    -path './.repo' -o \
    -path './.git' -o \
    -path './target' -o \
    -path '*/target' -o \
    -path './out' -o \
    -path '*/out' -o \
    -path './libraries/newlib' \
  \) -prune \
  -o -type f -print0 |
while IFS= read -r -d '' file; do
    # -Iによりバイナリファイルは対象外
    if grep -Iq 'com\.mochios' "$file"; then
        sed -i 's/com\.mochios/org.mochios/g' "$file"
        printf 'updated: %s\n' "$file"
    fi
done