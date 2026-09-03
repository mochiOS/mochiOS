#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

export DEBUG_QEMU_GUI=n
export DEBUG_QEMU_DEBUG=n
export SMOKE_TEST=1
export SMOKE_AUTO_LOGIN=1
export SMOKE_CHECK_SERVICE_LOGS="${SMOKE_CHECK_SERVICE_LOGS:-0}"
export NETWORK_CLIENT_SMOKE="${NETWORK_CLIENT_SMOKE:-0}"
export MPKG_RUNTIME_SMOKE="${MPKG_RUNTIME_SMOKE:-0}"
export QEMU_TCP_ECHO_SERVER="${QEMU_TCP_ECHO_SERVER:-n}"
export SMOKE_USER_DATABASE_FIXTURE="${SCRIPT_DIR}/tests/fixtures/mboot-users.db"

"${SCRIPT_DIR}/runner.sh"
