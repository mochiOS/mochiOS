#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$script_dir/../.." && pwd)
app=${1:-}
mode=${2:-run}
shift 2 || true
cargo_patches=("$@")

if [[ $app == check || $app == test ]]; then
    if [[ $app == test ]]; then
        cargo test --offline --manifest-path "$root/libraries/viewkit/Cargo.toml" \
            "${cargo_patches[@]}" --lib
    fi
    for name in files settings binder appstore edit; do
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
    *) echo "usage: $0 <files|settings|binder|appstore|edit|check|test>" >&2; exit 2 ;;
esac

# Cargo resolves host-only dependencies into Cargo.lock. Keep that lockfile in
# the ignored preview tree so a preview never rewrites the OS build's lockfile.
preview_dir="$root/out/preview/applications/${source_dir##*/}"
mkdir -p "$preview_dir"
cp "$source_dir/Cargo.toml" "$source_dir/Cargo.lock" "$preview_dir/"
for entry in src resources appicon.svg about.toml manifest.toml; do
    if [[ -e "$source_dir/$entry" && ! -e "$preview_dir/$entry" ]]; then
        ln -s "$source_dir/$entry" "$preview_dir/$entry"
    fi
done
if [[ $app == binder ]]; then
    mkdir -p "$preview_dir/crates"
    cp -a "$source_dir/crates/." "$preview_dir/crates/"
    # Binder embeds the shared cursor SVG via ../../resources.
    if [[ ! -e "$root/out/preview/resources" ]]; then
        ln -s "$root/resources" "$root/out/preview/resources"
    fi
fi
if [[ $app == appstore ]]; then
    mkdir -p "$root/out/preview/libraries"
    if [[ ! -e "$root/out/preview/libraries/viewkit" ]]; then
        ln -s "$root/libraries/viewkit" "$root/out/preview/libraries/viewkit"
    fi
fi
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
fi
export CARGO_TARGET_DIR="$root/out/preview/target"
export MOCHIOS_VERSION="$(sed -n 's/^release = "\([^"]*\)"/\1/p' "$root/version.toml" | head -n 1)"
export MOCHIOS_BUILD_NUMBER="$(sed -n 's/^build = \([0-9]*\)/\1/p' "$root/version.toml" | head -n 1)"
export MNU_VERSION="${MNU_VERSION:-preview}"
export MBOOT_VERSION="${MBOOT_VERSION:-preview}"

case "$app" in
    files)
        exec cargo "$cargo_command" --offline --manifest-path "$manifest" \
            "${cargo_patches[@]}" \
            --bin files
        ;;
    settings)
        exec cargo "$cargo_command" --offline --manifest-path "$manifest" \
            "${cargo_patches[@]}" \
            --bin settings
        ;;
    binder)
        exec cargo "$cargo_command" --offline --manifest-path "$manifest" \
            "${cargo_patches[@]}" \
            --bin binder
        ;;
    appstore)
        exec cargo "$cargo_command" --offline --manifest-path "$manifest" \
            "${cargo_patches[@]}" \
            --bin appstore
        ;;
esac
