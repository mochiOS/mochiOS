#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
CORE_ROOT="${ROOT_DIR}/core"

TARGET_DIR="${CORE_ROOT}/target/uefi"
KERNEL_TARGET="x86_64-unknown-none"
USER_TARGET="x86_64-unknown-none"
USER_BUILD_DIR="${TARGET_DIR}/user-build"
PLUGKIT_BUILD_DIR="${TARGET_DIR}/plugkit-build"

RUN_ID="workspace-$(date +%s)-$$"
RUN_DIR="${ROOT_DIR}/out/runner/${RUN_ID}"
ESP_DIR="${RUN_DIR}/esp"
ESP_IMG="${RUN_DIR}/esp.img"
INITFS_STAGE="${RUN_DIR}/initfs-root"
ROOTFS_STAGE="${RUN_DIR}/rootfs-root"
SIGNATURE_DB_STAGE="${RUN_DIR}/signature.db"
SERIAL_LOG="${RUN_DIR}/serial.log"

OVMF_CODE="${OVMF_CODE:-/usr/share/OVMF/OVMF_CODE_4M.fd}"
OVMF_VARS_TEMPLATE="${OVMF_VARS_TEMPLATE:-/usr/share/OVMF/OVMF_VARS_4M.fd}"
OVMF_VARS="${RUN_DIR}/OVMF_VARS_4M.fd"

die() {
    echo "fatal: $*" >&2
    exit 1
}

need_cmd() {
    command -v "$1" >/dev/null 2>&1 || die "required command not found: $1"
}

need_file() {
    [[ -f "$1" ]] || die "required file not found: $1"
}

need_cmd cargo
need_cmd openssl
need_cmd perl
need_cmd qemu-system-x86_64
need_cmd mke2fs
need_cmd mkfs.fat
need_cmd mmd
need_cmd mcopy
need_cmd tee
need_cmd sed
need_cmd wc
need_cmd truncate

need_file "${CORE_ROOT}/Cargo.toml"
need_file "${CORE_ROOT}/examples/user/Cargo.toml"
need_file "${CORE_ROOT}/examples/user/linker.ld"
need_file "${CORE_ROOT}/examples/plugkit/test/Cargo.toml"
need_file "${CORE_ROOT}/examples/plugkit/test/about.toml"
need_file "${CORE_ROOT}/scripts/generate_testdata.pl"
need_file "${CORE_ROOT}/scripts/rootfs.sh"
need_file "${CORE_ROOT}/scripts/signature_db.pl"
need_file "${CORE_ROOT}/scripts/cexts.sh"
need_file "${SCRIPT_DIR}/build-hello.sh"
need_file "${SCRIPT_DIR}/build-bootloader.sh"
need_file "${OVMF_CODE}"
need_file "${OVMF_VARS_TEMPLATE}"

mkdir -p "${RUN_DIR}" "${ESP_DIR}/EFI/BOOT" "${INITFS_STAGE}" "${ROOTFS_STAGE}"

echo "[step] build user/newlib runtime"
"${SCRIPT_DIR}/build-hello.sh"

# shellcheck disable=SC1091
source "${CORE_ROOT}/scripts/cexts.sh"

echo "[step] generate test data"
(
    cd "${CORE_ROOT}"
    ./scripts/generate_testdata.pl
)

echo "[step] build kernel"
(
    cd "${CORE_ROOT}"
    cargo build \
        --locked \
        --release \
        --target "${KERNEL_TARGET}" \
        --features kernel-bin \
        --manifest-path "${CORE_ROOT}/Cargo.toml"
)

echo "[step] build core.service userland"
(
    cd "${CORE_ROOT}"
    env RUSTFLAGS="-C relocation-model=static -C link-arg=-T${CORE_ROOT}/examples/user/linker.ld -C link-arg=-no-pie --cfg curve25519_dalek_backend=\"serial\"" \
        cargo build \
        --locked \
        --release \
        --target "${USER_TARGET}" \
        --target-dir "${USER_BUILD_DIR}" \
        --manifest-path "${CORE_ROOT}/examples/user/Cargo.toml"
)

echo "[step] build plugkit test"
(
    cd "${CORE_ROOT}"
    env RUSTFLAGS="-C relocation-model=static -C link-arg=-T${CORE_ROOT}/examples/user/linker.ld -C link-arg=-no-pie --cfg curve25519_dalek_backend=\"serial\"" \
        cargo build \
        --locked \
        --release \
        --target "${USER_TARGET}" \
        --target-dir "${PLUGKIT_BUILD_DIR}" \
        --manifest-path "${CORE_ROOT}/examples/plugkit/test/Cargo.toml"
)

