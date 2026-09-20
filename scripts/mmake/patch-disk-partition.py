#!/usr/bin/env python3
import json
import os
import sys
from pathlib import Path

BLOCK_SIZE = 4096
COMPARE_SIZE = 4 * 1024 * 1024


def identity(path: Path) -> dict[str, int]:
    value = path.stat()
    return {"device": value.st_dev, "inode": value.st_ino, "size": value.st_size, "mtime_ns": value.st_mtime_ns}


def load_state(path: Path) -> dict[str, object] | None:
    try:
        value = json.loads(path.read_text())
    except (FileNotFoundError, json.JSONDecodeError, OSError):
        return None
    return value if isinstance(value, dict) else None


def dirty_ranges(path: Path, current: dict[str, int]) -> list[tuple[int, int]] | None:
    value = load_state(path)
    if value is None or any(value.get(key) != current[key] for key in current):
        return None
    ranges = value.get("ranges")
    if not isinstance(ranges, list):
        return None
    result: list[tuple[int, int]] = []
    for item in ranges:
        if not isinstance(item, list) or len(item) != 2:
            return None
        start, end = item
        if not isinstance(start, int) or not isinstance(end, int) or start < 0 or end < start:
            return None
        result.append((start, end))
    return result


def save_state(path: Path, value: dict[str, object]) -> None:
    temporary = path.with_suffix(path.suffix + ".new")
    temporary.write_text(json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n")
    os.replace(temporary, path)


def data_ranges(source: int, size: int, scan_all: bool):
    if scan_all or not hasattr(os, "SEEK_DATA"):
        yield 0, size
        return
    cursor = 0
    while cursor < size:
        try:
            start = os.lseek(source, cursor, os.SEEK_DATA)
        except OSError:
            return
        if start >= size:
            return
        try:
            end = os.lseek(source, start, os.SEEK_HOLE)
        except OSError:
            end = size
        start = start // BLOCK_SIZE * BLOCK_SIZE
        end = min(size, (end + BLOCK_SIZE - 1) // BLOCK_SIZE * BLOCK_SIZE)
        yield start, end
        cursor = max(end, cursor + BLOCK_SIZE)


def patch_range(source: int, destination: int, destination_offset: int, start: int, end: int) -> None:
    cursor = start
    while cursor < end:
        length = min(COMPARE_SIZE, end - cursor)
        source_data = os.pread(source, length, cursor)
        if len(source_data) < length:
            source_data += bytes(length - len(source_data))
        destination_data = os.pread(destination, length, destination_offset + cursor)
        if len(destination_data) != length:
            raise SystemExit("short read from destination disk image")
        if source_data != destination_data:
            run_start = None
            for offset in range(0, length, BLOCK_SIZE):
                block_end = min(offset + BLOCK_SIZE, length)
                different = source_data[offset:block_end] != destination_data[offset:block_end]
                if different and run_start is None:
                    run_start = offset
                if run_start is not None and (not different or block_end == length):
                    run_end = offset if not different else block_end
                    data = source_data[run_start:run_end]
                    written = os.pwrite(destination, data, destination_offset + cursor + run_start)
                    if written != len(data):
                        raise SystemExit("short write to destination disk image")
                    run_start = None
        cursor += length


def main() -> None:
    if len(sys.argv) != 7:
        raise SystemExit("usage: patch-disk-partition.py <source> <disk> <offset-mib> <size-mib> <state> <dirty-state>")
    source_path = Path(sys.argv[1])
    destination_path = Path(sys.argv[2])
    offset = int(sys.argv[3]) * 1024 * 1024
    size = int(sys.argv[4]) * 1024 * 1024
    state_path = Path(sys.argv[5])
    dirty_path = Path(sys.argv[6])
    source_identity = identity(source_path)
    destination_identity = identity(destination_path)
    destination_backing = {
        "device": destination_identity["device"],
        "inode": destination_identity["inode"],
        "size": destination_identity["size"],
    }
    current_state = {
        "version": 2,
        "source": source_identity,
        "destination": destination_backing,
        "offset": offset,
        "size": size,
    }
    previous = load_state(state_path)
    if previous == current_state:
        return
    if source_identity["size"] > size:
        raise SystemExit(f"partition image is larger than its destination: {source_path}")

    previous_source = previous.get("source") if isinstance(previous, dict) else None
    previous_destination = previous.get("destination") if isinstance(previous, dict) else None
    same_source_backing = isinstance(previous_source, dict) and all(
        previous_source.get(key) == source_identity[key]
        for key in ("device", "inode", "size")
    )
    same_destination_backing = (
        previous_destination == destination_backing
        and previous.get("offset") == offset
        and previous.get("size") == size
    )
    ranges = (
        dirty_ranges(dirty_path, source_identity)
        if same_source_backing and same_destination_backing
        else None
    )
    with source_path.open("rb", buffering=0) as source_file, destination_path.open("r+b", buffering=0) as destination_file:
        selected_ranges = ranges if ranges is not None else data_ranges(source_file.fileno(), size, True)
        for start, end in selected_ranges:
            patch_range(source_file.fileno(), destination_file.fileno(), offset, start, end)
    save_state(state_path, current_state)
    dirty_path.unlink(missing_ok=True)


if __name__ == "__main__":
    main()
