#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

ACCOUNTS_HTTPS_SMOKE=1 \
QEMU_ACCELERATOR="${QEMU_ACCELERATOR:-tcg}" \
QEMU_TIMEOUT_SECONDS="${QEMU_TIMEOUT_SECONDS:-240}" \
    "${SCRIPT_DIR}/smoke-test.sh"
