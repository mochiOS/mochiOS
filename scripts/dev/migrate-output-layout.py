#!/usr/bin/env python3
"""Move legacy workspace outputs without copying data or discarding VM state."""
import argparse
import fcntl
import json
from pathlib import Path

# Paths are workspace-owned outputs, never another repository's producer output.
MOVES = {
    'artifacts': 'images/system', 'installer-artifacts': 'images/installer',
    'releases': 'images/releases', 'mmake': 'build/os',
    'preview': 'build/preview', 'host': 'build/host', 'mstudio': 'build/mstudio',
    'mdp': 'cache/mdp', 'rust-std': 'cache/rust', 'newlib-port': 'cache/newlib',
    'llvm-port': 'cache/llvm', 'vim-port': 'cache/vim', 'cexts': 'cache/cexts',
    'services-build': 'cache/services', 'system-libraries': 'cache/system-libraries',
    'tests': 'reports/tests', 'metrics': 'reports/metrics', 'checks': 'reports/checks', 'host-tests': 'reports/host-tests',
    'mdev-validation': 'reports/mdev-validation', 'mdp-migration': 'reports/mdp-migration',
    'native-webkit-validation': 'reports/native-webkit-validation',
}
# Existing compilers/SDKs and historical QCOW backing paths may embed these paths.
# New builds use canonical paths. Keep these aliases until old caches/VMs retire.
ALIASES = {'artifacts', 'installer-artifacts', 'mmake', 'mdp', 'rust-std', 'newlib-port', 'llvm-port', 'vim-port',
           'preview', 'system-libraries', 'native-webkit-validation'}


def migrate(root):
    """Preflight every move, lock managed VMs, then rename outputs in place."""
    root = Path(root).resolve()
    output = root / 'out'
    output.mkdir(exist_ok=True)
    with (output / '.layout.lock').open('a') as guard:
        fcntl.flock(guard, fcntl.LOCK_EX | fcntl.LOCK_NB)
        locks = []
        try:
            descriptors = []
            for config in sorted((output / 'develop').glob('*/vm.json')):
                if config.is_symlink():raise ValueError(f'VM descriptor is a symlink: {config}')
                lock = (config.parent / '.lock').open('a')
                locks.append(lock)
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
                data = json.loads(config.read_text())
                if not isinstance(data.get('image'), str) or not Path(data['image']).is_absolute():
                    raise ValueError(f'Invalid VM source-image path: {config}')
                descriptors.append((config, data))
            for old, new in MOVES.items():
                source, target = output / old, output / new
                if source.is_symlink():
                    if source.resolve() != target.resolve():raise ValueError(f'Unexpected output alias: {source}')
                elif source.exists():
                    if not source.is_dir():raise ValueError(f'Expected an output directory: {source}')
                    if target.exists() or target.is_symlink():raise ValueError(f'Both output paths exist: {source}, {target}; no files moved')
            for name in ('images', 'build', 'cache', 'sdk', 'develop', 'reports', 'tmp'):
                (output / name).mkdir(exist_ok=True)
            for old, new in MOVES.items():
                source, target = output / old, output / new
                if source.exists() and not source.is_symlink():
                    target.parent.mkdir(parents=True, exist_ok=True)
                    source.rename(target)
                    print(f'{source} -> {target}')
                if old in ALIASES and target.exists() and not source.is_symlink():
                    source.symlink_to(new, target_is_directory=True)
            # Change only the source location, preserving private disks and fingerprints.
            for config, data in descriptors:
                image = Path(data['image'])
                try:relative = image.resolve().relative_to(output)
                except ValueError:continue
                if relative.parts and relative.parts[0] in (*MOVES, 'images'):
                    relocated = output / MOVES.get(relative.parts[0], relative.parts[0]) / Path(*relative.parts[1:])
                    if not relocated.is_file():raise ValueError(f'Relocated VM source missing: {relocated}')
                    data['image'] = str(relocated)
                    temporary = config.with_suffix('.json.layout-new')
                    with temporary.open('x') as stream:
                        json.dump(data, stream, indent=2);stream.write('\n')
                    temporary.chmod(config.stat().st_mode & 0o777)
                    temporary.replace(config)
            report = {'format': 1, 'moves': MOVES, 'compatibility_aliases': sorted(name for name in ALIASES if (output / name).is_symlink())}
            destination = output / 'reports/output-layout.json'
            destination.write_text(json.dumps(report, indent=2) + '\n')
        finally:
            for lock in locks:lock.close()


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[2])
    args = parser.parse_args()
    try:migrate(args.root)
    except (OSError, ValueError, KeyError) as error:parser.exit(1, f'Output migration refused: {error}\n')
