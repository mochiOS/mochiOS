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
ENV_DEBUG_QEMU_GPU_BACKEND_SET=0
ENV_DRIVER_XHCI_SET=0
ENV_QEMU_NETWORK_SET=0
ENV_QEMU_NETWORK_MAC_SET=0
if [[ -v DEBUG_QEMU_KVM ]]; then ENV_DEBUG_QEMU_KVM_SET=1; ENV_DEBUG_QEMU_KVM="${DEBUG_QEMU_KVM}"; fi
if [[ -v DEBUG_QEMU_CPU ]]; then ENV_DEBUG_QEMU_CPU_SET=1; ENV_DEBUG_QEMU_CPU="${DEBUG_QEMU_CPU}"; fi
if [[ -v DEBUG_QEMU_SMP ]]; then ENV_DEBUG_QEMU_SMP_SET=1; ENV_DEBUG_QEMU_SMP="${DEBUG_QEMU_SMP}"; fi
if [[ -v DEBUG_QEMU_GUI ]]; then ENV_DEBUG_QEMU_GUI_SET=1; ENV_DEBUG_QEMU_GUI="${DEBUG_QEMU_GUI}"; fi
if [[ -v DEBUG_QEMU_DEBUG ]]; then ENV_DEBUG_QEMU_DEBUG_SET=1; ENV_DEBUG_QEMU_DEBUG="${DEBUG_QEMU_DEBUG}"; fi
if [[ -v DEBUG_QEMU_REQUIRE_USB ]]; then ENV_DEBUG_QEMU_REQUIRE_USB_SET=1; ENV_DEBUG_QEMU_REQUIRE_USB="${DEBUG_QEMU_REQUIRE_USB}"; fi
if [[ -v DEBUG_QEMU_VIRTIO_GPU ]]; then ENV_DEBUG_QEMU_VIRTIO_GPU_SET=1; ENV_DEBUG_QEMU_VIRTIO_GPU="${DEBUG_QEMU_VIRTIO_GPU}"; fi
if [[ -v DEBUG_QEMU_GPU_BACKEND ]]; then ENV_DEBUG_QEMU_GPU_BACKEND_SET=1; ENV_DEBUG_QEMU_GPU_BACKEND="${DEBUG_QEMU_GPU_BACKEND}"; fi
if [[ -v DRIVER_XHCI ]]; then ENV_DRIVER_XHCI_SET=1; ENV_DRIVER_XHCI="${DRIVER_XHCI}"; fi
if [[ -v QEMU_NETWORK ]]; then ENV_QEMU_NETWORK_SET=1; ENV_QEMU_NETWORK="${QEMU_NETWORK}"; fi
if [[ -v QEMU_NETWORK_MAC ]]; then ENV_QEMU_NETWORK_MAC_SET=1; ENV_QEMU_NETWORK_MAC="${QEMU_NETWORK_MAC}"; fi
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
if [[ "${ENV_DEBUG_QEMU_GPU_BACKEND_SET}" == "1" ]]; then DEBUG_QEMU_GPU_BACKEND="${ENV_DEBUG_QEMU_GPU_BACKEND}"; fi
if [[ "${ENV_DRIVER_XHCI_SET}" == "1" ]]; then DRIVER_XHCI="${ENV_DRIVER_XHCI}"; fi
if [[ "${ENV_QEMU_NETWORK_SET}" == "1" ]]; then QEMU_NETWORK="${ENV_QEMU_NETWORK}"; fi
if [[ "${ENV_QEMU_NETWORK_MAC_SET}" == "1" ]]; then QEMU_NETWORK_MAC="${ENV_QEMU_NETWORK_MAC}"; fi
ARTIFACT_DIR="${ARTIFACT_DIR:-${ROOT_DIR}/out/artifacts}"
RUN_ID="workspace-$(date +%s)-$$"
RUN_DIR="${ROOT_DIR}/out/runner/${RUN_ID}"
SERIAL_LOG="${RUN_DIR}/serial.log"
DRIVERS_LOG="${RUN_DIR}/drivers.log"
DISPLAY_LOG="${RUN_DIR}/display.driver.log"
SERVICE_MANAGER_LOG="${RUN_DIR}/service-manager.log"
NETWORK_LOG="${RUN_DIR}/network.log"
USER_LOG="${RUN_DIR}/user.log"
NETWORK_SERVER_LOG="${RUN_DIR}/network-smoke-server.log"
NETWORK_SERVER_READY="${RUN_DIR}/network-smoke-server.ready"
TLS_HTTP_SERVER_LOG="${RUN_DIR}/tls-http-smoke-server.log"
TLS_HTTP_SERVER_READY="${RUN_DIR}/tls-http-smoke-server.ready"
TLS_BAD_CV_SERVER_LOG="${RUN_DIR}/tls-bad-cv-server.log"
TLS_BAD_CV_SERVER_READY="${RUN_DIR}/tls-bad-cv-server.ready"
TLS_BAD_CV_SERVER="${ROOT_DIR}/out/tls-http-smoke-host-target/release/mochios-tls-bad-cv-server"
MONITOR_SOCKET="${RUN_DIR}/monitor.sock"
GPU_SCREENSHOT="${RUN_DIR}/virtio-gpu.ppm"
ROOTFS_IMAGE="${RUN_DIR}/rootfs.img"
MBOOT_VIRTIO_TRACE="${RUN_DIR}/mboot-virtio.trace"
QEMU_MBOOT_CONTROL_SOCKET="${QEMU_MBOOT_CONTROL_SOCKET:-}"
MBOOT_CONTROL_REQUIRED="${MBOOT_CONTROL_REQUIRED:-0}"
MBOOT_CONTROL_LOG="${MBOOT_CONTROL_LOG:-}"
SMOKE_USER_DATABASE_FIXTURE="${SMOKE_USER_DATABASE_FIXTURE:-}"
SMOKE_GUEST_COMMAND="${SMOKE_GUEST_COMMAND:-}"
SMOKE_GUEST_EXPECT="${SMOKE_GUEST_EXPECT:-}"
SMOKE_GUEST_EXPECTED_EXIT="${SMOKE_GUEST_EXPECTED_EXIT:-0}"
OVMF_CODE="${OVMF_CODE:-/usr/share/OVMF/OVMF_CODE_4M.fd}"
OVMF_VARS_TEMPLATE="${OVMF_VARS_TEMPLATE:-/usr/share/OVMF/OVMF_VARS_4M.fd}"
OVMF_VARS="${RUN_DIR}/OVMF_VARS_4M.fd"
GUI_MODE=1
if [[ "${DRIVER_XHCI:-n}" == "y" ]]; then
    ENABLE_XHCI="1"
