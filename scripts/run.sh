#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
KERNEL_ROOT="${REPO_ROOT}/src/core"
BOOT_ROOT="${REPO_ROOT}/src/core/examples/boot"
SERVICES_ROOT="${REPO_ROOT}/src/services"
ROOTFS_SOURCE_DIR="${KERNEL_ROOT}/examples/fs/rootfs"

TARGET_DIR="${REPO_ROOT}/target/mochios"
KERNEL_BUILD_DIR="${TARGET_DIR}/kernel-build"
BOOT_BUILD_DIR="${TARGET_DIR}/boot-build"
SERVICE_BUILD_DIR="${TARGET_DIR}/service-build"

KERNEL_TARGET_NAME="x86_64-unknown-none"
BOOT_TARGET_NAME="x86_64-unknown-uefi"
OVMF_CODE="${OVMF_CODE:-/usr/share/OVMF/OVMF_CODE_4M.fd}"
OVMF_VARS_TEMPLATE="${OVMF_VARS_TEMPLATE:-/usr/share/OVMF/OVMF_VARS_4M.fd}"
QEMU_DISPLAY="${QEMU_DISPLAY:-gtk}"

RUN_ID="$(date +%s)-$$"
RUN_DIR="${TARGET_DIR}/run-${RUN_ID}"
ESP_DIR="${RUN_DIR}/esp"
ESP_IMG="${RUN_DIR}/esp.img"
INITFS_STAGE="${RUN_DIR}/initfs-root"
ROOTFS_STAGE="${RUN_DIR}/rootfs-root"
OVMF_VARS="${OVMF_VARS:-${RUN_DIR}/OVMF_VARS_4M.fd}"
SERIAL_LOG="${TARGET_DIR}/serial.log"

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

need_path() {
    [[ -e "$1" ]] || die "required path not found: $1"
}

need_cmd cargo
need_cmd perl
need_cmd openssl
need_cmd qemu-system-x86_64
need_cmd mke2fs
need_cmd mkfs.fat
need_cmd mmd
need_cmd mcopy
need_cmd readelf
need_cmd strings
need_cmd stat
need_cmd tee
need_cmd sed
need_cmd wc
need_cmd grep
need_cmd head
need_cmd seq
need_cmd date
need_cmd cp
need_cmd install
need_cmd find
need_cmd truncate

need_file "${OVMF_CODE}"
need_file "${OVMF_VARS_TEMPLATE}"
need_path "${ROOTFS_SOURCE_DIR}"
need_file "${KERNEL_ROOT}/Cargo.toml"
need_file "${BOOT_ROOT}/Cargo.toml"
need_file "${SERVICES_ROOT}/Cargo.toml"
need_file "${SERVICES_ROOT}/core/Cargo.toml"
need_file "${SERVICES_ROOT}/logger/Cargo.toml"
need_file "${KERNEL_ROOT}/scripts/signature_db.pl"
need_file "${KERNEL_ROOT}/Cargo.lock"
need_file "${BOOT_ROOT}/Cargo.lock"
need_file "${SERVICES_ROOT}/Cargo.lock"

mkdir -p "${TARGET_DIR}" "${ESP_DIR}/EFI/BOOT" "${INITFS_STAGE}" "${ROOTFS_STAGE}"

echo "[build] kernel"
(cd "${KERNEL_ROOT}" && \
    cargo build \
        --locked \
        --release \
        --target "${KERNEL_TARGET_NAME}" \
        -p mnu \
        --target-dir "${KERNEL_BUILD_DIR}")

KERNEL_BIN="$(find "${KERNEL_BUILD_DIR}/${KERNEL_TARGET_NAME}/release" -maxdepth 1 -type f -name kernel | head -n 1)"
need_file "${KERNEL_BIN}"

echo "[build] bootloader"
(cd "${BOOT_ROOT}" && \
    cargo +nightly build \
        --locked \
        --release \
        --target "${BOOT_TARGET_NAME}" \
        --target-dir "${BOOT_BUILD_DIR}")

BOOT_BIN="$(find "${BOOT_BUILD_DIR}/${BOOT_TARGET_NAME}/release" -maxdepth 1 -type f \( -name 'boot' -o -name 'boot.efi' \) | head -n 1)"
need_file "${BOOT_BIN}"

echo "[build] core.service"
(cd "${SERVICES_ROOT}" && \
    RUSTUP_TOOLCHAIN=nightly cargo build \
        --locked \
        --release \
        -p core \
        --target-dir "${SERVICE_BUILD_DIR}")

SERVICE_BIN="$(find "${SERVICE_BUILD_DIR}/x86_64-unknown-none/release" -maxdepth 1 -type f -name core | head -n 1)"
need_file "${SERVICE_BIN}"

echo "[build] logger.service"
(cd "${SERVICES_ROOT}" && \
    RUSTUP_TOOLCHAIN=nightly cargo build \
        --locked \
        --release \
        -p logger \
        --target-dir "${SERVICE_BUILD_DIR}")

LOGGER_BIN="$(find "${SERVICE_BUILD_DIR}/x86_64-unknown-none/release" -maxdepth 1 -type f -name logger | head -n 1)"
need_file "${LOGGER_BIN}"

echo "[check] core.service binary"
stat "${SERVICE_BIN}"
readelf -h "${SERVICE_BIN}" | grep -E 'Type:|Entry point address:' || true

