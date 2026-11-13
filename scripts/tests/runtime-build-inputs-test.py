#!/usr/bin/env python3
"""Keep SDK input narrowing safe as native runtime dependencies evolve."""
from pathlib import Path
import re
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[2]


class RuntimeBuildInputsTest(unittest.TestCase):
    def test_local_runtime_dependency_closure_is_watched(self):
        manifests = {}
        for path in (ROOT / 'user/crates').glob('*/Cargo.toml'):
            data = tomllib.loads(path.read_text())
            manifests[data['package']['name']] = (path, data)
        visited = set()

        def visit(name):
            if name in visited or name not in manifests:
                return
            visited.add(name)
            path, data = manifests[name]
            sections = [data]
            sections.extend(data.get('target', {}).values())
            for section in sections:
                for key in ('dependencies', 'build-dependencies'):
                    for alias, value in section.get(key, {}).items():
                        visit(value.get('package', alias) if isinstance(value, dict) else alias)

        visit('mochi-user-newlib-runtime')
        text = (ROOT / 'build/mmake/inputs.mk').read_text()
        sdk_rule = text.split('runtime-sdk-inputs:', 1)[1].split('\nruntime-inputs:', 1)[0]
        for name in visited:
            directory = manifests[name][0].parent.relative_to(ROOT).as_posix()
            self.assertIn('@watch ' + directory + '/**', sdk_rule, name)
        self.assertNotIn('@watch user/crates/**', sdk_rule)

    def test_native_consumers_keep_rust_dependency_tracking(self):
        rules = {}
        consumers = []
        for path in (ROOT / 'build/mmake').glob('*.mk'):
            target = None
            for line in path.read_text().splitlines():
                if match := re.match(r'^([\w-]+):\s*(.*)$', line):
                    target = match[1]
                    rules[target] = match[2].split()
                elif target and line.startswith('\t') and '$(MOCHIOS_CARGO)' in line:
                    consumers.append(target)

        def depends(target, required, seen=None):
            seen = set() if seen is None else seen
            if target == required:
                return True
            if target in seen:
                return False
            seen.add(target)
            return any(depends(item, required, seen) for item in rules.get(target, []))

        self.assertGreater(len(consumers), 30)
        for target in consumers:
            self.assertTrue(depends(target, 'runtime-inputs'), target)
        self.assertTrue(depends('runtime', 'runtime-sdk-inputs'))
        self.assertFalse(depends('runtime', 'runtime-inputs'))


if __name__ == '__main__':
    unittest.main()
