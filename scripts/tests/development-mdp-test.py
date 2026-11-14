#!/usr/bin/env python3
"""Exercise a real producer-to-mmake composition without OS builds or a VM."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
MDP = ROOT / 'tools/mdp/mdp'


class CompositionTests(unittest.TestCase):
    def test_project_to_root_composition_and_tamper_rejection(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / 'mdp.toml').write_text('''format = 1
[package]
name = "example"
version = "1"
target = "test-target"
abi = "test-abi"
[build]
command = ["python3", "-c", "from pathlib import Path; Path('Example.mpkg').write_bytes(b'opaque test package')"]
[[files]]
source = "Example.mpkg"
area = "system"
destination = "applications/Example.mpkg"
''')
            for argv in (['lock'], ['sync'], ['build'], ['pack', '--output', 'example.tar.mdp']):
                subprocess.run([str(MDP), *argv], cwd=root, check=True, capture_output=True)
            (root / 'mdp.assembly.toml').write_text('''format = 1
target = "test-target"
abi = "test-abi"
[[artifacts]]
path = "example.tar.mdp"
''')
            subprocess.run([str(MDP), 'lock-assembly'], cwd=root, check=True, capture_output=True)
            # An assembly must not invoke the producer, even if its sources are gone.
            (root / 'mdp.toml').unlink()
            (root / 'Example.mpkg').unlink()
            output = root / 'assembled'
            command = ['mmake', '-v', 'mdp-assemble']
            env = dict(os.environ, MDP_ASSEMBLY_LOCK=str(root / 'mdp.assembly.lock'), MDP_OUTPUT=str(output))
            result = subprocess.run(command, cwd=ROOT, env=env, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertEqual((output / 'tree/system/applications/Example.mpkg').read_bytes(), b'opaque test package')
            self.assertTrue((output / 'receipt.json').is_file())
            with (root / 'example.tar.mdp').open('ab') as stream:
                stream.write(b'tampered')
            env['MDP_OUTPUT'] = str(root / 'bad-output')
            result = subprocess.run(command, cwd=ROOT, env=env, capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('hash mismatch', result.stdout + result.stderr)
            self.assertFalse((root / 'bad-output').exists())


if __name__ == '__main__':
    unittest.main()
