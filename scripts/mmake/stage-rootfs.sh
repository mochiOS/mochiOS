#!/usr/bin/env bash
set -euo pipefail

[[ $# -eq 3 ]] || { echo "usage: $0 <project-root> <mmake-output> <msign>" >&2; exit 2; }
root=$1
mmake_out=$2
msign=$3
stage=$mmake_out/image/rootfs
bin=$root/out/rust-std/target/x86_64-unknown-mochios/release
driver_bin=$root/out/services-build/target/x86_64-unknown-mochios/release
config=$root/.config

rm -rf "$stage.new"
stage_new=$stage.new
mkdir -p "$stage_new/bin" "$stage_new/tmp" "$stage_new/var/config" "$stage_new/libraries/system" "$stage_new/libraries/applications" "$stage_new/system/logs" "$stage_new/system/packages" "$stage_new/system/services"
chmod 01777 "$stage_new/tmp"
for category in account appearance general input network security; do mkdir -p "$stage_new/var/config/$category"; chmod 0777 "$stage_new/var/config/$category"; done
printf 'format=1\n' > "$stage_new/system/.installed"
chmod 0644 "$stage_new/system/.installed"

cp -a "$root/resources/." "$stage_new/"
chmod 0600 "$stage_new/system/users/users.db"
for directory in Desktop Documents Downloads Movies Music Pictures; do mkdir -p "$stage_new/home/root/$directory"; chmod 0700 "$stage_new/home/root/$directory"; done
mkdir -p "$stage_new/libraries/fonts" "$stage_new/system/resources/msh"
cp -a "$root/libraries/fonts/out/fonts/." "$stage_new/libraries/fonts/"
rm -f "$stage_new/libraries/fonts/.installed"
install -m 0644 "$root/binaries/msh/resources/ter-u12b.bdf" "$stage_new/system/resources/msh/ter-u12b.bdf"

install -m 0755 "$root/out/newlib-port/hello/hello.elf" "$stage_new/bin/hello"
for program in rust-std-demo test_app msh; do install -m 0755 "$bin/$program" "$stage_new/bin/$program"; done
coreutils=(echo ls pwd true false cat touch rm id useradd userdel userlist mpk net gcc test_gui test_desktop)
grep -qx 'KERNEL_PERFORMANCE_INSTRUMENTATION=y' "$config" && coreutils+=(mperf) || true
grep -qx 'USER_BUILD_SELFTESTS=y' "$config" && coreutils+=(selftest-capability selftest-process selftest-ext2-write) || true
for program in "${coreutils[@]}"; do install -m 0755 "$bin/$program" "$stage_new/bin/$program"; done

install_manifest() {
    local source=$1 name=$2 package_root=$stage_new/system/packages/$2
    mkdir -p "$package_root"
    install -m 0644 "$source" "$package_root/manifest.toml"
    "$msign" package built-in-record "$package_root/manifest.toml" --output "$package_root/verification.bin"
    chmod 0644 "$package_root/verification.bin"
}
install_manifest "$root/user/apps/rust-std-demo/manifest.toml" rust-std-demo
install_manifest "$root/applications/test.app/manifest.toml" viewkit-test
install_manifest "$root/binaries/msh/manifest.toml" msh
install_manifest "$root/binaries/coreutils/manifest.toml" coreutils

stage_app() {
    local source=$1 bundle=$2 binary=$3 icon=${4:-}
    local destination=$stage_new/applications/$bundle
    mkdir -p "$destination"
    install -m 0755 "$bin/$binary" "$destination/entry.elf"
    install -m 0644 "$source/about.toml" "$destination/about.toml"
    install -m 0644 "$source/manifest.toml" "$destination/manifest.toml"
    [[ -z $icon ]] || install -m 0644 "$source/$icon" "$destination/$icon"
}

stage_app "$root/applications/binder" Binder.app binder
for resource in appicon.svg close.svg maximize.svg minimize.svg mochios.svg; do install -m 0644 "$root/applications/binder/resources/$resource" "$stage_new/applications/Binder.app/$resource"; done
if [[ -d $root/applications/binder/resources/apps ]]; then cp -a "$root/applications/binder/resources/apps/." "$stage_new/applications/"; fi
stage_app "$root/applications/appstore" AppStore.app appstore
stage_app "$root/applications/test.app" test.app test_app
stage_app "$root/applications/terminal" Terminal.app terminal appicon.svg
stage_app "$root/applications/file" Files.app files appicon.svg
mkdir -p "$stage_new/applications/Files.app/icons"
for resource in folder.svg file.svg application.svg image.svg archive.svg disk.svg; do install -m 0644 "$root/applications/file/resources/icons/$resource" "$stage_new/applications/Files.app/icons/$resource"; done
stage_app "$root/applications/settings" Settings.app settings appicon.png
stage_app "$root/applications/installer" Installer.app installer appicon.svg
install_manifest "$root/applications/binder/manifest.toml" binder
install_manifest "$root/applications/appstore/manifest.toml" appstore
install_manifest "$root/applications/terminal/manifest.toml" terminal
install_manifest "$root/applications/file/manifest.toml" files
install_manifest "$root/applications/settings/manifest.toml" settings
install_manifest "$root/applications/installer/manifest.toml" installer

stage_service() {
    local source=$1 binary=$2 destination=$3 package=${4:-$1}
    install -m 0755 "$bin/$binary" "$stage_new/system/services/$destination"
    install_manifest "$root/services/$source/manifest.toml" "$package"
}
stage_service capability capability capability.service
stage_service display display display.driver
stage_service compositor compositor compositor.service
stage_service drivers drivers drivers.service
stage_service logger logger logger.service
stage_service input input input.service
stage_service linux linux linux.service
stage_service network network network.service
stage_service package package package.service
stage_service signature signature signature.service
stage_service tty tty tty.service
stage_service user user-service user.service user
stage_service service-manager service-manager service-manager.service
stage_service mboot-agent mboot-agent mboot-agent.service
stage_service secure-ui secure-ui secure-ui.service
stage_service update update update.service

if grep -qx 'DRIVER_XHCI=y' "$config"; then
    mkdir -p "$stage_new/bin/drivers/usb/qemu-usb.driver"
    install -m 0755 "$driver_bin/entry" "$stage_new/bin/drivers/usb/qemu-usb.driver/entry.elf"
    install_manifest "$root/drivers/usb-driver/manifest.toml" drivers/usb/qemu-usb.driver
fi
if grep -qx 'DRIVER_I8042=y' "$config"; then
    mkdir -p "$stage_new/bin/drivers/ps2/i8042.driver"
    install -m 0755 "$driver_bin/i8042-entry" "$stage_new/bin/drivers/ps2/i8042.driver/entry.elf"
    install_manifest "$root/drivers/ps2/i8042-driver/manifest.toml" drivers/ps2/i8042.driver
fi
if grep -qx 'DRIVER_VIRTIO_NET=y' "$config"; then
    mkdir -p "$stage_new/bin/drivers/network/virtio-net.driver"
    install -m 0755 "$driver_bin/virtio-net-driver" "$stage_new/bin/drivers/network/virtio-net.driver/virtio-net.driver"
    install_manifest "$root/drivers/virtio-net-driver/manifest.toml" drivers/network/virtio-net.driver
fi

touch "$stage_new/.ready"
rm -rf "$stage"
mv "$stage_new" "$stage"
