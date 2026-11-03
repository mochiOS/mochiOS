"""Discover native development applications from their own manifests."""
import pathlib
import tomllib


def applications(root):
    """Return source directories and declared binary names without an OS-owned app list."""
    result = []
    for source in sorted((pathlib.Path(root) / 'applications').iterdir()):
        cargo, manifest = source / 'Cargo.toml', source / 'manifest.toml'
        if not cargo.is_file() or not manifest.is_file():
            continue
        info, package = tomllib.loads(cargo.read_text()), tomllib.loads(manifest.read_text())
        if package.get('package', {}).get('kind') != 'application':
            continue
        binaries = info.get('bin', [])
        primary = [entry for entry in binaries if entry.get('name') == info['package']['name']]
        if not binaries and (source / 'src/main.rs').is_file():
            primary = [{'name': info['package']['name']}]
        if not primary and len(binaries) == 1:
            primary = binaries
        if len(primary) != 1 or not primary[0].get('name'):
            raise ValueError(f'Preview application has no unambiguous primary binary: {source}')
        result.append((source.name, primary[0]['name']))
    return result


if __name__ == '__main__':
    import argparse
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('root', type=pathlib.Path)
    parser.add_argument('--pairs', action='store_true', help='Print source directory and binary separated by a tab')
    parser.add_argument('--directories', action='store_true', help='Print source directories')
    args = parser.parse_args()
    for directory, binary in applications(args.root):
        # Existing preview launchers use package aliases for these two legacy names.
        if args.pairs:
            print(directory + '\t' + binary)
        elif args.directories:
            print(directory)
        else:
            print('test-app' if binary == 'test_app' else binary)
