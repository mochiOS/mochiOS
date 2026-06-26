#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
SERVICES_ROOT="${ROOT_DIR}/services"
OUT_ROOT="${ROOT_DIR}/out/services-build"
TARGET_DIR="${OUT_ROOT}/target"
TARGET_JSON="${ROOT_DIR}/services/core/x86_64-unknown-mochios.json"
DRIVERS_STAGE="${OUT_ROOT}/stage/drivers"
USB_STAGE="${OUT_ROOT}/stage/usb-driver"
NIGHTLY_TOOLCHAIN="${NIGHTLY_TOOLCHAIN:-nightly-2026-05-14}"
USER_ROOT="${ROOT_DIR}/user"
PLUGKIT_ROOT="${ROOT_DIR}/core/crates/PlugKit/plugkit"

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

need_cmd cargo
need_file "${SERVICES_ROOT}/Cargo.toml"
need_file "${SERVICES_ROOT}/drivers/Cargo.toml"
need_file "${SERVICES_ROOT}/usb-driver/Cargo.toml"
need_file "${TARGET_JSON}"

rm -rf "${OUT_ROOT}/stage"
mkdir -p "${DRIVERS_STAGE}" "${USB_STAGE}"
cp -R "${SERVICES_ROOT}/drivers/." "${DRIVERS_STAGE}/"
cp -R "${SERVICES_ROOT}/usb-driver/." "${USB_STAGE}/"
rm -f "${DRIVERS_STAGE}/Cargo.lock" "${USB_STAGE}/Cargo.lock"
perl -0pi -e "s#path = \"\\.\\./\\.\\./user/crates/platform\"#path = \"${USER_ROOT}/crates/platform\"#g; s#path = \"\\.\\./\\.\\./user/crates/runtime\"#path = \"${USER_ROOT}/crates/runtime\"#g; s#path = \"\\.\\./\\.\\./user/crates/syscall\"#path = \"${USER_ROOT}/crates/syscall\"#g" \
    "${DRIVERS_STAGE}/Cargo.toml" "${USB_STAGE}/Cargo.toml"
perl -0pi -e "s#plugkit = \\{ git = \"https://github.com/mochiOS/mnu\", package = \"plugkit\" \\}#plugkit = { path = \"${PLUGKIT_ROOT}\" }#g" \
    "${USB_STAGE}/Cargo.toml"

echo "[build] drivers.service"
cargo +"${NIGHTLY_TOOLCHAIN}" generate-lockfile \
    --offline \
    --manifest-path "${DRIVERS_STAGE}/Cargo.toml"
cargo +"${NIGHTLY_TOOLCHAIN}" build \
    -Z build-std=core,alloc,compiler_builtins \
    -Z json-target-spec \
    --release \
    --target "${TARGET_JSON}" \
    --target-dir "${TARGET_DIR}" \
    --locked \
    --manifest-path "${DRIVERS_STAGE}/Cargo.toml" \
    -p drivers

echo "[build] usb driver bundle"
cargo +"${NIGHTLY_TOOLCHAIN}" generate-lockfile \
    --offline \
    --manifest-path "${USB_STAGE}/Cargo.toml"
cargo +"${NIGHTLY_TOOLCHAIN}" build \
    -Z build-std=core,alloc,compiler_builtins \
    -Z json-target-spec \
    --release \
    --target "${TARGET_JSON}" \
    --target-dir "${TARGET_DIR}" \
    --locked \
    --manifest-path "${USB_STAGE}/Cargo.toml" \
    -p usb-driver

echo "[done] ${TARGET_DIR}/x86_64-unknown-mochios/release"
