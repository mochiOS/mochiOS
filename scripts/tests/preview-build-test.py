#!/usr/bin/env python3
"""Preview builds use one Cargo feature graph and preserve unchanged inputs."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class PreviewBuildTests(unittest.TestCase):
    def test_desktop_check_invokes_cargo_once_with_all_binaries(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            scripts = root / "scripts/dev"
            scripts.mkdir(parents=True)
            for name in ("desktop-ui.sh", "preview-app.sh"):
                shutil.copy2(ROOT / "scripts/dev" / name, scripts / name)
            (scripts / "prepare-preview.py").write_text(
                "import pathlib,sys\n"
                "p=pathlib.Path(sys.argv[1])/'out/preview';p.mkdir(parents=True,exist_ok=True)\n"
                "(p/'Cargo.toml').write_text('[workspace]\\n')\n"
            )
            (root / "version.toml").write_text('release = "26.0.0"\nbuild = 1\n')
            binary = root / "bin"
            binary.mkdir()
            cargo = binary / "cargo"
            cargo.write_text(
                '#!/bin/bash\nprintf "CALL\\n%s\\n" "$*" >> "$CARGO_CALL_LOG"\n'
                'printf "%s %s\\n" "$CARGO_INCREMENTAL" "$CARGO_PROFILE_DEV_DEBUG" >> "$CARGO_CALL_LOG"\n'
            )
            cargo.chmod(0o755)
            log = root / "cargo.log"
            env = dict(os.environ, PATH=f"{binary}:{os.environ['PATH']}", CARGO_CALL_LOG=str(log))
            env.pop("CARGO_INCREMENTAL", None)
            env.pop("CARGO_PROFILE_DEV_DEBUG", None)
            subprocess.run(["bash", str(scripts / "desktop-ui.sh"), "--check"], env=env, check=True)
            calls = log.read_text()
            self.assertEqual(calls.count("CALL\n"), 1)
            self.assertIn("--workspace --bins", calls)
            self.assertIn("1 0\n", calls)

    def test_preparation_does_not_touch_identical_protocol_sources(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            apps = ("binder", "file", "settings", "edit", "terminal", "appstore", "system-monitor",
                    "viewer", "calendar", "calculator", "mail", "keychain", "disk-utility", "fonts",
                    "installer", "test.app")
            for name in apps:
                app = root / "applications" / name
                app.mkdir(parents=True)
                (app / "Cargo.toml").write_text(f'[workspace]\n[package]\nname="{name}"\nversion="0.1.0"\n')
            protocol = root / "applications/binder/crates/binder_protocol/src"
            protocol.mkdir(parents=True)
            source = protocol / "lib.rs"
            source.write_text("pub const VALUE: u32 = 1;\n")
            (root / "applications/binder/Cargo.lock").write_text("version = 4\n")
            (root / "resources").mkdir()
            for name in ("tools/desktop-ime", "services/keychain"):
                path = root / name
                path.mkdir(parents=True)
                (path / "src").mkdir()
                (path / "Cargo.toml").write_text('[package]\nname="test"\nversion="0.1.0"\n')
            command = ["python3", str(ROOT / "scripts/dev/prepare-preview.py"), str(root)]
            subprocess.run(command, check=True)
            target = root / "out/preview/applications/binder/crates/binder_protocol/src/lib.rs"
            before = target.stat().st_mtime_ns
            os.utime(source, ns=(source.stat().st_atime_ns, source.stat().st_mtime_ns + 10_000_000_000))
            subprocess.run(command, check=True)
            self.assertEqual(target.stat().st_mtime_ns, before)
            source.write_text("pub const VALUE: u32 = 2;\n")
            subprocess.run(command, check=True)
            self.assertEqual(target.read_text(), source.read_text())


if __name__ == "__main__":
    unittest.main()
