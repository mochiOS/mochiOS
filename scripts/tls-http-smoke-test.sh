#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
TEST_ROOT="$(mktemp -d "${ROOT_DIR}/out/tls-http-smoke.XXXXXX")"
TEST_ARTIFACTS="${TEST_ROOT}/artifacts"

cleanup() {
    rm -rf "${TEST_ROOT}"
}
trap cleanup EXIT

echo "[step] build test-only bad CertificateVerify server"
CARGO_TARGET_DIR="${ROOT_DIR}/out/tls-http-smoke-host-target" \
    cargo build --release \
    --manifest-path "${SCRIPT_DIR}/tools/tls-bad-cv-server/Cargo.toml"

echo "[step] build test-only Web PKI image"
MOCHIOS_NETWORK_TEST_WEB_PKI=1 \
    "${SCRIPT_DIR}/build.sh" --cached
cp -a "${ROOT_DIR}/out/artifacts" "${TEST_ARTIFACTS}"

echo "[step] restore production Web PKI image"
"${SCRIPT_DIR}/build.sh" --cached
if grep -aFq ".test.mochios" "${ROOT_DIR}/out/artifacts/network.service"; then
    echo "fatal: production network.service contains the test-only PKI resolver" >&2
    exit 1
fi

echo "[step] run deterministic TLS and HTTP client smoke test"
ARTIFACT_DIR="${TEST_ARTIFACTS}" \
TLS_HTTP_CLIENT_SMOKE=1 \
QEMU_ACCELERATOR="${QEMU_ACCELERATOR:-tcg}" \
QEMU_TIMEOUT_SECONDS="${QEMU_TIMEOUT_SECONDS:-240}" \
    "${SCRIPT_DIR}/smoke-test.sh"
