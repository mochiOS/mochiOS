#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
ARTIFACT_DIR="${ARTIFACT_DIR:-${ROOT_DIR}/out/artifacts}"
RUN_ID="workspace-$(date +%s)-$$"
RUN_DIR="${ROOT_DIR}/out/runner/${RUN_ID}"
SERIAL_LOG="${RUN_DIR}/serial.log"
OVMF_CODE="${OVMF_CODE:-/usr/share/OVMF/OVMF_CODE_4M.fd}"
OVMF_VARS_TEMPLATE="${OVMF_VARS_TEMPLATE:-/usr/share/OVMF/OVMF_VARS_4M.fd}"
OVMF_VARS="${RUN_DIR}/OVMF_VARS_4M.fd"

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

need_cmd qemu-system-x86_64
need_cmd sed
need_cmd tee
need_cmd wc

need_file "${ARTIFACT_DIR}/esp.img"
need_file "${ARTIFACT_DIR}/rootfs.img"
need_file "${OVMF_CODE}"
need_file "${OVMF_VARS_TEMPLATE}"

mkdir -p "${RUN_DIR}"
cp "${OVMF_VARS_TEMPLATE}" "${OVMF_VARS}"
: > "${SERIAL_LOG}"

QEMU_ARGS=(
    -machine q35
    -m 512M
    -smp 4
    -cpu qemu64
    -serial stdio
    -display none
    -monitor none
    -no-reboot
    -drive "if=pflash,format=raw,readonly=on,file=${OVMF_CODE}"
    -drive "if=pflash,format=raw,file=${OVMF_VARS}"
    -drive "format=raw,file=${ARTIFACT_DIR}/esp.img"
    -drive "id=rootfs,if=none,format=raw,file=${ARTIFACT_DIR}/rootfs.img"
    -device "virtio-blk-pci,disable-modern=on,drive=rootfs"
)

if [[ "${DEBUG:-0}" != "0" ]]; then
    QEMU_ARGS+=(-s -S)
fi

echo "[run] qemu"
qemu-system-x86_64 "${QEMU_ARGS[@]}" > >(tee -a "${SERIAL_LOG}") 2>&1 &
QEMU_PID=$!

cleanup() {
    if [[ -n "${QEMU_PID:-}" ]]; then
        kill "${QEMU_PID}" 2>/dev/null || true
        wait "${QEMU_PID}" 2>/dev/null || true
    fi
}
trap cleanup EXIT

DISK_FOUND=0
EXT2_FOUND=0
SERVICE_FOUND=0
ROOTFS_EXEC_FOUND=0
HELLO_FOUND=0
WAITPID_FOUND=0
EXIT_FOUND=0
PERSIST_FOUND=0
NEXT_LINE=1

for _ in $(seq 1 900); do
    while IFS= read -r line; do
        [[ "${line}" == *"cext: loaded bundle disk"* ]] && DISK_FOUND=1
        [[ "${line}" == *"cext: loaded bundle ext2"* ]] && EXT2_FOUND=1
        [[ "${line}" == *"exec: loaded 'core.service' from initfs"* ]] && SERVICE_FOUND=1
        if [[ "${line}" == *"execve: loaded '/bin/hello' from cext"* || "${line}" == *"exec: loaded '/bin/hello' from cext"* ]]; then
            ROOTFS_EXEC_FOUND=1
        fi
        [[ "${line}" == *"core.service: persist verified"* ]] && PERSIST_FOUND=1
        [[ "${line}" == *"hello from mochiOS, argc=2"* ]] && HELLO_FOUND=1
        [[ "${line}" == *"waitpid status=0 exited=1 code=0"* ]] && WAITPID_FOUND=1
        [[ "${line}" == *"Process exiting with code: 0"* ]] && EXIT_FOUND=1
        if [[ "${line}" == *"PAGE FAULT"* || "${line}" == *"Faulting user context:"* || "${line}" == *"panic"* ]]; then
            die "fault or panic observed during QEMU run"
        fi
    done < <(sed -n "${NEXT_LINE},\$p" "${SERIAL_LOG}")

    NEXT_LINE="$(($(wc -l < "${SERIAL_LOG}") + 1))"

    if [[ "${DISK_FOUND}" -eq 1 && "${EXT2_FOUND}" -eq 1 && "${SERVICE_FOUND}" -eq 1 && "${ROOTFS_EXEC_FOUND}" -eq 1 && "${PERSIST_FOUND}" -eq 1 && "${HELLO_FOUND}" -eq 1 && "${WAITPID_FOUND}" -eq 1 && "${EXIT_FOUND}" -eq 1 ]]; then
        break
    fi

    if ! kill -0 "${QEMU_PID}" 2>/dev/null; then
        break
    fi

    sleep 0.1
done

[[ "${DISK_FOUND}" -eq 1 ]] || die "disk.cext load was not observed; see ${SERIAL_LOG}"
[[ "${EXT2_FOUND}" -eq 1 ]] || die "ext2.cext load was not observed; see ${SERIAL_LOG}"
[[ "${SERVICE_FOUND}" -eq 1 ]] || die "core.service launch was not observed; see ${SERIAL_LOG}"
[[ "${ROOTFS_EXEC_FOUND}" -eq 1 ]] || die "/bin/hello was not loaded from ext2 rootfs; see ${SERIAL_LOG}"
[[ "${PERSIST_FOUND}" -eq 1 ]] || die "rootfs persistence verification was not observed; see ${SERIAL_LOG}"
[[ "${HELLO_FOUND}" -eq 1 ]] || die "hello output was not observed; see ${SERIAL_LOG}"
[[ "${WAITPID_FOUND}" -eq 1 ]] || die "waitpid success output was not observed; see ${SERIAL_LOG}"
[[ "${EXIT_FOUND}" -eq 1 ]] || die "process exit(0) was not observed; see ${SERIAL_LOG}"

kill "${QEMU_PID}" 2>/dev/null || true
wait "${QEMU_PID}" 2>/dev/null || true
trap - EXIT

echo "[done] serial log: ${SERIAL_LOG}"
