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

apps=(files settings edit terminal appstore system-monitor viewer calendar calculator mail keychain disk-utility fonts installer test-app)
"$script_dir/preview-app.sh" workspace --check "$@"
[[ $mode != --check ]] || exit 0
preview_font_root="$root/libraries/fonts/out/fonts"
[[ -d $preview_font_root ]] || preview_font_root="$root/libraries/viewkit/resources/fonts"
export MOCHIOS_PREVIEW_FONTS_SYSTEM="${MOCHIOS_PREVIEW_FONTS_SYSTEM:-$preview_font_root}"
export MOCHIOS_PREVIEW_MAIL_DEMO="${MOCHIOS_PREVIEW_MAIL_DEMO:-1}"

# All application and desktop state belongs to this disposable session.
session_dir=$(mktemp -d "$root/out/preview/session.XXXXXXXX")
desktop_pid=
ime_pid=
keychain_pid=
ime_socket_dir=
cleanup() {
    trap - EXIT INT TERM
    if [[ -n $desktop_pid ]]; then
        kill -- "-$desktop_pid" 2>/dev/null || true
        wait "$desktop_pid" 2>/dev/null || true
    fi
    if [[ -n $keychain_pid ]]; then
        kill "$keychain_pid" 2>/dev/null || true
        wait "$keychain_pid" 2>/dev/null || true
    fi
    if [[ -n $ime_pid ]]; then
        kill "$ime_pid" 2>/dev/null || true
        wait "$ime_pid" 2>/dev/null || true
    fi
    if [[ -n $ime_socket_dir ]]; then rm -rf -- "$ime_socket_dir"; fi
    rm -rf -- "$session_dir"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

ime_dictionary=${MOCHIOS_PREVIEW_IME_DICTIONARY:-$root/out/mmake/components/ime/ja.mime}
[[ -r $ime_dictionary ]] || {
    echo "mochiOS IME dictionary is missing; run mmake ime-dictionary first." >&2
    exit 1
}
# Keep Unix socket names short even when the repository path is long.
ime_socket_dir=$(mktemp -d /tmp/mochios-ime.XXXXXXXX)
export MOCHIOS_PREVIEW_IME_SOCKET="$ime_socket_dir/input.sock"
"$root/out/preview/target/debug/mochios-preview-ime" "$ime_dictionary" "$MOCHIOS_PREVIEW_IME_SOCKET" &
ime_pid=$!
for ((attempt=0; attempt<100; attempt++)); do
    [[ ! -S $MOCHIOS_PREVIEW_IME_SOCKET ]] || break
    kill -0 "$ime_pid" 2>/dev/null || { wait "$ime_pid"; exit 1; }
    sleep 0.1
done
[[ -S $MOCHIOS_PREVIEW_IME_SOCKET ]] || { echo "mochiOS preview IME did not start." >&2; exit 1; }

export VIEWKIT_PREVIEW_CAPTURE=1
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
    [[ $app != test-app ]] || source_name=test.app
    source_dir="$root/applications/$source_name"
    bundle="$MOCHIOS_PREVIEW_APPS/$app.app"
    mkdir -p "$bundle"
    cp "$source_dir/manifest.toml" "$bundle/manifest.toml"
    # Hardlinks keep debug binaries cheap while confining catalogue entries to
    # the session. This directory lives on the build output's filesystem.
    preview_binary=$app
    [[ $app != test-app ]] || preview_binary=test_app
    ln "$root/out/preview/target/debug/$preview_binary" "$bundle/entry.elf"
    python3 "$script_dir/stage-application-resources.py" "$source_dir" "$bundle"
    if [[ -d "$source_dir/legal" && ! -e "$bundle/legal" ]]; then ln -s "$source_dir/legal" "$bundle/legal"; fi
done
# Keep the existing real Binder test process available for multi-window checks.
cp -a "$root/applications/binder/resources/apps/Test.app" "$MOCHIOS_PREVIEW_APPS/"

export MOCHIOS_PREVIEW_KEYCHAIN_SOCKET="$session_dir/keychain.sock"
python3 "$script_dir/keychain-preview-server.py" "$root" "$MOCHIOS_PREVIEW_KEYCHAIN_SOCKET" "$MOCHIOS_PREVIEW_CONFIG_ROOT/keychain" &
keychain_pid=$!
for ((attempt=0; attempt<100; attempt++)); do
    [[ ! -S $MOCHIOS_PREVIEW_KEYCHAIN_SOCKET ]] || break
    kill -0 "$keychain_pid" 2>/dev/null || { wait "$keychain_pid"; exit 1; }
    sleep 0.1
done
[[ -S $MOCHIOS_PREVIEW_KEYCHAIN_SOCKET ]] || { echo "Keychain preview service did not start." >&2; exit 1; }

echo "Starting mochiOS desktop; closing the window discards this session."
echo "mochiOS IME: Ctrl+Space switches input mode; Space converts; Enter confirms."
echo "Mouse captured while focused. Ctrl+Alt+G releases it; click inside to capture again."
setsid "$root/out/preview/target/debug/binder" &
desktop_pid=$!
wait "$desktop_pid"