echo "[step] build bootloader"
"${SCRIPT_DIR}/build-bootloader.sh"

HELLO_ELF="${ROOT_DIR}/out/newlib-port/hello/hello.elf"
KERNEL_BIN="${CORE_ROOT}/target/${KERNEL_TARGET}/release/kernel"
USER_BIN="${USER_BUILD_DIR}/${USER_TARGET}/release/user"
PLUGKIT_TEST_BIN="${PLUGKIT_BUILD_DIR}/${USER_TARGET}/release/entry"
BOOT_BIN="$(find "${CORE_ROOT}/target/uefi/boot-build-temp/x86_64-unknown-uefi/release" -maxdepth 1 -type f \( -name 'boot' -o -name 'boot.efi' \) | head -n 1)"

need_file "${HELLO_ELF}"
need_file "${KERNEL_BIN}"
need_file "${USER_BIN}"
need_file "${PLUGKIT_TEST_BIN}"
need_file "${BOOT_BIN}"

echo "[step] stage image files"
rm -rf "${ESP_DIR}" "${INITFS_STAGE}" "${ROOTFS_STAGE}"
mkdir -p "${ESP_DIR}/EFI/BOOT" "${INITFS_STAGE}/bin" "${INITFS_STAGE}/plugkit/test"

install -m 0644 "${KERNEL_BIN}" "${ESP_DIR}/kernel"
install -m 0644 "${BOOT_BIN}" "${ESP_DIR}/EFI/BOOT/BOOTX64.EFI"
install -m 0755 "${USER_BIN}" "${INITFS_STAGE}/core.service"
install -m 0755 "${HELLO_ELF}" "${INITFS_STAGE}/captest.bin"
install -m 0755 "${HELLO_ELF}" "${INITFS_STAGE}/unsigned.bin"
install -m 0755 "${HELLO_ELF}" "${INITFS_STAGE}/bin/hello"
install -m 0644 "${CORE_ROOT}/examples/plugkit/test/about.toml" "${INITFS_STAGE}/plugkit/test/about.toml"
install -m 0755 "${PLUGKIT_TEST_BIN}" "${INITFS_STAGE}/plugkit/test/entry.elf"

ROOT_DIR="${CORE_ROOT}" INITFS_STAGE="${INITFS_STAGE}" stage_module_cexts

echo "[step] build signature db"
SIGNATURE_DB_ARGS=(
    --output "${SIGNATURE_DB_STAGE}"
    --entry "core.service=${USER_BIN}"
    --entry "/plugkit/test/entry.elf=${PLUGKIT_TEST_BIN}"
    --entry "/captest.bin=${HELLO_ELF}"
    --entry "/bin/hello=${HELLO_ELF}"
)
while IFS= read -r -d '' module_path; do
    module_name="$(basename "${module_path}")"
    SIGNATURE_DB_ARGS+=(--entry "/Modules/${module_name}=${module_path}")
done < <(find "${INITFS_STAGE}/Modules" -maxdepth 1 -type f -name '*.cext' -print0 2>/dev/null || true)
perl "${CORE_ROOT}/scripts/signature_db.pl" "${SIGNATURE_DB_ARGS[@]}"

echo "[step] build rootfs"
ROOTFS_SOURCE_DIR="${CORE_ROOT}/examples/fs/rootfs" \
INITFS_STAGE="${INITFS_STAGE}" \
ROOTFS_STAGE="${ROOTFS_STAGE}" \
ROOTFS_IMG="${RUN_DIR}/rootfs.img" \
ROOTFS_CLEAN_INITFS=0 \
SIGNATURE_DB_SRC="${SIGNATURE_DB_STAGE}" \
bash "${CORE_ROOT}/scripts/rootfs.sh"
install -D -m 0755 "${HELLO_ELF}" "${ROOTFS_STAGE}/bin/hello"

echo "[step] build initfs"
truncate -s 16M "${RUN_DIR}/initfs.img"
mke2fs -q -t ext2 -b 1024 -d "${INITFS_STAGE}" -F "${RUN_DIR}/initfs.img"

