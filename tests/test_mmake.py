from pathlib import Path
import re
import subprocess
import tempfile
import unittest


class BuildGraphTests(unittest.TestCase):
    def test_kvm_entry_reuses_image_but_runs_tests_every_time(self):
        definitions = Path(__file__).resolve().parents[1] / "build/mmake/tests.mk"
        headers = re.findall(r"^([A-Za-z0-9_.-]+):([^\n]*)", definitions.read_text(), re.MULTILINE)
        provided = {name for name, _ in headers} | {"image"}
        external = sorted({dependency for _, dependencies in headers
                           for dependency in dependencies.split()} - provided)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "scripts/testing").mkdir(parents=True)
            (root / "tests").mkdir()
            (root / "source").write_text("unchanged input")
            (root / "scripts/testing/__init__.py").write_text("")
            (root / "tests/catalog.toml").write_text("")
            (root / "tests/test_testing.py").write_text("")
            (root / "counter.py").write_text(
                "from pathlib import Path\n"
                "p=Path('image.built')\n"
                "n=int(p.read_text()) if p.exists() else 0\n"
                "p.write_text(str(n+1))\n"
            )
            (root / "scripts/test.py").write_text(
                "import sys\nfrom pathlib import Path\n"
                "assert sys.argv[1:] == ['kvm'], sys.argv\n"
                "with Path('test.calls').open('a') as out: out.write('called\\n')\n"
            )
            (root / "Makefile").write_text(
                f"ROOT := {root}\nSCRIPTS := {root / 'scripts'}\n"
                f"MMAKE_OUT := {root / 'out'}\nOUT := {root / 'out'}\n"
                "CARGO_PATCHES :=\nMSIGN := unused\n"
                f"include {definitions}\n"
                "image:\n\t@watch source\n\t@output image.built\n\tpython3 counter.py\n"
                + "".join(name + ":\n" for name in external)
            )
            for _ in range(2):
                completed = subprocess.run(
                    ["mmake", "test"], cwd=root, capture_output=True, text=True
                )
                self.assertEqual(completed.returncode, 0, completed.stdout + completed.stderr)
            self.assertEqual((root / "image.built").read_text(), "1")
            self.assertEqual((root / "test.calls").read_text().splitlines(), ["called", "called"])


if __name__ == "__main__":
    unittest.main()
