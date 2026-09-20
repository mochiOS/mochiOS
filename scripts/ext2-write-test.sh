#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
ARTIFACT_DIR="${ARTIFACT_DIR:-${ROOT_DIR}/out/artifacts}"
MSIGN="${MSIGN:-${ROOT_DIR}/out/mmake/host-tools/release/msign}"
TEST_OUTPUT_ROOT="${EXT2_TEST_OUTPUT_ROOT:-${ROOT_DIR}/out/mmake}"
SELFTEST_BINARY="${SELFTEST_BINARY:-${TEST_OUTPUT_ROOT}/image/rootfs/bin/selftest-ext2-write}"
mkdir -p "${TEST_OUTPUT_ROOT}"
RUN_DIR="$(mktemp -d "${TEST_OUTPUT_ROOT}/ext2-write-test.XXXXXX")"
export TMPDIR="${RUN_DIR}"
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
[[ -f "${SELFTEST_BINARY}" ]] || die "missing mmake-built selftest-ext2-write: ${SELFTEST_BINARY}"
[[ -x "${MSIGN}" ]] || die "missing built-in record tool: ${MSIGN}"
[[ -f "${OVMF_CODE}" ]] || die "missing ${OVMF_CODE}"
[[ -f "${OVMF_VARS_TEMPLATE}" ]] || die "missing ${OVMF_VARS_TEMPLATE}"
if [[ "${EXT2_TEST_PREPARE_ONLY:-0}" == "1" ]]; then
    QEMU_CPU=qemu64
elif [[ "${QEMU_ACCEL}" == "kvm" ]]; then
    [[ -r /dev/kvm && -w /dev/kvm ]] || die "KVM requested but unavailable"
    QEMU_CPU=host
elif [[ "${QEMU_ACCEL}" != "tcg" ]]; then
    die "QEMU_ACCELERATOR must be kvm or tcg"
else
    QEMU_CPU=qemu64
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

install_test_manifest() {
    local package=$1 fixture=$2 record="${RUN_DIR}/${1}.verification.bin"
    "${MSIGN}" package built-in-record "${fixture}" --output "${record}"
    debugfs -w -R "rm /system/packages/${package}/manifest.toml" "${ROOTFS_IMAGE}" >/dev/null 2>&1
    debugfs -w -R "write ${fixture} /system/packages/${package}/manifest.toml" \
        "${ROOTFS_IMAGE}" >/dev/null 2>&1
    debugfs -w -R "rm /system/packages/${package}/verification.bin" "${ROOTFS_IMAGE}" >/dev/null 2>&1
    debugfs -w -R "write ${record} /system/packages/${package}/verification.bin" \
        "${ROOTFS_IMAGE}" >/dev/null 2>&1
    cmp -s "${fixture}" <(debugfs -R "cat /system/packages/${package}/manifest.toml" \
        "${ROOTFS_IMAGE}" 2>/dev/null) || die "${package} test manifest was not installed"
    cmp -s "${record}" <(debugfs -R "cat /system/packages/${package}/verification.bin" \
        "${ROOTFS_IMAGE}" 2>/dev/null) || die "${package} test verification record was not installed"
}

