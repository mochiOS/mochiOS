#!/usr/bin/env python3
"""Test the build orchestration with explicit compiler/download test doubles."""
import contextlib
import importlib.util
import io
import json
from pathlib import Path
import shlex
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('devtools', ROOT / 'scripts/mmake/build-development-tools.py')
devtools = importlib.util.module_from_spec(spec)
spec.loader.exec_module(devtools)


class DevelopmentToolsTest(unittest.TestCase):
    def write(self, path, content='fixture'):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content)

    def fixture(self, root):
        builder = devtools.Builder(root, 2, ['fake-cargo', 'CARGO_BUILD_JOBS=8'])
        self.write(root / '.config', 'USER_INSTALL_DEVELOPER_TOOLS=y\nUSER_DEVELOPER_TOOLS_JOBS=2\n')
        source = builder.output / 'src' / f'llvm-project-{devtools.VERSION}.src'
        for name in ['llvm', 'clang', 'lld', 'runtimes']:
            (source / name).mkdir(parents=True)
        devtools.write_json(source / '.mochios-source.json', {'sha256': devtools.SHA256})
        for directory in ['out/newlib-port/sdk/lib', 'out/newlib-port/sdk/sysroot/include',
                          'user/libc-port/include', 'user/crates', 'user/targets', 'core/crates/abi',
                          'services/mboot-protocol', 'libraries/libc/src',
                          'tools/devkit/crates/msign', 'tools/devkit/crates/linker-driver',
                          'tools/devkit/crates/file-publication']:
            self.write(root / directory / 'input', directory)
        for name in ['build-llvm-runtimes.sh', 'configure-llvm-target.sh']:
            self.write(root / 'scripts' / name)
        for name in ['libcxx-18-newlib-regex-masks.patch', 'libcxxabi-18-pthread-thread-atexit.patch',
                     'llvm-18-mochios-path.patch', 'clang-18-mochios-toolchain.patch']:
            self.write(root / 'scripts/llvm/patches' / name)
        for name in ['user/runtime/crt0.S', 'user/runtime/linker.ld', 'tools/devkit/Cargo.toml', 'tools/devkit/Cargo.lock',
                     'user/Cargo.toml', 'user/Cargo.lock', 'core/Cargo.toml', 'services/Cargo.toml',
                     'libraries/libc/Cargo.toml', 'libraries/libc/build.rs', 'build/rust-std-toolchain',
                     'out/mmake/fingerprints/rust.sha256', 'out/rust-std/sysroot-overlay/.ready',
                     'out/rust-std/sysroot-overlay/bin/rustc', 'out/rust-std/sysroot-overlay/bin/cargo']:
            self.write(root / name)
        compiler = root / 'fake-host-tool'
        self.write(compiler)
        calls = []

        def run(command):
            command = list(map(str, command))
            calls.append(command)
            if command[:2] == ['cmake', '--build']:
                directory = Path(command[2])
                if directory.name == 'build-host-tools':
                    for name in ['llvm-tblgen', 'clang-tblgen']:
                        self.write(directory / 'bin' / name)
                else:
                    for name in ['bin/clang-18', 'bin/lld', 'lib/clang/18/include/stddef.h']:
                        self.write(directory / name)
            elif command[0] == 'bash' and command[1].endswith('build-llvm-runtimes.sh'):
                directory = Path(command[3])
                for name in ['lib/libc++.a', 'lib/libc++abi.a', 'lib/libunwind.a', 'include/c++/v1/vector']:
                    self.write(directory / name)
            elif command[0] == 'fake-cargo':
                for name in ['ld', 'msign']:
                    self.write(root / 'out/rust-std/target/x86_64-unknown-mochios/release' / name)
        builder.run = run
        return builder, compiler, calls

    def test_stage_order_incremental_build_and_missing_output_repair(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            builder, compiler, calls = self.fixture(root)
            with patch.object(devtools.shutil, 'which', return_value=str(compiler)), patch.dict(
                    devtools.os.environ, {'MOCHIOS_LLD': str(compiler)}):
                builder.build()
                self.assertEqual([Path(c[2]).name for c in calls if c[:2] == ['cmake', '--build']],
                                 ['build-host-tools', 'build-target'])
                native = calls[-1]
                self.assertIn('--no-default-features', native)
                self.assertIn('CARGO_BUILD_JOBS=2', native)
                self.assertIn('clang-resource-headers', calls[-2])
                self.assertTrue(builder.output.joinpath('state/llvm.json').is_file())
                calls.clear()
                builder.jobs = 3
                builder.build()
                self.assertEqual(calls, [], 'job-count changes must not rebuild')
                (builder.output / 'build-target/bin/clang-18').unlink()
                builder.build()
                self.assertEqual(len(calls), 2, 'repair LLVM without rebuilding TableGen or Rust tools')
                calls.clear()
                self.write(root / 'out/newlib-port/sdk/lib/input', 'new SDK runtime')
                builder.build()
                self.assertEqual(len(calls), 4, 'SDK update rebuilds runtime, LLVM and Rust tools only')
                self.assertFalse(any('build-host-tools' in value for c in calls for value in c))

    def test_disabled_does_not_initialize_builder_or_download(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.write(root / '.config', 'USER_INSTALL_DEVELOPER_TOOLS=n\n')
            with patch.object(devtools, 'Builder', side_effect=AssertionError('must not build')), patch.object(
                    sys, 'argv', ['build-development-tools.py', '--root', str(root)]):
                devtools.main()
                receipt = root / 'out/mmake/components/development-tools.json'
                before = receipt.stat().st_mtime_ns
                devtools.main()
                self.assertEqual(before, receipt.stat().st_mtime_ns)
                self.assertEqual(json.loads(receipt.read_text()), {'enabled': False, 'outputs': []})

    def test_failed_build_never_leaves_success_receipt(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            builder = devtools.Builder(root, 2, [])
            product = root / 'clang'
            self.write(product)
            builder.stage('test', [], [product], lambda: None)
            product.unlink()

            def fail():
                raise subprocess.CalledProcessError(1, ['fake-compiler'])
            with self.assertRaises(subprocess.CalledProcessError):
                builder.stage('test', [], [product], fail)
            self.assertFalse((builder.output / 'state/test.json').exists())

    def test_empty_source_files_are_valid_but_empty_tools_are_not(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / 'empty.rs'
            self.write(path, '')
            self.assertEqual(devtools.identity(path)[3], 0)
            with self.assertRaisesRegex(ValueError, 'Empty development tool output'):
                devtools.outputs([path])

    def test_atomic_lockfile_replacement_with_same_bytes_keeps_cache_valid(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            builder = devtools.Builder(root, 2, [])
            lock, product = root / 'Cargo.lock', root / 'msign'
            self.write(lock, 'version=4\n')
            self.write(product, 'native tool fixture')
            calls = []
            builder.stage('native', [lock], [product], lambda: calls.append('build'))
            self.write(root / 'Cargo.lock.new', lock.read_text())
            (root / 'Cargo.lock.new').replace(lock)
            builder.stage('native', [lock], [product], lambda: calls.append('build'))
            self.assertEqual(calls, ['build'])
            self.write(lock, 'version=4\n[[package]]\nname="changed"\nversion="2"\n')
            builder.stage('native', [lock], [product], lambda: calls.append('build'))
            self.assertEqual(calls, ['build', 'build'])

    def test_bad_archive_and_unmanaged_sources_are_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            builder = devtools.Builder(root, 2, [])
            self.write(builder.output / devtools.ARCHIVE, 'not an official archive')
            with self.assertRaisesRegex(ValueError, 'checksum mismatch'):
                builder.source()
            source = builder.output / 'src' / f'llvm-project-{devtools.VERSION}.src'
            self.write(source / 'do-not-delete', 'user source')
            with self.assertRaisesRegex(ValueError, 'Refusing to replace'):
                builder.source()
            self.assertEqual((source / 'do-not-delete').read_text(), 'user source')

    def test_config_schema_validation_and_image_dependency(self):
        self.assertIn('development-tools config', (ROOT / 'build/mmake/image.mk').read_text())
        for enabled in ('y', 'n'):
            with tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                self.write(root / 'input', f'USER_INSTALL_DEVELOPER_TOOLS={enabled}\nUSER_DEVELOPER_TOOLS_JOBS=3\n')
                command = ['perl', str(ROOT / 'scripts/config/merge-config.pl'), '--default',
                           str(ROOT / 'build/defaults.config'), '--in', str(root / 'input'), '--out', str(root / '.config')]
                subprocess.run(command, check=True, stdout=subprocess.DEVNULL)
                self.assertEqual(devtools.configuration(root / '.config'), (enabled == 'y', 3))
                self.write(root / 'input', 'USER_DEVELOPER_TOOLS_JOBS=0\n')
                result = subprocess.run(command, capture_output=True)
                self.assertNotEqual(result.returncode, 0)

    @unittest.skipUnless(shutil.which('mmake'), 'mmake is required for the build graph check')
    def test_disabled_mmake_gate_does_not_restage_image(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.write(root / '.config', 'USER_INSTALL_DEVELOPER_TOOLS=n\n')
            gate = shlex.join([sys.executable, str(ROOT / 'scripts/mmake/build-development-tools.py'), '--root', str(root)])
            self.write(root / 'Makefile', 'development-tools:\n\t@always\n\t@watch .config\n'
                       '\t@output out/mmake/components/development-tools.json\n\t' + gate + '\n'
                       'rootfs-stage: development-tools\n\t@watch out/mmake/components/development-tools.json\n'
                       '\t@output rootfs.ready\n\ttouch rootfs.ready\n')
            command = ['mmake', 'rootfs-stage']
            subprocess.run(command, cwd=root, check=True, capture_output=True)
            stamp = (root / 'rootfs.ready').stat().st_mtime_ns
            subprocess.run(command, cwd=root, check=True, capture_output=True)
            self.assertEqual(stamp, (root / 'rootfs.ready').stat().st_mtime_ns)
            self.write(root / '.config', 'USER_INSTALL_DEVELOPER_TOOLS=y\n')
            result = subprocess.run(command, cwd=root, capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(stamp, (root / 'rootfs.ready').stat().st_mtime_ns)


if __name__ == '__main__':
    with contextlib.redirect_stdout(io.StringIO()):
        unittest.main()
