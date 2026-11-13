#!/usr/bin/env python3
"""Resolve image application sets; Store apps never become built-in packages."""
import argparse
from pathlib import Path
import re
import tomllib


def catalog(root):
    data = tomllib.loads((root / 'build/application-sets.toml').read_text())
    if data.get('format') != 1:
        raise ValueError('Unsupported application set format')
    seen = set()
    packages = set()
    for group in ('system', 'installer'):
        if not isinstance(data.get(group), list) or not data[group]:
            raise ValueError('Missing image application set')
        for app in data[group]:
            if set(app) != {'source', 'bundle', 'binary', 'package', 'target'}:
                raise ValueError('Incomplete image application entry')
            if any(not isinstance(value, str) or not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9._-]*', value)
                   for value in app.values()):
                raise ValueError('Invalid image application name')
            if not app['bundle'].endswith('.app') or app['bundle'] in seen:
                raise ValueError('Duplicate or invalid application bundle')
            if app['package'] in packages:
                raise ValueError('Duplicate built-in application package')
            packages.add(app['package'])
            seen.add(app['bundle'])
    for group in ('appstore', 'development'):
        for bundle in data['distribution'][group]:
            if not isinstance(bundle, str) or not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9._-]*\.app', bundle) or bundle in seen:
                raise ValueError('Overlapping application distribution sets')
            seen.add(bundle)
    return data


def plan(root, variant):
    if variant not in ('system', 'installer'):
        raise ValueError('Unknown image variant')
    data = catalog(root)
    apps = data['system'] + (data['installer'] if variant == 'installer' else [])
    for app in apps:
        source = root / 'applications' / app['source']
        manifest = tomllib.loads((source / 'manifest.toml').read_text())
        expected = '/system/applications/' + app['bundle'] + '/entry.elf'
        if manifest['package']['kind'] != 'application' or manifest['application']['entry'] != 'entry.elf' or not any(
            binary.get('path') == expected for binary in manifest.get('binary', [])
        ):
            raise ValueError('Application manifest does not match its image placement: ' + app['bundle'])
    return apps


def makefile(root):
    data = catalog(root)
    lines = ['# Generated from build/application-sets.toml by scripts/mmake/image-applications.py --makefile.']
    for group in ('system', 'installer'):
        targets = [app['target'] for app in data[group]]
        lines.append(group + '-apps: ' + ' '.join(targets))
        lines.append(group + '-applications-inputs: ' + ' '.join(target + '-inputs' for target in targets))
    return '\n'.join(lines) + '\n'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, required=True)
    parser.add_argument('--variant', choices=('system', 'installer'), default='system')
    parser.add_argument('--makefile', action='store_true')
    args = parser.parse_args()
    if args.makefile:
        print(makefile(args.root), end='')
    else:
        generated = args.root / 'build/mmake/application-sets.mk'
        if not generated.is_file() or generated.read_text() != makefile(args.root):
            raise ValueError('Application build targets are stale; regenerate build/mmake/application-sets.mk with --makefile')
        for app in plan(args.root, args.variant):
            print('\t'.join(app[key] for key in ('source', 'bundle', 'binary', 'package')))


if __name__ == '__main__':
    main()
