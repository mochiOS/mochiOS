#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

ROOTFS_STAGE="${ROOTFS_STAGE:?ROOTFS_STAGE is required}"
ROOTFS_IMG="${ROOTFS_IMG:?ROOTFS_IMG is required}"
HELLO_ELF="${HELLO_ELF:?HELLO_ELF is required}"
RUST_STD_DEMO_BIN="${RUST_STD_DEMO_BIN:-}"
RUST_STD_DEMO_MANIFEST_SRC="${RUST_STD_DEMO_MANIFEST_SRC:-}"
COREUTILS_MANIFEST_SRC="${COREUTILS_MANIFEST_SRC:-}"
MPK_DEMO_MPKG_SRC="${MPK_DEMO_MPKG_SRC:-}"
MPK_TEST_MPKG_SRC="${MPK_TEST_MPKG_SRC:-}"
SIGNATURE_DB_SRC="${SIGNATURE_DB_SRC:?SIGNATURE_DB_SRC is required}"
CORE_SERVICE_BIN="${CORE_SERVICE_BIN:-}"
CAPABILITY_SERVICE_BIN="${CAPABILITY_SERVICE_BIN:-}"
CAPABILITY_SERVICE_MANIFEST_SRC="${CAPABILITY_SERVICE_MANIFEST_SRC:-}"
DRIVERS_SERVICE_BIN="${DRIVERS_SERVICE_BIN:-}"
DRIVERS_SERVICE_MANIFEST_SRC="${DRIVERS_SERVICE_MANIFEST_SRC:-}"
LOGGER_SERVICE_BIN="${LOGGER_SERVICE_BIN:-}"
LOGGER_SERVICE_MANIFEST_SRC="${LOGGER_SERVICE_MANIFEST_SRC:-}"
INPUT_SERVICE_BIN="${INPUT_SERVICE_BIN:-}"
INPUT_SERVICE_MANIFEST_SRC="${INPUT_SERVICE_MANIFEST_SRC:-}"
PACKAGE_SERVICE_BIN="${PACKAGE_SERVICE_BIN:-}"
PACKAGE_SERVICE_MANIFEST_SRC="${PACKAGE_SERVICE_MANIFEST_SRC:-}"
SIGNATURE_SERVICE_BIN="${SIGNATURE_SERVICE_BIN:-}"
SIGNATURE_SERVICE_MANIFEST_SRC="${SIGNATURE_SERVICE_MANIFEST_SRC:-}"
TTY_SERVICE_BIN="${TTY_SERVICE_BIN:-}"
TTY_SERVICE_MANIFEST_SRC="${TTY_SERVICE_MANIFEST_SRC:-}"
MSH_BIN="${MSH_BIN:-}"
MSH_MANIFEST_SRC="${MSH_MANIFEST_SRC:-}"
MSH_FONT_SRC="${MSH_FONT_SRC:-}"
COREUTILS_BIN_DIR="${COREUTILS_BIN_DIR:-}"
USB_DRIVER_MANIFEST_SRC="${USB_DRIVER_MANIFEST_SRC:-}"
USB_DRIVER_ENTRY_BIN="${USB_DRIVER_ENTRY_BIN:-}"
USB_DRIVER_BUNDLE_ROOT="${USB_DRIVER_BUNDLE_ROOT:-/bin/drivers/usb/qemu-usb.driver}"
I8042_DRIVER_MANIFEST_SRC="${I8042_DRIVER_MANIFEST_SRC:-}"
I8042_DRIVER_ENTRY_BIN="${I8042_DRIVER_ENTRY_BIN:-}"
I8042_DRIVER_BUNDLE_ROOT="${I8042_DRIVER_BUNDLE_ROOT:-/bin/drivers/ps2/i8042.driver}"

die() {
    echo "fatal: $*" >&2
    exit 1
}

need_file() {
    [[ -f "$1" ]] || die "required file not found: $1"
}

need_cmd() {
    command -v "$1" >/dev/null 2>&1 || die "required command not found: $1"
}

need_cmd dd
need_cmd mke2fs
need_file "${HELLO_ELF}"
need_file "${SIGNATURE_DB_SRC}"
if [[ -n "${RUST_STD_DEMO_BIN}" ]]; then
    need_file "${RUST_STD_DEMO_BIN}"
fi

stage_driver_bundle() {
    local manifest_src="$1"
    local entry_bin="$2"
    local bundle_root="$3"
    if [[ -z "${manifest_src}" || -z "${entry_bin}" ]]; then
        return
    fi
    mkdir -p "${ROOTFS_STAGE}${bundle_root}"
    install -m 0755 "${entry_bin}" "${ROOTFS_STAGE}${bundle_root}/entry.elf"
}

stage_package_manifest() {
    local manifest_src="$1"
    local manifest_dst="$2"
    if [[ -z "${manifest_src}" ]]; then
        return
    fi
    mkdir -p "${ROOTFS_STAGE}$(dirname "${manifest_dst}")"
    install -m 0644 "${manifest_src}" "${ROOTFS_STAGE}${manifest_dst}"
}

rm -rf "${ROOTFS_STAGE}"
mkdir -p "${ROOTFS_STAGE}/bin"

install -m 0755 "${HELLO_ELF}" "${ROOTFS_STAGE}/bin/hello"
if [[ -n "${RUST_STD_DEMO_BIN}" ]]; then
    install -m 0755 "${RUST_STD_DEMO_BIN}" "${ROOTFS_STAGE}/bin/rust-std-demo"
    : > "${ROOTFS_STAGE}/rust.txt"
fi
if [[ -n "${MSH_BIN}" ]]; then
    install -m 0755 "${MSH_BIN}" "${ROOTFS_STAGE}/bin/msh"
