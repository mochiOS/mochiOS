"""System image inputs must be pinned and cannot escape their staging namespace."""
from pathlib import Path
import hashlib
import importlib.util
import tempfile
import unittest
from unittest.mock import patch
ROOT=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('system_libraries',ROOT/'scripts/mmake/stage-system-libraries.py')
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
class SystemLibraries(unittest.TestCase):
    def fixture(self, root, target='/system/libraries/example.so', provenance='built-in'):
        product=root/'input';source=product/'payload/root'/target.lstrip('/');source.parent.mkdir(parents=True);source.write_bytes(b'fixture library bytes')
        text=f'''format=1
[package]
id="org.example.library"
name="Example"
kind="binary"
provenance="{provenance}"
architecture="x86_64"
abi="mochios-1"
[[file]]
id="library"
path="{target}"
digest="sha256:{module.digest(source)}"
size={source.stat().st_size}
mode="0644"
[[binary]]
path="{target}"
kind="library"
file="library"
requires=[]
'''
        (product/'manifest.toml').write_text(text)
        catalog=root/'catalog.toml';catalog.write_text(f'''format=1
[[components]]
package="org.example.library"
source="input"
manifest_sha256="{module.digest(product/'manifest.toml')}"
''')
        return product,source,catalog
    def test_layout_and_existing_binding_commands(self):
        with tempfile.TemporaryDirectory() as t:
            root=Path(t);product,source,catalog=self.fixture(root);stage=root/'stage';calls=[]
            def signer(args,check):
                calls.append(args);output=Path(args[args.index('--output')+1]);output.write_bytes(Path(args[3]).read_bytes() if args[2]=='bind-built-in' else b'fixture record, not a signature')
            with patch.object(module.subprocess,'run',side_effect=signer):module.stage_components(root,catalog,stage,Path('/fixture/msign'))
            self.assertEqual((stage/'system/libraries/example.so').read_bytes(),source.read_bytes())
            self.assertEqual([c[2] for c in calls],['bind-built-in','built-in-record'])
            self.assertFalse((stage/'system/packages/org.example.library/source.toml').exists())
    def test_tampered_payload_fails_before_copy_or_binding(self):
        with tempfile.TemporaryDirectory() as t:
            root=Path(t);product,source,catalog=self.fixture(root);source.write_bytes(b'tampered')
            with patch.object(module.subprocess,'run') as signer,self.assertRaisesRegex(ValueError,'digest/size'):module.stage_components(root,catalog,root/'stage',Path('msign'))
            signer.assert_not_called();self.assertFalse((root/'stage').exists())
    def test_changed_manifest_and_non_builtin_inputs_fail_closed(self):
        with tempfile.TemporaryDirectory() as t:
            root=Path(t);product,source,catalog=self.fixture(root,provenance='verified-package')
            with self.assertRaisesRegex(ValueError,'identity/ABI'):module.stage_components(root,catalog,root/'stage',Path('msign'))
            (product/'manifest.toml').write_text((product/'manifest.toml').read_text()+'\n# changed\n')
            with self.assertRaisesRegex(ValueError,'manifest digest'):module.stage_components(root,catalog,root/'stage',Path('msign'))
    def test_other_system_paths_are_rejected(self):
        with tempfile.TemporaryDirectory() as t:
            root=Path(t);product,source,catalog=self.fixture(root,target='/system/services/foreign')
            with self.assertRaisesRegex(ValueError,'System library or license'):module.stage_components(root,catalog,root/'stage',Path('msign'))
        for target in ['/system/libraries/../services/foreign','/system/libraries//alias','/system/libraries/a\\b']:
            with self.assertRaises(ValueError):module.target_path(target)
    def test_payload_symlinks_and_existing_targets_are_rejected(self):
        with tempfile.TemporaryDirectory() as t:
            root=Path(t);product,source,catalog=self.fixture(root);content=source.read_bytes();source.unlink();outside=root/'outside';outside.write_bytes(content);source.symlink_to(outside)
            with self.assertRaisesRegex(ValueError,'regular contained'):module.stage_components(root,catalog,root/'stage',Path('msign'))
            source.unlink();source.write_bytes(content);stage=root/'stage';destination=stage/'system/libraries/example.so';destination.parent.mkdir(parents=True);destination.write_bytes(b'existing')
            with self.assertRaisesRegex(ValueError,'existing'):module.stage_components(root,catalog,stage,Path('msign'))
            self.assertEqual(destination.read_bytes(),b'existing')
