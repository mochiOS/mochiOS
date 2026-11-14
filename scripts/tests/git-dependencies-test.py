#!/usr/bin/env python3
"""Check repository portability and the matching OS integration overrides."""
import importlib.util
import json
from pathlib import Path
import shlex
import sys
import tempfile
import tomllib
import unittest

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('git_dependencies', ROOT / 'scripts/dev/check-git-dependencies.py')
audit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(audit)


class GitDependenciesTest(unittest.TestCase):
    def test_owned_manifests_are_repository_local_or_git(self):
        count, errors = audit.check(ROOT)
        self.assertGreater(count, 100)
        self.assertEqual(errors, [])

    def test_multiline_inline_tables_internal_paths_and_external_patches(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest = root / 'Cargo.toml'
            manifest.write_text('[dependencies]\nimage = {\n version="1",\n}\n'
                                'internal={path="crates/internal"}\nexternal={path="../sibling"}\n'
                                '[target.\'cfg(target_os="mochios")\'.dependencies]\n'
                                'protocol={path="../user/protocol"}\n'
                                '[patch."https://github.com/mochiOS/syscalls"]\n'
                                'client={path="../user/client"}\n'
                                '# ignored={path="../comment"}\n'
                                '[lib]\npath="../not-a-dependency.rs"\n')
            errors = audit.external_paths(root, manifest)
            self.assertEqual(len(errors), 3)
            self.assertTrue(all('internal' not in message for message in errors))

    def test_exported_sdk_and_os_cargo_overrides_match(self):
        config = tomllib.loads((ROOT / '.cargo/config.toml').read_text())
        explicit = {}
        for line in (ROOT / 'build/mmake/toolchain.mk').read_text().splitlines():
            if line.startswith('CARGO_PATCHES ') and (' := ' in line or ' += ' in line):
                args = shlex.split(line.split('= ', 1)[1].replace('$(ROOT)', str(ROOT)))
                self.assertEqual(len(args) % 2, 0)
                for key in args[1::2]:
                    # Cargo accepts the same dotted TOML fields as --config.
                    data = tomllib.loads(key)
                    for source, packages in data['patch'].items():
                        explicit.setdefault(source, {}).update(packages)
        for source, packages in config['patch'].items():
            for name, item in packages.items():
                if 'path' in item:
                    self.assertEqual(explicit[source][name]['path'], str(ROOT / item['path']))
        self.assertIn('mochios-appcore', explicit['https://github.com/mochiOS/devkit'])
        self.assertIn('mochios-keychain-protocol', explicit['https://github.com/mochiOS/syscalls'])


if __name__ == '__main__':
    unittest.main()
