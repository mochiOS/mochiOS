"""Exercise a specific guest pointer device without silently choosing another."""
import re
import time


class NativePointer:
    def __init__(self, qmp, device='tablet', width=1280, height=800):
        if device not in ('tablet', 'ps2') or width < 2 or height < 2:
            raise ValueError('Invalid native pointer configuration')
        self.qmp = qmp
        self.device = device
        self.width, self.height = width, height
        self.position = (width // 2, height // 2)

    def move(self, x, y, click=False):
        if not (0 <= x < self.width and 0 <= y < self.height):
            raise ValueError('Pointer target outside guest display')
        mice = self.qmp.command('human-monitor-command', {'command-line': 'info mice'})
        name = 'QEMU HID Tablet' if self.device == 'tablet' else 'QEMU PS/2 Mouse'
        match = re.search(r'Mouse #(\d+): ' + re.escape(name), mice)
        if not match:
            raise RuntimeError(f'Required guest device unavailable: {name}')
        self.qmp.command('human-monitor-command', {'command-line': 'mouse_set ' + match[1]})
        if self.device == 'tablet':
            events = [{'type': 'abs', 'data': {'axis': axis, 'value': round(value * 32767 / (extent - 1))}}
                      for axis, value, extent in [('x', x, self.width), ('y', y, self.height)]]
            self.qmp.command('input-send-event', {'events': events})
        else:
            dx, dy = x - self.position[0], y - self.position[1]
            while dx or dy:
                sx, sy = max(-80, min(80, dx)), max(-80, min(80, dy))
                self.qmp.command('input-send-event', {'events': [
                    {'type': 'rel', 'data': {'axis': 'x', 'value': sx}},
                    {'type': 'rel', 'data': {'axis': 'y', 'value': sy}}]})
                dx, dy = dx - sx, dy - sy
                time.sleep(.01)
        self.position = (x, y)
        if click:
            for down in [True, False]:
                self.qmp.command('input-send-event', {'events': [
                    {'type': 'btn', 'data': {'down': down, 'button': 'left'}}]})
                time.sleep(.15)
