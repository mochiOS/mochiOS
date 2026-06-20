#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
USER_ROOT="${ROOT_DIR}/user"
NEWLIB_ROOT="${ROOT_DIR}/libraries/newlib"

BOOTSTRAP_TARGET="x86_64-elf"
FINAL_TARGET="x86_64-unknown-mochios"
TARGET_JSON="${USER_ROOT}/targets/${FINAL_TARGET}.json"

OUT_ROOT="${ROOT_DIR}/out/newlib-port"
NEWLIB_BUILD_DIR="${OUT_ROOT}/build-newlib"
INSTALL_ROOT="${OUT_ROOT}/toolchain"
SYSROOT_DIR="${INSTALL_ROOT}/${BOOTSTRAP_TARGET}"
RUNTIME_TARGET_DIR="${OUT_ROOT}/cargo-target"
HELLO_DIR="${OUT_ROOT}/hello"
CRT0_S="${USER_ROOT}/runtime/crt0.S"
LINKER_SCRIPT="${USER_ROOT}/runtime/linker.ld"
HELLO_C="${USER_ROOT}/libc-port/tests/hello.c"

need_cmd() {
    command -v "$1" >/dev/null 2>&1 || {
        echo "missing command: $1" >&2
        exit 1
    }
}

need_file() {
    [[ -f "$1" ]] || {
        echo "missing file: $1" >&2
        exit 1
    }
}

need_dir() {
    [[ -d "$1" ]] || {
        echo "missing directory: $1" >&2
        exit 1
    }
}

need_dir "${ROOT_DIR}/.repo"
need_file "${NEWLIB_ROOT}/configure"
need_file "${USER_ROOT}/Cargo.toml"
need_file "${TARGET_JSON}"
need_file "${CRT0_S}"
need_file "${LINKER_SCRIPT}"
need_file "${HELLO_C}"

need_cmd cargo
need_cmd make
need_cmd nm
need_cmd readelf
need_cmd x86_64-elf-ar
need_cmd x86_64-elf-gcc
need_cmd x86_64-elf-ranlib

mkdir -p "${OUT_ROOT}" "${RUNTIME_TARGET_DIR}" "${HELLO_DIR}"

echo "[test] user existing tests"
cargo +nightly-2026-05-14 test \
    --manifest-path "${USER_ROOT}/Cargo.toml" \
    -p mochi-user-syscall

rm -rf "${NEWLIB_BUILD_DIR}" "${INSTALL_ROOT}"
mkdir -p "${NEWLIB_BUILD_DIR}" "${INSTALL_ROOT}"

echo "[build] configure newlib"
(
    cd "${NEWLIB_BUILD_DIR}"
    env \
        CC_FOR_TARGET=x86_64-elf-gcc \
        AR_FOR_TARGET=x86_64-elf-ar \
        RANLIB_FOR_TARGET=x86_64-elf-ranlib \
        "${NEWLIB_ROOT}/configure" \
        --target="${BOOTSTRAP_TARGET}" \
        --prefix="${INSTALL_ROOT}" \
        --disable-binutils \
        --disable-gas \
        --disable-gdb \
        --disable-gprof \
        --disable-libgloss \
        --disable-multilib \
        --disable-nls \
        --disable-shared \
        --disable-sim \
        --disable-werror \
        --disable-newlib-supplied-syscalls \
        --enable-newlib-multithread=no \
        --enable-newlib-retargetable-locking
)

echo "[build] newlib"
make -C "${NEWLIB_BUILD_DIR}" -j"$(nproc)" all-target-newlib

echo "[build] install newlib"
make -C "${NEWLIB_BUILD_DIR}" install-target-newlib

echo "[build] mochiOS runtime"
cargo +nightly-2026-05-14 build \
    -Z json-target-spec \
    -Z build-std=core,compiler_builtins \
    --manifest-path "${USER_ROOT}/Cargo.toml" \
    --package mochi-user-newlib-runtime \
    --release \
    --target "${TARGET_JSON}" \
    --target-dir "${RUNTIME_TARGET_DIR}"

RUNTIME_LIB="${RUNTIME_TARGET_DIR}/${FINAL_TARGET}/release/libmochi_user_newlib_runtime.a"
CRT0_O="${HELLO_DIR}/crt0.o"
HELLO_O="${HELLO_DIR}/hello.o"
HELLO_ELF="${HELLO_DIR}/hello.elf"
HELLO_MAP="${HELLO_DIR}/hello.map"

need_file "${RUNTIME_LIB}"

echo "[build] crt0"
x86_64-elf-gcc -c "${CRT0_S}" -o "${CRT0_O}"

echo "[build] hello.c"
x86_64-elf-gcc \
    --sysroot="${SYSROOT_DIR}" \
    -isystem "${SYSROOT_DIR}/include" \
    -ffreestanding \
    -O2 \
    -c "${HELLO_C}" \
    -o "${HELLO_O}"

echo "[link] hello.elf"
x86_64-elf-gcc \
    --sysroot="${SYSROOT_DIR}" \
    -L"${SYSROOT_DIR}/lib" \
    -static \
    -nostdlib \
    -nostartfiles \
    -Wl,-T,"${LINKER_SCRIPT}" \
    -Wl,-no-pie \
    -Wl,-z,noexecstack \
    -Wl,-Map,"${HELLO_MAP}" \
    -Wl,--start-group \
    "${CRT0_O}" \
    "${HELLO_O}" \
    "${RUNTIME_LIB}" \
    -lc \
    -lm \
    -lgcc \
    -Wl,--end-group \
    -o "${HELLO_ELF}"

echo "[check] hello.elf header"
readelf -h "${HELLO_ELF}" >/dev/null
readelf -l "${HELLO_ELF}" >/dev/null
if nm -u "${HELLO_ELF}" | grep . >/dev/null; then
    echo "hello.elf still has undefined symbols" >&2
    exit 1
fi

echo "[done] ${HELLO_ELF}"
