#!/usr/bin/env python3
"""Exercise the real image target graph with changing producer artifacts."""
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class ImageCachePropagationTests(unittest.TestCase):
    def test_new_composition_reaches_image_and_identical_inputs_remain_cached(self):
        with tempfile.TemporaryDirectory(prefix='image-cache-') as directory:
            root = Path(directory)
            def write(path, text):
                destination = root / path
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_text(text)
            write('build/mmake/image.mk', (ROOT / 'build/mmake/image.mk').read_text())
            for path in ['build/mdp/policy.toml', 'build/application-sets.toml', 'scripts/mmake/development-users.db', 'scripts/mmake/sync-ext2.py', 'resources/system/base', 'resources/libraries/base']:
                write(path, '')
            write('build/mdp/image.py', "from pathlib import Path\np=Path('out/build/os/image/rootfs/payload');p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(Path('mdp.assembly.lock').read_bytes())\n")
            write('scripts/mmake/make-ext2-image.sh', '#!/bin/sh\nset -eu\ncp "$1/payload" "$2"\n')
            write('producer.json', 'first artifact')
            write('Makefile', f'''ROOT := {root}
OUT := {root}/out
MMAKE_OUT := {root}/out/build/os
SCRIPTS := {root}/scripts
include build/mmake/image.mk
config:
\t@always
\ttrue
mdp-workspace-build: config
\t@always
\tcp producer.json mdp.assembly.lock
''')
            (root / 'scripts/mmake/make-ext2-image.sh').chmod(0o755)
            def build():
                result = subprocess.run(['mmake', '-v', 'rootfs'], cwd=root, capture_output=True, text=True)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                return result.stdout
            build()
            image = root / 'out/build/os/image/rootfs.img'
            self.assertEqual(image.read_text(), 'first artifact')
            write('producer.json', 'second artifact')
            build()
            self.assertEqual(image.read_text(), 'second artifact')
            output = build()
            self.assertIn('[skip] rootfs', output)
            write('resources/system/new-resource', 'changed product data')
            output = build()
            self.assertNotIn('[skip] rootfs\n', output)


if __name__ == '__main__':
    unittest.main()