else
    ENABLE_XHCI="0"
fi
QEMU_NETWORK="${QEMU_NETWORK:-${DRIVER_VIRTIO_NET:-y}}"
QEMU_NETWORK_MAC="${QEMU_NETWORK_MAC:-52:54:00:12:34:56}"
QEMU_NETWORK_PCAP="${QEMU_NETWORK_PCAP:-}"
QEMU_TCP_ECHO_SERVER="${QEMU_TCP_ECHO_SERVER:-${QEMU_NETWORK}}"
QEMU_CPU="${DEBUG_QEMU_CPU:-qemu64}"
QEMU_SMP="${DEBUG_QEMU_SMP:-1}"
QEMU_GL_DISPLAY="${QEMU_GL_DISPLAY:-auto}"
DRM_RENDER_NODE=""
for candidate in /dev/dri/renderD*; do
    if [[ -r "${candidate}" && -w "${candidate}" ]]; then
        DRM_RENDER_NODE="${candidate}"
        break
    fi
done

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

case "${QEMU_NETWORK}" in y|n) ;; *) die "QEMU_NETWORK must be 'y' or 'n'" ;; esac
case "${QEMU_TCP_ECHO_SERVER}" in
    y|n) ;;
    *) die "QEMU_TCP_ECHO_SERVER must be 'y' or 'n'" ;;
esac
case "${MBOOT_CONTROL_REQUIRED}" in
    0|1) ;;
    *) die "MBOOT_CONTROL_REQUIRED must be 0 or 1" ;;
esac
if [[ "${MBOOT_CONTROL_REQUIRED}" == "1" ]]; then
    [[ -n "${QEMU_MBOOT_CONTROL_SOCKET}" ]] ||
        die "MBOOT_CONTROL_REQUIRED requires QEMU_MBOOT_CONTROL_SOCKET"
    [[ -n "${MBOOT_CONTROL_LOG}" ]] ||
        die "MBOOT_CONTROL_REQUIRED requires MBOOT_CONTROL_LOG"
fi
[[ "${QEMU_NETWORK_MAC}" =~ ^([[:xdigit:]]{2}:){5}[[:xdigit:]]{2}$ ]] ||
    die "QEMU_NETWORK_MAC must be a MAC address: ${QEMU_NETWORK_MAC}"

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
case "${DEBUG_QEMU_GPU_BACKEND:-n}" in
    y)
        QEMU_GPU_BACKEND="virgl"
        ;;
    n)
        QEMU_GPU_BACKEND="2d"
        ;;
    *)
        die "DEBUG_QEMU_GPU_BACKEND must be 'y' or 'n': ${DEBUG_QEMU_GPU_BACKEND}"
        ;;
esac
case "${QEMU_GL_DISPLAY}" in
    auto | egl-headless | gtk | sdl) ;;
    *) die "QEMU_GL_DISPLAY must be 'auto', 'egl-headless', 'gtk', or 'sdl': ${QEMU_GL_DISPLAY}" ;;
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
QEMU_NETWORK_SETTLE_SECONDS="${QEMU_NETWORK_SETTLE_SECONDS:-5}"
[[ "${QEMU_NETWORK_SETTLE_SECONDS}" =~ ^[0-9]+$ ]] ||
    die "QEMU_NETWORK_SETTLE_SECONDS must be a non-negative integer"
NETWORK_CLIENT_SMOKE="${NETWORK_CLIENT_SMOKE:-${SMOKE_TEST:-0}}"
case "${NETWORK_CLIENT_SMOKE}" in 0|1) ;; *) die "NETWORK_CLIENT_SMOKE must be 0 or 1" ;; esac
[[ "${SMOKE_GUEST_EXPECTED_EXIT}" =~ ^[0-9]+$ ]] ||
    die "SMOKE_GUEST_EXPECTED_EXIT must be a non-negative integer"
