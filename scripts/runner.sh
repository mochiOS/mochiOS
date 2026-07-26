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
ENV_DEBUG_QEMU_VIRTIO_GPU_SET=0
ENV_QEMU_GPU_BACKEND_SET=0
ENV_DRIVER_XHCI_SET=0
if [[ -v DEBUG_QEMU_KVM ]]; then ENV_DEBUG_QEMU_KVM_SET=1; ENV_DEBUG_QEMU_KVM="${DEBUG_QEMU_KVM}"; fi
if [[ -v DEBUG_QEMU_CPU ]]; then ENV_DEBUG_QEMU_CPU_SET=1; ENV_DEBUG_QEMU_CPU="${DEBUG_QEMU_CPU}"; fi
if [[ -v DEBUG_QEMU_SMP ]]; then ENV_DEBUG_QEMU_SMP_SET=1; ENV_DEBUG_QEMU_SMP="${DEBUG_QEMU_SMP}"; fi
if [[ -v DEBUG_QEMU_GUI ]]; then ENV_DEBUG_QEMU_GUI_SET=1; ENV_DEBUG_QEMU_GUI="${DEBUG_QEMU_GUI}"; fi
if [[ -v DEBUG_QEMU_DEBUG ]]; then ENV_DEBUG_QEMU_DEBUG_SET=1; ENV_DEBUG_QEMU_DEBUG="${DEBUG_QEMU_DEBUG}"; fi
if [[ -v DEBUG_QEMU_REQUIRE_USB ]]; then ENV_DEBUG_QEMU_REQUIRE_USB_SET=1; ENV_DEBUG_QEMU_REQUIRE_USB="${DEBUG_QEMU_REQUIRE_USB}"; fi
if [[ -v DEBUG_QEMU_VIRTIO_GPU ]]; then ENV_DEBUG_QEMU_VIRTIO_GPU_SET=1; ENV_DEBUG_QEMU_VIRTIO_GPU="${DEBUG_QEMU_VIRTIO_GPU}"; fi
if [[ -v QEMU_GPU_BACKEND ]]; then ENV_QEMU_GPU_BACKEND_SET=1; ENV_QEMU_GPU_BACKEND="${QEMU_GPU_BACKEND}"; fi
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
if [[ "${ENV_DEBUG_QEMU_VIRTIO_GPU_SET}" == "1" ]]; then DEBUG_QEMU_VIRTIO_GPU="${ENV_DEBUG_QEMU_VIRTIO_GPU}"; fi
if [[ "${ENV_QEMU_GPU_BACKEND_SET}" == "1" ]]; then QEMU_GPU_BACKEND="${ENV_QEMU_GPU_BACKEND}"; fi
if [[ "${ENV_DRIVER_XHCI_SET}" == "1" ]]; then DRIVER_XHCI="${ENV_DRIVER_XHCI}"; fi
ARTIFACT_DIR="${ARTIFACT_DIR:-${ROOT_DIR}/out/artifacts}"
RUN_ID="workspace-$(date +%s)-$$"
RUN_DIR="${ROOT_DIR}/out/runner/${RUN_ID}"
SERIAL_LOG="${RUN_DIR}/serial.log"
DRIVERS_LOG="${RUN_DIR}/drivers.log"
DISPLAY_LOG="${RUN_DIR}/display.driver.log"
SERVICE_MANAGER_LOG="${RUN_DIR}/service-manager.log"
MONITOR_SOCKET="${RUN_DIR}/monitor.sock"
GPU_SCREENSHOT="${RUN_DIR}/virtio-gpu.ppm"
ROOTFS_IMAGE="${RUN_DIR}/rootfs.img"
OVMF_CODE="${OVMF_CODE:-/usr/share/OVMF/OVMF_CODE_4M.fd}"
OVMF_VARS_TEMPLATE="${OVMF_VARS_TEMPLATE:-/usr/share/OVMF/OVMF_VARS_4M.fd}"
OVMF_VARS="${RUN_DIR}/OVMF_VARS_4M.fd"
GUI_MODE=1
if [[ "${DRIVER_XHCI:-n}" == "y" ]]; then
    ENABLE_XHCI="1"
else
    ENABLE_XHCI="0"
fi
QEMU_CPU="${DEBUG_QEMU_CPU:-qemu64}"
QEMU_SMP="${DEBUG_QEMU_SMP:-1}"
QEMU_GPU_BACKEND="${QEMU_GPU_BACKEND:-2d}"

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

QEMU_ACCEL="${QEMU_ACCELERATOR:-}"
if [[ -z "${QEMU_ACCEL}" ]]; then
    if [[ "${DEBUG_QEMU_KVM:-y}" == "y" || "${KVM:-0}" == "1" ]]; then
        QEMU_ACCEL="kvm"
    else
        QEMU_ACCEL="tcg"
    fi