fi
if [[ -n "${MSH_FONT_SRC}" ]]; then
    mkdir -p "${ROOTFS_STAGE}/system/resources/msh"
    install -m 0644 "${MSH_FONT_SRC}" "${ROOTFS_STAGE}/system/resources/msh/ter-u12b.bdf"
fi
if [[ -n "${COREUTILS_BIN_DIR}" ]]; then
    for coreutil in echo ls pwd true false cat touch rm mpk selftest-capability; do
        install -m 0755 "${COREUTILS_BIN_DIR}/${coreutil}" "${ROOTFS_STAGE}/bin/${coreutil}"
    done
fi
install -m 0644 "${SIGNATURE_DB_SRC}" "${ROOTFS_STAGE}/signature.db"

mkdir -p "${ROOTFS_STAGE}/system/packages"
if [[ -n "${MPK_DEMO_MPKG_SRC}" ]]; then
    mkdir -p "${ROOTFS_STAGE}/system/samples"
    install -m 0644 "${MPK_DEMO_MPKG_SRC}" "${ROOTFS_STAGE}/system/samples/mpk-demo.mpkg"
fi
if [[ -n "${MPK_TEST_MPKG_SRC}" ]]; then
    mkdir -p "${ROOTFS_STAGE}/system/samples"
    install -m 0644 "${MPK_TEST_MPKG_SRC}" "${ROOTFS_STAGE}/system/samples/mpk-test.mpkg"
fi
stage_package_manifest "${RUST_STD_DEMO_MANIFEST_SRC}" "/system/packages/rust-std-demo/manifest.toml"
stage_package_manifest "${MSH_MANIFEST_SRC}" "/system/packages/msh/manifest.toml"
stage_package_manifest "${COREUTILS_MANIFEST_SRC}" "/system/packages/coreutils/manifest.toml"

if [[ -n "${CAPABILITY_SERVICE_BIN}" || -n "${DRIVERS_SERVICE_BIN}" || -n "${LOGGER_SERVICE_BIN}" || -n "${INPUT_SERVICE_BIN}" || -n "${PACKAGE_SERVICE_BIN}" || -n "${SIGNATURE_SERVICE_BIN}" || -n "${TTY_SERVICE_BIN}" ]]; then
    mkdir -p "${ROOTFS_STAGE}/system/services"
fi
if [[ -n "${CAPABILITY_SERVICE_BIN}" ]]; then
    install -m 0755 "${CAPABILITY_SERVICE_BIN}" "${ROOTFS_STAGE}/system/services/capability.service"
fi
if [[ -n "${DRIVERS_SERVICE_BIN}" ]]; then
    install -m 0755 "${DRIVERS_SERVICE_BIN}" "${ROOTFS_STAGE}/system/services/drivers.service"
fi
if [[ -n "${LOGGER_SERVICE_BIN}" ]]; then
    install -m 0755 "${LOGGER_SERVICE_BIN}" "${ROOTFS_STAGE}/system/services/logger.service"
fi
if [[ -n "${INPUT_SERVICE_BIN}" ]]; then
    install -m 0755 "${INPUT_SERVICE_BIN}" "${ROOTFS_STAGE}/system/services/input.service"
fi
if [[ -n "${PACKAGE_SERVICE_BIN}" ]]; then
    install -m 0755 "${PACKAGE_SERVICE_BIN}" "${ROOTFS_STAGE}/system/services/package.service"
fi
if [[ -n "${SIGNATURE_SERVICE_BIN}" ]]; then
    install -m 0755 "${SIGNATURE_SERVICE_BIN}" "${ROOTFS_STAGE}/system/services/signature.service"
fi
if [[ -n "${TTY_SERVICE_BIN}" ]]; then
    install -m 0755 "${TTY_SERVICE_BIN}" "${ROOTFS_STAGE}/system/services/tty.service"
fi

stage_package_manifest "${CAPABILITY_SERVICE_MANIFEST_SRC}" "/system/packages/capability/manifest.toml"
stage_package_manifest "${DRIVERS_SERVICE_MANIFEST_SRC}" "/system/packages/drivers/manifest.toml"
stage_package_manifest "${LOGGER_SERVICE_MANIFEST_SRC}" "/system/packages/logger/manifest.toml"
stage_package_manifest "${INPUT_SERVICE_MANIFEST_SRC}" "/system/packages/input/manifest.toml"
stage_package_manifest "${PACKAGE_SERVICE_MANIFEST_SRC}" "/system/packages/package/manifest.toml"
stage_package_manifest "${SIGNATURE_SERVICE_MANIFEST_SRC}" "/system/packages/signature/manifest.toml"
stage_package_manifest "${TTY_SERVICE_MANIFEST_SRC}" "/system/packages/tty/manifest.toml"
stage_package_manifest "${USB_DRIVER_MANIFEST_SRC}" "/system/packages${USB_DRIVER_BUNDLE_ROOT#/bin}/manifest.toml"
stage_package_manifest "${I8042_DRIVER_MANIFEST_SRC}" "/system/packages${I8042_DRIVER_BUNDLE_ROOT#/bin}/manifest.toml"
stage_driver_bundle "${USB_DRIVER_MANIFEST_SRC}" "${USB_DRIVER_ENTRY_BIN}" "${USB_DRIVER_BUNDLE_ROOT}"
stage_driver_bundle "${I8042_DRIVER_MANIFEST_SRC}" "${I8042_DRIVER_ENTRY_BIN}" "${I8042_DRIVER_BUNDLE_ROOT}"

rm -f "${ROOTFS_IMG}"
truncate -s 16M "${ROOTFS_IMG}"
mke2fs -q -t ext2 -b 4096 -d "${ROOTFS_STAGE}" -F "${ROOTFS_IMG}"