TLS_HTTP_CLIENT_SMOKE="${TLS_HTTP_CLIENT_SMOKE:-0}"
case "${TLS_HTTP_CLIENT_SMOKE}" in 0|1) ;; *) die "TLS_HTTP_CLIENT_SMOKE must be 0 or 1" ;; esac
ACCOUNTS_HTTPS_SMOKE="${ACCOUNTS_HTTPS_SMOKE:-0}"
case "${ACCOUNTS_HTTPS_SMOKE}" in 0|1) ;; *) die "ACCOUNTS_HTTPS_SMOKE must be 0 or 1" ;; esac
MPKG_RUNTIME_SMOKE="${MPKG_RUNTIME_SMOKE:-${SMOKE_TEST:-0}}"
case "${MPKG_RUNTIME_SMOKE}" in 0|1) ;; *) die "MPKG_RUNTIME_SMOKE must be 0 or 1" ;; esac
if [[ "${NETWORK_CLIENT_SMOKE}" == "1" ]]; then
    default_tcp_echo_port=$((20000 + $$ % 20000))
else
    default_tcp_echo_port=20000
fi
QEMU_TCP_ECHO_PORT="${QEMU_TCP_ECHO_PORT:-${NETWORK_SMOKE_PORT:-${default_tcp_echo_port}}}"
[[ "${QEMU_TCP_ECHO_PORT}" =~ ^[1-9][0-9]*$ && "${QEMU_TCP_ECHO_PORT}" -le 65535 ]] ||
    die "QEMU_TCP_ECHO_PORT must be a valid TCP port"
NETWORK_SMOKE_PORT="${QEMU_TCP_ECHO_PORT}"
TLS_HTTP_SMOKE_PORT="${TLS_HTTP_SMOKE_PORT:-$((40000 + $$ % 20000))}"
[[ "${TLS_HTTP_SMOKE_PORT}" =~ ^[1-9][0-9]*$ && "${TLS_HTTP_SMOKE_PORT}" -le 65530 ]] ||
    die "TLS_HTTP_SMOKE_PORT must be a valid base port with room for six listeners"
if [[ "${QEMU_TCP_ECHO_SERVER}" == "y" && "${QEMU_NETWORK}" != "y" ]]; then
    die "QEMU_TCP_ECHO_SERVER=y requires QEMU_NETWORK=y"
fi
if [[ "${NETWORK_CLIENT_SMOKE}" == "1" && "${QEMU_TCP_ECHO_SERVER}" != "y" ]]; then
    die "NETWORK_CLIENT_SMOKE requires QEMU_TCP_ECHO_SERVER=y"
fi
VIRTIO_GPU_TEST_KEYS="${VIRTIO_GPU_TEST_KEYS:-t e s t dot a p p ret}"
VIRTIO_GPU_TEST_APP_PATH="${VIRTIO_GPU_TEST_APP_PATH:-/applications/test.app/entry.elf}"
VIRTIO_GPU_POINTER_STRESS="${VIRTIO_GPU_POINTER_STRESS:-n}"
VIRTIO_GPU_STRESS_SWEEPS="${VIRTIO_GPU_STRESS_SWEEPS:-12}"
if [[ -z "${VIRTIO_GPU_PIXEL_CHECK+x}" ]]; then
    if [[ "${QEMU_GPU_BACKEND}" == "virgl" ]]; then
        VIRTIO_GPU_PIXEL_CHECK=n
        echo "[skip] virgl pixel capture: QEMU GL scanout has no HMP DisplaySurface"
    else
        VIRTIO_GPU_PIXEL_CHECK=y
    fi
fi
[[ "${VIRTIO_GPU_STRESS_SWEEPS}" =~ ^[1-9][0-9]*$ ]] ||
    die "VIRTIO_GPU_STRESS_SWEEPS must be a positive integer"

need_file "${ARTIFACT_DIR}/disk.img"
need_file "${OVMF_CODE}"
need_file "${OVMF_VARS_TEMPLATE}"
need_file "${SCRIPT_DIR}/check-smoke-logs.sh"
if [[ "${QEMU_TCP_ECHO_SERVER}" == "y" ]]; then
    need_cmd perl
    need_file "${SCRIPT_DIR}/network-smoke-server.pl"
fi
if [[ "${NETWORK_CLIENT_SMOKE}" == "1" ]]; then
    need_cmd nc
fi
if [[ "${MBOOT_CONTROL_REQUIRED}" == "1" ]]; then
    need_cmd nc
    need_file "${MBOOT_CONTROL_LOG}"
fi
if [[ "${TLS_HTTP_CLIENT_SMOKE}" == "1" ]]; then
    need_cmd python3
    need_file "${SCRIPT_DIR}/tls-http-smoke-server.py"
    need_file "${TLS_BAD_CV_SERVER}"
fi

mkdir -p "${RUN_DIR}"
cp "${OVMF_VARS_TEMPLATE}" "${OVMF_VARS}"
OS_DISK="${ARTIFACT_DIR}/disk.img"
if [[ "${SMOKE_TEST:-0}" == "1" ]]; then
    OS_DISK="${RUN_DIR}/disk.img"
    cp "${ARTIFACT_DIR}/disk.img" "${OS_DISK}"
