#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

command -v curl >/dev/null 2>&1 || {
    echo "fatal: curl is required" >&2
    exit 1
}
command -v python3 >/dev/null 2>&1 || {
    echo "fatal: python3 is required" >&2
    exit 1
}

echo "[test] deterministic Developer PKI TLS synchronization"
cargo test \
    --manifest-path "${ROOT_DIR}/services/update/Cargo.toml" \
    --test deterministic_sync \
    -- --ignored --nocapture
