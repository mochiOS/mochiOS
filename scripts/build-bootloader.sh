#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
BOOT_ROOT="${ROOT_DIR}/boot"
TARGET_DIR="${ROOT_DIR}/out/bootloader/target"

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
need_file "${BOOT_ROOT}/Cargo.toml"

echo "[build] bootloader"
env RUSTFLAGS="--cfg curve25519_dalek_backend=\"serial\"" \
cargo +nightly build \
    --offline \
    --release \
    --target x86_64-unknown-uefi \
    --target-dir "${TARGET_DIR}" \
    --manifest-path "${BOOT_ROOT}/Cargo.toml"

BOOT_BIN="$(find "${TARGET_DIR}/x86_64-unknown-uefi/release" -maxdepth 1 -type f \( -name 'boot' -o -name 'boot.efi' \) | head -n 1)"
need_file "${BOOT_BIN}"

echo "[done] ${BOOT_BIN}"
