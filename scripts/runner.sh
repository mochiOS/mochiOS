#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
CORE_ROOT="${ROOT_DIR}/core"
SERVICES_ROOT="${ROOT_DIR}/services"

TARGET_DIR="${CORE_ROOT}/target/uefi"
KERNEL_TARGET="x86_64-unknown-none"

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
need_file "${SERVICES_ROOT}/core/Cargo.toml"
need_file "${SCRIPT_DIR}/build-hello.sh"
need_file "${SCRIPT_DIR}/build-bootloader.sh"
need_file "${SCRIPT_DIR}/build-core-service.sh"
need_file "${SCRIPT_DIR}/build-rootfs.sh"
need_file "${SCRIPT_DIR}/build-signature-db.pl"
need_file "${SCRIPT_DIR}/stage-cext-bundles.sh"
need_file "${OVMF_CODE}"
need_file "${OVMF_VARS_TEMPLATE}"

mkdir -p "${RUN_DIR}" "${ESP_DIR}/EFI/BOOT" "${ESP_DIR}/system" "${INITFS_STAGE}" "${ROOTFS_STAGE}"

echo "[step] build user/newlib runtime"
"${SCRIPT_DIR}/build-hello.sh"

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

echo "[step] build core.service"
"${SCRIPT_DIR}/build-core-service.sh"

echo "[step] build bootloader"
"${SCRIPT_DIR}/build-bootloader.sh"

HELLO_ELF="${ROOT_DIR}/out/newlib-port/hello/hello.elf"
KERNEL_BIN="${CORE_ROOT}/target/${KERNEL_TARGET}/release/kernel"
SERVICE_BIN="${ROOT_DIR}/out/services-core/target/x86_64-unknown-mochios/release/core"
BOOT_BIN="$(find "${ROOT_DIR}/out/bootloader/target/x86_64-unknown-uefi/release" -maxdepth 1 -type f \( -name 'boot' -o -name 'boot.efi' \) | head -n 1)"

need_file "${HELLO_ELF}"
need_file "${KERNEL_BIN}"
need_file "${SERVICE_BIN}"
need_file "${BOOT_BIN}"

echo "[step] stage image files"
rm -rf "${ESP_DIR}" "${INITFS_STAGE}" "${ROOTFS_STAGE}"
mkdir -p "${ESP_DIR}/EFI/BOOT" "${ESP_DIR}/system" "${INITFS_STAGE}"

install -m 0644 "${KERNEL_BIN}" "${ESP_DIR}/system/kernel.elf"
install -m 0644 "${BOOT_BIN}" "${ESP_DIR}/EFI/BOOT/BOOTX64.EFI"
install -m 0755 "${SERVICE_BIN}" "${INITFS_STAGE}/core.service"

echo "[step] stage cext bundles"
mapfile -t CEXT_SIGNATURE_ENTRIES < <(INITFS_STAGE="${INITFS_STAGE}" bash "${SCRIPT_DIR}/stage-cext-bundles.sh")

echo "[step] build signature db"
SIGNATURE_DB_ARGS=(
    --output "${SIGNATURE_DB_STAGE}"
    --entry "core.service=${SERVICE_BIN}"
    --entry "/bin/hello=${HELLO_ELF}"
)
for entry in "${CEXT_SIGNATURE_ENTRIES[@]}"; do
    SIGNATURE_DB_ARGS+=(--entry "${entry}")
done
perl "${SCRIPT_DIR}/build-signature-db.pl" "${SIGNATURE_DB_ARGS[@]}"

echo "[step] build rootfs"
ROOTFS_STAGE="${ROOTFS_STAGE}" \
ROOTFS_IMG="${RUN_DIR}/rootfs.img" \
HELLO_ELF="${HELLO_ELF}" \
SIGNATURE_DB_SRC="${SIGNATURE_DB_STAGE}" \
bash "${SCRIPT_DIR}/build-rootfs.sh"

echo "[step] build initfs"
truncate -s 16M "${RUN_DIR}/initfs.img"
mke2fs -q -t ext2 -b 1024 -d "${INITFS_STAGE}" -F "${RUN_DIR}/initfs.img"

install -m 0644 "${RUN_DIR}/initfs.img" "${ESP_DIR}/system/initfs.img"
install -m 0644 "${RUN_DIR}/rootfs.img" "${ESP_DIR}/system/rootfs.img"

echo "[step] build esp.img"
rm -f "${ESP_IMG}"
truncate -s 128M "${ESP_IMG}"
mkfs.fat -F 32 -n EFI "${ESP_IMG}" >/dev/null
MTOOLS_SKIP_CHECK=1 mmd -i "${ESP_IMG}" ::/EFI
MTOOLS_SKIP_CHECK=1 mmd -i "${ESP_IMG}" ::/EFI/BOOT
MTOOLS_SKIP_CHECK=1 mmd -i "${ESP_IMG}" ::/system
MTOOLS_SKIP_CHECK=1 mcopy -i "${ESP_IMG}" "${ESP_DIR}/system/kernel.elf" ::/system/kernel.elf
MTOOLS_SKIP_CHECK=1 mcopy -i "${ESP_IMG}" "${ESP_DIR}/system/initfs.img" ::/system/initfs.img
MTOOLS_SKIP_CHECK=1 mcopy -i "${ESP_IMG}" "${ESP_DIR}/system/rootfs.img" ::/system/rootfs.img
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
WAITPID_FOUND=0
ROOTFS_EXEC_FOUND=0
NEXT_LINE=1

for _ in $(seq 1 600); do
    while IFS= read -r line; do
        if [[ "${line}" == *"hello from mochiOS, argc=2"* ]]; then
            HELLO_FOUND=1
        fi
        if [[ "${line}" == *"waitpid status=0 exited=1 code=0"* ]]; then
            WAITPID_FOUND=1
        fi
        if [[ "${line}" == *"execve: loaded '/bin/hello' from cext"* || "${line}" == *"exec: loaded '/bin/hello' from cext"* ]]; then
            ROOTFS_EXEC_FOUND=1
        fi
        if [[ "${line}" == *"Process exiting with code: 0"* ]]; then
            EXIT_FOUND=1
        fi
        if [[ "${line}" == *"PAGE FAULT"* || "${line}" == *"Faulting user context:"* || "${line}" == *"panic"* ]]; then
            die "fault or panic observed during QEMU run"
        fi
    done < <(sed -n "${NEXT_LINE},\$p" "${SERIAL_LOG}")

    NEXT_LINE="$(($(wc -l < "${SERIAL_LOG}") + 1))"

    if [[ "${HELLO_FOUND}" -eq 1 && "${WAITPID_FOUND}" -eq 1 && "${EXIT_FOUND}" -eq 1 && "${ROOTFS_EXEC_FOUND}" -eq 1 ]]; then
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
if [[ "${ROOTFS_EXEC_FOUND}" -ne 1 ]]; then
    die "/bin/hello was not loaded from cext/rootfs; see ${SERIAL_LOG}"
fi
if [[ "${WAITPID_FOUND}" -ne 1 ]]; then
    die "waitpid success output was not observed; see ${SERIAL_LOG}"
fi
if [[ "${EXIT_FOUND}" -ne 1 ]]; then
    die "process exit(0) was not observed; see ${SERIAL_LOG}"
fi

kill "${QEMU_PID}" 2>/dev/null || true
wait "${QEMU_PID}" 2>/dev/null || true
trap - EXIT

echo "[done] serial log: ${SERIAL_LOG}"
