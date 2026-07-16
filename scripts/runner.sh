#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
CONFIG_FILE="${ROOT_DIR}/.config"
if [[ -f "${CONFIG_FILE}" ]]; then
    # shellcheck disable=SC1090
    source "${CONFIG_FILE}"
fi
ARTIFACT_DIR="${ARTIFACT_DIR:-${ROOT_DIR}/out/artifacts}"
RUN_ID="workspace-$(date +%s)-$$"
RUN_DIR="${ROOT_DIR}/out/runner/${RUN_ID}"
SERIAL_LOG="${RUN_DIR}/serial.log"
OVMF_CODE="${OVMF_CODE:-/usr/share/OVMF/OVMF_CODE_4M.fd}"
OVMF_VARS_TEMPLATE="${OVMF_VARS_TEMPLATE:-/usr/share/OVMF/OVMF_VARS_4M.fd}"
OVMF_VARS="${RUN_DIR}/OVMF_VARS_4M.fd"
GUI_MODE=1
if [[ "${DRIVER_XHCI:-n}" == "y" ]]; then
    ENABLE_XHCI="1"
else
    ENABLE_XHCI="0"
fi
if [[ "${DEBUG_QEMU_KVM:-y}" == "y" || "${KVM:-0}" == "1" ]]; then
    ENABLE_KVM="1"
else
    ENABLE_KVM="0"
fi
if [[ "${ENABLE_KVM}" == "1" ]]; then
    QEMU_ACCEL="kvm"
    QEMU_CPU="host"
else
    QEMU_ACCEL="tcg"
    QEMU_CPU="qemu64"
fi

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

need_file "${ARTIFACT_DIR}/disk.img"
need_file "${OVMF_CODE}"
need_file "${OVMF_VARS_TEMPLATE}"

mkdir -p "${RUN_DIR}"
cp "${OVMF_VARS_TEMPLATE}" "${OVMF_VARS}"
: > "${SERIAL_LOG}"

if [[ "${ENABLE_KVM}" == "1" && ( ! -r /dev/kvm || ! -w /dev/kvm ) ]]; then
    die "KVM requested but /dev/kvm is not readable/writable; set DEBUG_QEMU_KVM=n or fix /dev/kvm permissions"
fi

QEMU_ARGS=(
    -machine "q35,accel=${QEMU_ACCEL}"
    -m 512M
    -smp 4
    -cpu "${QEMU_CPU}"
    -serial stdio
    -no-reboot
    -drive "if=pflash,format=raw,readonly=on,file=${OVMF_CODE}"
    -drive "if=pflash,format=raw,file=${OVMF_VARS}"
    -drive "id=osdisk,if=none,format=raw,file=${ARTIFACT_DIR}/disk.img"
    -device "virtio-blk-pci,disable-modern=on,drive=osdisk,bootindex=1"
)

if [[ "${ENABLE_XHCI}" == "1" ]]; then
    QEMU_ARGS+=(
        -device "qemu-xhci,id=xhci"
        -device "usb-tablet,bus=xhci.0"
    )
fi

if [[ "${DEBUG_QEMU_GUI:-y}" != "y" || "${NOGUI:-0}" == "1" ]]; then
    GUI_MODE=0
    QEMU_ARGS+=(
        -display none
        -monitor none
    )
fi

if [[ "${DEBUG_QEMU_DEBUG:-n}" == "y" || "${DEBUG:-0}" != "0" ]]; then
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
DRIVERS_SERVICE_FOUND=0
USB_BUNDLE_FOUND=0
USB_DRIVER_FOUND=0
USB_CONTROLLER_FOUND=0
USB_ENUM_DONE_FOUND=0
NEXT_LINE=1

for _ in $(seq 1 900); do
    while IFS= read -r line; do
        [[ "${line}" == *"cext: loaded bundle disk"* ]] && DISK_FOUND=1
        [[ "${line}" == *"cext: loaded bundle ext2"* ]] && EXT2_FOUND=1
        [[ "${line}" == *"exec: loaded 'core.service' from initfs"* ]] && SERVICE_FOUND=1
        [[ "${line}" == *"core.service: drivers.service spawned pid="* ]] && DRIVERS_SERVICE_FOUND=1
        if [[ "${ENABLE_XHCI}" == "1" ]]; then
            [[ "${line}" == *"drivers.service: bundle verified /bin/drivers/usb/"* ]] && USB_BUNDLE_FOUND=1
            [[ "${line}" == *"drivers.service: spawned driver pid="* ]] && USB_DRIVER_FOUND=1
            [[ "${line}" == *"usb-driver: PCI USB controller"* ]] && USB_CONTROLLER_FOUND=1
            [[ "${line}" == *"usb-driver: enumeration complete"* ]] && USB_ENUM_DONE_FOUND=1
        fi
        if [[ "${line}" == *"PAGE FAULT"* || "${line}" == *"Faulting user context:"* || "${line}" == *"panic"* ]]; then
            die "fault or panic observed during QEMU run"
        fi
    done < <(sed -n "${NEXT_LINE},\$p" "${SERIAL_LOG}")

    NEXT_LINE="$(($(wc -l < "${SERIAL_LOG}") + 1))"

    if [[ "${DISK_FOUND}" -eq 1 && "${EXT2_FOUND}" -eq 1 && "${SERVICE_FOUND}" -eq 1 && "${DRIVERS_SERVICE_FOUND}" -eq 1 ]]; then
        if [[ "${ENABLE_XHCI}" != "1" || ( "${USB_BUNDLE_FOUND}" -eq 1 && "${USB_DRIVER_FOUND}" -eq 1 && "${USB_CONTROLLER_FOUND}" -eq 1 && "${USB_ENUM_DONE_FOUND}" -eq 1 ) ]]; then
            break
        fi
    fi

    if ! kill -0 "${QEMU_PID}" 2>/dev/null; then
        break
    fi

    sleep 0.1
done

if [[ "${GUI_MODE}" -eq 1 ]]; then
    trap - EXIT
    echo "[done] serial log: ${SERIAL_LOG}"
    wait "${QEMU_PID}"
    exit 0
fi

[[ "${DISK_FOUND}" -eq 1 ]] || die "disk.cext load was not observed; see ${SERIAL_LOG}"
[[ "${EXT2_FOUND}" -eq 1 ]] || die "ext2.cext load was not observed; see ${SERIAL_LOG}"
[[ "${SERVICE_FOUND}" -eq 1 ]] || die "core.service launch was not observed; see ${SERIAL_LOG}"
[[ "${DRIVERS_SERVICE_FOUND}" -eq 1 ]] || die "drivers.service launch was not observed; see ${SERIAL_LOG}"
if [[ "${ENABLE_XHCI}" == "1" ]]; then
    [[ "${USB_BUNDLE_FOUND}" -eq 1 ]] || die "USB driver bundle verification was not observed; see ${SERIAL_LOG}"
    [[ "${USB_DRIVER_FOUND}" -eq 1 ]] || die "USB driver launch was not observed; see ${SERIAL_LOG}"
    [[ "${USB_CONTROLLER_FOUND}" -eq 1 ]] || die "USB controller detection was not observed; see ${SERIAL_LOG}"
    [[ "${USB_ENUM_DONE_FOUND}" -eq 1 ]] || die "USB driver completion was not observed; see ${SERIAL_LOG}"
fi

kill "${QEMU_PID}" 2>/dev/null || true
wait "${QEMU_PID}" 2>/dev/null || true
trap - EXIT

echo "[done] serial log: ${SERIAL_LOG}"