install_test_entrypoint() {
    extract_rootfs
    debugfs -w -R 'rm /applications/Binder.app/entry.elf' "${ROOTFS_IMAGE}" >/dev/null 2>&1
    debugfs -w -R "write ${SELFTEST_BINARY} /applications/Binder.app/entry.elf" \
        "${ROOTFS_IMAGE}" >/dev/null 2>&1
    debugfs -w -R 'set_inode_field /applications/Binder.app/entry.elf mode 0100755' \
        "${ROOTFS_IMAGE}" >/dev/null 2>&1
    debugfs -R "dump /applications/Binder.app/entry.elf ${RUN_DIR}/installed-selftest" \
        "${ROOTFS_IMAGE}" >/dev/null 2>&1
    cmp -s "${SELFTEST_BINARY}" "${RUN_DIR}/installed-selftest" ||
        die "selftest entrypoint was not installed"
    install_test_manifest binder "${SCRIPT_DIR}/fixtures/ext2-write-binder-manifest.toml"
    e2fsck -fn "${ROOTFS_IMAGE}" >&2 || die "e2fsck failed before guest boot"
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
        if grep -Fq 'selftest-ext2-write: FAIL' "${log}"; then
            die "guest self-test failed; see ${log}"
        fi
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
        -m 1G -smp 1 -cpu "${QEMU_CPU}" -no-reboot -display none -monitor none \
        -serial "file:${log}" \
        -drive "if=pflash,format=raw,readonly=on,file=${OVMF_CODE}" \
        -drive "if=pflash,format=raw,file=${vars}" \
        -drive "id=osdisk,if=none,format=raw,file=${DISK_IMAGE}" \
        -device "virtio-blk-pci,disable-modern=on,drive=osdisk,bootindex=1" \
        -object "rng-random,id=rng0,filename=/dev/urandom" \
        -device "virtio-rng-pci,rng=rng0" &
    QEMU_PID=$!

    wait_for_log "${log}" "selftest-ext2-write: pass ${mode}"
    grep -Fq "exec: loaded '/applications/Binder.app/entry.elf'" "${log}" ||
        die "test entrypoint did not execute during ${mode} boot"
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
    local filler_size
    local reserve_blocks=4096
    local stat_output

    extract_rootfs
    printf 'enospc\n' >"${marker_source}"
    debugfs -w -R "write ${marker_source} /tmp/ext2-write-enospc.mode" \
        "${ROOTFS_IMAGE}" >/dev/null 2>&1
    before="$(free_blocks)"
    ((before > reserve_blocks * 2)) || die "not enough free blocks to prepare ENOSPC fixture"
    filler_blocks=$((before - reserve_blocks))
    dd if=/dev/urandom of="${filler_source}" bs=4096 count="${filler_blocks}" status=none
    debugfs -w -R "write ${filler_source} /ext2-write-enospc-filler" \
        "${ROOTFS_IMAGE}" >/dev/null 2>&1
    stat_output="$(LC_ALL=C debugfs -R 'stat /ext2-write-enospc-filler' \
        "${ROOTFS_IMAGE}" 2>/dev/null)"
    [[ "${stat_output}" =~ Size:[[:space:]]*([0-9]+) ]] ||
        die "ENOSPC filler inode was not created"
    filler_size="${BASH_REMATCH[1]}"
    ((filler_size == filler_blocks * 4096)) ||
        die "ENOSPC filler was truncated: expected $((filler_blocks * 4096)) bytes, got ${filler_size}"
    after="$(free_blocks)"
    ((after >= reserve_blocks / 2 && after <= reserve_blocks)) ||
        die "ENOSPC fixture left unexpected free block count: ${after} (target ${reserve_blocks})"
    e2fsck -fn "${ROOTFS_IMAGE}" >&2 || die "e2fsck failed before ENOSPC boot"
    dd if="${ROOTFS_IMAGE}" of="${DISK_IMAGE}" bs=512 \
        seek="${ROOTFS_START_SECTOR}" conv=notrunc status=none
}

extract_and_check() {
    local marker="$1"
    extract_rootfs
    [[ "$(debugfs -R "cat ${marker}" "${ROOTFS_IMAGE}" 2>/dev/null)" == "pass" ]] ||
        die "missing successful guest marker ${marker}"
    e2fsck -fn "${ROOTFS_IMAGE}" >&2 || die "e2fsck failed after ${marker}"
}

install_test_entrypoint
if [[ "${EXT2_TEST_PREPARE_ONLY:-0}" == "1" ]]; then
    prepare_enospc_disk
    echo "ext2 write and ENOSPC fixtures validated"
    exit 0
fi
boot_and_wait prepare
extract_and_check /tmp/ext2-write-prepare.pass
boot_and_wait verify
extract_and_check /tmp/ext2-write-verify.pass
prepare_enospc_disk
boot_and_wait enospc
extract_rootfs
e2fsck -fn "${ROOTFS_IMAGE}" >&2 || die "e2fsck failed after ENOSPC boot"

echo "ext2 write persistence test passed (${QEMU_ACCEL})"
