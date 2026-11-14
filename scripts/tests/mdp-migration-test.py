#!/usr/bin/env python3
"""Check all producer registrations and adversarial image-composition boundaries."""
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import struct
import subprocess
import sys
import tarfile
import tempfile
import tomllib
import unittest
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT/'tools/mdp/src'))
from mdp.workspace import read, order
from mdp.cli import lock_project, pack, digest
from mdp.pipeline import expand
spec=importlib.util.spec_from_file_location('image_composition', ROOT/'build/mdp/image.py')
image=importlib.util.module_from_spec(spec);spec.loader.exec_module(image)


class MigrationTests(unittest.TestCase):
    def test_every_application_and_system_producer_is_registered(self):
        root,data,projects=read(ROOT/'mdp.workspace.toml')
        names=order(projects)
        registered={p['path'] for p in projects.values()}
        for project in ET.parse(ROOT/'default.xml').iter('project'):
            self.assertIn(project.get('path'),registered)

        paths={p['source'].relative_to(root).as_posix() for name,p in projects.items() if name in names}
        apps=tomllib.loads((ROOT/'build/application-sets.toml').read_text())
        for group in ('system','installer'):
            for app in apps[group]:self.assertIn('applications/'+app['source'],paths)
        for path in ('core','boot','user','services','drivers','cexts','binaries/msh','binaries/coreutils','libraries/viewkit','libraries/fonts','libraries/rust','tools/devkit','build'):
            self.assertIn(path,paths)
        for path in (ROOT/'applications').iterdir():
            if (path/'manifest.toml').is_file() and (path/'Cargo.toml').is_file():
                self.assertIn('applications/'+path.name,paths)
        self.assertEqual(len(names),len(set(names)))

    def test_producer_shell_commands_expand_without_calling_root_build(self):
        _,data,projects=read(ROOT/'mdp.workspace.toml')
        bindings=json.loads((ROOT/'out/mdp/context.json').read_text())
        for project in projects.values():
            for recipe_path in project['source'].glob('mdp*.recipe.json'):
                recipe=json.loads(recipe_path.read_text())
                variables=dict(bindings,**recipe.get('bindings',{}),PROJECT=str(project['source']))
                for step in recipe.get('steps',[]):
                    command=expand(step,variables)
                    self.assertNotRegex(command,r'\bmmake\s+(build|image|runtime|viewkit|apps)\b')
                    result=subprocess.run(['bash','-n','-c',command],capture_output=True,text=True)
                    self.assertEqual(result.returncode,0,command+'\n'+result.stderr)

    def test_root_build_plan_never_builds_or_downloads_producers(self):
        result=subprocess.run(['mmake','-n','build'],cwd=ROOT,capture_output=True,text=True)
        self.assertEqual(result.returncode,0,result.stdout+result.stderr)
        self.assertNotRegex(result.stdout,r'\b(cargo|rustc|cmake|curl|wget)\b|mdp workspace build|build-newlib.sh')
        self.assertIn('build/mdp/image.py',result.stdout)

    def test_cargo_launcher_keeps_diagnostic_only_lock_changes(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);lock=root/'Cargo.lock';lock.write_text('version=4\n')
            command=[sys.executable,str(ROOT/'user/scripts/cargo-preserve-lock.py'),str(lock),'--',sys.executable,'-c',
                     "from pathlib import Path; p=Path("+repr(str(lock))+"); p.write_text(p.read_text()+'[[patch.unused]]\\nname=\"example\"\\nversion=\"1\"\\n')"]
            result=subprocess.run(command,capture_output=True,text=True)
            self.assertEqual(result.returncode,0,result.stderr)
            self.assertIn('patch.unused',lock.read_text())
            self.assertIn('retain and commit',result.stderr)

    def package(self,root,payload=b'valid',extra=False):
        elf=bytearray(64);elf[:6]=b'\x7fELF\x02\x01';struct.pack_into('<HH',elf,16,2,62)
        manifest={'format':1,'package':{'kind':'application','architecture':'x86_64','abi':'mochios-1'},
                  'application':{'entry':'entry.elf'},'file':[{'id':'entry','path':'$/entry.elf','mode':'0755','size':64,'digest':'sha256:'+hashlib.sha256(elf).hexdigest()}]}
        memory=io.BytesIO()
        with tarfile.open(fileobj=memory,mode='w') as tar:
            for name,content in [('manifest.toml',image.toml_text(manifest).encode()),('payload/bundle/entry.elf',bytes(elf))]+([('payload/bundle/undeclared',payload)] if extra else []):
                member=tarfile.TarInfo(name);member.size=len(content);tar.addfile(member,io.BytesIO(content))
        body=memory.getvalue();header=b'MPKG\x01\0\0\0\x20\0\0\0'+struct.pack('<Q',len(body))+b'\0'*12
        result=root/'app.mpkg';result.write_bytes(header+body);return result

    def test_real_mpkg_inventory_and_entry_validation(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);destination=root/'bundle';destination.mkdir()
            manifest=image.extract_application(self.package(root),destination)
            self.assertEqual((destination/'entry.elf').stat().st_size,64)
            self.assertEqual(manifest['application']['entry'],'entry.elf')
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);destination=root/'bundle';destination.mkdir()
            with self.assertRaisesRegex(image.Error,'undeclared'):image.extract_application(self.package(root,extra=True),destination)

    def test_image_policy_rejects_unauthorized_placement(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);(root/'build/mdp').mkdir(parents=True)
            (root/'build/mdp/policy.toml').write_bytes((ROOT/'build/mdp/policy.toml').read_bytes())
            (root/'.config').write_bytes(b'config');(root/'version.toml').write_bytes(b'version')
            (root/'payload').write_bytes(b'cannot become kernel')
            project=root/'mdp.toml'
            project.write_text('format=1\n[package]\nname="app-example"\nversion="1"\ntarget="x86_64-mochios"\nabi="mochios-1"\nprofile_sha256="'+digest(root/'.config')+'"\nversion_sha256="'+digest(root/'version.toml')+'"\n[[files]]\nsource="payload"\narea="boot"\ndestination="system/kernel.elf"\nmode=420\n')
            lock_project(project);archive=root/'app.tar.mdp';manifest=pack(project,archive)
            from mdp.cli import verify
            package=verify(archive)['package']
            lock=root/'assembly.lock';lock.write_text(json.dumps({'format':1,'target':'x86_64-mochios','abi':'mochios-1','artifacts':[dict(path=archive.name,sha256=digest(archive),package=package)]}))
            with self.assertRaisesRegex(image.Error,'may not export'):image.artifact_tree(root,lock)


if __name__=='__main__':unittest.main()