fi
: > "${SERIAL_LOG}"

if [[ -n "${SMOKE_USER_DATABASE_FIXTURE}" ]]; then
    need_file "${SMOKE_USER_DATABASE_FIXTURE}"
    ROOTFS_START_SECTOR=$((2048 + IMAGE_ESP_SIZE_MB * 2048))
    ROOTFS_SIZE_SECTORS=$(((IMAGE_DISK_SIZE_MB - IMAGE_ESP_SIZE_MB - 2) * 2048))
    dd if="${OS_DISK}" of="${ROOTFS_IMAGE}" bs=512 \
        skip="${ROOTFS_START_SECTOR}" count="${ROOTFS_SIZE_SECTORS}" status=none
    debugfs -w -R 'rm /system/users/users.db' "${ROOTFS_IMAGE}" >/dev/null 2>&1
    debugfs -w -R "write ${SMOKE_USER_DATABASE_FIXTURE} /system/users/users.db" \
        "${ROOTFS_IMAGE}" >/dev/null 2>&1 ||
        die "could not install the isolated smoke user database"
    debugfs -w -R 'set_inode_field /system/users/users.db mode 0100600' \
        "${ROOTFS_IMAGE}" >/dev/null 2>&1 ||
        die "could not secure the isolated smoke user database"
    dd if="${ROOTFS_IMAGE}" of="${OS_DISK}" bs=512 \
        seek="${ROOTFS_START_SECTOR}" conv=notrunc status=none
fi

QEMU_ARGS=(
    -machine "q35,accel=${QEMU_ACCEL}"
    -m 1G
    -smp "${QEMU_SMP}"
    -cpu "${QEMU_CPU}"
    -rtc base=utc,clock=host
    -serial stdio
    -no-reboot
    -drive "if=pflash,format=raw,readonly=on,file=${OVMF_CODE}"
    -drive "if=pflash,format=raw,file=${OVMF_VARS}"
    -drive "id=osdisk,if=none,format=raw,file=${OS_DISK}"
    -device "virtio-blk-pci,disable-modern=on,drive=osdisk,bootindex=1"
    -object "rng-random,id=rng0,filename=/dev/urandom"
    -device "virtio-rng-pci,rng=rng0"
)

if [[ "${DEBUG_QEMU_MBOOT_TRACE:-n}" == "y" ]]; then
    QEMU_ARGS+=(-trace "enable=virtio_serial_*,file=${MBOOT_VIRTIO_TRACE}")
fi

if [[ -n "${QEMU_MBOOT_CONTROL_SOCKET}" ]]; then
    [[ -S "${QEMU_MBOOT_CONTROL_SOCKET}" ]] ||
        die "mBoot control socket is not listening: ${QEMU_MBOOT_CONTROL_SOCKET}"
    QEMU_ARGS+=(
        -chardev "socket,id=mbootctl,path=${QEMU_MBOOT_CONTROL_SOCKET},server=off,reconnect-ms=1000"
        -device "virtio-serial-pci,id=mboot-serial,disable-legacy=on,max_ports=2"
        -device "virtserialport,id=mboot-control,chardev=mbootctl,name=org.mochios.mboot.control"
    )
fi

if [[ "${QEMU_NETWORK}" == "y" ]]; then
    QEMU_ARGS+=(
        -netdev "user,id=net0"
        -device "virtio-net-pci,disable-legacy=on,netdev=net0,mac=${QEMU_NETWORK_MAC}"
    )
    if [[ -n "${QEMU_NETWORK_PCAP}" ]]; then
        QEMU_ARGS+=(
            -object "filter-dump,id=netdump,netdev=net0,file=${QEMU_NETWORK_PCAP}"
        )
    fi
fi

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
        if [[ "${QEMU_GL_DISPLAY}" == "auto" ]]; then
            if [[ -n "${DRM_RENDER_NODE}" ]]; then
                QEMU_GL_DISPLAY="egl-headless"
            else
                die "headless virgl requires an accessible DRM render node; no usable /dev/dri/renderD* was found (set QEMU_GL_DISPLAY=gtk only for an explicit desktop GL run)"
            fi
        fi
        case "${QEMU_GL_DISPLAY}" in
            egl-headless) QEMU_ARGS+=(-display egl-headless) ;;
            gtk) QEMU_ARGS+=(-display gtk,gl=on) ;;
            sdl) QEMU_ARGS+=(-display sdl,gl=on) ;;
        esac
    else
        QEMU_ARGS+=(-display none)
    fi
    if [[ "${DEBUG_QEMU_VIRTIO_GPU:-n}" == "y" || "${NETWORK_CLIENT_SMOKE}" == "1" || "${TLS_HTTP_CLIENT_SMOKE}" == "1" || "${ACCOUNTS_HTTPS_SMOKE}" == "1" || "${MBOOT_CONTROL_REQUIRED}" == "1" || -n "${SMOKE_GUEST_COMMAND}" ]]; then
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
NETWORK_SERVER_PID=""
TLS_HTTP_SERVER_PID=""
TLS_BAD_CV_SERVER_PID=""

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
    if [[ -n "${NETWORK_SERVER_PID:-}" ]]; then
        kill -TERM "${NETWORK_SERVER_PID}" 2>/dev/null || true
        wait "${NETWORK_SERVER_PID}" 2>/dev/null || true
        NETWORK_SERVER_PID=""
    fi
    if [[ -n "${TLS_HTTP_SERVER_PID:-}" ]]; then
        kill -TERM "${TLS_HTTP_SERVER_PID}" 2>/dev/null || true
        wait "${TLS_HTTP_SERVER_PID}" 2>/dev/null || true
        TLS_HTTP_SERVER_PID=""
    fi
    if [[ -n "${TLS_BAD_CV_SERVER_PID:-}" ]]; then
        kill -TERM "${TLS_BAD_CV_SERVER_PID}" 2>/dev/null || true
        wait "${TLS_BAD_CV_SERVER_PID}" 2>/dev/null || true
        TLS_BAD_CV_SERVER_PID=""
    fi
}

