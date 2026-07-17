#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
CONFIG_FILE="${ROOT_DIR}/.config"
ENV_DEBUG_QEMU_KVM_SET=0
ENV_DEBUG_QEMU_CPU_SET=0
ENV_DEBUG_QEMU_SMP_SET=0
ENV_DEBUG_QEMU_GUI_SET=0
ENV_DEBUG_QEMU_DEBUG_SET=0
ENV_DEBUG_QEMU_REQUIRE_USB_SET=0
ENV_DRIVER_XHCI_SET=0
if [[ -v DEBUG_QEMU_KVM ]]; then ENV_DEBUG_QEMU_KVM_SET=1; ENV_DEBUG_QEMU_KVM="${DEBUG_QEMU_KVM}"; fi
if [[ -v DEBUG_QEMU_CPU ]]; then ENV_DEBUG_QEMU_CPU_SET=1; ENV_DEBUG_QEMU_CPU="${DEBUG_QEMU_CPU}"; fi
if [[ -v DEBUG_QEMU_SMP ]]; then ENV_DEBUG_QEMU_SMP_SET=1; ENV_DEBUG_QEMU_SMP="${DEBUG_QEMU_SMP}"; fi
if [[ -v DEBUG_QEMU_GUI ]]; then ENV_DEBUG_QEMU_GUI_SET=1; ENV_DEBUG_QEMU_GUI="${DEBUG_QEMU_GUI}"; fi
if [[ -v DEBUG_QEMU_DEBUG ]]; then ENV_DEBUG_QEMU_DEBUG_SET=1; ENV_DEBUG_QEMU_DEBUG="${DEBUG_QEMU_DEBUG}"; fi
if [[ -v DEBUG_QEMU_REQUIRE_USB ]]; then ENV_DEBUG_QEMU_REQUIRE_USB_SET=1; ENV_DEBUG_QEMU_REQUIRE_USB="${DEBUG_QEMU_REQUIRE_USB}"; fi
if [[ -v DRIVER_XHCI ]]; then ENV_DRIVER_XHCI_SET=1; ENV_DRIVER_XHCI="${DRIVER_XHCI}"; fi
if [[ -f "${CONFIG_FILE}" ]]; then
    # shellcheck disable=SC1090
    source "${CONFIG_FILE}"
fi
if [[ "${ENV_DEBUG_QEMU_KVM_SET}" == "1" ]]; then DEBUG_QEMU_KVM="${ENV_DEBUG_QEMU_KVM}"; fi
if [[ "${ENV_DEBUG_QEMU_CPU_SET}" == "1" ]]; then DEBUG_QEMU_CPU="${ENV_DEBUG_QEMU_CPU}"; fi
if [[ "${ENV_DEBUG_QEMU_SMP_SET}" == "1" ]]; then DEBUG_QEMU_SMP="${ENV_DEBUG_QEMU_SMP}"; fi
if [[ "${ENV_DEBUG_QEMU_GUI_SET}" == "1" ]]; then DEBUG_QEMU_GUI="${ENV_DEBUG_QEMU_GUI}"; fi
if [[ "${ENV_DEBUG_QEMU_DEBUG_SET}" == "1" ]]; then DEBUG_QEMU_DEBUG="${ENV_DEBUG_QEMU_DEBUG}"; fi
if [[ "${ENV_DEBUG_QEMU_REQUIRE_USB_SET}" == "1" ]]; then DEBUG_QEMU_REQUIRE_USB="${ENV_DEBUG_QEMU_REQUIRE_USB}"; fi
if [[ "${ENV_DRIVER_XHCI_SET}" == "1" ]]; then DRIVER_XHCI="${ENV_DRIVER_XHCI}"; fi
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
else
    QEMU_ACCEL="tcg"
fi
QEMU_CPU="${DEBUG_QEMU_CPU:-qemu64}"
QEMU_SMP="${DEBUG_QEMU_SMP:-1}"

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
need_cmd grep
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
    -smp "${QEMU_SMP}"
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

NEXT_LINE=1

log_has() {
    grep -Fq "$1" "${SERIAL_LOG}"
}

for _ in $(seq 1 900); do
    while IFS= read -r line; do
        if [[ "${line}" == *"PAGE FAULT"* || "${line}" == *"Faulting user context:"* || "${line}" == *"panic"* ]]; then
            die "fault or panic observed during QEMU run"
        fi
    done < <(sed -n "${NEXT_LINE},\$p" "${SERIAL_LOG}")

    NEXT_LINE="$(($(wc -l < "${SERIAL_LOG}") + 1))"

    if log_has "cext: loaded bundle disk" \
        && log_has "cext: loaded bundle ext2" \
        && log_has "exec: loaded 'core.service' from initfs" \
        && log_has "core.service: drivers.service spawned pid="; then
        if [[ "${DEBUG_QEMU_REQUIRE_USB:-n}" != "y" ]] || { log_has "drivers.service: bundle verified /bin/drivers/usb/" \
            && log_has "drivers.service: spawned driver pid=" \
            && log_has "usb-driver: PCI USB controller" \
            && log_has "usb-driver: enumeration complete"; }; then
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

log_has "cext: loaded bundle disk" || die "disk.cext load was not observed; see ${SERIAL_LOG}"
log_has "cext: loaded bundle ext2" || die "ext2.cext load was not observed; see ${SERIAL_LOG}"
log_has "exec: loaded 'core.service' from initfs" || die "core.service launch was not observed; see ${SERIAL_LOG}"
log_has "core.service: drivers.service spawned pid=" || die "drivers.service launch was not observed; see ${SERIAL_LOG}"
if [[ "${DEBUG_QEMU_REQUIRE_USB:-n}" == "y" ]]; then
    log_has "drivers.service: bundle verified /bin/drivers/usb/" || die "USB driver bundle verification was not observed; see ${SERIAL_LOG}"
    log_has "drivers.service: spawned driver pid=" || die "USB driver launch was not observed; see ${SERIAL_LOG}"
    log_has "usb-driver: PCI USB controller" || die "USB controller detection was not observed; see ${SERIAL_LOG}"
    log_has "usb-driver: enumeration complete" || die "USB driver completion was not observed; see ${SERIAL_LOG}"
fi

kill "${QEMU_PID}" 2>/dev/null || true
wait "${QEMU_PID}" 2>/dev/null || true
trap - EXIT

echo "[done] serial log: ${SERIAL_LOG}"
