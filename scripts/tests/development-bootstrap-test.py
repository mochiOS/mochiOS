#!/usr/bin/env python3
"""Bootstrap installs only expected local command links."""
import importlib.util
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
import uuid

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('bootstrap', ROOT / 'scripts/dev/bootstrap.py')
bootstrap = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bootstrap)


class BootstrapTests(unittest.TestCase):
    def test_install_idempotence_and_commands(self):
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp) / 'bin'
            bootstrap.install(ROOT, directory)
            bootstrap.install(ROOT, directory)
            for name in ('mdp', 'mdev'):
                result = subprocess.run([str(directory / name), '--help'], capture_output=True, text=True)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertTrue((directory / name).is_symlink())

    def test_refuse_foreign_command_before_any_install(self):
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp)
            (directory / 'mdev').write_text('unrelated command')
            with self.assertRaises(ValueError):
                bootstrap.install(ROOT, directory)
            self.assertFalse((directory / 'mdp').exists())
            self.assertEqual((directory / 'mdev').read_text(), 'unrelated command')

    def test_refuse_foreign_symlink(self):
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp)
            (directory / 'mdp').symlink_to('/does-not-exist')
            with self.assertRaises(ValueError):
                bootstrap.install(ROOT, directory)

    def test_run_wrapper_preserves_configured_backend(self):
        """mmake's launcher must retain a VM's explicit settings on later runs."""
        md = ROOT / 'tools/mdev/mdev'
        name = 'wrapper-check-' + uuid.uuid4().hex[:12]
        store = ROOT / 'out/develop'
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp)
            image = directory / 'base.img'
            image.write_bytes(b'base')
            backend = directory / 'backend'
            backend.write_text('#!/bin/sh\ntest "$EXAMPLE_SETTING" = preserved\n')
            backend.chmod(0o755)
            command = [str(md), '--directory', str(store)]
            subprocess.run([*command, 'create', name, '--image', str(image), '--runner', str(backend),
                            '--env', 'EXAMPLE_SETTING=preserved'], check=True, capture_output=True)
            try:
                result = subprocess.run(['bash', str(ROOT / 'scripts/dev/run-vm.sh')], cwd=ROOT,
                                        env=dict(os.environ, MDEV_NAME=name), capture_output=True, text=True)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            finally:
                subprocess.run([*command, 'remove', name, '--yes'], check=True, capture_output=True)


if __name__ == '__main__':
    unittest.main()
