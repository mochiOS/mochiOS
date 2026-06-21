#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

ROOTFS_STAGE="${ROOTFS_STAGE:?ROOTFS_STAGE is required}"
ROOTFS_IMG="${ROOTFS_IMG:?ROOTFS_IMG is required}"
HELLO_ELF="${HELLO_ELF:?HELLO_ELF is required}"
SIGNATURE_DB_SRC="${SIGNATURE_DB_SRC:?SIGNATURE_DB_SRC is required}"

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

rm -rf "${ROOTFS_STAGE}"
mkdir -p "${ROOTFS_STAGE}/bin"

install -m 0755 "${HELLO_ELF}" "${ROOTFS_STAGE}/bin/hello"
install -m 0644 "${SIGNATURE_DB_SRC}" "${ROOTFS_STAGE}/signature.db"

rm -f "${ROOTFS_IMG}"
truncate -s 16M "${ROOTFS_IMG}"
mke2fs -q -t ext2 -b 1024 -d "${ROOTFS_STAGE}" -F "${ROOTFS_IMG}"
