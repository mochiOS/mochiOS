#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
CORE_ROOT="${ROOT_DIR}/core"
TARGET_DIR="${CORE_ROOT}/target/uefi/boot-build-temp"

need_cmd() {
    command -v "$1" >/dev/null 2>&1 || {
        echo "missing command: $1" >&2
        exit 1
    }
}

need_file() {
    [[ -f "$1" ]] || {
        echo "missing file: $1" >&2
        exit 1
    }
}

need_cmd cargo
need_cmd cp
need_cmd mktemp
need_cmd perl

need_file "${CORE_ROOT}/examples/boot/Cargo.toml"
need_file "${CORE_ROOT}/crates/abi/Cargo.toml"

TMPDIR="$(mktemp -d /tmp/mochios-boot.XXXXXX)"
cleanup() {
    rm -rf "${TMPDIR}"
}
trap cleanup EXIT

cp -a "${CORE_ROOT}/examples/boot/." "${TMPDIR}/"
perl -0pi -e "s#\\.\\./\\.\\./crates/abi#${CORE_ROOT}/crates/abi#g" "${TMPDIR}/Cargo.toml"

echo "[build] bootloader"
cargo +nightly build \
    --offline \
    --release \
    --target x86_64-unknown-uefi \
    --target-dir "${TARGET_DIR}" \
    --manifest-path "${TMPDIR}/Cargo.toml"

BOOT_BIN="$(find "${TARGET_DIR}/x86_64-unknown-uefi/release" -maxdepth 1 -type f \( -name 'boot' -o -name 'boot.efi' \) | head -n 1)"
need_file "${BOOT_BIN}"

echo "[done] ${BOOT_BIN}"