cleanup_files() {
    if [[ "${SMOKE_TEST:-0}" == "1" && "${KEEP_SMOKE_ARTIFACTS:-0}" != "1" ]]; then
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

mboot_log_has() {
    grep -aFq "$1" "${MBOOT_CONTROL_LOG}"
}

mboot_control_complete() {
    [[ "${MBOOT_CONTROL_REQUIRED}" == "0" ]] && return 0
    mboot_log_has "guest connected" \
        && mboot_log_has "protocol synchronized" \
        && mboot_log_has "protocol negotiated: version=1" \
        && mboot_log_has "guest boot stage: kernel" \
        && mboot_log_has "guest boot stage: userspace" \
        && mboot_log_has "guest boot stage: display" \
        && mboot_log_has "guest boot stage: desktop" \
        && grep -aEq 'guest heartbeat: uptime=[1-9][0-9]*ms' "${MBOOT_CONTROL_LOG}"
}

hmp_command() {
    printf '%s\n' "$1" | nc -U -q 0 -w 2 "${MONITOR_SOCKET}" >/dev/null
}

hmp_screendump() {
    printf 'screendump %s virtio-gpu 0\n' "${GPU_SCREENSHOT}" |
        nc -U -q 0 -w 2 "${MONITOR_SOCKET}"
}

hmp_type_text() {
    local input="$1"
    local character
    local key
    local index
    for ((index = 0; index < ${#input}; index++)); do
        character="${input:index:1}"
        case "${character}" in
            ' ') key="spc" ;;
            '.') key="dot" ;;
            '-') key="minus" ;;
            '/') key="slash" ;;
            ':') key="shift-semicolon" ;;
            [a-z0-9]) key="${character}" ;;
            *) die "unsupported smoke-test keyboard character: ${character}" ;;
        esac
        hmp_command "sendkey ${key}"
        sleep 0.05
    done
    hmp_command "sendkey ret"
}

wait_for_log() {
    local label="$1"
    local pattern="$2"
    local timeout="$3"
    local deadline=$((SECONDS + timeout))
    while ((SECONDS < deadline)); do
        log_has "${pattern}" && return 0
        kill -0 "${QEMU_PID}" 2>/dev/null ||
            die "QEMU exited while waiting for ${label}; see ${SERIAL_LOG}"
        sleep 0.1
    done
    die "timed out waiting for ${label}: pattern='${pattern}' log=${SERIAL_LOG}"
}

wait_for_new_log() {
    local label="$1"
    local first_line="$2"
    local pattern="$3"
    local timeout="$4"
    local deadline=$((SECONDS + timeout))
    while ((SECONDS < deadline)); do
        if sed -n "${first_line},\$p" "${SERIAL_LOG}" | grep -aFq -- "${pattern}"; then
            return 0
        fi
        kill -0 "${QEMU_PID}" 2>/dev/null ||
            die "QEMU exited while waiting for ${label}; see ${SERIAL_LOG}"
        sleep 0.1
    done
    die "timed out waiting for ${label}: pattern='${pattern}' log=${SERIAL_LOG}"
}

run_guest_command() {
    local expected_exit="$1"
    local command="$2"
    shift 2
    local first_line=$(($(wc -l < "${SERIAL_LOG}") + 1))
    local exit_relative
    local prompt_line
    local pattern
    hmp_type_text "${command}"
    for pattern in "$@"; do
        wait_for_new_log "guest command '${command}'" "${first_line}" "${pattern}" 30
    done
    wait_for_new_log "guest command exit '${command}'" "${first_line}" \
        "Process exiting with code: ${expected_exit}" 30
    exit_relative="$(
        sed -n "${first_line},\$p" "${SERIAL_LOG}" |
            grep -anF -- "Process exiting with code: ${expected_exit}" |
            tail -n 1 |
            cut -d: -f1
    )"
    [[ -n "${exit_relative}" ]] || die "could not locate guest command exit line"
    prompt_line=$((first_line + exit_relative))
    wait_for_new_log "shell prompt after '${command}'" "${prompt_line}" "/ $" 10
}

