#!/usr/bin/env python3
"""Install checkout-backed host CLI entry points without pip or network access."""
import argparse
import os
from pathlib import Path
import sys


def install(root, bin_dir):
    """Install only managed symlinks, refusing to overwrite unrelated commands."""
    if sys.version_info < (3, 11):
        raise ValueError('Python 3.11 or newer is required')
    commands = {name: root / 'tools' / name / name for name in ('mdp', 'mdev')}
    for name, source in commands.items():
        if not source.is_file() or not os.access(source, os.X_OK):
            raise ValueError(f'{source} is missing or not executable; sync the workspace projects first')
        destination = bin_dir / name
        if os.path.lexists(destination):
            if not destination.is_symlink() or destination.resolve() != source.resolve():
                raise ValueError(f'refusing to replace another command: {destination}')
    bin_dir.mkdir(parents=True, exist_ok=True)
    for name, source in commands.items():
        destination = bin_dir / name
        if destination.is_symlink():
            print(f'already installed: {destination}')
            continue
        # link() has no overwrite semantics, so concurrent installers cannot clobber commands.
        try:
            destination.symlink_to(source.resolve())
        except FileExistsError:
            if not destination.is_symlink() or destination.resolve() != source.resolve():
                raise ValueError(f'command appeared during installation: {destination}') from None
        print(f'installed: {destination}')
    if str(bin_dir) not in os.environ.get('PATH', '').split(os.pathsep):
        print(f'Add this directory to PATH: {bin_dir}')


def main():
    """Bootstrap local tools; SDK acquisition remains an explicit MDP operation."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--bin-dir', type=Path, default=Path.home() / '.local' / 'bin')
    args = parser.parse_args()
    try:
        install(Path(__file__).resolve().parents[2], args.bin_dir.expanduser().absolute())
        return 0
    except (ValueError, OSError) as error:
        print(f'bootstrap: {error}', file=sys.stderr)
        return 1


if __name__ == '__main__':
    raise SystemExit(main())
