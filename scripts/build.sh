#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
CORE_ROOT="${ROOT_DIR}/core"

KERNEL_TARGET="x86_64-unknown-none"
NIGHTLY_TOOLCHAIN="${NIGHTLY_TOOLCHAIN:-nightly-2026-05-14}"
BUILD_ROOT="${ROOT_DIR}/out/image-build"
ARTIFACT_DIR="${ROOT_DIR}/out/artifacts"
SERVICES_BUILD_ROOT="${ROOT_DIR}/out/services-build"
ESP_DIR="${BUILD_ROOT}/esp"
ESP_IMG="${BUILD_ROOT}/esp.img"
INITFS_STAGE="${BUILD_ROOT}/initfs-root"
INITFS_IMG="${BUILD_ROOT}/initfs.img"
ROOTFS_STAGE="${BUILD_ROOT}/rootfs-root"
ROOTFS_IMG="${BUILD_ROOT}/rootfs.img"
SIGNATURE_DB_STAGE="${BUILD_ROOT}/signature.db"
CEXT_BUNDLES_DIR="${ROOT_DIR}/out/cexts/bundles"
DRIVERS_BUNDLE_ROOT="/bin/drivers/usb/qemu-usb.driver"

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

need_dir() {
    [[ -d "$1" ]] || die "required directory not found: $1"
}

need_cmd cargo
need_cmd install
need_cmd mcopy
need_cmd mke2fs
need_cmd mkfs.fat
need_cmd mmd
need_cmd openssl
need_cmd perl
need_cmd repo
need_cmd sha256sum
need_cmd truncate

need_dir "${ROOT_DIR}/.repo"
need_file "${CORE_ROOT}/Cargo.toml"
need_file "${SCRIPT_DIR}/build-hello.sh"
need_file "${SCRIPT_DIR}/build-bootloader.sh"
need_file "${SCRIPT_DIR}/build-core-service.sh"
need_file "${SCRIPT_DIR}/build-drivers.sh"
need_file "${SCRIPT_DIR}/build-rootfs.sh"
need_file "${SCRIPT_DIR}/build-signature-db.pl"
need_file "${SCRIPT_DIR}/build-cexts.sh"
need_file "${SCRIPT_DIR}/stage-cext-bundles.sh"

echo "[clean] build directories"
rm -rf "${BUILD_ROOT}" "${ARTIFACT_DIR}"
mkdir -p "${ESP_DIR}/EFI/BOOT" "${ESP_DIR}/system" "${INITFS_STAGE}" "${ROOTFS_STAGE}" "${ARTIFACT_DIR}"

echo "[step] build user runtime and newlib"
"${SCRIPT_DIR}/build-hello.sh"

echo "[step] build cext bundles"
"${SCRIPT_DIR}/build-cexts.sh"

echo "[step] build kernel"
(
    cd "${CORE_ROOT}"
    env RUSTFLAGS="--cfg curve25519_dalek_backend=\"serial\"" \
    cargo +"${NIGHTLY_TOOLCHAIN}" build \
        -Z build-std=core,alloc,compiler_builtins \
        --locked \
        --release \
        --target "${KERNEL_TARGET}" \
        --features kernel-bin \
        --manifest-path "${CORE_ROOT}/Cargo.toml"
)

echo "[step] build core.service"
"${SCRIPT_DIR}/build-core-service.sh"

echo "[step] build drivers.service and usb driver bundle"
bash "${SCRIPT_DIR}/build-drivers.sh"

echo "[step] build bootloader"
"${SCRIPT_DIR}/build-bootloader.sh"

