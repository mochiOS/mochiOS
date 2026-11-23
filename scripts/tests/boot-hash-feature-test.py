#!/usr/bin/env python3
"""Keep freestanding SHA policy from disabling native CPU dispatch workspace-wide."""
from pathlib import Path
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[2]


class HashFeatureBoundaryTests(unittest.TestCase):
    def test_shared_image_format_does_not_force_a_cpu_backend(self):
        library = tomllib.loads((ROOT / 'boot/crates/system-image/Cargo.toml').read_text())
        features = library['dependencies']['sha2'].get('features', [])
        self.assertNotIn('force-soft', features)
        self.assertNotIn('force-soft-compact', features)

    def test_uefi_executable_preserves_software_backend(self):
        executable = tomllib.loads((ROOT / 'boot/Cargo.toml').read_text())
        self.assertIn('force-soft', executable['dependencies']['sha2']['features'])


if __name__ == '__main__':
    unittest.main()
