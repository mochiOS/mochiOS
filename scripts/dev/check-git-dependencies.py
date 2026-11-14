#!/usr/bin/env python3
"""Reject Cargo dependency paths that escape the repository owning a manifest.

Repository-internal workspace dependencies remain portable. Development-tree
overrides belong in the OS checkout's .cargo/config.toml or build commands.
"""
import argparse
from pathlib import Path
import re
import subprocess


def external_paths(repository, manifest):
    """Find escaping dependency/patch paths without rewriting TOML 1.1 tables."""
    repository = repository.resolve()
    section = ''
    result = []
    for number, line in enumerate(manifest.read_text().splitlines(), 1):
        code = line.split('#', 1)[0].strip()
        if code.startswith('['):
            section = code.strip('[]')
        dependency = re.search(r'(^|\.)(dependencies|dev-dependencies|build-dependencies)(\.|$)', section)
        if not dependency and not section.startswith('patch.'):
            continue
        for match in re.finditer(r'\bpath\s*=\s*["\']([^"\']+)["\']', code):
            target = (manifest.parent / match[1]).resolve()
            if not target.is_relative_to(repository):
                result.append(f'{manifest}:{number}: dependency path escapes its repository: {match[1]}')
    return result


def check(root):
    """Audit owned repositories; upstream Rust/newlib workspace paths are internal."""
    projects = [root]
    projects += [root / path for path in (root / '.repo/project.list').read_text().splitlines()
                 if path not in ('libraries/rust', 'libraries/newlib') and (root / path).is_dir()]
    errors = []
    count = 0
    for project in projects:
        tracked = subprocess.check_output(['git', '-C', str(project), 'ls-files', 'Cargo.toml', '*Cargo.toml'], text=True)
        for name in tracked.splitlines():
            manifest = project / name
            if manifest.is_file():
                count += 1
                errors.extend(external_paths(project, manifest))
    return count, errors


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[2])
    args = parser.parse_args()
    count, errors = check(args.root.resolve())
    if errors:
        raise SystemExit('\n'.join(errors))
    print(f'Checked {count} Cargo manifests: no cross-repository path dependencies')
