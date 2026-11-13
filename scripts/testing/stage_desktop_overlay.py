#!/usr/bin/env python3
"""Stage the ordinary desktop in a private native validation image overlay."""
import argparse
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[2]


def stage(output, msign):
    """Use normal service/driver manifests and existing built-in image verification."""
    output = output.resolve()
    if output.exists() or not output.is_relative_to((ROOT / 'out').resolve()):
        raise ValueError('Use a fresh overlay directory below workspace out/')
    native = ROOT / 'out/rust-std/target/x86_64-unknown-mochios/release'
    drivers = ROOT / 'out/services-build/target/x86_64-unknown-mochios/release'
    output.mkdir(parents=True)
    # Mirror the ordinary image's writable configuration locations. Capability
    # checks still constrain which categories each service/application can use.
    for category in ('account', 'appearance', 'diagnostics', 'general', 'input', 'network', 'security'):
        directory = output / 'var/config' / category
        directory.mkdir(parents=True)
        directory.chmod(0o755 if category == 'account' else 0o777)
    workspace = output / 'var/config/workspace'
    workspace.mkdir()
    workspace.chmod(0o1777)
    for directory in ('libraries/applications', 'libraries/fonts'):
        (output / directory).mkdir(parents=True)

    def install(source, destination, mode=0o644):
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, destination)
        destination.chmod(mode)

    def bind(source, name):
        destination = output / 'system/packages' / name
        destination.mkdir(parents=True, exist_ok=True)
        subprocess.run([str(msign), 'package', 'bind-built-in', str(source),
                        '--rootfs', str(output), '--output', str(destination / 'manifest.toml')], check=True)
        subprocess.run([str(msign), 'package', 'built-in-record', str(destination / 'manifest.toml'),
                        '--output', str(destination / 'verification.bin')], check=True)

    for source, binary, name in [
        ('capability', 'capability', 'capability.service'),
        ('display', 'display', 'display.driver'),
        ('compositor', 'compositor', 'compositor.service'),
        ('drivers', 'drivers', 'drivers.service'),
        ('filesystem', 'filesystem-service', 'filesystem.service'),
        ('logger', 'logger', 'logger.service'),
        ('input', 'input', 'input.service'),
        ('linux', 'linux', 'linux.service'),
        ('network', 'network', 'network.service'),
        ('package', 'package', 'package.service'),
        ('posix', 'posix-service', 'posix.service'),
        ('signature', 'signature', 'signature.service'),
        ('tty', 'tty', 'tty.service'),
        ('user', 'user-service', 'user.service'),
        ('keychain', 'keychain-service', 'keychain.service'),
        ('service-manager', 'service-manager', 'service-manager.service'),
        ('mboot-agent', 'mboot-agent', 'mboot-agent.service'),
        ('secure-ui', 'secure-ui', 'secure-ui.service'),
        ('update', 'update', 'update.service'),
        ('workspace', 'workspace-service', 'workspace.service'),
    ]:
        install(native / binary, output / 'system/services' / name, 0o755)
        bind(ROOT / 'services' / source / 'manifest.toml', source)
    for directory, name, binary in [('binder', 'Binder.app', 'binder'),
                                     ('terminal', 'Terminal.app', 'terminal')]:
        source = ROOT / 'applications' / directory
        bundle = output / 'system/applications' / name
        install(native / binary, bundle / 'entry.elf', 0o755)
        install(source / 'manifest.toml', bundle / 'manifest.toml')
        if directory == 'binder':
            for resource in ('appicon.svg', 'close.svg', 'maximize.svg', 'minimize.svg', 'mochios.svg'):
                install(source / 'resources' / resource, bundle / resource)
        else:
            install(source / 'appicon.png', bundle / 'appicon.png')
        subprocess.run(['python3', str(ROOT / 'scripts/dev/stage-application-resources.py'),
                        str(source), str(bundle)], check=True)
        bind(source / 'manifest.toml', directory)
    config = (ROOT / '.config').read_text().splitlines()
    for option, source, binary, destination, package in [
        ('DRIVER_XHCI=y', 'usb-driver', 'entry', 'usb/qemu-usb.driver/entry.elf', 'usb/qemu-usb.driver'),
        ('DRIVER_I8042=y', 'ps2/i8042-driver', 'i8042-entry', 'ps2/i8042.driver/entry.elf', 'ps2/i8042.driver'),
        ('DRIVER_VIRTIO_NET=y', 'virtio-net-driver', 'virtio-net-driver',
         'network/virtio-net.driver/virtio-net.driver', 'network/virtio-net.driver'),
    ]:
        if option in config:
            install(drivers / binary, output / 'system/bin/drivers' / destination, 0o755)
            bind(ROOT / 'drivers' / source / 'manifest.toml', 'drivers/' + package)
    install(ROOT / 'binaries/msh/resources/ter-u12b.bdf', output / 'system/resources/msh/ter-u12b.bdf')
    install(ROOT / 'out/mmake/components/ime/ja.mime', output / 'system/libraries/ime/ja.mime')
    install(ROOT / 'build/ime/ja.mime.sha256', output / 'system/libraries/ime/ja.mime.sha256')
    print('Ordinary desktop overlay:', output)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--msign', type=Path, required=True)
    args = parser.parse_args()
    stage(args.output, args.msign.resolve(strict=True))
