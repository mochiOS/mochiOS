#!/usr/bin/env python3
"""Check producer ordering, job forwarding and fail-fast build orchestration."""
import importlib.util
from pathlib import Path
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('workspace_entry', ROOT / 'scripts/dev/build-workspace.py')
entry = importlib.util.module_from_spec(spec)
spec.loader.exec_module(entry)


class WorkspaceBuildTests(unittest.TestCase):
    def test_lock_precedes_build_and_jobs_are_forwarded(self):
        calls = []
        def run(command, **kwargs):
            calls.append((command, kwargs))
        entry.build(ROOT, 3, run)
        self.assertEqual(len(calls), 2)
        self.assertEqual(calls[0][0][2:], ['workspace', 'lock', '--file', str(ROOT / 'mdp.workspace.toml')])
        self.assertEqual(calls[1][0][2:], ['workspace', 'build', '--file', str(ROOT / 'mdp.workspace.toml'), '-j', '3'])
        for command, kwargs in calls:
            self.assertEqual(Path(command[1]), ROOT / 'tools/mdp/mdp')
            self.assertEqual(kwargs, dict(cwd=ROOT, check=True))

    def test_failure_does_not_continue_to_later_phase(self):
        for failed in (1, 2):
            calls = []
            def run(command, **kwargs):
                calls.append(command)
                if len(calls) == failed:
                    raise subprocess.CalledProcessError(1, command)
            with self.assertRaises(subprocess.CalledProcessError):
                entry.build(ROOT, 2, run)
            self.assertEqual(len(calls), failed)

    def test_invalid_job_count_never_updates_lock(self):
        def unexpected(*args, **kwargs):
            self.fail('invalid jobs started a command')
        with self.assertRaises(ValueError):
            entry.build(ROOT, 0, unexpected)

    def test_image_consumers_wait_for_producer_stage(self):
        rules = (ROOT / 'build/mmake/image.mk').read_text()
        for target in ('rootfs-stage', 'installer-rootfs-stage'):
            self.assertIn(target + ': config mdp-workspace-build', rules)
        result = subprocess.run(['mmake', '-n', '-j', '4', 'build'], cwd=ROOT, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertLess(result.stdout.index('build-workspace.py'), result.stdout.index('build/mdp/image.py'))
        self.assertEqual(result.stdout.count('build-workspace.py'), 1)


if __name__ == '__main__':
    unittest.main()
