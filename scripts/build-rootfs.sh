#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

ROOTFS_STAGE="${ROOTFS_STAGE:?ROOTFS_STAGE is required}"
ROOTFS_IMG="${ROOTFS_IMG:?ROOTFS_IMG is required}"
HELLO_ELF="${HELLO_ELF:?HELLO_ELF is required}"
SIGNATURE_DB_SRC="${SIGNATURE_DB_SRC:?SIGNATURE_DB_SRC is required}"
CORE_SERVICE_BIN="${CORE_SERVICE_BIN:-}"
CAPABILITY_SERVICE_BIN="${CAPABILITY_SERVICE_BIN:-}"
CAPABILITY_SERVICE_MANIFEST_SRC="${CAPABILITY_SERVICE_MANIFEST_SRC:-}"
DRIVERS_SERVICE_BIN="${DRIVERS_SERVICE_BIN:-}"
DRIVERS_SERVICE_MANIFEST_SRC="${DRIVERS_SERVICE_MANIFEST_SRC:-}"
USB_DRIVER_MANIFEST_SRC="${USB_DRIVER_MANIFEST_SRC:-}"
USB_DRIVER_ENTRY_BIN="${USB_DRIVER_ENTRY_BIN:-}"
USB_DRIVER_BUNDLE_ROOT="${USB_DRIVER_BUNDLE_ROOT:-/bin/drivers/usb/qemu-usb.driver}"
PS2_KEYBOARD_DRIVER_MANIFEST_SRC="${PS2_KEYBOARD_DRIVER_MANIFEST_SRC:-}"
PS2_KEYBOARD_DRIVER_ENTRY_BIN="${PS2_KEYBOARD_DRIVER_ENTRY_BIN:-}"
PS2_KEYBOARD_DRIVER_BUNDLE_ROOT="${PS2_KEYBOARD_DRIVER_BUNDLE_ROOT:-/bin/drivers/ps2/keyboard.driver}"
PS2_MOUSE_DRIVER_MANIFEST_SRC="${PS2_MOUSE_DRIVER_MANIFEST_SRC:-}"
PS2_MOUSE_DRIVER_ENTRY_BIN="${PS2_MOUSE_DRIVER_ENTRY_BIN:-}"
PS2_MOUSE_DRIVER_BUNDLE_ROOT="${PS2_MOUSE_DRIVER_BUNDLE_ROOT:-/bin/drivers/ps2/mouse.driver}"

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

stage_driver_bundle() {
    local manifest_src="$1"
    local entry_bin="$2"
    local bundle_root="$3"
    if [[ -z "${manifest_src}" || -z "${entry_bin}" ]]; then
        return
    fi
    mkdir -p "${ROOTFS_STAGE}${bundle_root}"
    install -m 0644 "${manifest_src}" "${ROOTFS_STAGE}${bundle_root}/about.toml"
    install -m 0755 "${entry_bin}" "${ROOTFS_STAGE}${bundle_root}/entry.elf"
}

rm -rf "${ROOTFS_STAGE}"
mkdir -p "${ROOTFS_STAGE}/bin"

install -m 0755 "${HELLO_ELF}" "${ROOTFS_STAGE}/bin/hello"
install -m 0644 "${SIGNATURE_DB_SRC}" "${ROOTFS_STAGE}/signature.db"

if [[ -n "${CAPABILITY_SERVICE_BIN}" || -n "${DRIVERS_SERVICE_BIN}" ]]; then
    mkdir -p "${ROOTFS_STAGE}/system/services"
fi
if [[ -n "${CAPABILITY_SERVICE_BIN}" ]]; then
    install -m 0755 "${CAPABILITY_SERVICE_BIN}" "${ROOTFS_STAGE}/system/services/capability.service"
fi
if [[ -n "${CAPABILITY_SERVICE_MANIFEST_SRC}" ]]; then
    install -m 0644 "${CAPABILITY_SERVICE_MANIFEST_SRC}" "${ROOTFS_STAGE}/system/services/capability.service.toml"
fi
if [[ -n "${DRIVERS_SERVICE_BIN}" ]]; then
    install -m 0755 "${DRIVERS_SERVICE_BIN}" "${ROOTFS_STAGE}/system/services/drivers.service"
fi
if [[ -n "${DRIVERS_SERVICE_MANIFEST_SRC}" ]]; then
    install -m 0644 "${DRIVERS_SERVICE_MANIFEST_SRC}" "${ROOTFS_STAGE}/system/services/drivers.service.toml"
fi
stage_driver_bundle "${USB_DRIVER_MANIFEST_SRC}" "${USB_DRIVER_ENTRY_BIN}" "${USB_DRIVER_BUNDLE_ROOT}"
stage_driver_bundle "${PS2_KEYBOARD_DRIVER_MANIFEST_SRC}" "${PS2_KEYBOARD_DRIVER_ENTRY_BIN}" "${PS2_KEYBOARD_DRIVER_BUNDLE_ROOT}"
stage_driver_bundle "${PS2_MOUSE_DRIVER_MANIFEST_SRC}" "${PS2_MOUSE_DRIVER_ENTRY_BIN}" "${PS2_MOUSE_DRIVER_BUNDLE_ROOT}"

rm -f "${ROOTFS_IMG}"
truncate -s 16M "${ROOTFS_IMG}"
mke2fs -q -t ext2 -b 4096 -d "${ROOTFS_STAGE}" -F "${ROOTFS_IMG}"
