#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
CEXTS_DIR="${CEXTS_DIR:-${ROOT_DIR}/cexts}"
INITFS_STAGE="${INITFS_STAGE:?INITFS_STAGE is required}"

die() {
    echo "fatal: $*" >&2
    exit 1
}

need_file() {
    [[ -e "$1" ]] || die "required file not found: $1"
}

mkdir -p "${INITFS_STAGE}"

while IFS= read -r -d '' bundle_dir; do
    bundle_name="$(basename "${bundle_dir}")"
    manifest="${bundle_dir}/manifest.toml"
    entry="${bundle_dir}/entry"
    need_file "${manifest}"
    need_file "${entry}"

    target_dir="${INITFS_STAGE}/${bundle_name}"
    rm -rf "${target_dir}"
    mkdir -p "${target_dir}"
    install -m 0644 "${manifest}" "${target_dir}/manifest.toml"
    install -m 0644 "${entry}" "${target_dir}/entry"

    printf '/%s/manifest.toml=%s\n' "${bundle_name}" "${manifest}"
    printf '/%s/entry=%s\n' "${bundle_name}" "${entry}"
done < <(find "${CEXTS_DIR}" -mindepth 1 -maxdepth 1 -type d -name '*.cext' -print0 | sort -z)
