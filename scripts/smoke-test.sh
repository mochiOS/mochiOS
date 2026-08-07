#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
MBOOTD_MANIFEST="${ROOT_DIR}/mboot/Cargo.toml"
MBOOTD_BINARY="${ROOT_DIR}/mboot/target/debug/mbootd"
MBOOT_CONTROL_DIR="${ROOT_DIR}/out/mboot-control-smoke"
MBOOT_CONTROL_SOCKET="/tmp/mochios-mboot-control-$$.sock"
MBOOT_CONTROL_LOG="${MBOOT_CONTROL_DIR}/mbootd-$$.log"
MBOOTD_PID=""

cleanup() {
    if [[ -n "${MBOOTD_PID}" ]]; then
        kill -TERM "${MBOOTD_PID}" 2>/dev/null || true
        wait "${MBOOTD_PID}" 2>/dev/null || true
    fi
    rm -f "${MBOOT_CONTROL_SOCKET}"
}

trap cleanup EXIT

cargo build --manifest-path "${MBOOTD_MANIFEST}" -p mbootd
mkdir -p "${MBOOT_CONTROL_DIR}"
: > "${MBOOT_CONTROL_LOG}"
stdbuf -oL -eL "${MBOOTD_BINARY}" "${MBOOT_CONTROL_SOCKET}" \
    >"${MBOOT_CONTROL_LOG}" 2>&1 &
MBOOTD_PID=$!
for _ in {1..100}; do
    if [[ -S "${MBOOT_CONTROL_SOCKET}" ]]; then
        break
    fi
    kill -0 "${MBOOTD_PID}" 2>/dev/null || {
        cat "${MBOOT_CONTROL_LOG}" >&2
        echo "fatal: mbootd exited before creating its control socket" >&2
        exit 1
    }
    sleep 0.05
done
[[ -S "${MBOOT_CONTROL_SOCKET}" ]] || {
    echo "fatal: mbootd control socket was not created" >&2
    exit 1
}

export DEBUG_QEMU_GUI=n
export DEBUG_QEMU_DEBUG=n
export SMOKE_TEST=1
export NETWORK_CLIENT_SMOKE="${NETWORK_CLIENT_SMOKE:-0}"
export MPKG_RUNTIME_SMOKE="${MPKG_RUNTIME_SMOKE:-0}"
export QEMU_TCP_ECHO_SERVER="${QEMU_TCP_ECHO_SERVER:-n}"
export QEMU_MBOOT_CONTROL_SOCKET="${MBOOT_CONTROL_SOCKET}"
export MBOOT_CONTROL_REQUIRED=1
export MBOOT_CONTROL_LOG
export SMOKE_USER_DATABASE_FIXTURE="${SCRIPT_DIR}/tests/fixtures/mboot-users.db"

if ! "${SCRIPT_DIR}/runner.sh"; then
    echo "--- mbootd log ---" >&2
    cat "${MBOOT_CONTROL_LOG}" >&2
    exit 1
fi
echo "[done] mbootd log: ${MBOOT_CONTROL_LOG}"
