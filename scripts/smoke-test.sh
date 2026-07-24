#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

export DEBUG_QEMU_GUI=n
export DEBUG_QEMU_DEBUG=n
export SMOKE_TEST=1

exec "${SCRIPT_DIR}/runner.sh"
