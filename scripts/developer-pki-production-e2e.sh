#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

if [[ "${1:-}" == "--root-public-record" ]]; then
    [[ $# -eq 2 ]] || {
        echo "usage: $0 [--root-public-record <root-public.json>]" >&2
        exit 2
    }
    command -v python3 >/dev/null 2>&1 || {
        echo "fatal: python3 is required when --root-public-record is used" >&2
        exit 1
    }
    MOCHIOS_DEVELOPER_ROOT_PUBLIC_KEYS_HEX="$(python3 -c '
import base64
import json
import pathlib
import sys

record = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
public_key = base64.b64decode(record["public_key"], validate=True)
if len(public_key) != 32:
    raise SystemExit("fatal: Offline Root public key must be 32 bytes")
print(public_key.hex())
' "$2")"
    export MOCHIOS_DEVELOPER_ROOT_PUBLIC_KEYS_HEX
elif [[ $# -ne 0 ]]; then
    echo "usage: $0 [--root-public-record <root-public.json>]" >&2
    exit 2
fi

if [[ -z "${MOCHIOS_DEVELOPER_ROOT_PUBLIC_KEYS_HEX:-}" ]]; then
    echo "fatal: MOCHIOS_DEVELOPER_ROOT_PUBLIC_KEYS_HEX or --root-public-record is required" >&2
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
