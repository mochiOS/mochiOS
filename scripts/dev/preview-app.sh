#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$script_dir/../.." && pwd)
app=${1:-}
mode=${2:-run}
shift 2 || true
cargo_patches=()
while (($#)); do
    # Host previews use the installed Rust standard library. OS build-std
    # patches point at a generated sysroot that need not exist on the host.
    if [[ $1 == --config && $# -ge 2 && $2 == patch.crates-io.rustc-std-workspace-*.path=* ]]; then
        shift 2
        continue
    fi
    cargo_patches+=("$1")
    shift
done

if [[ $app == check || $app == test ]]; then
    if [[ $app == test ]]; then
        python3 "$script_dir/prepare-preview.py" "$root"
        env CARGO_TARGET_DIR="$root/out/preview/target" CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
            cargo test --offline --manifest-path "$root/out/preview/Cargo.toml" \
            "${cargo_patches[@]}" -p viewkit --lib
    fi
    for name in files settings binder appstore edit terminal system-monitor viewer calendar calculator mail keychain; do
        "$0" "$name" "--$app" "${cargo_patches[@]}"
    done
    exit 0
fi

case "$app" in
    files) source_dir="$root/applications/file" ;;
    settings) source_dir="$root/applications/settings" ;;
    binder) source_dir="$root/applications/binder" ;;
    appstore) source_dir="$root/applications/appstore" ;;
    edit) source_dir="$root/applications/edit" ;;
    terminal) source_dir="$root/applications/terminal" ;;
    system-monitor) source_dir="$root/applications/system-monitor" ;;
    ime) source_dir="$root/tools/desktop-ime" ;;
    test-app) source_dir="$root/applications/test.app" ;;
    keychain) source_dir="$root/applications/keychain" ;;
    keychain-service) source_dir="$root/services/keychain" ;;
    mail) source_dir="$root/applications/mail" ;;
    calculator) source_dir="$root/applications/calculator" ;;
    calendar) source_dir="$root/applications/calendar" ;;
    viewer) source_dir="$root/applications/viewer" ;;
    *) echo "usage: $0 <files|settings|binder|appstore|edit|terminal|system-monitor|viewer|calendar|calculator|mail|keychain|keychain-service|check|test>" >&2; exit 2 ;;
esac

# Source manifests/lockfiles remain untouched; host dependencies share one
# isolated workspace and a single build cache.
python3 "$script_dir/prepare-preview.py" "$root"
preview_dir="$root/out/preview/applications/${source_dir##*/}"
[[ $app != ime ]] || preview_dir="$root/out/preview/tools/desktop-ime"
[[ $app != keychain-service ]] || preview_dir="$root/out/preview/tools/keychain-service"
manifest="$preview_dir/Cargo.toml"

command -v cargo >/dev/null || { echo "cargo is required" >&2; exit 1; }
case "$mode" in
    run)
        [[ -n ${DISPLAY:-}${WAYLAND_DISPLAY:-} ]] || {
            echo "a Linux graphical session is required (DISPLAY or WAYLAND_DISPLAY)" >&2
            exit 1
        }
        cargo_command=run
        ;;
    --check) cargo_command=build ;;
    --test) cargo_command=test ;;
    *) echo "unsupported preview mode: $mode" >&2; exit 2 ;;
esac

if [[ $mode == run ]]; then
    preview_root="$root/out/preview/home"
    mkdir -p "$preview_root"/{Desktop,Documents,Downloads,Movies,Music,Pictures}
    export MOCHIOS_PREVIEW_ROOT="$preview_root"
    export MOCHIOS_PREVIEW_CONFIG_ROOT="${MOCHIOS_PREVIEW_CONFIG_ROOT:-$root/out/preview/config}"
    mkdir -p "$MOCHIOS_PREVIEW_CONFIG_ROOT"
fi
preview_binary=$app
[[ $app != test-app ]] || preview_binary=test_app
[[ $app != ime ]] || preview_binary=mochios-preview-ime
export CARGO_TARGET_DIR="$root/out/preview/target"
# Host UI builds do not need multi-gigabyte DWARF/incremental caches. Callers
# can opt back into either when debugging the renderer.
export CARGO_PROFILE_DEV_DEBUG="${CARGO_PROFILE_DEV_DEBUG:-0}"
export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-0}"
export MOCHIOS_VERSION="$(sed -n 's/^release = "\([^"]*\)"/\1/p' "$root/version.toml" | head -n 1)"
export MOCHIOS_BUILD_NUMBER="$(sed -n 's/^build = \([0-9]*\)/\1/p' "$root/version.toml" | head -n 1)"
export MNU_VERSION="${MNU_VERSION:-preview}"
export MBOOT_VERSION="${MBOOT_VERSION:-preview}"

if [[ ( $app == keychain || $app == mail ) && $mode == run && -z ${MOCHIOS_PREVIEW_KEYCHAIN_SOCKET:-} ]]; then
    # Build both executables before pinning their inodes in the host service.
    "$0" "$app" --check "${cargo_patches[@]}"
    "$0" keychain-service --check "${cargo_patches[@]}"
    keychain_socket_dir=$(mktemp -d /tmp/mochios-keychain.XXXXXXXX)
    export MOCHIOS_PREVIEW_KEYCHAIN_SOCKET="$keychain_socket_dir/keychain.sock"
    python3 "$script_dir/keychain-preview-server.py" "$root" "$MOCHIOS_PREVIEW_KEYCHAIN_SOCKET" "$keychain_socket_dir/vault" &
    keychain_pid=$!
    cleanup_keychain() {
        kill "$keychain_pid" 2>/dev/null || true
        wait "$keychain_pid" 2>/dev/null || true
        rm -rf -- "$keychain_socket_dir"
    }
    trap cleanup_keychain EXIT
    trap 'exit 130' INT
    trap 'exit 143' TERM
    for ((attempt=0; attempt<100; attempt++)); do
        [[ ! -S $MOCHIOS_PREVIEW_KEYCHAIN_SOCKET ]] || break
        kill -0 "$keychain_pid" 2>/dev/null || { wait "$keychain_pid"; exit 1; }
        sleep 0.1
    done
    [[ -S $MOCHIOS_PREVIEW_KEYCHAIN_SOCKET ]] || { echo "Keychain preview service did not start." >&2; exit 1; }
    "$CARGO_TARGET_DIR/debug/$preview_binary"
    exit $?
fi

exec cargo "$cargo_command" --offline --manifest-path "$manifest" \
    "${cargo_patches[@]}" --bin "$preview_binary"