rm -rf "${INITFS_STAGE}" "${ROOTFS_STAGE}"
mkdir -p "${ESP_DIR}/EFI/BOOT" "${INITFS_STAGE}" "${ROOTFS_STAGE}"

cp -a "${ROOTFS_SOURCE_DIR}/." "${ROOTFS_STAGE}/"
cp -a "${ROOTFS_SOURCE_DIR}/." "${INITFS_STAGE}/"

mkdir -p "${ROOTFS_STAGE}/system/services"

install -m 0755 "${SERVICE_BIN}" "${INITFS_STAGE}/core.service"
install -m 0755 "${LOGGER_BIN}" "${ROOTFS_STAGE}/system/services/logger.service"

SIGNATURE_DB_STAGE="${TARGET_DIR}/signature.db"
echo "[build] signature db"
perl "${KERNEL_ROOT}/scripts/signature_db.pl" \
    --output "${SIGNATURE_DB_STAGE}" \
    --entry "core.service=${SERVICE_BIN}" \
    --entry "/system/services/logger.service=${LOGGER_BIN}"
install -m 0644 "${SIGNATURE_DB_STAGE}" "${ROOTFS_STAGE}/signature.db"

echo "[build] rootfs"
ROOTFS_IMG="${TARGET_DIR}/rootfs.img"
truncate -s 16M "${ROOTFS_IMG}"
mke2fs -q -t ext2 -b 1024 -d "${ROOTFS_STAGE}" -F "${ROOTFS_IMG}"

echo "[build] initfs"
INITFS_IMG="${TARGET_DIR}/initfs.img"
truncate -s 16M "${INITFS_IMG}"
mke2fs -q -t ext2 -b 1024 -d "${INITFS_STAGE}" -F "${INITFS_IMG}"

install -m 0644 "${INITFS_IMG}" "${ESP_DIR}/initfs.img"
install -m 0644 "${ROOTFS_IMG}" "${ESP_DIR}/rootfs.img"
install -m 0644 "${KERNEL_BIN}" "${ESP_DIR}/kernel"
install -m 0644 "${BOOT_BIN}" "${ESP_DIR}/EFI/BOOT/BOOTX64.EFI"

rm -f "${ESP_IMG}"
truncate -s 64M "${ESP_IMG}"
mkfs.fat -F 32 -n EFI "${ESP_IMG}"

MTOOLS_SKIP_CHECK=1 mmd -i "${ESP_IMG}" ::/EFI
MTOOLS_SKIP_CHECK=1 mmd -i "${ESP_IMG}" ::/EFI/BOOT

MTOOLS_SKIP_CHECK=1 mcopy -i "${ESP_IMG}" "${ESP_DIR}/kernel" ::/kernel
MTOOLS_SKIP_CHECK=1 mcopy -i "${ESP_IMG}" "${ESP_DIR}/initfs.img" ::/initfs
MTOOLS_SKIP_CHECK=1 mcopy -i "${ESP_IMG}" "${ESP_DIR}/rootfs.img" ::/rootfs
MTOOLS_SKIP_CHECK=1 mcopy -i "${ESP_IMG}" "${ESP_DIR}/EFI/BOOT/BOOTX64.EFI" ::/EFI/BOOT/BOOTX64.EFI

if [[ ! -f "${OVMF_VARS}" ]]; then
    cp "${OVMF_VARS_TEMPLATE}" "${OVMF_VARS}"
fi

rm -f "${SERIAL_LOG}"
: > "${SERIAL_LOG}"

QEMU_ARGS=(
    -machine q35
    -m 512M
    -smp 4
    -cpu qemu64
    -serial stdio
    -display "${QEMU_DISPLAY}"
    -vga std
    -monitor none
    -no-reboot
    -drive "if=pflash,format=raw,readonly=on,file=${OVMF_CODE}"
    -drive "if=pflash,format=raw,file=${OVMF_VARS}"
    -drive "format=raw,file=${ESP_IMG}"
)

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

PASS_FOUND=0
NEXT_LINE=1

for _ in $(seq 1 600); do
    while IFS= read -r line; do
        if [[ "$line" == *"logger.service: resident logger online"* ]]; then
            PASS_FOUND=1
            break
        fi

        if [[ "$line" == *"panic"* || "$line" == *"PAGE FAULT"* || "$line" == *"Faulting user context:"* ]]; then
            echo "fatal: boot log reported an error" >&2
            exit 1
        fi
    done < <(sed -n "${NEXT_LINE},\$p" "${SERIAL_LOG}")

    NEXT_LINE="$(($(wc -l < "${SERIAL_LOG}") + 1))"

    if [[ "${PASS_FOUND}" -eq 1 ]]; then
        break
    fi

    if ! kill -0 "${QEMU_PID}" 2>/dev/null; then
        break
    fi

    sleep 0.1
done

if [[ "${PASS_FOUND}" -ne 1 ]]; then
    echo "fatal: core.service did not become resident" >&2
    echo "serial log: ${SERIAL_LOG}" >&2
    exit 1
fi

kill "${QEMU_PID}" 2>/dev/null || true
wait "${QEMU_PID}" 2>/dev/null || true
trap - EXIT

echo "[run] core.service became resident"
