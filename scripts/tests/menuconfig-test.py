#!/usr/bin/env python3
"""Compile and exercise the real menuconfig parser, search and terminal controls."""
import fcntl
import os
from pathlib import Path
import pty
import select
import shlex
import shutil
import struct
import subprocess
import tempfile
import termios
import time
import unittest

ROOT = Path(__file__).resolve().parents[2]


@unittest.skipUnless(shutil.which('cc'), 'host C compiler required')
class MenuconfigTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temporary = tempfile.TemporaryDirectory()
        cls.directory = Path(cls.temporary.name)
        cls.binary = cls.directory / 'menuconfig'
        subprocess.run(['cc', '-std=c11', '-Wall', '-Wextra', '-Werror', '-O2',
                        str(ROOT / 'build/menuconfig.c'), '-lncurses', '-o', str(cls.binary)], check=True)
        harness = cls.directory / 'search.c'
        harness.write_text('#define main menuconfig_entry\n#include ' + '"' + str(ROOT / 'build/menuconfig.c') + '"\n' + r'''
#undef main
#include <assert.h>
int main(int argc, char **argv) {
    (void)argc;
    static ConfigState state;
    assert(load_schema(&state, argv[1]));
    MenuViewState view = {0};
    size_t last = SIZE_INVALID;
    assert(search_option(&state, &view, "application_terra", &last));
    assert(!strcmp(state.options[last].key, "APPLICATION_TERRA_BUILTIN"));
    assert(!strcmp(state.menus[view.current_menu].title, "Applications"));
    size_t before = last;
    assert(search_option(&state, &view, "APPLICATION_TERRA", &last));
    assert(before == last); /* Wrap around the entire registry. */
    last = SIZE_INVALID;
    assert(search_option(&state, &view, "-s -S", &last)); /* Description only. */
    assert(!strcmp(state.options[last].key, "DEBUG_QEMU_DEBUG"));
    assert(!search_option(&state, &view, "no_such_option", &last));
    assert(!search_option(&state, &view, "", &last));
    assert(!strcmp(state.options[last].value, state.options[last].default_value));
    return 0;
}
''')
        cls.search = cls.directory / 'search'
        subprocess.run(['cc', '-std=c11', '-Wall', '-Wextra', '-Werror', '-O2',
                        str(harness), '-lncurses', '-o', str(cls.search)], check=True)

    @classmethod
    def tearDownClass(cls):
        cls.temporary.cleanup()

    def check_schema(self, text):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / 'schema'
            path.write_text(text)
            return subprocess.run([str(self.binary), '--schema', str(path), '--config', str(path.parent / '.config'),
                                   '--check'], capture_output=True, text=True)

    def test_real_schema_and_noninteractive_check_do_not_write_config(self):
        result = subprocess.run([str(self.binary), '--schema', str(ROOT / 'build/schema.conf'),
                                 '--config', str(self.directory / 'absent.config'), '--check'], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('27 options validated', result.stdout)
        self.assertFalse((self.directory / 'absent.config').exists())

    def test_all_options_have_help_and_consistent_defaults(self):
        defaults = dict(line.split('=', 1) for line in (ROOT / 'build/defaults.config').read_text().splitlines()
                        if line and not line.startswith('#'))
        options = []
        help_count = 0
        for line in (ROOT / 'build/schema.conf').read_text().splitlines():
            words = shlex.split(line)
            if words and words[0] in ('bool', 'string', 'choice'):
                options.append(words[1])
                self.assertEqual(words[3], defaults[words[1]], words[1])
            if words and words[0] == 'help':
                help_count += 1
                self.assertGreater(len(words[1]), 30)
        self.assertEqual(help_count, len(options))
        self.assertIn('DRIVER_VIRTIO_NET', options)
        self.assertNotIn('ENABLE_KPTI', options)  # No consumer currently implements this switch.

    def test_search_matches_keys_and_descriptions_and_wraps(self):
        subprocess.run([str(self.search), str(ROOT / 'build/schema.conf')], check=True)

    def test_invalid_help_and_duplicate_keys_are_rejected(self):
        for schema in ('help "orphan"\n', 'bool TEST "Test" y\nhelp "one"\nhelp "two"\n',
                       'bool TEST "Test" y\nhelp not_quoted\n',
                       'bool TEST "Test" y\nbool TEST "Duplicate" n\n'):
            with self.subTest(schema=schema):
                self.assertNotEqual(self.check_schema(schema).returncode, 0)

    def test_merge_rejects_invalid_new_and_existing_boolean_values(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            for key in ('APPLICATION_MAIL_BUILTIN', 'KERNEL_PERFORMANCE_INSTRUMENTATION',
                        'DEBUG_QEMU_GPU_BACKEND', 'REQUIRE_UEFI_SECURE_BOOT'):
                (root / 'input').write_text(key + '=maybe\n')
                result = subprocess.run(['perl', str(ROOT / 'scripts/config/merge-config.pl'), '--default',
                                         str(ROOT / 'build/defaults.config'), '--in', str(root / 'input'),
                                         '--out', str(root / 'output')], capture_output=True)
                self.assertNotEqual(result.returncode, 0, key)
            (root / 'input').write_text('DEBUG_QEMU_SMP=0\n')
            result = subprocess.run(['perl', str(ROOT / 'scripts/config/merge-config.pl'), '--default',
                                     str(ROOT / 'build/defaults.config'), '--in', str(root / 'input'),
                                     '--out', str(root / 'output')], capture_output=True)
            self.assertNotEqual(result.returncode, 0)

    def test_terminal_search_help_toggle_reset_and_save(self):
        with tempfile.TemporaryDirectory() as folder:
            config = Path(folder) / '.config'
            config.write_text('APPLICATION_MAIL_BUILTIN=n\nUNRELATED_SETTING=preserved\nENABLE_KPTI=n\n')
            master, slave = pty.openpty()
            fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 24, 100, 0, 0))
            process = subprocess.Popen([str(self.binary), '--schema', str(ROOT / 'build/schema.conf'),
                                        '--config', str(config)], stdin=slave, stdout=slave, stderr=slave,
                                       env=dict(os.environ, TERM='xterm-256color'))
            os.close(slave)
            output = bytearray()
            def until(needle):
                deadline = time.monotonic() + 8
                while needle not in output:
                    self.assertIsNone(process.poll(), bytes(output).decode(errors='replace'))
                    self.assertLess(time.monotonic(), deadline, bytes(output).decode(errors='replace'))
                    ready, _, _ = select.select([master], [], [], 0.1)
                    if ready:
                        output.extend(os.read(master, 65536))
                output.clear()
            try:
                until(b'mochiOS BuildConfig')
                os.write(master, b'/APPLICATION_MAIL_BUILTIN\n')
                until(b'Include Mail')
                os.write(master, b'?')
                until(b'Option help')
                os.write(master, b'\ny')
                until(b'modified')
                os.write(master, b'ds')  # Restore n, then save.
                until(b'saved')
                os.write(master, b'q')
                self.assertEqual(process.wait(timeout=5), 0)
                text = config.read_text()
                self.assertIn('APPLICATION_MAIL_BUILTIN=n', text)
                self.assertIn('UNRELATED_SETTING=preserved', text)
                self.assertIn('ENABLE_KPTI=n', text)
                self.assertIn('APPLICATION_TERRA_BUILTIN=n', text)
            finally:
                if process.poll() is None:
                    process.kill()
                    process.wait()
                os.close(master)


if __name__ == '__main__':
    unittest.main()
