#!/usr/bin/env python3
"""Create one ignored Cargo workspace for Linux UI development."""

import pathlib
import re
import shutil
import sys
import tomllib

root = pathlib.Path(sys.argv[1]).resolve()
preview = root / "out/preview"
from preview_catalog import applications as discover_applications
applications = tuple(name for name, _ in discover_applications(root))


def link(source, target):
    if not target.exists():
        target.symlink_to(source, target_is_directory=source.is_dir())


def write_if_changed(path, text):
    if not path.exists() or path.read_text() != text:
        path.write_text(text)


def copy_if_changed(source, target):
    source, target = pathlib.Path(source), pathlib.Path(target)
    if not target.exists() or source.read_bytes() != target.read_bytes():
        shutil.copy2(source, target)
    return str(target)

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
    for entry in ("src", "resources", "legal", "appicon.svg", "appicon.png", "manifest.toml"):
        if (source / entry).exists():
            link(source / entry, destination / entry)
    app_manifest=tomllib.loads((source / "manifest.toml").read_text())
    for resource in app_manifest.get("application", {}).get("resources", []):
        relative=pathlib.PurePosixPath(resource)
        if relative.is_absolute() or ".." in relative.parts:
            raise ValueError(f"Unsafe application resource: {resource}")
        if (source / resource).exists():
            (destination / resource).parent.mkdir(parents=True, exist_ok=True)
            link(source / resource, destination / resource)
    members.append(f'"applications/{name}"')
    if name == "binder":
        shutil.copytree(source / "crates/binder_protocol", destination / "crates/binder_protocol", dirs_exist_ok=True, copy_function=copy_if_changed)
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
    + '\n]\n\n[profile.dev]\npanic = "abort"\ndebug = 0\nincremental = true\n'
    + '\n[profile.dev.package.viewkit]\nopt-level = 2\n'
    # Document thumbnails run off the UI thread, but unoptimized image codecs
    # can still exceed the worker deadline for ordinary desktop-size images.
    + '\n[profile.dev.package.image]\nopt-level = 2\n'
    + '\n[profile.dev.package.png]\nopt-level = 2\n'
    + '\n[profile.dev.package.fdeflate]\nopt-level = 2\n'
    + '\n[profile.dev.package.zune-jpeg]\nopt-level = 2\n'
    + '\n[profile.dev.package.binder]\nopt-level = 1\n'
    + '\n[profile.dev.package.argon2]\nopt-level = 3\n'
    + '\n[profile.dev.package.blake2]\nopt-level = 3\n'
)
if not (preview / "Cargo.lock").exists():
    shutil.copyfile(root / "applications/binder/Cargo.lock", preview / "Cargo.lock")