start_network_echo_server() {
    local mode="persistent"
    : > "${NETWORK_SERVER_LOG}"
    perl "${SCRIPT_DIR}/network-smoke-server.pl" \
        "${NETWORK_SMOKE_PORT}" "${NETWORK_SERVER_READY}" "${mode}" \
        > "${NETWORK_SERVER_LOG}" 2>&1 &
    NETWORK_SERVER_PID=$!
    for _ in {1..100}; do
        if [[ -s "${NETWORK_SERVER_READY}" ]]; then
            echo "[run] TCP echo server guest=10.0.2.2:${NETWORK_SMOKE_PORT} mode=${mode}"
            return 0
        fi
        kill -0 "${NETWORK_SERVER_PID}" 2>/dev/null ||
            die "network smoke server exited before becoming ready; see ${NETWORK_SERVER_LOG}"
        sleep 0.05
    done
    die "network smoke server did not become ready; see ${NETWORK_SERVER_LOG}"
}

start_tls_http_server() {
    : > "${TLS_HTTP_SERVER_LOG}"
    rm -f "${TLS_HTTP_SERVER_READY}"
    python3 "${SCRIPT_DIR}/tls-http-smoke-server.py" \
        "${TLS_HTTP_SMOKE_PORT}" "${TLS_HTTP_SERVER_READY}" \
        > "${TLS_HTTP_SERVER_LOG}" 2>&1 &
    TLS_HTTP_SERVER_PID=$!
    for _ in {1..100}; do
        if [[ -s "${TLS_HTTP_SERVER_READY}" ]]; then
            echo "[run] TLS HTTP smoke server guest=10.0.2.2:${TLS_HTTP_SMOKE_PORT}"
            return 0
        fi
        kill -0 "${TLS_HTTP_SERVER_PID}" 2>/dev/null ||
            die "TLS HTTP smoke server exited before becoming ready; see ${TLS_HTTP_SERVER_LOG}"
        sleep 0.05
    done
    die "TLS HTTP smoke server did not become ready; see ${TLS_HTTP_SERVER_LOG}"
}

start_tls_bad_cv_server() {
    : > "${TLS_BAD_CV_SERVER_LOG}"
    rm -f "${TLS_BAD_CV_SERVER_READY}"
    "${TLS_BAD_CV_SERVER}" \
        "$((TLS_HTTP_SMOKE_PORT + 4))" "${TLS_BAD_CV_SERVER_READY}" \
        "${ROOT_DIR}/user/crates/tls-client/test-fixtures/server.cert.pem" \
        "${ROOT_DIR}/user/crates/tls-client/test-fixtures/server.key.pem" \
        > "${TLS_BAD_CV_SERVER_LOG}" 2>&1 &
    TLS_BAD_CV_SERVER_PID=$!
    for _ in {1..100}; do
        if [[ -s "${TLS_BAD_CV_SERVER_READY}" ]]; then
            echo "[run] bad CertificateVerify server guest=10.0.2.2:$((TLS_HTTP_SMOKE_PORT + 4))"
            return 0
        fi
        kill -0 "${TLS_BAD_CV_SERVER_PID}" 2>/dev/null ||
            die "bad CertificateVerify server exited before becoming ready; see ${TLS_BAD_CV_SERVER_LOG}"
        sleep 0.05
    done
    die "bad CertificateVerify server did not become ready; see ${TLS_BAD_CV_SERVER_LOG}"
}

start_qemu() {
    echo "[run] qemu accelerator=${QEMU_ACCEL} gpu=${QEMU_GPU_BACKEND} gl-display=${QEMU_GL_DISPLAY} network=${QEMU_NETWORK} mac=${QEMU_NETWORK_MAC}"

    GALLIUM_DRIVER=d3d12 \
    MESA_D3D12_DEFAULT_ADAPTER_NAME=AMD \
    qemu-system-x86_64 "${QEMU_ARGS[@]}" > >(tee -a "${SERIAL_LOG}") 2>&1 &

    QEMU_PID=$!
}

if [[ "${QEMU_TCP_ECHO_SERVER}" == "y" ]]; then
    start_network_echo_server
fi
if [[ "${TLS_HTTP_CLIENT_SMOKE}" == "1" ]]; then
    start_tls_http_server
    start_tls_bad_cv_server
