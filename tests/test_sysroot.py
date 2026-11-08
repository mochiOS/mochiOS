from pathlib import Path
import tempfile
import unittest


class SysrootTests(unittest.TestCase):
    def test_sysroot_preparation_keeps_compiler_mtime_until_inputs_change(self):
        import os
        import subprocess
        script = Path(__file__).resolve().parents[1] / "scripts/mmake/prepare-rust-sysroot.sh"
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            base = root / "base"
            for name in ["base/bin", "base/lib/rustlib/src", "build", "command",
                         "libraries/rust/library/std", "libraries/rust/src/mochios-backtrace",
                         "libraries/rust/src/mochios-libunwind", "libraries/rust/vendor/rustc-literal-escaper"]:
                (root / name).mkdir(parents=True, exist_ok=True)
            for tool in ("rustc", "rustdoc"):
                (base / "bin" / tool).write_text("compiler")
            (root / "build/rust-std-toolchain").write_text("fixture")
            source = root / "libraries/rust/library/std/lib.rs"
            source.write_text("first")
            command = root / "command/rustc"
            command.write_text('#!/bin/sh\nprintf "%s\\n" "$FIXTURE_SYSROOT"\n')
            command.chmod(0o700)
            env = dict(os.environ, PATH=str(command.parent) + os.pathsep + os.environ["PATH"],
                       FIXTURE_SYSROOT=str(base))
            def prepare():
                subprocess.run(["bash", str(script), str(root), str(root / "out/mmake")],
                               env=env, check=True, capture_output=True)
            prepare()
            wrapper = root / "out/rust-std/sysroot-overlay/bin/rustc"
            original = wrapper.stat().st_mtime_ns
            prepare()
            self.assertEqual(wrapper.stat().st_mtime_ns, original)
            source.write_text("changed through the existing link")
            prepare()
            self.assertEqual(wrapper.stat().st_mtime_ns, original)
            linked = wrapper.parents[1] / "lib/rustlib/src/rust/library/std/lib.rs"
            self.assertEqual(linked.read_text(), source.read_text())
            (source.parent / "new.rs").write_text("new layout")
            prepare()
            self.assertNotEqual(wrapper.stat().st_mtime_ns, original)
            refreshed = wrapper.stat().st_mtime_ns
            (base / "bin/rustc").write_text("updated compiler")
            prepare()
            self.assertNotEqual(wrapper.stat().st_mtime_ns, refreshed)



if __name__ == "__main__":
    unittest.main()
