#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
USER_ROOT="${ROOT_DIR}/user"
OUT_ROOT="${ROOT_DIR}/out/rust-std"
TARGET_JSON="${USER_ROOT}/targets/x86_64-unknown-mochios.json"
TOOLCHAIN="${NIGHTLY_TOOLCHAIN:-nightly}"
BOOTSTRAP_TARGET="x86_64-elf"
SYSROOT_DIR="${ROOT_DIR}/out/newlib-port/toolchain/${BOOTSTRAP_TARGET}"
CRT0_O="${ROOT_DIR}/out/newlib-port/hello/crt0.o"
RUNTIME_LIB="${ROOT_DIR}/out/newlib-port/cargo-target/x86_64-unknown-mochios/release/libmochi_user_newlib_runtime.a"
LINKER_SCRIPT="${USER_ROOT}/runtime/linker.ld"
SYSROOT_OVERLAY="${OUT_ROOT}/sysroot-overlay"
LIBC_OVERRIDE_PATH="${ROOT_DIR}/libraries/libc"
LIBC_BUILD_HASH="$(cksum "${LIBC_OVERRIDE_PATH}/build.rs" | awk '{print $1}')"
TARGET_DIR="${OUT_ROOT}/target-libc-patch-${LIBC_BUILD_HASH}"
STABLE_TARGET_DIR="${OUT_ROOT}/target"
RUSTUP_HOME_LOCAL="${OUT_ROOT}/rustup-home-${LIBC_BUILD_HASH}"
OVERLAY_TOOLCHAIN="mochios-overlay-${LIBC_BUILD_HASH}"

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

