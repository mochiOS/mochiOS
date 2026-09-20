#!/usr/bin/env bash
set -euo pipefail

[[ $# -eq 5 ]] || {
    echo "usage: $0 DISK_IMAGE VERSION_TOML OUTPUT_DIR RELEASE BUILD" >&2
    exit 2
}

disk_image=$1
version_file=$2
output_dir=$3
release=$4
build=$5

for command in python3 zstd sha256sum; do
    command -v "$command" >/dev/null 2>&1 || {
        echo "required command not found: $command" >&2
        exit 1
    }
done

[[ -f "$disk_image" && -f "$version_file" ]] || {
    echo "disk image or version.toml is missing" >&2
    exit 1
}

# release/build in the repository's version.toml remain the source of truth.
# The staged copy also exposes `version` for Admin without changing the OS format.
python3 - "$version_file" "$release" "$build" <<'PY'
import re
import sys
import tomllib

path, release, build = sys.argv[1:]
with open(path, "rb") as source:
    version = tomllib.load(source)
if not re.fullmatch(r"\d+\.\d+(?:\.\d+)?", release):
    raise SystemExit(f"invalid release version: {release}")
if not re.fullmatch(r"\d+", build):
    raise SystemExit(f"invalid build number: {build}")
if version.get("release") != release or version.get("build") != int(build):
    raise SystemExit("build outputs do not match version.toml")
if "version" in version and version["version"] != release:
    raise SystemExit("version and release disagree in version.toml")
PY

mkdir -p "$output_dir"
[[ -z "$(find "$output_dir" -mindepth 1 -maxdepth 1 -print -quit)" ]] || {
    echo "output directory is not empty: $output_dir" >&2
    exit 1
}

zstd -T0 -10 "$disk_image" -o "$output_dir/disk.img.zst"
cp "$version_file" "$output_dir/version.toml"
if ! grep -q '^[[:space:]]*version[[:space:]]*=' "$version_file"; then
    printf '\nversion = "%s"\n' "$release" >> "$output_dir/version.toml"
fi

(
    cd "$output_dir"
    sha256sum disk.img.zst version.toml > SHA256SUMS
    zstd -t disk.img.zst
    sha256sum -c SHA256SUMS
)

python3 - "$output_dir" <<'PY'
from pathlib import Path
import sys

actual = {path.name for path in Path(sys.argv[1]).iterdir()}
expected = {"disk.img.zst", "version.toml", "SHA256SUMS"}
if actual != expected:
    raise SystemExit(f"unexpected release artifact contents: {sorted(actual)}")
PY
