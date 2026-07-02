#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
SERVICES_ROOT="${ROOT_DIR}/services"
DRIVERS_ROOT="${ROOT_DIR}/drivers"
OUT_ROOT="${ROOT_DIR}/out/services-build"
TARGET_DIR="${OUT_ROOT}/target"
TARGET_JSON="${ROOT_DIR}/services/core/x86_64-unknown-mochios.json"
DRIVERS_STAGE="${OUT_ROOT}/stage/drivers"
CAPABILITY_STAGE="${OUT_ROOT}/stage/capability"
LOGGER_STAGE="${OUT_ROOT}/stage/logger"
INPUT_STAGE="${OUT_ROOT}/stage/input"
TTY_STAGE="${OUT_ROOT}/stage/tty"
USB_STAGE="${OUT_ROOT}/stage/usb-driver"
I8042_STAGE="${OUT_ROOT}/stage/i8042-driver"
NIGHTLY_TOOLCHAIN="${NIGHTLY_TOOLCHAIN:-nightly-2026-05-14}"
USER_ROOT="${ROOT_DIR}/user"
USB_DRIVER_ROOT="${DRIVERS_ROOT}/usb-driver"
I8042_DRIVER_ROOT="${DRIVERS_ROOT}/ps2/i8042-driver"
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
need_file "${SERVICES_ROOT}/capability/Cargo.toml"
need_file "${SERVICES_ROOT}/drivers/Cargo.toml"
need_file "${SERVICES_ROOT}/logger/Cargo.toml"
need_file "${SERVICES_ROOT}/input/Cargo.toml"
need_file "${SERVICES_ROOT}/tty/Cargo.toml"
need_file "${USB_DRIVER_ROOT}/Cargo.toml"
need_file "${I8042_DRIVER_ROOT}/Cargo.toml"
need_file "${TARGET_JSON}"

rm -rf "${OUT_ROOT}/stage"
mkdir -p "${CAPABILITY_STAGE}" "${DRIVERS_STAGE}" "${LOGGER_STAGE}" "${INPUT_STAGE}" "${TTY_STAGE}" "${USB_STAGE}" "${I8042_STAGE}"
cp -R "${SERVICES_ROOT}/capability/." "${CAPABILITY_STAGE}/"
cp -R "${SERVICES_ROOT}/drivers/." "${DRIVERS_STAGE}/"
cp -R "${SERVICES_ROOT}/logger/." "${LOGGER_STAGE}/"
cp -R "${SERVICES_ROOT}/input/." "${INPUT_STAGE}/"
cp -R "${SERVICES_ROOT}/tty/." "${TTY_STAGE}/"
cp -R "${USB_DRIVER_ROOT}/." "${USB_STAGE}/"
cp -R "${I8042_DRIVER_ROOT}/." "${I8042_STAGE}/"
rm -f "${CAPABILITY_STAGE}/Cargo.lock" "${DRIVERS_STAGE}/Cargo.lock" "${LOGGER_STAGE}/Cargo.lock" "${INPUT_STAGE}/Cargo.lock" "${TTY_STAGE}/Cargo.lock" "${USB_STAGE}/Cargo.lock" "${I8042_STAGE}/Cargo.lock"
perl -0pi -e "s#path = \"\\.\\./\\.\\./user/crates/platform\"#path = \"${USER_ROOT}/crates/platform\"#g; s#path = \"\\.\\./\\.\\./user/crates/runtime\"#path = \"${USER_ROOT}/crates/runtime\"#g; s#path = \"\\.\\./\\.\\./user/crates/syscall\"#path = \"${USER_ROOT}/crates/syscall\"#g" \
    "${CAPABILITY_STAGE}/Cargo.toml" "${DRIVERS_STAGE}/Cargo.toml" "${LOGGER_STAGE}/Cargo.toml" "${INPUT_STAGE}/Cargo.toml" "${TTY_STAGE}/Cargo.toml" "${USB_STAGE}/Cargo.toml"
perl -0pi -e "s#path = \"\\.\\./\\.\\./\\.\\./user/crates/platform\"#path = \"${USER_ROOT}/crates/platform\"#g; s#path = \"\\.\\./\\.\\./\\.\\./user/crates/runtime\"#path = \"${USER_ROOT}/crates/runtime\"#g; s#path = \"\\.\\./\\.\\./\\.\\./user/crates/syscall\"#path = \"${USER_ROOT}/crates/syscall\"#g" \
    "${I8042_STAGE}/Cargo.toml"
