#!/usr/bin/env python3
"""Keep generated built-in manifests compatible with native readers and TOML."""
import importlib.util
from pathlib import Path
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('image_builder', ROOT / 'build/mdp/image.py')
image = importlib.util.module_from_spec(spec)
spec.loader.exec_module(image)


class ManifestSerializationTests(unittest.TestCase):
    def test_native_catalog_fields_are_bare_and_round_trip(self):
        data = {'package': {'name': 'Files', 'id': 'org.mochios.files'},
                'application': {'entry': 'entry.elf'},
                'binary': [{'file': 'entry', 'path': '/system/applications/Files.app/entry.elf'}]}
        text = image.toml_text(data)
        self.assertIn('[package]\nname = "Files"', text)
        self.assertIn('[application]\nentry = "entry.elf"', text)
        self.assertEqual(tomllib.loads(text), data)

    def test_non_bare_keys_and_nested_capabilities_round_trip(self):
        data = {'capabilities': {'process.inspect': True, 'space key': ['a', 'b']},
                'table.dot': {'quoted"key': 'value'}, 'number': 3}
        self.assertEqual(tomllib.loads(image.toml_text(data)), data)


if __name__ == '__main__':
    unittest.main()
