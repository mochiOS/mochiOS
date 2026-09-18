#!/usr/bin/env python3
import hashlib
import json
import os
import re
import shutil
import stat
import subprocess
import sys
from pathlib import Path


def snapshot(root: Path) -> dict[str, dict[str, object]]:
    result: dict[str, dict[str, object]] = {}
    for path in sorted(root.rglob("*")):
        relative = path.relative_to(root).as_posix()
        if relative == ".ready":
            continue
        if any(character.isspace() for character in relative):
            raise SystemExit(f"unsupported whitespace in filesystem path: {relative}")
        metadata = path.lstat()
        if stat.S_ISLNK(metadata.st_mode):
            raise SystemExit(f"symbolic links are not supported in filesystem image: {relative}")
        if stat.S_ISDIR(metadata.st_mode):
            result[relative] = {"type": "dir", "mode": stat.S_IMODE(metadata.st_mode)}
        elif stat.S_ISREG(metadata.st_mode):
            digest = hashlib.sha256()
            with path.open("rb") as stream:
                for chunk in iter(lambda: stream.read(1024 * 1024), b""):
                    digest.update(chunk)
            result[relative] = {
                "type": "file",
                "mode": stat.S_IMODE(metadata.st_mode),
                "sha256": digest.hexdigest(),
            }
        else:
            raise SystemExit(f"unsupported filesystem entry: {relative}")
    return result


def save(path: Path, value: dict[str, dict[str, object]]) -> None:
    temporary = path.with_suffix(path.suffix + ".new")
    temporary.write_text(json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n")
    os.replace(temporary, path)


def inode_mode(entry: dict[str, object]) -> str:
    kind = stat.S_IFDIR if entry["type"] == "dir" else stat.S_IFREG
    return f"0{kind | int(entry['mode']):o}"


def record_dirty_ranges(image: Path, trace_path: Path) -> None:
    writes: list[tuple[int, int]] = []
    image_fds: set[tuple[str, str]] = set()
    positions: dict[tuple[str, str], int] = {}
    lines = trace_path.read_text(errors="replace").splitlines()
    pwrite = re.compile(r"^(\d+)\s+pwrite64\((\d+), .*, (\d+), (\d+)\)\s+=\s+(\d+)$")
    seek = re.compile(r"^(\d+)\s+lseek\((\d+), (\d+), SEEK_SET\)\s+=\s+(\d+)$")
    write = re.compile(r"^(\d+)\s+write\((\d+), .*, (\d+)\)\s+=\s+(\d+)$")
    for line in lines:
        if match := pwrite.match(line):
            key = (match.group(1), match.group(2))
            image_fds.add(key)
            length = int(match.group(5))
            if length > 0:
                offset = int(match.group(4))
                writes.append((offset, offset + length))
    for line in lines:
        if match := seek.match(line):
            key = (match.group(1), match.group(2))
            if key in image_fds:
                positions[key] = int(match.group(4))
        elif match := write.match(line):
            key = (match.group(1), match.group(2))
            if key in positions:
                length = int(match.group(4))
                if length > 0:
                    offset = positions[key]
                    writes.append((offset, offset + length))
                    positions[key] += length

    aligned = sorted(
        (start // 4096 * 4096, (end + 4095) // 4096 * 4096) for start, end in writes
    )
    merged: list[list[int]] = []
    for start, end in aligned:
        if merged and start <= merged[-1][1]:
            merged[-1][1] = max(merged[-1][1], end)
        else:
            merged.append([start, end])
    metadata = image.stat()
    dirty = {
        "device": metadata.st_dev,
        "inode": metadata.st_ino,
        "size": metadata.st_size,
        "mtime_ns": metadata.st_mtime_ns,
        "ranges": merged,
    }
    dirty_path = image.with_suffix(image.suffix + ".dirty.json")
    temporary = dirty_path.with_suffix(dirty_path.suffix + ".new")
    temporary.write_text(json.dumps(dirty, sort_keys=True, separators=(",", ":")) + "\n")
    os.replace(temporary, dirty_path)


def quote(path: str) -> str:
    return '"/' + path.replace('"', '\\"') + '"'


def sync(root: Path, image: Path, state_path: Path) -> None:
    old = json.loads(state_path.read_text())
    new = snapshot(root)
    commands: list[str] = []
    removed = sorted(set(old) - set(new), key=lambda item: (item.count("/"), item), reverse=True)
    for path in removed:
        commands.append(("rmdir " if old[path]["type"] == "dir" else "rm ") + quote(path))
    for path, entry in sorted(new.items(), key=lambda item: (item[0].count("/"), item[0])):
        previous = old.get(path)
        if entry["type"] == "dir" and previous is None:
            commands.append("mkdir " + quote(path))
        elif entry["type"] == "file" and previous != entry:
            if previous is not None:
                commands.append("rm " + quote(path))
            commands.append(f'write "{root / path}" {quote(path)}')
        if previous != entry:
            commands.append(f"set_inode_field {quote(path)} mode {inode_mode(entry)}")
            commands.append(f"set_inode_field {quote(path)} uid 0")
            commands.append(f"set_inode_field {quote(path)} gid 0")
    if commands:
        trace_path = image.with_suffix(image.suffix + ".debugfs.trace")
        debugfs = ["debugfs", "-w", "-f", "-", str(image)]
        traced = shutil.which("strace") is not None
        if traced:
            probe = subprocess.run(
                ["strace", "-qq", "-o", os.devnull, "true"],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            traced = probe.returncode == 0
        command = (
            ["strace", "-qq", "-f", "-e", "trace=pwrite64,write,lseek", "-o", str(trace_path)]
            + debugfs
            if traced
            else debugfs
        )
        process = subprocess.run(
            command,
            input="\n".join(commands) + "\n",
            text=True,
            stdout=subprocess.DEVNULL,
        )
        if process.returncode != 0:
            raise SystemExit(process.returncode)
        if traced:
            record_dirty_ranges(image, trace_path)
            trace_path.unlink(missing_ok=True)
        else:
            image.with_suffix(image.suffix + ".dirty.json").unlink(missing_ok=True)
    save(state_path, new)


def main() -> None:
    if len(sys.argv) != 5 or sys.argv[1] not in {"record", "sync"}:
        raise SystemExit("usage: sync-ext2.py <record|sync> <stage> <image> <state>")
    mode, root, image, state_path = sys.argv[1], Path(sys.argv[2]), Path(sys.argv[3]), Path(sys.argv[4])
    if mode == "record":
        save(state_path, snapshot(root))
    else:
        sync(root, image, state_path)


if __name__ == "__main__":
    main()