prepare_sysroot_overlay() {
    local base_sysroot
    base_sysroot="$(rustc +"${TOOLCHAIN}" --print sysroot)"
    local rustlib_overlay

    mkdir -p "${OUT_ROOT}"
    rm -rf "${SYSROOT_OVERLAY}.tmp"
    mkdir -p "${SYSROOT_OVERLAY}.tmp"

    for item in "${base_sysroot}"/*; do
        local name dest
        name="$(basename "${item}")"
        dest="${SYSROOT_OVERLAY}.tmp/${name}"
        if [[ "${name}" == "bin" || "${name}" == "lib" ]]; then
            continue
        fi
        ln -s "${item}" "${dest}"
    done

    mkdir -p "${SYSROOT_OVERLAY}.tmp/bin"
    for item in "${base_sysroot}/bin"/*; do
        local name dest
        name="$(basename "${item}")"
        dest="${SYSROOT_OVERLAY}.tmp/bin/${name}"
        case "${name}" in
            rustc|rustdoc)
                ;;
            *)
                ln -s "${item}" "${dest}"
                ;;
        esac
    done

    cat > "${SYSROOT_OVERLAY}.tmp/bin/rustc" <<EOF
#!/usr/bin/env bash
set -euo pipefail
exec "${base_sysroot}/bin/rustc" --sysroot "${SYSROOT_OVERLAY}" "\$@"
EOF
    chmod +x "${SYSROOT_OVERLAY}.tmp/bin/rustc"

    cat > "${SYSROOT_OVERLAY}.tmp/bin/rustdoc" <<EOF
#!/usr/bin/env bash
set -euo pipefail
exec "${base_sysroot}/bin/rustdoc" --sysroot "${SYSROOT_OVERLAY}" "\$@"
EOF
    chmod +x "${SYSROOT_OVERLAY}.tmp/bin/rustdoc"

    rustlib_overlay="${SYSROOT_OVERLAY}.tmp/lib/rustlib"
    mkdir -p "${rustlib_overlay}"
    for item in "${base_sysroot}/lib/rustlib"/*; do
        local name dest
        name="$(basename "${item}")"
        dest="${rustlib_overlay}/${name}"
        if [[ "${name}" == "src" ]]; then
            continue
        fi
        ln -s "${item}" "${dest}"
    done

    mkdir -p "${rustlib_overlay}/src"
    cp -rs "${base_sysroot}/lib/rustlib/src/rust" "${rustlib_overlay}/src/"

    rm "${rustlib_overlay}/src/rust/library/std/src/sys/pal/unix/mod.rs"
    cp "${ROOT_DIR}/libraries/rust/library/std/src/sys/pal/unix/mod.rs" \
        "${rustlib_overlay}/src/rust/library/std/src/sys/pal/unix/mod.rs"
    rm "${rustlib_overlay}/src/rust/library/std/build.rs"
    cp "${ROOT_DIR}/libraries/rust/library/std/build.rs" \
        "${rustlib_overlay}/src/rust/library/std/build.rs"
    rm "${rustlib_overlay}/src/rust/library/std/src/sys/args/unix.rs"
    cp "${ROOT_DIR}/libraries/rust/library/std/src/sys/args/unix.rs" \
        "${rustlib_overlay}/src/rust/library/std/src/sys/args/unix.rs"
    rm "${rustlib_overlay}/src/rust/library/std/src/os/unix/mod.rs"
    cp "${ROOT_DIR}/libraries/rust/library/std/src/os/unix/mod.rs" \
        "${rustlib_overlay}/src/rust/library/std/src/os/unix/mod.rs"
    rm "${rustlib_overlay}/src/rust/library/std/src/os/linux/mod.rs"
    cp "${ROOT_DIR}/libraries/rust/library/std/src/os/linux/mod.rs" \
        "${rustlib_overlay}/src/rust/library/std/src/os/linux/mod.rs"
    rm "${rustlib_overlay}/src/rust/library/std/src/os/linux/fs.rs"
    cp "${ROOT_DIR}/libraries/rust/library/std/src/os/linux/fs.rs" \
        "${rustlib_overlay}/src/rust/library/std/src/os/linux/fs.rs"
    rm "${rustlib_overlay}/src/rust/library/std/src/os/unix/fs.rs"
    cp "${ROOT_DIR}/libraries/rust/library/std/src/os/unix/fs.rs" \
        "${rustlib_overlay}/src/rust/library/std/src/os/unix/fs.rs"
    rm "${rustlib_overlay}/src/rust/library/std/src/os/mod.rs"
    cp "${ROOT_DIR}/libraries/rust/library/std/src/os/mod.rs" \
        "${rustlib_overlay}/src/rust/library/std/src/os/mod.rs"
    rm "${rustlib_overlay}/src/rust/library/std/src/sys/paths/unix.rs"
    cp "${ROOT_DIR}/libraries/rust/library/std/src/sys/paths/unix.rs" \
        "${rustlib_overlay}/src/rust/library/std/src/sys/paths/unix.rs"
    rm "${rustlib_overlay}/src/rust/library/std/src/sys/random/mod.rs"
    cp "${ROOT_DIR}/libraries/rust/library/std/src/sys/random/mod.rs" \
        "${rustlib_overlay}/src/rust/library/std/src/sys/random/mod.rs"
    rm "${rustlib_overlay}/src/rust/library/std/src/sys/fs/unix.rs"
    cp "${ROOT_DIR}/libraries/rust/library/std/src/sys/fs/unix.rs" \
        "${rustlib_overlay}/src/rust/library/std/src/sys/fs/unix.rs"
    rm "${rustlib_overlay}/src/rust/library/std/src/sys/sync/mutex/mod.rs"
    cp "${ROOT_DIR}/libraries/rust/library/std/src/sys/sync/mutex/mod.rs" \
        "${rustlib_overlay}/src/rust/library/std/src/sys/sync/mutex/mod.rs"
    rm "${rustlib_overlay}/src/rust/library/std/src/sys/sync/condvar/mod.rs"
    cp "${ROOT_DIR}/libraries/rust/library/std/src/sys/sync/condvar/mod.rs" \
        "${rustlib_overlay}/src/rust/library/std/src/sys/sync/condvar/mod.rs"
    rm "${rustlib_overlay}/src/rust/library/std/src/sys/sync/thread_parking/mod.rs"
    cp "${ROOT_DIR}/libraries/rust/library/std/src/sys/sync/thread_parking/mod.rs" \
        "${rustlib_overlay}/src/rust/library/std/src/sys/sync/thread_parking/mod.rs"
    rm "${rustlib_overlay}/src/rust/library/std/src/sys/sync/once/mod.rs"
    cp "${ROOT_DIR}/libraries/rust/library/std/src/sys/sync/once/mod.rs" \
        "${rustlib_overlay}/src/rust/library/std/src/sys/sync/once/mod.rs"
    rm "${rustlib_overlay}/src/rust/library/std/src/sys/sync/rwlock/mod.rs"
    cp "${ROOT_DIR}/libraries/rust/library/std/src/sys/sync/rwlock/mod.rs" \
        "${rustlib_overlay}/src/rust/library/std/src/sys/sync/rwlock/mod.rs"
    rm "${rustlib_overlay}/src/rust/library/std/src/sys/thread_local/mod.rs"
    cp "${ROOT_DIR}/libraries/rust/library/std/src/sys/thread_local/mod.rs" \
        "${rustlib_overlay}/src/rust/library/std/src/sys/thread_local/mod.rs"
    rm "${rustlib_overlay}/src/rust/library/proc_macro/Cargo.toml"
    cp "${ROOT_DIR}/libraries/rust/library/proc_macro/Cargo.toml" \
        "${rustlib_overlay}/src/rust/library/proc_macro/Cargo.toml"
    rm "${rustlib_overlay}/src/rust/library/Cargo.toml"
    cp "${ROOT_DIR}/libraries/rust/library/Cargo.toml" \
        "${rustlib_overlay}/src/rust/library/Cargo.toml"
    rm "${rustlib_overlay}/src/rust/library/.cargo/config.toml"
    cp "${base_sysroot}/lib/rustlib/src/rust/library/.cargo/config.toml" \
        "${rustlib_overlay}/src/rust/library/.cargo/config.toml"
    mkdir -p "${rustlib_overlay}/src/rust/vendor"
    cp -a "${ROOT_DIR}/libraries/rust/vendor/rustc-literal-escaper" \
        "${rustlib_overlay}/src/rust/vendor/"

    rm -rf "${SYSROOT_OVERLAY}"
    mv "${SYSROOT_OVERLAY}.tmp" "${SYSROOT_OVERLAY}"
}

need_cmd cargo
need_cmd rustc
need_cmd rustup
need_cmd x86_64-elf-gcc
need_file "${TARGET_JSON}"
need_dir "${ROOT_DIR}/libraries/rust/library"
need_file "${LIBC_OVERRIDE_PATH}/Cargo.toml"
need_file "${LINKER_SCRIPT}"

if [[ ! -f "${CRT0_O}" || ! -f "${RUNTIME_LIB}" || ! -d "${SYSROOT_DIR}/lib" ]]; then
    bash "${SCRIPT_DIR}/build-hello.sh"
fi

need_file "${CRT0_O}"
need_file "${RUNTIME_LIB}"
need_dir "${SYSROOT_DIR}/lib"

mkdir -p "${OUT_ROOT}"
prepare_sysroot_overlay
mkdir -p "${RUSTUP_HOME_LOCAL}"
if [[ ! -e "${RUSTUP_HOME_LOCAL}/toolchains/${OVERLAY_TOOLCHAIN}" ]]; then
    env RUSTUP_HOME="${RUSTUP_HOME_LOCAL}" \
        rustup toolchain link "${OVERLAY_TOOLCHAIN}" "${SYSROOT_OVERLAY}"
fi

RUSTFLAGS=(
    "-C" "linker=x86_64-elf-gcc"
    "-C" "link-arg=--sysroot=${SYSROOT_DIR}"
    "-C" "link-arg=-L${SYSROOT_DIR}/lib"
    "-C" "link-arg=-static"
    "-C" "link-arg=-nostdlib"
    "-C" "link-arg=-nostartfiles"
    "-C" "link-arg=-Wl,-T,${LINKER_SCRIPT}"
    "-C" "link-arg=-Wl,-no-pie"
    "-C" "link-arg=-Wl,-z,noexecstack"
    "-C" "link-arg=-Wl,--start-group"
    "-C" "link-arg=${CRT0_O}"
    "-C" "link-arg=${RUNTIME_LIB}"
    "-C" "link-arg=-lc"
    "-C" "link-arg=-lm"
    "-C" "link-arg=-lgcc"
    "-C" "link-arg=-Wl,--end-group"
)

build_std_app() {
    local manifest_path="$1"
    local app_name="$2"
    local app_out="${TARGET_DIR}/x86_64-unknown-mochios/release/${app_name}"
    local stable_app_out="${STABLE_TARGET_DIR}/x86_64-unknown-mochios/release/${app_name}"

    need_file "${manifest_path}"
    echo "[build] ${app_name}"
    env RUSTUP_HOME="${RUSTUP_HOME_LOCAL}" RUSTFLAGS="${RUSTFLAGS[*]}" \
        rustup run "${OVERLAY_TOOLCHAIN}" cargo build \
            -Z build-std=std,panic_abort,compiler_builtins \
            -Z json-target-spec \
            --config "patch.crates-io.libc.path='${LIBC_OVERRIDE_PATH}'" \
            --manifest-path "${manifest_path}" \
            --bin "${app_name}" \
            --release \
            --target "${TARGET_JSON}" \
            --target-dir "${TARGET_DIR}"

    need_file "${app_out}"
    mkdir -p "$(dirname "${stable_app_out}")"
    cp "${app_out}" "${stable_app_out}"
    echo "[done] ${app_out}"
}

build_std_app "${USER_ROOT}/apps/rust-std-demo/Cargo.toml" "rust-std-demo"
build_std_app "${ROOT_DIR}/binaries/msh/Cargo.toml" "msh"
build_std_app "${ROOT_DIR}/binaries/coreutils/Cargo.toml" "echo"
build_std_app "${ROOT_DIR}/binaries/coreutils/Cargo.toml" "ls"
build_std_app "${ROOT_DIR}/binaries/coreutils/Cargo.toml" "pwd"
build_std_app "${ROOT_DIR}/binaries/coreutils/Cargo.toml" "true"
build_std_app "${ROOT_DIR}/binaries/coreutils/Cargo.toml" "false"
build_std_app "${ROOT_DIR}/binaries/coreutils/Cargo.toml" "cat"
build_std_app "${ROOT_DIR}/binaries/coreutils/Cargo.toml" "touch"
build_std_app "${ROOT_DIR}/binaries/coreutils/Cargo.toml" "rm"
build_std_app "${ROOT_DIR}/binaries/coreutils/Cargo.toml" "mpk"