fi

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
COMPLETION_OBSERVED_AT=0
MBOOT_LOGIN_READY_AT=0
MBOOT_LOGIN_SENT=0
DEADLINE=$((SECONDS + QEMU_TIMEOUT_SECONDS))
while ((SECONDS < DEADLINE)); do
    while IFS= read -r line; do
        if [[ "${line}" == *"PAGE FAULT"* || "${line}" == *"Faulting user context:"* || "${line}" == *"panic"* || "${line}" == *"Error: MochiOs("* ]]; then
            die "fatal runtime error observed during QEMU run"
        fi
    done < <(sed -n "${NEXT_LINE},\$p" "${SERIAL_LOG}")

    NEXT_LINE="$(($(wc -l < "${SERIAL_LOG}") + 1))"

    if [[ "${MBOOT_CONTROL_REQUIRED}" == "1" && "${MBOOT_LOGIN_SENT}" == "0" ]] \
        && log_has "exec: loaded '/system/services/secure-ui.service'"; then
        if [[ "${MBOOT_LOGIN_READY_AT}" == "0" ]]; then
            MBOOT_LOGIN_READY_AT="${SECONDS}"
        elif ((SECONDS - MBOOT_LOGIN_READY_AT >= 2)) && [[ -S "${MONITOR_SOCKET}" ]]; then
            hmp_command "sendkey ret"
            MBOOT_LOGIN_SENT=1
        fi
    fi

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
        && log_has "exec: loaded '/bin/drivers/network/virtio-net.driver/virtio-net.driver'" \
        && log_has "exec: loaded '/system/services/network.service'" \
        && log_has "exec: loaded '/system/services/user.service'" \
        && log_has "exec: loaded '/system/services/secure-ui.service'" \
        && mboot_control_complete; then
        if [[ "${COMPLETION_OBSERVED_AT}" -eq 0 ]]; then
            COMPLETION_OBSERVED_AT="${SECONDS}"
        elif ((SECONDS - COMPLETION_OBSERVED_AT >= 2)); then
            COMPLETED=1
            break
        fi
    fi

    if ! kill -0 "${QEMU_PID}" 2>/dev/null; then
        die "QEMU exited before the smoke-test completion logs were observed; see ${SERIAL_LOG}"
    fi

    sleep 0.1
done

[[ "${COMPLETED}" == "1" ]] ||
    die "QEMU smoke test timed out after ${QEMU_TIMEOUT_SECONDS}s; see ${SERIAL_LOG}"

if [[ "${MBOOT_CONTROL_REQUIRED}" == "1" ]]; then
    echo "[check] mBoot control reached desktop and reported heartbeat"
fi

if [[ "${QEMU_NETWORK}" == "y" && "${QEMU_NETWORK_SETTLE_SECONDS}" -gt 0 ]]; then
    sleep "${QEMU_NETWORK_SETTLE_SECONDS}"
fi

if [[ "${NETWORK_CLIENT_SMOKE}" == "1" ]]; then
    run_guest_command 0 "net resolve localhost" "localhost -> 127.0.0.1"
    run_guest_command 0 \
        "net tcp-connect 10.0.2.2 ${NETWORK_SMOKE_PORT}" \
        "Connected to 10.0.2.2:${NETWORK_SMOKE_PORT}"
    run_guest_command 0 \
        "net tcp-send 10.0.2.2 ${NETWORK_SMOKE_PORT} mochios-tcp-smoke" \
        "sent=17 received=17 data=mochios-tcp-smoke"
    for _ in {1..100}; do
        [[ "$(grep -Fc 'network-smoke-server: echoed=' "${NETWORK_SERVER_LOG}")" -ge 2 ]] && break
        sleep 0.05
    done
    grep -Fq "network-smoke-server: echoed=0" "${NETWORK_SERVER_LOG}" ||
        die "network smoke server did not observe the connect-only guest FIN"
    grep -Fq "network-smoke-server: echoed=17" "${NETWORK_SERVER_LOG}" ||
        die "network smoke server did not echo the expected payload"
fi

if [[ -n "${SMOKE_GUEST_COMMAND}" ]]; then
    if [[ -n "${SMOKE_GUEST_EXPECT}" ]]; then
        run_guest_command "${SMOKE_GUEST_EXPECTED_EXIT}" \
            "${SMOKE_GUEST_COMMAND}" "${SMOKE_GUEST_EXPECT}"
    else
        run_guest_command "${SMOKE_GUEST_EXPECTED_EXIT}" "${SMOKE_GUEST_COMMAND}"
    fi
fi

if [[ "${TLS_HTTP_CLIENT_SMOKE}" == "1" ]]; then
    run_guest_command 0 \
        "net tls-connect tls.test.mochios ${TLS_HTTP_SMOKE_PORT}" \
        "TLS version: TLS 1.3" \
        "Server hostname: tls.test.mochios" \
        "Certificate issuer:"
    run_guest_command 0 \
        "net https-get https://tls.test.mochios:${TLS_HTTP_SMOKE_PORT}/content-length" \
        "Status: 200" \
        'Body:' \
        '"framing":"content-length"'
    run_guest_command 0 \
        "net https-get https://tls.test.mochios:${TLS_HTTP_SMOKE_PORT}/chunked" \
        "Status: 200" \
        "mochiOS chunked smoke"
    run_guest_command 1 \
        "net tls-connect untrusted.test.mochios $((TLS_HTTP_SMOKE_PORT + 1))" \
        "TLS failure: CertificateInvalid"
    run_guest_command 1 \
        "net tls-connect mismatch.test.mochios ${TLS_HTTP_SMOKE_PORT}" \
        "TLS failure: HostnameMismatch"
    run_guest_command 1 \
        "net tls-connect expired.test.mochios $((TLS_HTTP_SMOKE_PORT + 2))" \
        "TLS failure: CertificateInvalid"
    run_guest_command 1 \
        "net tls-connect tls.test.mochios $((TLS_HTTP_SMOKE_PORT + 4))" \
        "TLS failure: CertificateInvalid"
    run_guest_command 1 \
        "net tls-connect tls.test.mochios $((TLS_HTTP_SMOKE_PORT + 5))" \
        "TLS failure: Timeout"
    run_guest_command 1 \
        "net https-get https://tls.test.mochios:$((TLS_HTTP_SMOKE_PORT + 3))/content-length" \
        "HTTP failure: Tls"
    run_guest_command 1 \
        "net https-get https://tls.test.mochios:${TLS_HTTP_SMOKE_PORT}/header-overflow" \
        "HTTP failure: HeaderLimit"
    run_guest_command 1 \
        "net https-get https://tls.test.mochios:${TLS_HTTP_SMOKE_PORT}/bad-content-length" \
        "HTTP failure: InvalidResponse"
    run_guest_command 1 \
        "net https-get https://tls.test.mochios:${TLS_HTTP_SMOKE_PORT}/bad-chunk" \
        "HTTP failure: ChunkError"
    run_guest_command 1 \
        "net https-get https://tls.test.mochios:${TLS_HTTP_SMOKE_PORT}/redirect-http" \
        "HTTP failure: RedirectRejected"
    run_guest_command 1 \
        "net https-get https://tls.test.mochios:${TLS_HTTP_SMOKE_PORT}/body-overflow" \
        "HTTP failure: BodyLimit"
    run_guest_command 0 "net stats" \
        "tls_connections_attempted=" \
        "tls_decrypt_failures=1" \
        "http_requests="
