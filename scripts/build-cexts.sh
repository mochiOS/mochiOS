#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
CEXTS_ROOT="${ROOT_DIR}/cexts"
OUT_ROOT="${ROOT_DIR}/out/cexts"
TARGET_DIR="${OUT_ROOT}/target"
BUNDLES_DIR="${OUT_ROOT}/bundles"
NIGHTLY_TOOLCHAIN="${NIGHTLY_TOOLCHAIN:-nightly-2026-05-14}"
TARGET_TRIPLE="x86_64-unknown-none"

die() {
    echo "fatal: $*" >&2
    exit 1
}

need_cmd() {
    command -v "$1" >/dev/null 2>&1 || die "required command not found: $1"
}

need_file() {
    [[ -f "$1" ]] || die "required file not found: $1"
}

build_module() {
    local package_name="$1"
    local archive_name="$2"
    local bundle_name="$3"
    shift 3
    local deps=("$@")
    local archive_path elf_out entry_out bundle_dir manifest_src
    local pack_args

    echo "[build] cext ${bundle_name}"
    cargo +"${NIGHTLY_TOOLCHAIN}" build \
        -Z build-std=core,compiler_builtins \
        --release \
        --target "${TARGET_TRIPLE}" \
        --target-dir "${TARGET_DIR}" \
        --manifest-path "${CEXTS_ROOT}/Cargo.toml" \
        -p "${package_name}"

    archive_path="${TARGET_DIR}/${TARGET_TRIPLE}/release/${archive_name}"
    [[ -f "${archive_path}" ]] || die "archive for ${package_name} was not produced"

    bundle_dir="${BUNDLES_DIR}/${bundle_name}.cext"
    elf_out="${bundle_dir}/${bundle_name}.elf"
    entry_out="${bundle_dir}/entry"
    manifest_src="${CEXTS_ROOT}/${bundle_name}.cext/manifest.toml"
    mkdir -p "${bundle_dir}"

    ld -shared -nostdlib -z noexecstack \
        --whole-archive "${archive_path}" --no-whole-archive \
        -o "${elf_out}"
    readelf -h "${elf_out}" >/dev/null
    if ! readelf -rW "${elf_out}" | awk '/R_X86_64_/ && $3 != "R_X86_64_RELATIVE" { bad = 1 } END { exit bad }'; then
        die "unsupported relocation remained in ${elf_out}"
    fi

    pack_args=(
        --name "${bundle_name}"
        --version 1
        --elf "${elf_out}"
        --out "${entry_out}"
    )
    for dep in "${deps[@]}"; do
        pack_args+=(--dep "${dep}")
    done
    perl "${SCRIPT_DIR}/pack-cext.pl" "${pack_args[@]}"

    install -m 0644 "${manifest_src}" "${bundle_dir}/manifest.toml"
}

need_cmd awk
need_cmd cargo
need_cmd find
need_cmd install
need_cmd ld
need_cmd perl
need_cmd readelf

need_file "${CEXTS_ROOT}/Cargo.toml"
need_file "${SCRIPT_DIR}/pack-cext.pl"
need_file "${CEXTS_ROOT}/disk.cext/manifest.toml"
need_file "${CEXTS_ROOT}/ext2.cext/manifest.toml"

rm -rf "${BUNDLES_DIR}"
mkdir -p "${BUNDLES_DIR}"

build_module "mochi-disk-cext" "libmochi_disk_cext.a" "disk"
build_module "mochi-ext2-cext" "libmochi_ext2_cext.a" "ext2" "disk"

echo "[done] ${BUNDLES_DIR}"