fi
case "${QEMU_ACCEL}" in
    kvm)
        [[ -r /dev/kvm && -w /dev/kvm ]] ||
            die "KVM requested but /dev/kvm is not readable and writable"
        ;;
    tcg) ;;
    *) die "QEMU_ACCELERATOR must be 'kvm' or 'tcg': ${QEMU_ACCEL}" ;;
esac
case "${QEMU_GPU_BACKEND}" in
    2d | virgl) ;;
    *) die "QEMU_GPU_BACKEND must be '2d' or 'virgl': ${QEMU_GPU_BACKEND}" ;;
esac

need_cmd qemu-system-x86_64
need_cmd dd
need_cmd debugfs
need_cmd grep
need_cmd sed
need_cmd tee
need_cmd wc

QEMU_TIMEOUT_SECONDS="${QEMU_TIMEOUT_SECONDS:-120}"
[[ "${QEMU_TIMEOUT_SECONDS}" =~ ^[1-9][0-9]*$ ]] ||
    die "QEMU_TIMEOUT_SECONDS must be a positive integer"
VIRTIO_GPU_TEST_KEYS="${VIRTIO_GPU_TEST_KEYS:-t e s t dot a p p ret}"
VIRTIO_GPU_TEST_APP_PATH="${VIRTIO_GPU_TEST_APP_PATH:-/applications/test.app/entry.elf}"
VIRTIO_GPU_POINTER_STRESS="${VIRTIO_GPU_POINTER_STRESS:-n}"
VIRTIO_GPU_STRESS_SWEEPS="${VIRTIO_GPU_STRESS_SWEEPS:-12}"
VIRTIO_GPU_PIXEL_CHECK="${VIRTIO_GPU_PIXEL_CHECK:-y}"
[[ "${VIRTIO_GPU_STRESS_SWEEPS}" =~ ^[1-9][0-9]*$ ]] ||
    die "VIRTIO_GPU_STRESS_SWEEPS must be a positive integer"

need_file "${ARTIFACT_DIR}/disk.img"
need_file "${OVMF_CODE}"
need_file "${OVMF_VARS_TEMPLATE}"
need_file "${SCRIPT_DIR}/check-smoke-logs.sh"

mkdir -p "${RUN_DIR}"
cp "${OVMF_VARS_TEMPLATE}" "${OVMF_VARS}"
OS_DISK="${ARTIFACT_DIR}/disk.img"
if [[ "${SMOKE_TEST:-0}" == "1" ]]; then
    OS_DISK="${RUN_DIR}/disk.img"
    cp "${ARTIFACT_DIR}/disk.img" "${OS_DISK}"
fi
: > "${SERIAL_LOG}"

QEMU_ARGS=(
    -machine "q35,accel=${QEMU_ACCEL}"
    -m 512M
    -smp "${QEMU_SMP}"
    -cpu "${QEMU_CPU}"
    -serial stdio
    -no-reboot
    -drive "if=pflash,format=raw,readonly=on,file=${OVMF_CODE}"
    -drive "if=pflash,format=raw,file=${OVMF_VARS}"
    -drive "id=osdisk,if=none,format=raw,file=${OS_DISK}"
    -device "virtio-blk-pci,disable-modern=on,drive=osdisk,bootindex=1"
)

if [[ "${DEBUG_QEMU_VIRTIO_GPU:-n}" == "y" ]]; then
    need_cmd nc
    if [[ "${QEMU_GPU_BACKEND}" == "virgl" ]]; then
        QEMU_ARGS+=(-device "virtio-gpu-gl-pci,id=virtio-gpu")
    else
        QEMU_ARGS+=(-device "virtio-gpu-pci,id=virtio-gpu")
    fi
fi

if [[ "${ENABLE_XHCI}" == "1" ]]; then
    QEMU_ARGS+=(
        -device "qemu-xhci,id=xhci"
        -device "usb-tablet,bus=xhci.0"
    )
fi

if [[ "${DEBUG_QEMU_GUI:-y}" != "y" || "${NOGUI:-0}" == "1" ]]; then
    GUI_MODE=0
    if [[ "${DEBUG_QEMU_VIRTIO_GPU:-n}" == "y" && "${QEMU_GPU_BACKEND}" == "virgl" ]]; then
        QEMU_ARGS+=(-display egl-headless)
    else
        QEMU_ARGS+=(-display none)
    fi
    if [[ "${DEBUG_QEMU_VIRTIO_GPU:-n}" == "y" ]]; then
        QEMU_ARGS+=(-monitor "unix:${MONITOR_SOCKET},server=on,wait=off")
    else
        QEMU_ARGS+=(-monitor none)
    fi
