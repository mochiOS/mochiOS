#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
SERVICES_ROOT="${ROOT_DIR}/services"
SERVICE_ROOT="${SERVICES_ROOT}/core"
TARGET_JSON="${SERVICE_ROOT}/x86_64-unknown-mochios.json"
TARGET_DIR="${ROOT_DIR}/out/services-core/target"
NIGHTLY_TOOLCHAIN="${NIGHTLY_TOOLCHAIN:-nightly-2026-05-14}"

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
need_file "${SERVICE_ROOT}/Cargo.toml"
need_file "${TARGET_JSON}"

echo "[build] core.service"
cargo +"${NIGHTLY_TOOLCHAIN}" build \
    -Z build-std=core,alloc,compiler_builtins \
    -Z json-target-spec \
    --release \
    --target "${TARGET_JSON}" \
    --target-dir "${TARGET_DIR}" \
    --manifest-path "${SERVICE_ROOT}/Cargo.toml"

SERVICE_BIN="${TARGET_DIR}/x86_64-unknown-mochios/release/core"
need_file "${SERVICE_BIN}"

echo "[done] ${SERVICE_BIN}"