HELLO_ELF="${ROOT_DIR}/out/newlib-port/hello/hello.elf"
KERNEL_BIN="${CORE_ROOT}/target/${KERNEL_TARGET}/release/kernel"
SERVICE_BIN="${ROOT_DIR}/out/services-core/target/x86_64-unknown-mochios/release/core"
DRIVERS_SERVICE_BIN="${ROOT_DIR}/out/services-build/target/x86_64-unknown-mochios/release/drivers"
CAPABILITY_SERVICE_BIN="${ROOT_DIR}/out/services-build/target/x86_64-unknown-mochios/release/capability"
USB_DRIVER_BIN="${ROOT_DIR}/out/services-build/target/x86_64-unknown-mochios/release/entry"
USB_DRIVER_MANIFEST_SRC="${ROOT_DIR}/drivers/usb-driver/about.toml"
BOOT_RELEASE_DIR="${ROOT_DIR}/out/bootloader/target/x86_64-unknown-uefi/release"

if [[ -f "${BOOT_RELEASE_DIR}/boot.efi" ]]; then
    BOOT_BIN="${BOOT_RELEASE_DIR}/boot.efi"
elif [[ -f "${BOOT_RELEASE_DIR}/boot" ]]; then
    BOOT_BIN="${BOOT_RELEASE_DIR}/boot"
else
    die "bootloader binary was not found in ${BOOT_RELEASE_DIR}"
fi

need_file "${HELLO_ELF}"
need_file "${KERNEL_BIN}"
need_file "${SERVICE_BIN}"
need_file "${DRIVERS_SERVICE_BIN}"
need_file "${CAPABILITY_SERVICE_BIN}"
need_file "${USB_DRIVER_BIN}"
need_file "${USB_DRIVER_MANIFEST_SRC}"
need_file "${BOOT_BIN}"
need_dir "${CEXT_BUNDLES_DIR}"

echo "[step] stage initfs"
rm -rf "${INITFS_STAGE}"
mkdir -p "${INITFS_STAGE}"
install -m 0755 "${SERVICE_BIN}" "${INITFS_STAGE}/core.service"
mapfile -t CEXT_SIGNATURE_ENTRIES < <(
    CEXTS_DIR="${CEXT_BUNDLES_DIR}" \
    INITFS_STAGE="${INITFS_STAGE}" \
    bash "${SCRIPT_DIR}/stage-cext-bundles.sh"
)
for unexpected in bin captest.bin unsigned.bin plugkit testdata hello.txt config; do
    [[ ! -e "${INITFS_STAGE}/${unexpected}" ]] || die "unexpected initfs payload: ${unexpected}"
done

echo "[step] build signature database"
SIGNATURE_DB_ARGS=(
    --output "${SIGNATURE_DB_STAGE}"
    --entry "core.service=${SERVICE_BIN}"
    --entry "/system/services/capability.service=${CAPABILITY_SERVICE_BIN}"
    --entry "/system/services/drivers.service=${DRIVERS_SERVICE_BIN}"
    --entry "${DRIVERS_BUNDLE_ROOT}/entry.elf=${USB_DRIVER_BIN}"
    --entry "/bin/hello=${HELLO_ELF}"
)
for entry in "${CEXT_SIGNATURE_ENTRIES[@]}"; do
    SIGNATURE_DB_ARGS+=(--entry "${entry}")
done
perl "${SCRIPT_DIR}/build-signature-db.pl" "${SIGNATURE_DB_ARGS[@]}"

echo "[step] build rootfs"
ROOTFS_STAGE="${ROOTFS_STAGE}" \
ROOTFS_IMG="${ROOTFS_IMG}" \
HELLO_ELF="${HELLO_ELF}" \
SIGNATURE_DB_SRC="${SIGNATURE_DB_STAGE}" \
CORE_SERVICE_BIN="${SERVICE_BIN}" \
CAPABILITY_SERVICE_BIN="${CAPABILITY_SERVICE_BIN}" \
DRIVERS_SERVICE_BIN="${DRIVERS_SERVICE_BIN}" \
USB_DRIVER_MANIFEST_SRC="${USB_DRIVER_MANIFEST_SRC}" \
USB_DRIVER_ENTRY_BIN="${USB_DRIVER_BIN}" \
USB_DRIVER_BUNDLE_ROOT="${DRIVERS_BUNDLE_ROOT}" \
bash "${SCRIPT_DIR}/build-rootfs.sh"

echo "[step] build initfs image"
truncate -s 16M "${INITFS_IMG}"
mke2fs -q -t ext2 -b 1024 -d "${INITFS_STAGE}" -F "${INITFS_IMG}"

