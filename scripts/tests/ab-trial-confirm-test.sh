#!/usr/bin/env bash
set -euo pipefail
[[ $# -eq 3 ]] || { echo "usage: $0 <mmake-out> <seed-tool> <confirm-tool>" >&2; exit 2; }
mmake_out=$1
seed_tool=$2
confirm_tool=$3
test_dir=$(mktemp -d "$mmake_out/image/ab-trial-confirm-test.XXXXXX")
trap 'rm -rf -- "$test_dir"' EXIT
"$seed_tool" "$test_dir/unattempted.img" trial-B
cp -- "$test_dir/unattempted.img" "$test_dir/unattempted.expected.img"
if "$confirm_tool" "$test_dir/unattempted.img" >"$test_dir/confirm.log" 2>&1; then
    echo "unattempted B was incorrectly confirmed" >&2
    exit 1
fi
grep -Fq 'NoPendingTrial' "$test_dir/confirm.log"
cmp -s "$test_dir/unattempted.expected.img" "$test_dir/unattempted.img"
"$seed_tool" "$test_dir/stable-a.img"
cp -- "$test_dir/stable-a.img" "$test_dir/stable-a.expected.img"
if "$confirm_tool" "$test_dir/stable-a.img" >"$test_dir/stable.log" 2>&1; then
    echo "stable A was incorrectly confirmed as B" >&2
    exit 1
fi
grep -Fq 'no pending system B trial' "$test_dir/stable.log"
cmp -s "$test_dir/stable-a.expected.img" "$test_dir/stable-a.img"
echo "host confirmation tool rejects unattempted and absent B trials"