fi

if [[ "${ACCOUNTS_HTTPS_SMOKE}" == "1" ]]; then
    run_guest_command 0 \
        "net tls-connect accounts.mochios.org 443" \
        "TLS version: TLS 1.3" \
        "Server hostname: accounts.mochios.org"
    run_guest_command 0 \
        "net https-get https://accounts.mochios.org/health" \
        "Status: 200" \
        '"service":"accounts"' \
        '"status":"ok"'
fi

if [[ "${MPKG_RUNTIME_SMOKE}" == "1" ]]; then
    run_guest_command 0 "mpk /system/samples/mpk-test.mpkg"
fi

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
        SCREENSHOT_RESPONSE=""
        for _ in {1..300}; do
            SCREENSHOT_RESPONSE="$(hmp_screendump 2>&1 || true)"
            if [[ -s "${GPU_SCREENSHOT}" ]] \
                && "${SCRIPT_DIR}/check-virtio-gpu-pixels.pl" \
                    "${GPU_SCREENSHOT}" >/dev/null 2>&1; then
                PIXELS_READY=1
                break
            fi
            sleep 0.1
        done
        if [[ ! -f "${GPU_SCREENSHOT}" ]]; then
            if [[ "${SCREENSHOT_RESPONSE}" == *"Error: no surface"* ]]; then
                die "virtio-gpu screendump failed: QEMU reported no DisplaySurface"
            fi
            die "virtio-gpu screendump failed; monitor returned no image"
        fi
        [[ "${PIXELS_READY}" == "1" ]] ||
            die "virtio-gpu scanout did not reach the expected test scene"
        "${SCRIPT_DIR}/check-virtio-gpu-pixels.pl" "${GPU_SCREENSHOT}"
    else
        hmp_screendump >/dev/null
    fi
fi

sleep 1
cleanup
QEMU_PID=""

if [[ "${TLS_HTTP_CLIENT_SMOKE}" == "1" ]]; then
    grep -Fq "path=/content-length close-notify=complete" "${TLS_HTTP_SERVER_LOG}" ||
        die "TLS HTTP server did not complete close_notify for Content-Length response"
    grep -Fq "path=/chunked close-notify=complete" "${TLS_HTTP_SERVER_LOG}" ||
        die "TLS HTTP server did not complete close_notify for chunked response"
    for path in /header-overflow /bad-content-length /bad-chunk /redirect-http /body-overflow; do
        grep -Fq "path=${path} request=received" "${TLS_HTTP_SERVER_LOG}" ||
            die "TLS HTTP server did not receive expected failure request ${path}"
    done
    grep -Fq "record=tampered" "${TLS_HTTP_SERVER_LOG}" ||
        die "TLS record tamper proxy did not alter an encrypted server record"
    grep -Fq "bad-certificate-verify=sent" "${TLS_BAD_CV_SERVER_LOG}" ||
        die "bad CertificateVerify server did not send the altered signature"
fi

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
debugfs -R 'cat /system/logs/services/network.log' "${ROOTFS_IMAGE}" \
    > "${NETWORK_LOG}" 2>/dev/null || die "network.service log could not be read"
debugfs -R 'cat /system/logs/services/user.log' "${ROOTFS_IMAGE}" \
    > "${USER_LOG}" 2>/dev/null || die "user.service log could not be read"
grep -Fq 'user.service: ready users=2' "${USER_LOG}" ||
    die "user.service did not load its initial database; see ${USER_LOG}"

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
    "${SERIAL_LOG}" "${SERVICE_MANAGER_LOG}" "${DRIVERS_LOG}" "${NETWORK_LOG}" \
    "${NETWORK_CLIENT_SMOKE}" "${ENABLE_XHCI}" "${TLS_HTTP_CLIENT_SMOKE}" \
    "${ACCOUNTS_HTTPS_SMOKE}" "${MPKG_RUNTIME_SMOKE}"

echo "[done] serial log: ${SERIAL_LOG}"