echo "[step] build esp image"
rm -rf "${ESP_DIR}"
mkdir -p "${ESP_DIR}/EFI/BOOT" "${ESP_DIR}/system"
install -m 0644 "${BOOT_BIN}" "${ESP_DIR}/EFI/BOOT/BOOTX64.EFI"
install -m 0644 "${KERNEL_BIN}" "${ESP_DIR}/system/kernel.elf"
install -m 0644 "${INITFS_IMG}" "${ESP_DIR}/system/initfs.img"

truncate -s 128M "${ESP_IMG}"
mkfs.fat -F 32 -n EFI "${ESP_IMG}" >/dev/null
MTOOLS_SKIP_CHECK=1 mmd -i "${ESP_IMG}" ::/EFI
MTOOLS_SKIP_CHECK=1 mmd -i "${ESP_IMG}" ::/EFI/BOOT
MTOOLS_SKIP_CHECK=1 mmd -i "${ESP_IMG}" ::/system
MTOOLS_SKIP_CHECK=1 mcopy -i "${ESP_IMG}" "${ESP_DIR}/EFI/BOOT/BOOTX64.EFI" ::/EFI/BOOT/BOOTX64.EFI
MTOOLS_SKIP_CHECK=1 mcopy -i "${ESP_IMG}" "${ESP_DIR}/system/kernel.elf" ::/system/kernel.elf
MTOOLS_SKIP_CHECK=1 mcopy -i "${ESP_IMG}" "${ESP_DIR}/system/initfs.img" ::/system/initfs.img

echo "[step] collect artifacts"
install -m 0644 "${ESP_IMG}" "${ARTIFACT_DIR}/esp.img"
install -m 0644 "${INITFS_IMG}" "${ARTIFACT_DIR}/initfs.img"
install -m 0644 "${ROOTFS_IMG}" "${ARTIFACT_DIR}/rootfs.img"
install -m 0644 "${KERNEL_BIN}" "${ARTIFACT_DIR}/kernel.elf"
install -m 0644 "${BOOT_BIN}" "${ARTIFACT_DIR}/BOOTX64.EFI"
install -m 0755 "${SERVICE_BIN}" "${ARTIFACT_DIR}/core.service"
install -m 0755 "${CAPABILITY_SERVICE_BIN}" "${ARTIFACT_DIR}/capability.service"
install -m 0644 "${SIGNATURE_DB_STAGE}" "${ARTIFACT_DIR}/signature.db"
install -m 0755 "${DRIVERS_SERVICE_BIN}" "${ARTIFACT_DIR}/drivers.service"
install -m 0755 "${USB_DRIVER_BIN}" "${ARTIFACT_DIR}/usb-driver.entry"

echo "[step] record exact repo manifest"
(
    cd "${ROOT_DIR}"
    repo manifest -r -o "${ARTIFACT_DIR}/manifest.xml"
)

ROOT_COMMIT="$(
    git -C "${ROOT_DIR}" rev-parse HEAD 2>/dev/null ||
        printf '%s' "unknown"
)"

cat > "${ARTIFACT_DIR}/build-info.txt" <<EOF
build_number=${BUILD_NUMBER:-unassigned}
manifest_commit=${ROOT_COMMIT}
github_sha=${GITHUB_SHA:-unknown}
github_run_id=${GITHUB_RUN_ID:-unknown}
built_at=$(date -u '+%Y-%m-%dT%H:%M:%SZ')
EOF

echo "[step] generate checksums"
(
    cd "${ARTIFACT_DIR}"
    sha256sum \
        esp.img \
        initfs.img \
        rootfs.img \
        kernel.elf \
        BOOTX64.EFI \
        core.service \
        drivers.service \
        usb-driver.entry \
        signature.db \
        manifest.xml \
        build-info.txt \
        > SHA256SUMS
)

echo "[done] artifacts:"
find "${ARTIFACT_DIR}" -maxdepth 1 -type f -printf '  %f\n' | sort
