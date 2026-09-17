#!/usr/bin/env python3
import hashlib
import json
import os
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
        process = subprocess.run(
            ["debugfs", "-w", "-f", "-", str(image)],
            input="\n".join(commands) + "\n",
            text=True,
            stdout=subprocess.DEVNULL,
        )
        if process.returncode != 0:
            raise SystemExit(process.returncode)
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
