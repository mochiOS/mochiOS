#!/usr/bin/env python3
"""Create one ignored Cargo workspace for Linux UI development."""

import pathlib
import re
import shutil
import sys

root = pathlib.Path(sys.argv[1]).resolve()
preview = root / "out/preview"
applications = ("binder", "file", "settings", "edit", "terminal", "appstore", "system-monitor", "viewer", "calendar", "calculator", "mail", "keychain", "disk-utility", "test.app")


def link(source, target):
    if not target.exists():
        target.symlink_to(source, target_is_directory=source.is_dir())


def write_if_changed(path, text):
    if not path.exists() or path.read_text() != text:
        path.write_text(text)


members = []
for name in applications:
    source = root / "applications" / name
    destination = preview / "applications" / name
    destination.mkdir(parents=True, exist_ok=True)
    manifest = (source / "Cargo.toml").read_text()
    # Per-app workspaces/profiles belong to the OS source layout. Host builds
    # share a resolver, lockfile and profile so dependencies are compiled once.
    manifest = re.sub(r"(?ms)^\[(?:workspace|profile\.[^]]+)\]\n.*?(?=^\[|\Z)", "", manifest)
    manifest = re.sub(
        r'(\bpath\s*=\s*")(\.\./[^"]+)(")',
        lambda match: match[1] + (source / match[2]).resolve().as_posix() + match[3],
        manifest,
    )
    write_if_changed(destination / "Cargo.toml", manifest)
    for entry in ("src", "resources", "appicon.svg", "appicon.png", "manifest.toml"):
        if (source / entry).exists():
            link(source / entry, destination / entry)
    members.append(f'"applications/{name}"')
    if name == "binder":
        shutil.copytree(source / "crates/binder_protocol", destination / "crates/binder_protocol", dirs_exist_ok=True)
        members.append('"applications/binder/crates/binder_protocol"')

# The host input adapter shares input.service's library, not a separate IME implementation.
ime_source = root / "tools/desktop-ime"
ime_destination = preview / "tools/desktop-ime"
ime_destination.mkdir(parents=True, exist_ok=True)
ime_manifest = (ime_source / "Cargo.toml").read_text().replace("[workspace]\n", "")
ime_manifest = re.sub(
    r'(\bpath\s*=\s*")(\.\./[^"]+)(")',
    lambda match: match[1] + (ime_source / match[2]).resolve().as_posix() + match[3],
    ime_manifest,
)
write_if_changed(ime_destination / "Cargo.toml", ime_manifest)
link(ime_source / "src", ime_destination / "src")
members.append('"tools/desktop-ime"')

# Native keychain core is shared by the Linux development daemon.
keychain_source = root / "services/keychain"
keychain_destination = preview / "tools/keychain-service"
keychain_destination.mkdir(parents=True, exist_ok=True)
keychain_manifest = (keychain_source / "Cargo.toml").read_text()
keychain_manifest = re.sub(
    r'(\bpath\s*=\s*")(\.\./[^"]+)(")',
    lambda match: match[1] + (keychain_source / match[2]).resolve().as_posix() + match[3],
    keychain_manifest,
)
write_if_changed(keychain_destination / "Cargo.toml", keychain_manifest)
link(keychain_source / "src", keychain_destination / "src")
members.append('"tools/keychain-service"')

link(root / "resources", preview / "resources")
write_if_changed(preview / "Cargo.toml",
    '[workspace]\nresolver = "2"\nmembers = [\n    ' + ",\n    ".join(members)
    + '\n]\n\n[profile.dev]\npanic = "abort"\ndebug = 0\nincremental = false\n'
    + '\n[profile.dev.package.viewkit]\nopt-level = 2\n'
    + '\n[profile.dev.package.binder]\nopt-level = 1\n'
    + '\n[profile.dev.package.argon2]\nopt-level = 3\n'
    + '\n[profile.dev.package.blake2]\nopt-level = 3\n'
)
if not (preview / "Cargo.lock").exists():
    shutil.copyfile(root / "applications/binder/Cargo.lock", preview / "Cargo.lock")
