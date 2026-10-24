#!/usr/bin/env python3
"""Launch the real vault core with a development-only, executable-pinned client registry."""
import os
import pathlib
import sys
import tomllib

root, socket, storage = map(pathlib.Path, sys.argv[1:])
root = root.resolve()
binaries = root / "out/preview/target/debug"
command = [str(binaries / "keychain-service"), "--socket", str(socket), "--data-root", str(storage), "--auto-session"]
for directory in sorted((root / "applications").iterdir()):
    manifest_path = directory / "manifest.toml"
    cargo_path = directory / "Cargo.toml"
    if not manifest_path.exists() or not cargo_path.exists():
        continue
    manifest = tomllib.loads(manifest_path.read_text())
    package = manifest.get("package", {}).get("id", "")
    requested = {cap for binary in manifest.get("binary", []) for cap in binary.get("requires", [])}
    if "keychain.access" not in requested:
        continue
    cargo = tomllib.loads(cargo_path.read_text())
    targets = cargo.get("bin", [])
    if not targets:
        continue
    binary = binaries / targets[0]["name"]
    if not binary.is_file():
        continue
    manager = package == "org.mochios.keychain" and "keychain.manage" in requested
    command.extend(["--manager" if manager else "--client", str(binary), package])
os.execv(command[0], command)
