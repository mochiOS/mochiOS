#!/usr/bin/env python3
"""Resolve distribution manifests into a bounded native boot-module index."""
import pathlib
import struct
import sys
import tomllib

MAX_BYTES = 64 * 1024 * 1024
MAX_ENTRIES = 64
HEADER_SIZE = 24
ENTRY_SIZE = 24


def package_bytes(root, bundle, manifest):
    metadata = manifest["cext"]
    if metadata["abi"] != 4 or type(metadata["abi"]) is not int:
        raise ValueError(f"{bundle}: unsupported native module ABI")
    entry = metadata["entry"]
    if not isinstance(entry, str) or not entry:
        raise ValueError(f"{bundle}: entry must be a nonempty path")
    path = (root / entry.lstrip("/") if entry.startswith("/") else bundle / entry).resolve(strict=True)
    if not path.is_relative_to(root) or not path.is_file():
        raise ValueError(f"{bundle}: entry escapes the selected resources")
    if path.stat().st_size > MAX_BYTES:
        raise ValueError(f"{path}: module exceeds 64 MiB")
    data = path.read_bytes()
    if len(data) < 32 or len(data) > MAX_BYTES or data[:4] != b"MCEX":
        raise ValueError(f"{path}: invalid native module package")
    abi, version, name_len, dependencies, header_len, elf_len = struct.unpack_from("<HHHHIQ", data, 4)
    if abi != 4 or type(metadata["version"]) is not int or version != metadata["version"]:
        raise ValueError(f"{path}: module ABI/version differs from manifest")
    cursor = 32 + name_len
    if name_len == 0 or cursor > header_len or header_len > len(data):
        raise ValueError(f"{path}: invalid module name range")
    if data[32:cursor].decode("utf-8") != metadata["name"]:
        raise ValueError(f"{path}: module name differs from manifest")
    for _ in range(dependencies):
        if cursor + 2 > header_len:
            raise ValueError(f"{path}: truncated dependency length")
        length = struct.unpack_from("<H", data, cursor)[0]
        cursor += 2
        if cursor + length > header_len:
            raise ValueError(f"{path}: truncated dependency")
        data[cursor:cursor + length].decode("utf-8")
        cursor += length
    if cursor != header_len or elf_len == 0 or header_len + elf_len != len(data):
        raise ValueError(f"{path}: invalid module payload range")
    if data[header_len:header_len + 7] != b"\x7fELF\x02\x01\x01":
        raise ValueError(f"{path}: native module must contain little-endian ELF64")
    return data


def build(stage):
    root = pathlib.Path(stage).resolve(strict=True)
    if not root.is_dir():
        raise ValueError(f"{root}: selected resources must be a directory")
    selected = []
    total = HEADER_SIZE
    for bundle in sorted(root.glob("*.cext")):
        if not bundle.is_dir() or not bundle.resolve().is_relative_to(root):
            raise ValueError(f"{bundle}: invalid extension bundle directory")
        manifest_path = bundle / "manifest.toml"
        if not manifest_path.resolve(strict=True).is_relative_to(root):
            raise ValueError(f"{bundle}: manifest escapes the selected resources")
        with manifest_path.open("rb") as source:
            manifest = tomllib.load(source)
        load = manifest["load"]
        if not isinstance(load["stage"], str) or not load["stage"]:
            raise ValueError(f"{bundle}: load stage must be a nonempty string")
        if load["stage"] != "boot":
            continue
        if type(load["required"]) is not bool:
            raise ValueError(f"{bundle}: required must be a boolean")
        provides = manifest["provides"]["interfaces"]
        if not isinstance(provides, list) or not provides or any(item != "block.device" for item in provides):
            raise ValueError(f"{bundle}: no supported native adapter for interfaces")
        # No dependency resolver runs in the kernel. Reject unsupported boot
        # dependency policy rather than passing names for it to interpret.
        if manifest["requires"]["interfaces"] != []:
            raise ValueError(f"{bundle}: unsupported boot interface dependency")
        data = package_bytes(root, bundle, manifest)
        selected.append((data, int(load["required"])))
        total += ENTRY_SIZE + len(data)
        if len(selected) > MAX_ENTRIES or total > MAX_BYTES:
            raise ValueError("native boot module index exceeds its published bound")
    if not selected:
        return b""
    header = struct.pack("<8sIIQ", b"MNUXMOD\0", 1, ENTRY_SIZE, len(selected))
    records = []
    offset = HEADER_SIZE + ENTRY_SIZE * len(selected)
    for data, required in selected:
        records.append(struct.pack("<QQII", offset, len(data), 1, required))
        offset += len(data)
    return b"".join([header, *records, *(data for data, _ in selected)])


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("usage: pack-boot-extensions.py <selected-stage>")
    try:
        sys.stdout.buffer.write(build(sys.argv[1]))
    except (OSError, ValueError, KeyError, TypeError, struct.error) as error:
        raise SystemExit(f"boot extensions: {error}") from error