elif [[ "${DEBUG_QEMU_VIRTIO_GPU:-n}" == "y" && "${QEMU_GPU_BACKEND}" == "virgl" ]]; then
    QEMU_ARGS+=(-display gtk,gl=on)
fi

if [[ "${DEBUG_QEMU_DEBUG:-n}" == "y" || "${DEBUG:-0}" != "0" ]]; then
    QEMU_ARGS+=(-s -S)
fi

QEMU_PID=""

cleanup() {
    if [[ -n "${QEMU_PID:-}" ]]; then
        kill -TERM "${QEMU_PID}" 2>/dev/null || true
        for _ in {1..20}; do
            if ! kill -0 "${QEMU_PID}" 2>/dev/null; then
                break
            fi
            sleep 0.1
        done
        kill -KILL "${QEMU_PID}" 2>/dev/null || true
        wait "${QEMU_PID}" 2>/dev/null || true
    fi
}

cleanup_files() {
    if [[ "${SMOKE_TEST:-0}" == "1" ]]; then
        rm -f "${OS_DISK}" "${ROOTFS_IMAGE}" "${OVMF_VARS}"
    fi
}

cleanup_all() {
    cleanup
    cleanup_files
}
trap cleanup_all EXIT

log_has() {
    grep -aFq "$1" "${SERIAL_LOG}"
}

hmp_command() {
    printf '%s\n' "$1" | nc -U -q 0 -w 2 "${MONITOR_SOCKET}" >/dev/null
}

start_qemu() {
    echo "[run] qemu accelerator=${QEMU_ACCEL} gpu=${QEMU_GPU_BACKEND}"
    qemu-system-x86_64 "${QEMU_ARGS[@]}" > >(tee -a "${SERIAL_LOG}") 2>&1 &
    QEMU_PID=$!
}

start_qemu

if [[ "${GUI_MODE}" -eq 1 ]]; then
    echo "[done] serial log: ${SERIAL_LOG}"
    set +e
    wait "${QEMU_PID}"
    QEMU_STATUS=$?
    set -e
    exit "${QEMU_STATUS}"
fi

NEXT_LINE=1
COMPLETED=0
DEADLINE=$((SECONDS + QEMU_TIMEOUT_SECONDS))
while ((SECONDS < DEADLINE)); do
    while IFS= read -r line; do
        if [[ "${line}" == *"PAGE FAULT"* || "${line}" == *"Faulting user context:"* || "${line}" == *"panic"* ]]; then
            die "fault or panic observed during QEMU run"
        fi
    done < <(sed -n "${NEXT_LINE},\$p" "${SERIAL_LOG}")

    NEXT_LINE="$(($(wc -l < "${SERIAL_LOG}") + 1))"

    if log_has "cext: loaded bundle disk" \
        && log_has "cext: loaded bundle ext2" \
        && log_has "Kernel initialization complete. Entering idle loop" \
        && log_has "exec: loaded 'core.service' from initfs" \
        && log_has "exec: loaded '/system/services/logger.service'" \
        && log_has "exec: loaded '/system/services/capability.service'" \
        && log_has "exec: loaded '/system/services/service-manager.service'" \
        && log_has "exec: loaded '/system/services/drivers.service'" \
        && log_has "exec: loaded '/system/services/input.service'" \
        && log_has "exec: loaded '/system/services/display.driver'" \
        && log_has "exec: loaded '/system/services/compositor.service'" \
        && log_has "exec: loaded '/bin/drivers/ps2/i8042.driver/entry.elf'" \
        && log_has "exec: loaded '/system/services/tty.service'"; then
        COMPLETED=1
        break
    fi

    if ! kill -0 "${QEMU_PID}" 2>/dev/null; then
        die "QEMU exited before the smoke-test completion logs were observed; see ${SERIAL_LOG}"
    fi

    sleep 0.1
done

[[ "${COMPLETED}" == "1" ]] ||
    die "QEMU smoke test timed out after ${QEMU_TIMEOUT_SECONDS}s; see ${SERIAL_LOG}"

