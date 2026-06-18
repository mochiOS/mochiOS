#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_JSON="${ROOT_DIR}/targets/x86_64-unknown-mochios.json"
SYSROOT_DIR="${ROOT_DIR}/target/mochios-sysroot"

mkdir -p "${SYSROOT_DIR}"

cargo +nightly build \
  -Z build-std=core,alloc,compiler_builtins \
  -Z json-target-spec \
  --target "${TARGET_JSON}" \
  --manifest-path "${ROOT_DIR}/../services/core/Cargo.toml" \
  --release

echo "sysroot build completed at ${SYSROOT_DIR}"
