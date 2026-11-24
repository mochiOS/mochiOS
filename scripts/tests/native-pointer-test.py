#!/usr/bin/env python3
import sys
import unittest
from pathlib import Path
from unittest.mock import patch
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from testing.pointer import NativePointer

class FakeQmp:
    def __init__(self, mice='Mouse #3: QEMU HID Tablet (absolute)\n* Mouse #2: QEMU PS/2 Mouse\n'):
        self.mice, self.calls = mice, []
    def command(self, name, arguments):
        self.calls.append((name, arguments))
        return self.mice if arguments.get('command-line') == 'info mice' else None

class Tests(unittest.TestCase):
    @patch('testing.pointer.time.sleep')
    def test_default_exercises_usb_absolute_edges_and_click_release(self, _):
        q = FakeQmp()
        p = NativePointer(q)
        p.move(1279, 799, True)
        self.assertIn(('human-monitor-command', {'command-line': 'mouse_set 3'}), q.calls)
        events = [e for name, args in q.calls if name == 'input-send-event' for e in args['events']]
        self.assertEqual([e['data']['value'] for e in events if e['type'] == 'abs'], [32767, 32767])
        self.assertEqual([e['data']['down'] for e in events if e['type'] == 'btn'], [True, False])
        self.assertFalse(any(e['type'] == 'rel' for e in events))
    def test_missing_tablet_does_not_pass_by_falling_back_to_ps2(self):
        with self.assertRaises(RuntimeError):
            NativePointer(FakeQmp('Mouse #2: QEMU PS/2 Mouse')).move(10, 10)
    @patch('testing.pointer.time.sleep')
    def test_relative_device_is_explicit_and_accumulates_motion(self, _):
        q = FakeQmp()
        NativePointer(q, 'ps2').move(10, 20)
        events = [e for name, args in q.calls if name == 'input-send-event' for e in args['events']]
        self.assertEqual(sum(e['data']['value'] for e in events if e['data']['axis'] == 'x'), -630)
        self.assertEqual(sum(e['data']['value'] for e in events if e['data']['axis'] == 'y'), -380)

if __name__ == '__main__': unittest.main()
