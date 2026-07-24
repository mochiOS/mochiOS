#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
ARTIFACT_DIR="${ARTIFACT_DIR:-${ROOT_DIR}/out/artifacts}"
RUN_DIR="$(mktemp -d "/tmp/mochios-ext2-write-test.XXXXXX")"
DISK_IMAGE="${RUN_DIR}/disk.img"
ROOTFS_IMAGE="${RUN_DIR}/rootfs.img"
OVMF_CODE="${OVMF_CODE:-/usr/share/OVMF/OVMF_CODE_4M.fd}"
OVMF_VARS_TEMPLATE="${OVMF_VARS_TEMPLATE:-/usr/share/OVMF/OVMF_VARS_4M.fd}"
QEMU_ACCEL="${QEMU_ACCELERATOR:-kvm}"
QEMU_TIMEOUT_SECONDS="${QEMU_TIMEOUT_SECONDS:-120}"
QEMU_PID=""

die() {
    echo "fatal: $*" >&2
    exit 1
}

cleanup() {
    local status=$?
    if [[ -n "${QEMU_PID}" ]]; then
        kill -TERM "${QEMU_PID}" 2>/dev/null || true
        wait "${QEMU_PID}" 2>/dev/null || true
    fi
    if [[ "${status}" -eq 0 && "${KEEP_TEST_OUTPUT:-0}" != "1" ]]; then
        rm -rf "${RUN_DIR}"
    else
        echo "ext2 write test output retained at ${RUN_DIR}" >&2
    fi
}
trap cleanup EXIT

for command in qemu-system-x86_64 dd debugfs dumpe2fs e2fsck; do
    command -v "${command}" >/dev/null 2>&1 || die "required command not found: ${command}"
done
[[ -f "${ARTIFACT_DIR}/disk.img" ]] || die "missing ${ARTIFACT_DIR}/disk.img"
[[ -f "${ARTIFACT_DIR}/selftest-ext2-write" ]] || die "missing selftest-ext2-write artifact"
[[ -f "${OVMF_CODE}" ]] || die "missing ${OVMF_CODE}"
[[ -f "${OVMF_VARS_TEMPLATE}" ]] || die "missing ${OVMF_VARS_TEMPLATE}"
if [[ "${QEMU_ACCEL}" == "kvm" ]]; then
    [[ -r /dev/kvm && -w /dev/kvm ]] || die "KVM requested but unavailable"
elif [[ "${QEMU_ACCEL}" != "tcg" ]]; then
    die "QEMU_ACCELERATOR must be kvm or tcg"
fi

# shellcheck disable=SC1091
source "${ROOT_DIR}/.config"
ROOTFS_START_SECTOR=$((2048 + IMAGE_ESP_SIZE_MB * 2048))
ROOTFS_SIZE_SECTORS=$(((IMAGE_DISK_SIZE_MB - IMAGE_ESP_SIZE_MB - 2) * 2048))
cp "${ARTIFACT_DIR}/disk.img" "${DISK_IMAGE}"

extract_rootfs() {
    dd if="${DISK_IMAGE}" of="${ROOTFS_IMAGE}" bs=512 \
        skip="${ROOTFS_START_SECTOR}" count="${ROOTFS_SIZE_SECTORS}" status=none
}

install_test_entrypoint() {
    extract_rootfs
    debugfs -w -R 'rm /bin/msh' "${ROOTFS_IMAGE}" >/dev/null 2>&1
    debugfs -w -R "write ${ARTIFACT_DIR}/selftest-ext2-write /bin/msh" \
        "${ROOTFS_IMAGE}" >/dev/null 2>&1
    debugfs -w -R 'set_inode_field /bin/msh mode 0100755' \
        "${ROOTFS_IMAGE}" >/dev/null 2>&1
    debugfs -w -R 'rm /system/packages/msh/manifest.toml' \
        "${ROOTFS_IMAGE}" >/dev/null 2>&1
    debugfs -w -R "write ${SCRIPT_DIR}/fixtures/ext2-write-msh-manifest.toml /system/packages/msh/manifest.toml" \
        "${ROOTFS_IMAGE}" >/dev/null 2>&1
    debugfs -w -R 'rm /system/packages/service-manager/manifest.toml' \
        "${ROOTFS_IMAGE}" >/dev/null 2>&1
    debugfs -w -R "write ${SCRIPT_DIR}/fixtures/ext2-write-service-manager-manifest.toml /system/packages/service-manager/manifest.toml" \
        "${ROOTFS_IMAGE}" >/dev/null 2>&1
    debugfs -w -R 'rm /system/packages/tty/manifest.toml' \
        "${ROOTFS_IMAGE}" >/dev/null 2>&1
    debugfs -w -R "write ${SCRIPT_DIR}/fixtures/ext2-write-tty-manifest.toml /system/packages/tty/manifest.toml" \
        "${ROOTFS_IMAGE}" >/dev/null 2>&1
    dd if="${ROOTFS_IMAGE}" of="${DISK_IMAGE}" bs=512 \
        seek="${ROOTFS_START_SECTOR}" conv=notrunc status=none
}

