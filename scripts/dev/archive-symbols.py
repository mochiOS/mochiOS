#!/usr/bin/env python3
"""Archive debug information by the SHA-256 of the exact deployed ELF."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import tempfile
import tomllib
import zlib


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def elf_info(path):
    size = path.stat().st_size
    with path.open('rb') as stream:
        head = stream.read(64)
        if len(head) != 64 or head[:6] != b'\x7fELF\x02\x01':
            raise ValueError(f'unsupported ELF: {path}')
        kind, machine = struct.unpack_from('<HH', head, 16)
        if kind not in (2, 3) or machine != 62:
            raise ValueError(f'unsupported ELF type/architecture: {path}')

        def read(at, length):
            if at < 0 or length < 0 or at + length > size or length > 16 * 1024 * 1024:
                raise ValueError(f'invalid ELF span: {path}')
            stream.seek(at)
            value = stream.read(length)
            if len(value) != length:
                raise ValueError(f'truncated ELF: {path}')
            return value

        phoff, shoff = struct.unpack_from('<QQ', head, 32)
        phsize, phcount, shsize, shcount, names_index = struct.unpack_from('<HHHHH', head, 54)
        if phsize != 56 or shcount and (shsize != 64 or names_index >= shcount):
            raise ValueError(f'unsupported ELF tables: {path}')
        loads = []
        for offset in range(0, phcount * phsize, phsize):
            header = read(phoff + offset, phsize)
            tag, flags, _, address, _, _, memory, _ = struct.unpack('<IIQQQQQQ', header)
            if tag == 1:
                loads.append(dict(address=address, size=memory, flags=flags))
        sections = read(shoff, shcount * shsize) if shcount else b''
        names = b''
        if shcount:
            at, length = struct.unpack_from('<QQ', sections, names_index * shsize + 24)
            names = read(at, length)
        section_names = set()
        debuglink_crc = None
        for offset in range(0, len(sections), shsize or 64):
            name_at = struct.unpack_from('<I', sections, offset)[0]
            end = names.find(b'\0', name_at)
            if end < 0:
                raise ValueError(f'invalid section name: {path}')
            name = names[name_at:end]
            section_names.add(name)
            if name == b'.gnu_debuglink':
                at, length = struct.unpack_from('<QQ', sections, offset + 24)
                data = read(at, length)
                end = data.find(b'\0')
                crc_at = (end + 4) & ~3
                if end < 0 or crc_at + 4 > len(data):
                    raise ValueError(f'invalid debug link: {path}')
                debuglink_crc = struct.unpack_from('<I', data, crc_at)[0]
        return dict(elf_type=kind, architecture=machine, load_segments=loads,
                    has_line_information=b'.debug_line' in section_names,
                    has_symbols=b'.symtab' in section_names, debuglink_crc=debuglink_crc)


def atomic_json(path, value):
    temporary = path.with_suffix(path.suffix + '.new')
    with temporary.open('w') as stream:
        json.dump(value, stream, ensure_ascii=False, indent=2, sort_keys=True)
        stream.write('\n')
        stream.flush()
        os.fsync(stream.fileno())
    temporary.replace(path)


def archive_image(path, store, objcopy, debug_source=None):
    identifier = digest(path)
    info = elf_info(path)
    directory = store / 'objects' / identifier
    if directory.exists():
        saved = json.loads((directory / 'image.json').read_text())
        if saved['image_sha256'] != identifier or digest(directory / 'image.debug') != saved['debug_sha256']:
            raise ValueError(f'corrupt symbol object: {directory}')
        return saved
    directory.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='.symbols-', dir=directory.parent) as staging:
        staging = Path(staging)
        debug = staging / 'image.debug'
        if debug_source:
            crc = 0
            with debug_source.open('rb') as stream:
                for chunk in iter(lambda: stream.read(1024 * 1024), b''):
                    crc = zlib.crc32(chunk, crc)
            if info['debuglink_crc'] != crc:
                raise ValueError(f'debug link does not match: {path}, {debug_source}')
            shutil.copyfile(debug_source, debug)
        elif info['has_symbols'] or info['has_line_information']:
            subprocess.run([objcopy, '--only-keep-debug', str(path.resolve()), str(debug.resolve())], check=True)
        else:
            # Sectionless executables have no extractable debug sections.
            # Preserve the exact object and explicitly report unavailable data.
            shutil.copyfile(path, debug)
        debug_info = elf_info(debug)
        if debug_info['architecture'] != info['architecture'] or digest(path) != identifier:
            raise ValueError(f'image changed during symbol capture: {path}')
        info.update(schema_version=1, image_sha256=identifier, debug_sha256=digest(debug),
                    debug_file=f'objects/{identifier}/image.debug',
                    has_line_information=debug_info['has_line_information'], has_symbols=debug_info['has_symbols'])
        atomic_json(staging / 'image.json', info)
        staging.rename(directory)
    return info


def revisions(root):
    def git(directory, *args):
        return subprocess.check_output(['git', '-C', str(directory), *args], text=True).strip()
    result = {'.': dict(commit=git(root, 'rev-parse', 'HEAD'), dirty=bool(git(root, 'status', '--porcelain', '--untracked-files=normal', '--ignore-submodules=all')))}
    for entry in git(root, 'ls-tree', '-rz', 'HEAD').split('\0'):
        if not entry:
            continue
        fields, path = entry.split('\t', 1)
        if fields.startswith('160000 '):
            directory = root / path
            if directory.is_dir() and Path(git(directory, 'rev-parse', '--show-toplevel')).resolve() == directory.resolve():
                result[path] = dict(commit=git(directory, 'rev-parse', 'HEAD'),
                                    dirty=bool(git(directory, 'status', '--porcelain', '--untracked-files=normal', '--ignore-submodules=all')))
            else:
                result[path] = dict(commit=fields.split()[2], initialized=False)
    return result


def export_store(store, destination):
    if destination.is_symlink():
        raise ValueError('symbol export destination must not be a symlink')
    store, destination = store.resolve(), destination.resolve()
    if destination == store or destination in store.parents or store in destination.parents:
        raise ValueError('symbol export must be separate from the source store')
    destination.parent.mkdir(parents=True, exist_ok=True)
    marker = 'FORMAT'
    if destination.exists() and ((not destination.is_dir())
                                or not (destination / marker).is_file()
                                or (destination / marker).read_text() != 'mochios-symbol-export-v1\n'):
        raise ValueError('refusing to replace a directory not created by symbol export')
    index = json.loads((store / 'index.json').read_text())
    with tempfile.TemporaryDirectory(prefix='.symbol-export-', dir=destination.parent) as staging:
        staging = Path(staging)
        for image in index['images']:
            identifier = image['image_sha256']
            if len(identifier) != 64 or any(c not in '0123456789abcdef' for c in identifier):
                raise ValueError('invalid symbol image identity')
            source = store / 'objects' / identifier
            if digest(source / 'image.debug') != image['debug_sha256']:
                raise ValueError(f'corrupt debug image: {identifier}')
            target = staging / 'objects' / identifier
            target.mkdir(parents=True)
            for name in ('image.json', 'image.debug'):
                try:
                    os.link(source / name, target / name)
                except OSError:
                    shutil.copyfile(source / name, target / name)
        atomic_json(staging / 'index.json', index)
        (staging / marker).write_text('mochios-symbol-export-v1\n')
        with tempfile.TemporaryDirectory(prefix='.symbol-backup-', dir=destination.parent) as backup:
            previous = Path(backup) / 'previous'
            if destination.exists():
                destination.rename(previous)
            try:
                staging.rename(destination)
            except OSError:
                if previous.exists():
                    previous.rename(destination)
                raise
    print(f'{destination}: exported {len(index["images"])} symbol objects')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path.cwd())
    parser.add_argument('--rootfs', type=Path)
    parser.add_argument('--elf', type=Path, action='append', default=[])
    parser.add_argument('--kernel', type=Path)
    parser.add_argument('--kernel-debug', type=Path)
    parser.add_argument('--store', type=Path, required=True)
    parser.add_argument('--objcopy', default='objcopy')
    parser.add_argument('--export', type=Path)
    args = parser.parse_args()
    if args.export:
        export_store(args.store, args.export)
        return
    if args.kernel_debug and not args.kernel:
        parser.error('--kernel-debug requires --kernel')
    args.store.mkdir(parents=True, exist_ok=True)
    images = {}
    selected = [(path, str(path), None) for path in args.elf]
    if args.rootfs:
        for directory, directories, files in os.walk(args.rootfs, followlinks=False):
            directories[:] = sorted(name for name in directories if not (Path(directory) / name).is_symlink())
            for name in sorted(files):
                path = Path(directory) / name
                if path.is_symlink() or not path.is_file():
                    continue
                with path.open('rb') as stream:
                    head = stream.read(20)
                if len(head) == 20 and head[:6] == b'\x7fELF\x02\x01' \
                        and struct.unpack_from('<HH', head, 16) in ((2, 62), (3, 62)):
                    selected.append((path, '/' + path.relative_to(args.rootfs).as_posix(), None))
    if args.kernel:
        selected.append((args.kernel, '<kernel>', args.kernel_debug))
    for path, label, debug_source in selected:
        record = archive_image(path, args.store, args.objcopy, debug_source)
        identifier = record['image_sha256']
        if identifier not in images:
            images[identifier] = {**record, 'paths': []}
        if label not in images[identifier]['paths']:
            images[identifier]['paths'].append(label)
    if not images:
        raise ValueError('no ELF images selected')
    root = args.root.resolve()
    version = tomllib.loads((root / 'version.toml').read_text())
    index = dict(schema_version=1, os=version, revisions=revisions(root),
                 config_sha256=digest(root / '.config') if (root / '.config').is_file() else None,
                 images=[images[key] for key in sorted(images)])
    atomic_json(args.store / 'index.json', index)
    print(f'{args.store / "index.json"}: {len(images)} unique ELF images')


if __name__ == '__main__':
    try:
        main()
    except (ValueError, OSError, subprocess.CalledProcessError, KeyError) as error:
        raise SystemExit(f'archive-symbols: {error}')
