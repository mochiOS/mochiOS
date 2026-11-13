#!/usr/bin/env bash
set -euo pipefail

[[ $# -ge 3 && $# -le 4 ]] || { echo "usage: $0 <project-root> <mmake-output> <msign> [system|installer]" >&2; exit 2; }
root=$1
mmake_out=$2
msign=$3
variant=${4:-system}
app_plan=$(python3 "$root/scripts/mmake/image-applications.py" --root "$root" --variant "$variant")
image_out=$mmake_out
[[ $variant != installer ]] || image_out=$mmake_out/installer
stage=$image_out/image/rootfs
bin=$root/out/rust-std/target/x86_64-unknown-mochios/release
driver_bin=$root/out/services-build/target/x86_64-unknown-mochios/release
config=$root/.config

rm -rf "$stage.new"
stage_new=$stage.new
mkdir -p \
    "$stage_new/bin" \
    "$stage_new/applications" \
    "$stage_new/libraries/applications" \
    "$stage_new/libraries/fonts" \
    "$stage_new/tmp" \
    "$stage_new/var/config" \
    "$stage_new/var/config/applications/org.mochios.edit/users" \
    "$stage_new/var/lib/accounts" \
    "$stage_new/var/lib/keychain" \
    "$stage_new/var/lib/certificate" \
    "$stage_new/var/lib/diagnostics" \
    "$stage_new/var/lib/packages" \
    "$stage_new/var/lib/security" \
    "$stage_new/var/config/workspace" \
    "$stage_new/var/log/services" \
    "$stage_new/system/applications" \
    "$stage_new/system/bin" \
    "$stage_new/system/libraries/ime" \
    "$stage_new/system/libraries/fonts" \
    "$stage_new/system/packages" \
    "$stage_new/system/services"
chmod 0700 "$stage_new/var/lib/diagnostics" "$stage_new/var/lib/keychain"
chmod 01777 "$stage_new/var/config/applications/org.mochios.edit/users"
chmod 01777 "$stage_new/var/config/workspace"
chmod 01777 "$stage_new/tmp"
for category in account appearance diagnostics general input network security; do mkdir -p "$stage_new/var/config/$category"; chmod 0777 "$stage_new/var/config/$category"; done
chmod 0755 "$stage_new/var/config/account"
if [[ $variant == system ]]; then
    printf 'format=1\n' > "$stage_new/system/.installed"
    chmod 0644 "$stage_new/system/.installed"
fi

cp -a "$root/resources/system/." "$stage_new/system/"
cp -a "$root/resources/libraries/." "$stage_new/system/libraries/"
python3 "$root/scripts/mmake/stage-system-libraries.py" \
    --root "$root" --catalog "$root/build/system-libraries.toml" \
    --stage "$stage_new" --msign "$msign"
install -m 0600 "$root/resources/system/users/users.db" "$stage_new/var/lib/accounts/users.db"
rm -rf "$stage_new/system/users"
for directory in Desktop Documents Downloads Movies Music Pictures; do mkdir -p "$stage_new/home/root/$directory"; chmod 0700 "$stage_new/home/root/$directory"; done
if grep -qx 'DEVELOPMENT_DEFAULT_ACCOUNT=y' "$config"; then
    install -m 0600 "$root/scripts/mmake/development-users.db" "$stage_new/var/lib/accounts/users.db"
    printf 'auto_login=true\nauto_login_user=testuser\n' > "$stage_new/var/config/account/settings.conf"
    chmod 0644 "$stage_new/var/config/account/settings.conf"
    : > "$stage_new/.mochios-ownership"
    printf 'home/testuser 1000 1000\n' >> "$stage_new/.mochios-ownership"
    for directory in Desktop Documents Downloads Movies Music Pictures; do mkdir -p "$stage_new/home/testuser/$directory"; chmod 0700 "$stage_new/home/testuser/$directory"; done
    chmod 0700 "$stage_new/home/testuser"
    for directory in Desktop Documents Downloads Movies Music Pictures; do printf 'home/testuser/%s 1000 1000\n' "$directory" >> "$stage_new/.mochios-ownership"; done
fi
mkdir -p "$stage_new/system/libraries/fonts" "$stage_new/system/resources/msh"
cp -a "$root/libraries/fonts/out/fonts/." "$stage_new/system/libraries/fonts/"
rm -f "$stage_new/system/libraries/fonts/.installed"
install -m 0644 "$root/binaries/msh/resources/ter-u12b.bdf" "$stage_new/system/resources/msh/ter-u12b.bdf"
install -m 0644 "$mmake_out/components/ime/ja.mime" "$stage_new/system/libraries/ime/ja.mime"
install -m 0644 "$root/build/ime/ja.mime.sha256" "$stage_new/system/libraries/ime/ja.mime.sha256"

install -m 0755 "$root/out/newlib-port/hello/hello.elf" "$stage_new/system/bin/hello"
for program in rust-std-demo msh; do install -m 0755 "$bin/$program" "$stage_new/system/bin/$program"; done
coreutils=(sudo echo ls pwd true false cat touch rm id useradd userdel userlist mpk net gcc os-test test_gui test_desktop)
grep -qx 'KERNEL_PERFORMANCE_INSTRUMENTATION=y' "$config" && coreutils+=(mperf) || true
grep -qx 'USER_BUILD_SELFTESTS=y' "$config" && coreutils+=(selftest-capability selftest-process selftest-ext2-write selftest-system-layout) || true
for program in "${coreutils[@]}"; do install -m 0755 "$bin/$program" "$stage_new/system/bin/$program"; done

if grep -qx 'USER_INSTALL_DEVELOPER_TOOLS=y' "$config"; then
    python3 "$root/scripts/mmake/stage-development-tools.py" "$stage_new" \
        --sdk "$root/out/newlib-port/sdk" \
        --llvm "$root/out/llvm-port/build-target" \
        --rust-tools "$bin"
    vim_stage=$root/out/vim-port/release/stage/system
    test -s "$root/out/vim-port/release/.ready"
    printf '%s  %s\n' "$(cat "$root/out/vim-port/release/.ready")" "$vim_stage/bin/vim" | sha256sum -c -
    install -m 0755 "$vim_stage/bin/vim" "$stage_new/system/bin/vim"
    mkdir -p "$stage_new/system/share/licenses"
    cp -a "$vim_stage/share/vim" "$vim_stage/share/terminfo" "$stage_new/system/share/"
    cp -a "$vim_stage/share/licenses/vim" "$stage_new/system/share/licenses/"
fi

install_manifest() {
    local source=$1 name=$2 package_root=$stage_new/system/packages/$2
    local binding_args=(--rootfs "$stage_new" --map-path /bin=/system/bin)
    if grep -Fq '"window.create"' "$source" && ! grep -Fq '"ipc.server"' "$source"; then
        echo "fatal: $source requires window.create but cannot receive window events without ipc.server" >&2
        exit 1
    fi
    if [[ -e $package_root/manifest.toml || -e $package_root/verification.bin ]]; then
        echo "fatal: duplicate built-in package directory: $name" >&2
        exit 1
    fi
    mkdir -p "$package_root"
    if [[ $name == coreutils ]]; then
        binding_args+=(--exclude-binary /bin/test_app)
        grep -qx 'KERNEL_PERFORMANCE_INSTRUMENTATION=y' "$config" || binding_args+=(--exclude-binary /bin/mperf)
        if ! grep -qx 'USER_BUILD_SELFTESTS=y' "$config"; then
            binding_args+=(--exclude-binary /bin/selftest-ext2-write --exclude-binary /bin/selftest-system-layout)
        fi
    fi
    "$msign" package bind-built-in "$source" "${binding_args[@]}" --output "$package_root/manifest.toml"
    "$msign" package built-in-record "$package_root/manifest.toml" --output "$package_root/verification.bin"
    chmod 0644 "$package_root/verification.bin"
}
install_manifest "$root/user/apps/rust-std-demo/manifest.toml" rust-std-demo
install_manifest "$root/binaries/msh/manifest.toml" msh
install_manifest "$root/binaries/coreutils/manifest.toml" coreutils
if grep -qx 'USER_INSTALL_DEVELOPER_TOOLS=y' "$config"; then
    install_manifest "$root/scripts/vim/manifest.toml" vim
    developer_package=$stage_new/system/packages/development-tools
    "$msign" package built-in-record "$developer_package/manifest.toml" --output "$developer_package/verification.bin"
    chmod 0644 "$developer_package/verification.bin"
fi

stage_app() {
    local source=$1 bundle=$2 binary=$3
    local destination=$stage_new/system/applications/$bundle
    mkdir -p "$destination"
    install -m 0755 "$bin/$binary" "$destination/entry.elf"
    install -m 0644 "$source/manifest.toml" "$destination/manifest.toml"
    python3 "$root/scripts/dev/stage-application-resources.py" "$source" "$destination"
}

# Staging and built-in registration use the same selected set. No trust record
# is created for Store applications or the development-only test application.
while IFS=$'\t' read -r source bundle binary package; do
    stage_app "$root/applications/$source" "$bundle" "$binary"
    install_manifest "$root/applications/$source/manifest.toml" "$package"
done <<< "$app_plan"

# Legal documents are shared OS resources, independently of Installer.app.
mkdir -p "$stage_new/system/resources/legal"
cp -a "$root/applications/installer/legal/." "$stage_new/system/resources/legal/"

stage_service() {
    local source=$1 binary=$2 destination=$3 package=${4:-$1}
    install -m 0755 "$bin/$binary" "$stage_new/system/services/$destination"
    install_manifest "$root/services/$source/manifest.toml" "$package"
}
stage_service capability capability capability.service
stage_service display display display.driver
stage_service compositor compositor compositor.service
stage_service drivers drivers drivers.service
stage_service filesystem filesystem-service filesystem.service
stage_service logger logger logger.service
stage_service input input input.service
stage_service linux linux linux.service
stage_service network network network.service
stage_service package package package.service
stage_service posix posix-service posix.service
stage_service signature signature signature.service
stage_service tty tty tty.service
stage_service user user-service user.service user
stage_service keychain keychain-service keychain.service
stage_service service-manager service-manager service-manager.service
stage_service mboot-agent mboot-agent mboot-agent.service
stage_service secure-ui secure-ui secure-ui.service
stage_service update update update.service
stage_service workspace workspace-service workspace.service

if grep -qx 'DRIVER_XHCI=y' "$config"; then
    mkdir -p "$stage_new/system/bin/drivers/usb/qemu-usb.driver"
    install -m 0755 "$driver_bin/entry" "$stage_new/system/bin/drivers/usb/qemu-usb.driver/entry.elf"
    install_manifest "$root/drivers/usb-driver/manifest.toml" drivers/usb/qemu-usb.driver
fi
if grep -qx 'DRIVER_I8042=y' "$config"; then
    mkdir -p "$stage_new/system/bin/drivers/ps2/i8042.driver"
    install -m 0755 "$driver_bin/i8042-entry" "$stage_new/system/bin/drivers/ps2/i8042.driver/entry.elf"
    install_manifest "$root/drivers/ps2/i8042-driver/manifest.toml" drivers/ps2/i8042.driver
fi
if grep -qx 'DRIVER_VIRTIO_NET=y' "$config"; then
    mkdir -p "$stage_new/system/bin/drivers/network/virtio-net.driver"
    install -m 0755 "$driver_bin/virtio-net-driver" "$stage_new/system/bin/drivers/network/virtio-net.driver/virtio-net.driver"
    install_manifest "$root/drivers/virtio-net-driver/manifest.toml" drivers/network/virtio-net.driver
fi

touch "$stage_new/.ready"
rm -rf "$stage"
mv "$stage_new" "$stage"