wait_for_log() {
    local log="$1"
    local pattern="$2"
    local deadline=$((SECONDS + QEMU_TIMEOUT_SECONDS))
    until grep -Fq "${pattern}" "${log}"; do
        ((SECONDS < deadline)) || die "timed out waiting for '${pattern}' in ${log}"
        kill -0 "${QEMU_PID}" 2>/dev/null || die "QEMU exited while waiting for '${pattern}'"
        if grep -Fq 'Process exiting with code: 1' "${log}"; then
            die "guest self-test failed while waiting for '${pattern}' in ${log}"
        fi
        sleep 0.1
    done
}

boot_and_wait() {
    local mode="$1"
    local log="${RUN_DIR}/${mode}.serial.log"
    local vars="${RUN_DIR}/${mode}.vars.fd"
    cp "${OVMF_VARS_TEMPLATE}" "${vars}"
    : >"${log}"

    qemu-system-x86_64 \
        -machine "q35,accel=${QEMU_ACCEL}" \
        -m 512M -smp 1 -cpu qemu64 -no-reboot -display none -monitor none \
        -serial "file:${log}" \
        -drive "if=pflash,format=raw,readonly=on,file=${OVMF_CODE}" \
        -drive "if=pflash,format=raw,file=${vars}" \
        -drive "id=osdisk,if=none,format=raw,file=${DISK_IMAGE}" \
        -device "virtio-blk-pci,disable-modern=on,drive=osdisk,bootindex=1" &
    QEMU_PID=$!

    wait_for_log "${log}" "selftest-ext2-write: pass ${mode}"
    grep -Fq "exec: loaded '/system/services/tty.service'" "${log}" ||
        die "existing tty.service did not execute during ${mode} boot"
    if grep -Eq 'PAGE FAULT|Faulting user context:|panic' "${log}"; then
        die "fault or panic during selftest-ext2-write ${mode}"
    fi
    kill -TERM "${QEMU_PID}"
    wait "${QEMU_PID}" 2>/dev/null || true
    QEMU_PID=""
}

free_blocks() {
    local line
    line="$(dumpe2fs -h "${ROOTFS_IMAGE}" 2>/dev/null | grep '^Free blocks:')"
    line="${line#*:}"
    echo "${line//[[:space:]]/}"
}

prepare_enospc_disk() {
    local marker_source="${RUN_DIR}/enospc.mode"
    local filler_source="${RUN_DIR}/enospc.filler"
    local before
    local filler_blocks
    local after

    extract_rootfs
    printf 'enospc\n' >"${marker_source}"
    debugfs -w -R "write ${marker_source} /tmp/ext2-write-enospc.mode" \
        "${ROOTFS_IMAGE}" >/dev/null 2>&1
    before="$(free_blocks)"
    ((before > 128)) || die "not enough free blocks to prepare ENOSPC fixture"
    filler_blocks=$((before - 128))
    dd if=/dev/urandom of="${filler_source}" bs=4096 count="${filler_blocks}" status=none
    debugfs -w -R "write ${filler_source} /ext2-write-enospc-filler" \
        "${ROOTFS_IMAGE}" >/dev/null 2>&1
    after="$(free_blocks)"
    ((after > 0 && after < 1036)) ||
        die "ENOSPC fixture left unexpected free block count: ${after}"
    e2fsck -fn "${ROOTFS_IMAGE}" >/dev/null || die "e2fsck failed before ENOSPC boot"
    dd if="${ROOTFS_IMAGE}" of="${DISK_IMAGE}" bs=512 \
        seek="${ROOTFS_START_SECTOR}" conv=notrunc status=none
}

extract_and_check() {
    local marker="$1"
    extract_rootfs
    [[ "$(debugfs -R "cat ${marker}" "${ROOTFS_IMAGE}" 2>/dev/null)" == "pass" ]] ||
        die "missing successful guest marker ${marker}"
    e2fsck -fn "${ROOTFS_IMAGE}" >/dev/null || die "e2fsck failed after ${marker}"
}

install_test_entrypoint
boot_and_wait prepare
extract_and_check /tmp/ext2-write-prepare.pass
boot_and_wait verify
extract_and_check /tmp/ext2-write-verify.pass
prepare_enospc_disk
boot_and_wait enospc
extract_rootfs
e2fsck -fn "${ROOTFS_IMAGE}" >/dev/null || die "e2fsck failed after ENOSPC boot"

echo "ext2 write persistence test passed (${QEMU_ACCEL})"
