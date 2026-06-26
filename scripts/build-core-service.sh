#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
SERVICES_ROOT="${ROOT_DIR}/services"
SERVICE_ROOT="${SERVICES_ROOT}/core"
TARGET_JSON="${SERVICE_ROOT}/x86_64-unknown-mochios.json"
TARGET_DIR="${ROOT_DIR}/out/services-core/target"
STAGE_ROOT="${ROOT_DIR}/out/services-core/stage"
NIGHTLY_TOOLCHAIN="${NIGHTLY_TOOLCHAIN:-nightly-2026-05-14}"
USER_ROOT="${ROOT_DIR}/user"

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

rm -rf "${STAGE_ROOT}"
mkdir -p "${STAGE_ROOT}"
cp -R "${SERVICE_ROOT}/." "${STAGE_ROOT}/"
rm -f "${STAGE_ROOT}/Cargo.lock"
perl -0pi -e "s#path = \"\\.\\./\\.\\./user/crates/platform\"#path = \"${USER_ROOT}/crates/platform\"#g; s#path = \"\\.\\./\\.\\./user/crates/runtime\"#path = \"${USER_ROOT}/crates/runtime\"#g; s#path = \"\\.\\./\\.\\./user/crates/syscall\"#path = \"${USER_ROOT}/crates/syscall\"#g" \
    "${STAGE_ROOT}/Cargo.toml"

echo "[build] core.service"
cargo +"${NIGHTLY_TOOLCHAIN}" generate-lockfile \
    --offline \
    --manifest-path "${STAGE_ROOT}/Cargo.toml"
cargo +"${NIGHTLY_TOOLCHAIN}" build \
    -Z build-std=core,alloc,compiler_builtins \
    -Z json-target-spec \
    --offline \
    --release \
    --target "${TARGET_JSON}" \
    --target-dir "${TARGET_DIR}" \
    --manifest-path "${STAGE_ROOT}/Cargo.toml"

SERVICE_BIN="${TARGET_DIR}/x86_64-unknown-mochios/release/core"
need_file "${SERVICE_BIN}"

echo "[done] ${SERVICE_BIN}"
