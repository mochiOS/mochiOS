#!/usr/bin/env python3
"""Verify output relocation preserves content and refuses unsafe migrations."""
import fcntl
import importlib.util
import json
import subprocess
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('output_layout', Path(__file__).resolve().parents[1] / 'dev/migrate-output-layout.py')
layout = importlib.util.module_from_spec(spec)
spec.loader.exec_module(layout)


class OutputLayoutTests(unittest.TestCase):
    def test_relocation_is_idempotent_and_preserves_vm_disk_and_identity(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp);old = root / 'out/artifacts';old.mkdir(parents=True)
            image = old / 'disk.img';image.write_bytes(b'original image');inode=image.stat().st_ino
            vm = root / 'out/develop/test';vm.mkdir(parents=True)
            (vm/'disk.img').write_bytes(b'private guest data')
            (vm/'vm.json').write_text(json.dumps({'format':1, 'image':str(image), 'persistent':True}))
            cache=root/'out/llvm-port';cache.mkdir();(cache/'object.o').write_bytes(b'cached')
            layout.migrate(root);layout.migrate(root)
            target=root/'out/images/system/disk.img'
            self.assertEqual(target.stat().st_ino,inode)
            self.assertEqual((vm/'disk.img').read_bytes(),b'private guest data')
            self.assertEqual(json.loads((vm/'vm.json').read_text())['image'],str(target.resolve()))
            self.assertEqual(image.read_bytes(),b'original image')
            self.assertEqual((cache/'object.o').read_bytes(),b'cached')

    def test_collision_prevents_all_moves(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp)
            for name in ('artifacts','mmake','build/os'):(root/'out'/name).mkdir(parents=True)
            with self.assertRaisesRegex(ValueError,'Both output paths'):layout.migrate(root)
            self.assertTrue((root/'out/artifacts').is_dir())
            self.assertFalse((root/'out/images/system').exists())

    def test_running_vm_prevents_migration(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);vm=root/'out/develop/test';vm.mkdir(parents=True)
            (vm/'vm.json').write_text('{}')
            with (vm/'.lock').open('a') as lock:
                fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
                with self.assertRaises(BlockingIOError):layout.migrate(root)
            self.assertFalse((root/'out/images').exists())

    def test_installer_export_can_repeat_when_image_is_already_hardlinked(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);work=root/'out/build/os';image=work/'installer/image'
            (image/'rootfs').mkdir(parents=True);(image/'rootfs/.ready').touch()
            (image/'disk.img').write_bytes(b'unchanged image')
            (work/'components').mkdir()
            for asset in ('BOOTX64.EFI','kernel.elf','kernel.meta'):
                (work/'components'/asset).write_bytes(b'fixture')
            scripts=root/'scripts/mmake';scripts.mkdir(parents=True)
            for name in ('make-ext2-image.sh','make-data-image.sh','stage-initfs.sh','make-esp-image.sh','make-disk-image.sh'):
                (scripts/name).write_text('#!/bin/sh\nexit 0\n')
            command=['bash',str(Path(__file__).resolve().parents[1]/'mmake/make-installer-image.sh'),str(root),str(work)]
            for _ in range(2):subprocess.run(command,check=True,capture_output=True)
            exported=root/'out/images/installer/disk.img'
            self.assertEqual(exported.stat().st_ino,(image/'disk.img').stat().st_ino)
            self.assertFalse(exported.with_name('disk.img.new').exists())

    def test_unexpected_alias_is_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);out=root/'out';out.mkdir()
            (out/'mmake').symlink_to('/tmp')
            with self.assertRaisesRegex(ValueError,'Unexpected output alias'):layout.migrate(root)


if __name__=='__main__':unittest.main()
