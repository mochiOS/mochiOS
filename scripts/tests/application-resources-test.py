#!/usr/bin/env python3
"""Regression tests for shared native/preview bundle resource staging."""
import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location(
    "resources", Path(__file__).resolve().parents[1] / "dev/stage-application-resources.py")
resources = importlib.util.module_from_spec(spec)
spec.loader.exec_module(resources)

class ResourcesTests(unittest.TestCase):
    def test_declared_nested_resources_are_copied_as_regular_files(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source, target = root / "source", root / "bundle"
            (source / "resources/icons").mkdir(parents=True)
            (source / "manifest.toml").write_text('[application]\nresources = ["icons/type.svg"]\n')
            (source / "resources/icons/type.svg").write_text("svg")
            resources.stage(source, target)
            self.assertEqual((target / "icons/type.svg").read_text(), "svg")
            self.assertFalse((target / "icons/type.svg").is_symlink())

    def test_rejects_traversal_and_external_source_links(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "source"
            source.mkdir()
            (root / "external.svg").write_text("external")
            (source / "link.svg").symlink_to(root / "external.svg")
            for resource in ("../external.svg", "/tmp/external.svg", "link.svg"):
                (source / "manifest.toml").write_text(f'[application]\nresources = ["{resource}"]\n')
                with self.assertRaises(ValueError):
                    resources.stage(source, root / "bundle")

    def test_rejects_external_destination_directory(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source, target, external = root / "source", root / "bundle", root / "external"
            (source / "icons").mkdir(parents=True)
            target.mkdir()
            external.mkdir()
            (source / "manifest.toml").write_text('[application]\nresources = ["icons/type.svg"]\n')
            (source / "icons/type.svg").write_text("svg")
            (target / "icons").symlink_to(external, target_is_directory=True)
            with self.assertRaises(ValueError):
                resources.stage(source, target)
            self.assertFalse((external / "type.svg").exists())

if __name__ == "__main__":
    unittest.main()