install -m 0644 "${RUN_DIR}/initfs.img" "${ESP_DIR}/initfs.img"
install -m 0644 "${RUN_DIR}/rootfs.img" "${ESP_DIR}/rootfs.img"

echo "[step] build esp.img"
rm -f "${ESP_IMG}"
truncate -s 64M "${ESP_IMG}"
mkfs.fat -F 32 -n EFI "${ESP_IMG}" >/dev/null
MTOOLS_SKIP_CHECK=1 mmd -i "${ESP_IMG}" ::/EFI
MTOOLS_SKIP_CHECK=1 mmd -i "${ESP_IMG}" ::/EFI/BOOT
MTOOLS_SKIP_CHECK=1 mcopy -i "${ESP_IMG}" "${ESP_DIR}/kernel" ::/kernel
MTOOLS_SKIP_CHECK=1 mcopy -i "${ESP_IMG}" "${ESP_DIR}/initfs.img" ::/initfs
MTOOLS_SKIP_CHECK=1 mcopy -i "${ESP_IMG}" "${ESP_DIR}/rootfs.img" ::/rootfs
MTOOLS_SKIP_CHECK=1 mcopy -i "${ESP_IMG}" "${ESP_DIR}/EFI/BOOT/BOOTX64.EFI" ::/EFI/BOOT/BOOTX64.EFI

cp "${OVMF_VARS_TEMPLATE}" "${OVMF_VARS}"
: > "${SERIAL_LOG}"

QEMU_ARGS=(
    -machine q35
    -m 512M
    -smp 4
    -cpu qemu64
    -serial stdio
    -display none
    -monitor none
    -no-reboot
    -drive "if=pflash,format=raw,readonly=on,file=${OVMF_CODE}"
    -drive "if=pflash,format=raw,file=${OVMF_VARS}"
    -drive "format=raw,file=${ESP_IMG}"
)

if [[ "${DEBUG:-0}" != "0" ]]; then
    QEMU_ARGS+=(-s -S)
fi

echo "[run] qemu"
qemu-system-x86_64 "${QEMU_ARGS[@]}" > >(tee -a "${SERIAL_LOG}") 2>&1 &
QEMU_PID=$!

cleanup() {
    if [[ -n "${QEMU_PID:-}" ]]; then
        kill "${QEMU_PID}" 2>/dev/null || true
        wait "${QEMU_PID}" 2>/dev/null || true
    fi
}
trap cleanup EXIT

HELLO_FOUND=0
EXIT_FOUND=0
PASS_FOUND=0
NEXT_LINE=1

for _ in $(seq 1 600); do
    while IFS= read -r line; do
        if [[ "${line}" == *"hello from mochiOS, argc="* ]]; then
            HELLO_FOUND=1
        fi
        if [[ "${line}" == *"Process exiting with code: 0"* ]]; then
            EXIT_FOUND=1
        fi
        if [[ "${line}" == *"USERLAND SELF-TEST PASS"* ]]; then
            PASS_FOUND=1
        fi
        if [[ "${line}" == *"USERLAND SELF-TEST FAIL"* ]]; then
            die "userland self-test reported FAIL"
        fi
        if [[ "${line}" == *"PAGE FAULT"* || "${line}" == *"Faulting user context:"* || "${line}" == *"panic"* ]]; then
            die "fault or panic observed during QEMU run"
        fi
    done < <(sed -n "${NEXT_LINE},\$p" "${SERIAL_LOG}")

    NEXT_LINE="$(($(wc -l < "${SERIAL_LOG}") + 1))"

    if [[ "${HELLO_FOUND}" -eq 1 && "${EXIT_FOUND}" -eq 1 && "${PASS_FOUND}" -eq 1 ]]; then
        break
    fi

    if ! kill -0 "${QEMU_PID}" 2>/dev/null; then
        break
    fi

    sleep 0.1
done

if [[ "${HELLO_FOUND}" -ne 1 ]]; then
    die "hello output was not observed; see ${SERIAL_LOG}"
fi
if [[ "${EXIT_FOUND}" -ne 1 ]]; then
    die "process exit(0) was not observed; see ${SERIAL_LOG}"
fi
if [[ "${PASS_FOUND}" -ne 1 ]]; then
    die "userland self-test did not report PASS; see ${SERIAL_LOG}"
fi

kill "${QEMU_PID}" 2>/dev/null || true
wait "${QEMU_PID}" 2>/dev/null || true
trap - EXIT

echo "[done] serial log: ${SERIAL_LOG}"
