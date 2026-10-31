#!/usr/bin/env python3
"""Regression tests for the UI architecture guard's rejection paths."""

import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location(
    "ui_boundaries", Path(__file__).resolve().parents[1] / "dev/check-ui-boundaries.py"
)
guard = importlib.util.module_from_spec(spec)
spec.loader.exec_module(guard)


class BoundaryTests(unittest.TestCase):
    def test_elevation_divergence_is_detected_without_cross_repo_source_imports(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for relative in ("libraries/viewkit", "tools/devkit/crates/appcore",
                             "services/compositor"):
                project = root / relative
                (project / "src").mkdir(parents=True)
                (project / "Cargo.toml").write_text('[dependencies]\n')
            data = root / "libraries/viewkit/src/theme/window_elevation.rs"
            data.parent.mkdir()
            data.write_text("pub const ALPHA: u8 = 24;\n")
            compositor = root / "services/compositor/src/window_elevation.rs"
            compositor.write_bytes(data.read_bytes())
            self.assertEqual(guard.audit(root), [])
            compositor.write_text("pub const ALPHA: u8 = 25;\n")
            self.assertEqual(guard.audit(root),
                             ["ViewKit/compositor window elevation copies differ"])

    def test_external_source_import_is_rejected_but_internal_backend_is_allowed(self):
        with tempfile.TemporaryDirectory() as temporary:
            project = Path(temporary) / "libraries/viewkit"
            (project / "src").mkdir(parents=True)
            source = project / "src/lib.rs"
            source.write_text('#[path = "backend.rs"]\nmod backend;\n')
            self.assertEqual(guard.source_issues(project), [])
            source.write_text('#[path = "../../../applications/binder/src/main.rs"]\nmod shell;\n')
            self.assertEqual(len(guard.source_issues(project)), 1)
            source.write_text('include!("../../../applications/binder/src/main.rs");\n')
            self.assertEqual(len(guard.source_issues(project)), 1)

    def test_framework_cannot_depend_on_app_service_or_appcore(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            project = root / "libraries/viewkit"
            project.mkdir(parents=True)
            manifest = project / "Cargo.toml"
            manifest.write_text(
                '[dependencies]\nabi = {\n path = "../../user/crates/abi",\n}\n'
            )
            self.assertEqual(guard.dependency_issues(project, root, viewkit=True), [])
            manifest.write_text(
                '[dependencies]\n'
                'app = { path = "../../applications/binder" }\n'
                'service = { path = "../../services/compositor" }\n'
                'alias = { package = "mochios-appcore", version = "0.1" }\n'
            )
            self.assertEqual(len(guard.dependency_issues(project, root, viewkit=True)), 3)


if __name__ == "__main__":
    unittest.main()
