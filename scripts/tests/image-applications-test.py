#!/usr/bin/env python3
"""Exercise real image staging with fixture payloads and an explicit mock signer."""
import importlib.util
from pathlib import Path
import shutil
import subprocess
import tempfile
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('image_applications', ROOT / 'scripts/mmake/image-applications.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
SYSTEM = {'Binder.app', 'Settings.app', 'Files.app', 'Terminal.app', 'AppStore.app',
          'DiskUtility.app', 'Viewer.app', 'Fonts.app', 'SystemMonitor.app', 'Keychain.app'}
STORE = {'Terra.app', 'Edit.app', 'Calendar.app', 'Mail.app', 'Calculator.app'}


class DistributionTests(unittest.TestCase):
    def test_exact_sets_and_generated_build_targets(self):
        data = module.catalog(ROOT)
        self.assertEqual({app['bundle'] for app in module.plan(ROOT, 'system')}, SYSTEM)
        self.assertEqual({app['bundle'] for app in module.plan(ROOT, 'installer')}, SYSTEM | {'Installer.app'})
        self.assertEqual(set(data['distribution']['appstore']), STORE)
        self.assertEqual(data['distribution']['development'], ['test.app'])
        self.assertEqual((ROOT / 'build/mmake/application-sets.mk').read_text(), module.makefile(ROOT))
        generated = module.makefile(ROOT)
        for target in ['edit', 'mail', 'calculator', 'calendar', 'test-app', 'terra']:
            self.assertNotIn(target, generated.splitlines()[1].split()[1:])
        self.assertIn('rootfs-stage: system-programs ', (ROOT / 'build/mmake/image.mk').read_text())
        self.assertIn('system-programs: services system-apps ', (ROOT / 'build/mmake/binaries.mk').read_text())

    def fixture(self, folder):
        root = folder / 'root'; output = folder / 'out'; root.mkdir()
        def write(name, data=b'fixture\n'):
            path = root / name; path.parent.mkdir(parents=True, exist_ok=True); path.write_bytes(data); return path
        # Only image sources are present: Store/development repositories need not
        # be checked out or compiled to create either distribution image.
        for app in module.plan(ROOT, 'installer'):
            source = root / 'applications' / app['source']; source.parent.mkdir(exist_ok=True)
            source.symlink_to(ROOT / 'applications' / app['source'], target_is_directory=True)
        for name in ['services', 'user', 'binaries/msh', 'binaries/coreutils']:
            destination = root / name; destination.parent.mkdir(parents=True, exist_ok=True)
            destination.symlink_to(ROOT / name, target_is_directory=True)
        for name in ['image-applications.py', 'stage-application-resources.py', 'stage-system-libraries.py']:
            source = ROOT / 'scripts' / ('dev' if name == 'stage-application-resources.py' else 'mmake') / name
            destination = root / 'scripts' / ('dev' if name == 'stage-application-resources.py' else 'mmake') / name
            destination.parent.mkdir(parents=True, exist_ok=True); shutil.copyfile(source, destination)
        write('build/application-sets.toml', (ROOT / 'build/application-sets.toml').read_bytes())
        write('build/mmake/application-sets.mk', module.makefile(ROOT).encode())
        write('build/system-libraries.toml', b'format = 1\ncomponents = []\n')
        write('build/ime/ja.mime.sha256')
        write('resources/system/users/users.db')
        (root / 'resources/libraries').mkdir(parents=True)
        write('libraries/fonts/out/fonts/fixture.ttf')
        write('out/newlib-port/hello/hello.elf')
        write('.config', b'USER_INSTALL_DEVELOPER_TOOLS=n\nUSER_BUILD_SELFTESTS=n\n')
        binaries = {'rust-std-demo', 'msh', 'sudo', 'echo', 'ls', 'pwd', 'true', 'false', 'cat', 'touch', 'rm', 'id',
                    'useradd', 'userdel', 'userlist', 'mpk', 'net', 'gcc', 'os-test', 'test_gui', 'test_desktop'}
        binaries |= {app['binary'] for app in module.plan(ROOT, 'installer')}
        # Match real service staging, without duplicating its service list.
        import re
        for row in re.findall(r'^stage_service (.+)$', (ROOT / 'scripts/mmake/stage-rootfs.sh').read_text(), re.M):
            binaries.add(row.split()[1])
        for binary in binaries:
            write('out/rust-std/target/x86_64-unknown-mochios/release/' + binary)
        (output / 'components/ime').mkdir(parents=True); (output / 'components/ime/ja.mime').write_bytes(b'fixture')
        signer = write('mock-msign', b'''#!/usr/bin/env python3
# Test double only: this creates no valid production signature or trust record.
import pathlib,shutil,sys,tomllib
args=sys.argv[1:]; out=pathlib.Path(args[args.index('--output')+1])
out.parent.mkdir(parents=True,exist_ok=True)
if args[1]=='bind-built-in':
 source=pathlib.Path(args[2]); data=tomllib.loads(source.read_text())
 if data['package'].get('kind')=='application':
  root=pathlib.Path(args[args.index('--rootfs')+1])
  for binary in data.get('binary',[]): assert (root/binary['path'].lstrip('/')).is_file()
 shutil.copyfile(source,out)
elif args[1]=='built-in-record': out.write_bytes(b'TEST DOUBLE - NOT A SIGNATURE')
else: raise SystemExit('unexpected signer operation')
'''); signer.chmod(0o755)
        return root, output, signer

    def stage(self, root, output, signer, variant='system', success=True):
        result = subprocess.run(['bash', str(ROOT / 'scripts/mmake/stage-rootfs.sh'), str(root), str(output), str(signer), variant],
                                capture_output=True, text=True)
        if success:
            self.assertEqual(result.returncode, 0, result.stderr)
        else:
            self.assertNotEqual(result.returncode, 0)
        return output / ('installer/image/rootfs' if variant == 'installer' else 'image/rootfs')

    def test_real_stager_removes_old_bundles_and_separates_boot_markers_and_trust_records(self):
        with tempfile.TemporaryDirectory() as folder:
            root, output, signer = self.fixture(Path(folder))
            old = output / 'image/rootfs/system/applications/Edit.app'; old.mkdir(parents=True)
            (old / 'entry.elf').write_bytes(b'stale Store application')
            stage = self.stage(root, output, signer)
            self.assertEqual({p.name for p in (stage / 'system/applications').iterdir()}, SYSTEM)
            self.assertTrue((stage / 'system/.installed').is_file())
            for app in module.plan(ROOT, 'system'):
                record = tomllib.loads((stage / 'system/packages' / app['package'] / 'manifest.toml').read_text())
                source = tomllib.loads((ROOT / 'applications' / app['source'] / 'manifest.toml').read_text())
                self.assertEqual(record['package']['id'], source['package']['id'], app['bundle'])
            self.assertFalse((stage / 'system/bin/test_app').exists())
            for package in ['edit', 'mail', 'calendar', 'calculator', 'installer', 'viewkit-test', 'terra']:
                self.assertFalse((stage / 'system/packages' / package).exists(), package)
            before = (stage / '.ready').stat().st_mtime_ns
            installer = self.stage(root, output, signer, 'installer')
            self.assertEqual({p.name for p in (installer / 'system/applications').iterdir()}, SYSTEM | {'Installer.app'})
            self.assertFalse((installer / 'system/.installed').exists())
            self.assertTrue((installer / 'system/packages/installer/verification.bin').is_file())
            self.assertEqual((stage / '.ready').stat().st_mtime_ns, before)
            self.assertTrue((stage / 'system/.installed').exists())
            self.assertEqual((installer / 'system/applications/Installer.app/legal/terms.txt').read_bytes(),
                             (ROOT / 'applications/installer/legal/terms.txt').read_bytes())

    def test_unknown_variant_and_missing_binary_cannot_replace_a_good_stage(self):
        with tempfile.TemporaryDirectory() as folder:
            root, output, signer = self.fixture(Path(folder))
            stage = self.stage(root, output, signer)
            sentinel = stage / 'preserve'; sentinel.write_bytes(b'previous-good-image')
            self.stage(root, output, signer, 'typo', success=False)
            self.assertTrue(sentinel.is_file())
            (root / 'out/rust-std/target/x86_64-unknown-mochios/release/binder').unlink()
            self.stage(root, output, signer, success=False)
            self.assertEqual(sentinel.read_bytes(), b'previous-good-image')

    def test_stale_build_targets_cannot_stage_an_unbuilt_application_set(self):
        with tempfile.TemporaryDirectory() as folder:
            root, output, signer = self.fixture(Path(folder))
            (root / 'build/mmake/application-sets.mk').write_text('system-apps: edit\n')
            self.stage(root, output, signer, success=False)
            self.assertFalse((output / 'image/rootfs/.ready').exists())

    def test_overlapping_catalog_is_rejected(self):
        with tempfile.TemporaryDirectory() as folder:
            root, _, _ = self.fixture(Path(folder))
            path = root / 'build/application-sets.toml'
            path.write_text(path.read_text().replace('"Terra.app",', '"Binder.app",'))
            with self.assertRaises(ValueError):
                module.plan(root, 'system')


if __name__ == '__main__':
    unittest.main()