if [[ "${DEBUG_QEMU_VIRTIO_GPU:-n}" == "y" ]]; then
    sleep 1
    for key in ${VIRTIO_GPU_TEST_KEYS}; do
        hmp_command "sendkey ${key}"
        sleep 0.15
    done
    APP_DEADLINE=$((SECONDS + 30))
    while ((SECONDS < APP_DEADLINE)); do
        if log_has "exec: loaded '${VIRTIO_GPU_TEST_APP_PATH}'"; then
            break
        fi
        sleep 0.1
    done
    log_has "exec: loaded '${VIRTIO_GPU_TEST_APP_PATH}'" ||
        die "configured ViewKit application did not launch; see ${SERIAL_LOG}"
    sleep 2
    if [[ "${VIRTIO_GPU_POINTER_STRESS}" == "y" ]]; then
        {
            printf '%s\n' "mouse_move 0 360" "mouse_move -600 0"
            for ((sweep = 0; sweep < VIRTIO_GPU_STRESS_SWEEPS; sweep++)); do
                for _ in {1..25}; do
                    printf '%s\n' "mouse_move 48 0"
                    sleep 0.01
                done
                for _ in {1..25}; do
                    printf '%s\n' "mouse_move -48 0"
                    sleep 0.01
                done
            done
        } | nc -U -q 1 "${MONITOR_SOCKET}" >/dev/null
    else
        for _ in {1..12}; do
            hmp_command "mouse_move 1 1"
            sleep 0.05
        done
    fi
    if grep -aEq "PAGE FAULT|Faulting user context:|EXCEPTION:|panic" "${SERIAL_LOG}"; then
        die "fault or panic observed after ViewKit pointer input; see ${SERIAL_LOG}"
    fi

    if [[ "${VIRTIO_GPU_PIXEL_CHECK}" == "y" ]]; then
        PIXELS_READY=0
        for _ in {1..300}; do
            printf 'screendump %s virtio-gpu 0\n' "${GPU_SCREENSHOT}" |
                nc -U -q 0 -w 2 "${MONITOR_SOCKET}" >/dev/null
            if [[ -s "${GPU_SCREENSHOT}" ]] \
                && "${SCRIPT_DIR}/check-virtio-gpu-pixels.pl" \
                    "${GPU_SCREENSHOT}" >/dev/null 2>&1; then
                PIXELS_READY=1
                break
            fi
            sleep 0.1
        done
        need_file "${GPU_SCREENSHOT}"
        [[ "${PIXELS_READY}" == "1" ]] ||
            die "virtio-gpu scanout did not reach the expected test scene"
        "${SCRIPT_DIR}/check-virtio-gpu-pixels.pl" "${GPU_SCREENSHOT}"
    else
        printf 'screendump %s virtio-gpu 0\n' "${GPU_SCREENSHOT}" |
            nc -U -q 0 -w 2 "${MONITOR_SOCKET}" >/dev/null
    fi
fi

sleep 1
cleanup
QEMU_PID=""

ROOTFS_START_SECTOR=$((2048 + IMAGE_ESP_SIZE_MB * 2048))
ROOTFS_SIZE_SECTORS=$(((IMAGE_DISK_SIZE_MB - IMAGE_ESP_SIZE_MB - 2) * 2048))
dd if="${OS_DISK}" of="${ROOTFS_IMAGE}" bs=512 \
    skip="${ROOTFS_START_SECTOR}" count="${ROOTFS_SIZE_SECTORS}" status=none
debugfs -R 'cat /system/logs/services/drivers.log' "${ROOTFS_IMAGE}" \
    > "${DRIVERS_LOG}" 2>/dev/null || die "drivers.service log could not be read"
debugfs -R 'cat /system/logs/services/display.driver.log' "${ROOTFS_IMAGE}" \
    > "${DISPLAY_LOG}" 2>/dev/null || die "display.driver log could not be read"
debugfs -R 'cat /system/logs/services/service-manager.log' "${ROOTFS_IMAGE}" \
    > "${SERVICE_MANAGER_LOG}" 2>/dev/null || die "service-manager.service log could not be read"

if [[ "${DEBUG_QEMU_VIRTIO_GPU:-n}" == "y" ]]; then
    LAST_DISPLAY_BACKEND="$(grep -F "display.driver:" "${DISPLAY_LOG}" |
        grep -E "backend=|using framebuffer fallback" | tail -n 1)"
    [[ "${LAST_DISPLAY_BACKEND}" == *"backend=virtio-gpu"* ]] ||
        die "display.driver did not select the virtio-gpu backend; see ${DISPLAY_LOG}"
    if [[ "${QEMU_GPU_BACKEND}" == "virgl" ]]; then
        grep -Fq "display.driver: virgl capset=" "${DISPLAY_LOG}" ||
            die "display.driver did not negotiate a virgl capset; see ${DISPLAY_LOG}"
    fi
fi

"${SCRIPT_DIR}/check-smoke-logs.sh" \
    "${SERIAL_LOG}" "${SERVICE_MANAGER_LOG}" "${DRIVERS_LOG}"

echo "[done] serial log: ${SERIAL_LOG}"
