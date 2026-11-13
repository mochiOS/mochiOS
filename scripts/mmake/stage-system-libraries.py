#!/usr/bin/env python3
"""Stage pinned components before System sealing; never write a running System."""
import argparse
import hashlib
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import tomllib


def digest(path):
    """Hash a regular input without reading a whole library into memory."""
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def target_path(value):
    """Limit this build input to System libraries and their license notices."""
    if not isinstance(value, str) or not value.startswith(('/system/libraries/', '/system/share/licenses/')):
        raise ValueError('Component target must be a System library or license')
    parts = value.split('/')[1:]
    if any(not p or p in ('.', '..') for p in parts) or any(ord(c) < 32 or c == '\\' for c in value):
        raise ValueError('Invalid System component path')
    return PurePosixPath(value)


def stage_components(root, catalog, stage, msign):
    """Validate all pinned payloads, then use the existing built-in binding path."""
    root = root.resolve(); stage = stage.resolve()
    config = tomllib.loads(catalog.read_text())
    if config.get('format') != 1 or not isinstance(config.get('components'), list):
        raise ValueError('Unsupported System component catalog')
    plans = []; targets = set(); packages = set()
    for item in config['components']:
        name = item['package']; relative = Path(item['source'])
        if not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9._-]*', name) or name in packages:
            raise ValueError('Invalid or duplicate component identity')
        if relative.is_absolute() or '..' in relative.parts:
            raise ValueError('Component input must be workspace-relative')
        source = (root / relative).resolve(strict=True)
        if not source.is_relative_to(root) or stage.is_relative_to(source):
            raise ValueError('Component input escapes workspace or contains output')
        manifest = source / 'manifest.toml'
        if manifest.is_symlink(): raise ValueError('Symlink System component manifest')
        text = manifest.read_bytes()
        if hashlib.sha256(text).hexdigest() != item['manifest_sha256']:
            raise ValueError('System component manifest digest mismatch')
        data = tomllib.loads(text.decode('utf-8'))
        if data.get('format') != 1 or data['package']['id'] != name or data['package'].get('provenance') != 'built-in' or data['package'].get('kind') != 'binary' or data['package'].get('architecture') != 'x86_64' or data['package'].get('abi') != 'mochios-1':
            raise ValueError('System component identity/ABI mismatch')
        package = stage / 'system/packages' / name
        if package.exists() or package.is_symlink():
            raise ValueError('System component package already exists')
        files = data.get('file', [])
        if not files: raise ValueError('Empty System component')
        bindings = {}; pending = []
        for record in files:
            target = target_path(record['path']); destination = stage / str(target).lstrip('/')
            if record['id'] in bindings or target in targets or destination.exists() or destination.is_symlink():
                raise ValueError('Duplicate or existing System component file')
            for parent in destination.parents:
                if parent == stage: break
                if parent.is_symlink(): raise ValueError('Symlink in System output')
            payload = source / 'payload/root' / str(target).lstrip('/')
            if payload.resolve(strict=True) != payload or not payload.is_file():
                raise ValueError('Component payload must be a regular contained file')
            expected = record['digest'].removeprefix('sha256:')
            if record['digest'] != 'sha256:' + expected or not re.fullmatch('[0-9a-f]{64}', expected) or record['size'] != payload.stat().st_size or digest(payload) != expected:
                raise ValueError('System component payload digest/size mismatch')
            mode = record['mode']
            if mode not in ('0644', '0444', '0755', '0555'):
                raise ValueError('Unsupported System component mode')
            bindings[record['id']] = record; targets.add(target)
            pending.append((payload, destination, expected, int(mode, 8)))
        for binary in data.get('binary', []):
            record = bindings.get(binary.get('file'))
            if record is None or binary['path'] != record['path'] or binary.get('kind') not in ('library', 'application'):
                raise ValueError('Unbound System component binary')
        packages.add(name); plans.append((package, text, pending))
    for package, text, files in plans:
        for source, destination, expected, mode in files:
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, destination); destination.chmod(mode)
            if digest(destination) != expected: raise ValueError('Component changed while copying')
        package.mkdir(parents=True)
        captured = package / 'source.toml'; captured.write_bytes(text)
        subprocess.run([str(msign), 'package', 'bind-built-in', str(captured), '--rootfs', str(stage), '--output', str(package / 'manifest.toml')], check=True)
        subprocess.run([str(msign), 'package', 'built-in-record', str(package / 'manifest.toml'), '--output', str(package / 'verification.bin')], check=True)
        captured.unlink(); (package / 'verification.bin').chmod(0o644)


def main():
    """Consume only explicit pinned inputs; download/build is owned by producers."""
    parser = argparse.ArgumentParser(description=__doc__)
    for flag in ('root', 'catalog', 'stage', 'msign'): parser.add_argument('--' + flag, type=Path, required=True)
    args = parser.parse_args()
    stage_components(args.root, args.catalog, args.stage, args.msign)

if __name__ == '__main__': main()
