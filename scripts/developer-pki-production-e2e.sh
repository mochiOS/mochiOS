#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

if [[ -z "${MOCHIOS_DEVELOPER_ROOT_PUBLIC_KEYS_HEX:-}" ]]; then
    echo "fatal: MOCHIOS_DEVELOPER_ROOT_PUBLIC_KEYS_HEX is required" >&2
    exit 2
fi
command -v curl >/dev/null 2>&1 || {
    echo "fatal: curl is required" >&2
    exit 1
}

echo "[test] production DeveloperCA TLS synchronization"
cargo test \
    --manifest-path "${ROOT_DIR}/services/update/Cargo.toml" \
    --test production_e2e \
    -- --ignored --nocapture
