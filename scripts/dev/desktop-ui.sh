#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$script_dir/../.." && pwd)
mode=${1:-run}
shift || true
[[ $(uname -s) == Linux ]] || { echo "mmake ui requires Linux" >&2; exit 1; }
case "$mode" in
    run)
        [[ -n ${DISPLAY:-}${WAYLAND_DISPLAY:-} ]] || {
            echo "mmake ui requires a Linux graphical session (DISPLAY or WAYLAND_DISPLAY)" >&2
            exit 1
        }
        ;;
    --check) ;;
    *) echo "usage: $0 [run|--check] [cargo patch options...]" >&2; exit 2 ;;
esac

apps=(files settings edit terminal appstore system-monitor viewer)
for app in binder "${apps[@]}"; do
    "$script_dir/preview-app.sh" "$app" --check "$@"
done
[[ $mode != --check ]] || exit 0

# All application and desktop state belongs to this disposable session.
session_dir=$(mktemp -d "$root/out/preview/session.XXXXXXXX")
desktop_pid=
cleanup() {
    trap - EXIT INT TERM
    if [[ -n $desktop_pid ]]; then
        kill -- "-$desktop_pid" 2>/dev/null || true
        wait "$desktop_pid" 2>/dev/null || true
    fi
    rm -rf -- "$session_dir"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

export MOCHIOS_PREVIEW_ROOT="$session_dir/home"
export MOCHIOS_PREVIEW_CONFIG_ROOT="$session_dir/config"
export MOCHIOS_PREVIEW_APPS="$session_dir/applications"
export MOCHIOS_PREVIEW_WALLPAPER="$root/resources/libraries/wallpapers/default.png"
preview_home="$MOCHIOS_PREVIEW_ROOT"
mkdir -p "$preview_home"/{Desktop,Documents,Downloads,Movies,Music,Pictures} "$MOCHIOS_PREVIEW_APPS"
printf 'mochiOS Kinako\n\nA temporary desktop for native UI development.\n' > "$preview_home/Documents/Welcome.txt"
mkdir -p "$preview_home/Documents/Projects" "$preview_home/Downloads/Examples"

for app in "${apps[@]}"; do
    source_name=$app
    [[ $app != files ]] || source_name=file
    source_dir="$root/applications/$source_name"
    bundle="$MOCHIOS_PREVIEW_APPS/$app.app"
    mkdir -p "$bundle"
    cp "$source_dir/manifest.toml" "$bundle/manifest.toml"
    # Hardlinks keep debug binaries cheap while confining catalogue entries to
    # the session. This directory lives on the build output's filesystem.
    ln "$root/out/preview/target/debug/$app" "$bundle/entry.elf"
    for icon in appicon.png appicon.svg; do
        if [[ -f "$source_dir/$icon" ]]; then ln -s "$source_dir/$icon" "$bundle/$icon"; fi
    done
done
# Keep the existing real Binder test process available for multi-window checks.
cp -a "$root/applications/binder/resources/apps/Test.app" "$MOCHIOS_PREVIEW_APPS/"

echo "Starting mochiOS desktop; closing the window discards this session."
setsid "$root/out/preview/target/debug/binder" &
desktop_pid=$!
wait "$desktop_pid"