perl -0pi -e "s#plugkit = \\{ git = \"https://github.com/mochiOS/mnu\", package = \"plugkit\" \\}#plugkit = { path = \"${PLUGKIT_ROOT}\" }#g" \
    "${USB_STAGE}/Cargo.toml" "${I8042_STAGE}/Cargo.toml"

echo "[build] capability.service"
cargo +"${NIGHTLY_TOOLCHAIN}" generate-lockfile  \
    --manifest-path "${CAPABILITY_STAGE}/Cargo.toml"
cargo +"${NIGHTLY_TOOLCHAIN}" build \
    -Z build-std=core,alloc,compiler_builtins \
    -Z json-target-spec \
    --release \
    --target "${TARGET_JSON}" \
    --target-dir "${TARGET_DIR}" \
    --manifest-path "${CAPABILITY_STAGE}/Cargo.toml" \
    -p capability

echo "[build] drivers.service"
cargo +"${NIGHTLY_TOOLCHAIN}" generate-lockfile  \
    --manifest-path "${DRIVERS_STAGE}/Cargo.toml"
cargo +"${NIGHTLY_TOOLCHAIN}" build \
    -Z build-std=core,alloc,compiler_builtins \
    -Z json-target-spec \
    --release \
    --target "${TARGET_JSON}" \
    --target-dir "${TARGET_DIR}" \
    --manifest-path "${DRIVERS_STAGE}/Cargo.toml" \
    -p drivers

echo "[build] logger.service"
cargo +"${NIGHTLY_TOOLCHAIN}" generate-lockfile  \
    --manifest-path "${LOGGER_STAGE}/Cargo.toml"
cargo +"${NIGHTLY_TOOLCHAIN}" build \
    -Z build-std=core,alloc,compiler_builtins \
    -Z json-target-spec \
    --release \
    --target "${TARGET_JSON}" \
    --target-dir "${TARGET_DIR}" \
    --manifest-path "${LOGGER_STAGE}/Cargo.toml" \
    -p logger

echo "[build] input.service"
cargo +"${NIGHTLY_TOOLCHAIN}" generate-lockfile  \
    --manifest-path "${INPUT_STAGE}/Cargo.toml"
cargo +"${NIGHTLY_TOOLCHAIN}" build \
    -Z build-std=core,alloc,compiler_builtins \
    -Z json-target-spec \
    --release \
    --target "${TARGET_JSON}" \
    --target-dir "${TARGET_DIR}" \
    --manifest-path "${INPUT_STAGE}/Cargo.toml" \
    -p input

echo "[build] tty.service"
cargo +"${NIGHTLY_TOOLCHAIN}" generate-lockfile \
    --manifest-path "${TTY_STAGE}/Cargo.toml"
cargo +"${NIGHTLY_TOOLCHAIN}" build \
    -Z build-std=core,alloc,compiler_builtins \
    -Z json-target-spec \
    --release \
    --target "${TARGET_JSON}" \
    --target-dir "${TARGET_DIR}" \
    --manifest-path "${TTY_STAGE}/Cargo.toml" \
    -p tty

echo "[build] usb driver bundle"
cargo +"${NIGHTLY_TOOLCHAIN}" generate-lockfile \
    --manifest-path "${USB_STAGE}/Cargo.toml"
cargo +"${NIGHTLY_TOOLCHAIN}" build \
    -Z build-std=core,alloc,compiler_builtins \
    -Z json-target-spec \
    --release \
    --target "${TARGET_JSON}" \
    --target-dir "${TARGET_DIR}" \
    --manifest-path "${USB_STAGE}/Cargo.toml" \
    -p usb-driver

echo "[build] i8042 driver bundle"
cargo +"${NIGHTLY_TOOLCHAIN}" generate-lockfile \
    --manifest-path "${I8042_STAGE}/Cargo.toml"
cargo +"${NIGHTLY_TOOLCHAIN}" build \
    -Z build-std=core,alloc,compiler_builtins \
    -Z json-target-spec \
    --release \
    --target "${TARGET_JSON}" \
    --target-dir "${TARGET_DIR}" \
    --manifest-path "${I8042_STAGE}/Cargo.toml" \
    -p i8042-driver

echo "[done] ${TARGET_DIR}/x86_64-unknown-mochios/release"
