#!/usr/bin/env python3
"""Native desktop boot acceptance; timing and later functional checks are separate."""
import argparse
import hashlib
import json
import os
import re
from pathlib import Path
import selectors
import shutil
import subprocess
import sys
import time
import tempfile
root = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(root / 'scripts'))
from testing.qmp import Qmp
p = argparse.ArgumentParser(description='Measure full desktop presentation from QEMU launch, then verify real mouse/keyboard interaction. Requires Linux X11, KVM, virgl, Pillow, OVMF, qemu-img, sfdisk and debugfs. Fresh guest; host page cache is not cleared.')
p.add_argument('name')
p.add_argument('--seconds', type=float, default=25)
p.add_argument('--image', type=Path, default=root / 'out/images/system/disk.img')
p.add_argument('--memory', default='1G')
p.add_argument('--gtk', action='store_true')
p.add_argument('--early-input', action='store_true', help='Send Caps Lock at 8s and hover Applications at 8.6s; inspect the captured input indicator and tooltip')
p.add_argument('--interact-at', type=float, default=11.0)
args = p.parse_args()
if not re.fullmatch('[A-Za-z0-9_-]{1,32}', args.name):
    raise SystemExit('name must contain 1–32 ASCII letters, digits, underscores or hyphens')
if not args.gtk or args.seconds < 20 or args.interact_at < 10:
    raise SystemExit('Acceptance requires --gtk, at least 20 seconds, and interaction after the boot capture interval')
d = root / 'out/reports/boot-performance' / args.name
d.mkdir(parents=True)
(root / 'out/tmp').mkdir(exist_ok=True)
control = tempfile.TemporaryDirectory(prefix='boot-', dir=root / 'out/tmp')
socket_path = Path(control.name) / 'qmp.sock'
shutil.copyfile('/usr/share/OVMF/OVMF_VARS_4M.snakeoil.fd', d / 'vars.fd')
subprocess.run(['qemu-img', 'create', '-q', '-f', 'qcow2', '-F', 'raw', '-b', str(args.image.resolve()), str(d / 'disk.qcow2')], check=True)
command = ['qemu-system-x86_64', '-name', 'mochiOS-boot-' + args.name, '-machine', 'q35,accel=kvm,smm=on', '-m', args.memory, '-smp', '2', '-cpu', 'host', '-rtc', 'base=utc,clock=host', '-no-reboot', '-no-shutdown', '-boot', 'order=c,menu=off,strict=on', '-drive', 'if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.secboot.fd', '-drive', 'if=pflash,format=raw,file=vars.fd', '-global', 'driver=cfi.pflash01,property=secure,value=on', '-drive', 'id=osdisk,if=none,format=qcow2,file=disk.qcow2', '-device', 'virtio-blk-pci,disable-modern=on,drive=osdisk,bootindex=1', '-object', 'rng-random,id=rng0,filename=/dev/urandom', '-device', 'virtio-rng-pci,rng=rng0', '-netdev', 'user,id=net0', '-device', 'virtio-net-pci,disable-legacy=on,netdev=net0,mac=52:54:00:12:34:56', '-device', 'virtio-gpu-gl-pci,id=virtio-gpu,disable-legacy=on', '-device', 'qemu-xhci,id=xhci', '-device', 'usb-tablet,bus=xhci.0', '-display', 'gtk,gl=on,show-cursor=off' if args.gtk else 'egl-headless', '-serial', 'stdio', '-monitor', 'none', '-qmp', 'unix:' + str(socket_path) + ',server=on,wait=off']
parts = json.loads(subprocess.check_output(['sfdisk', '--json', str(args.image)]))['partitiontable']['partitions']
data_partition = next((x for x in parts if x['type'].lower() == '6d6f6368-694f-5300-8000-6d5061727402'))
initial_view = {'driver': 'raw', 'offset': data_partition['start'] * 512, 'size': data_partition['size'] * 512, 'file': {'driver': 'file', 'filename': str(args.image.resolve())}}
subprocess.run(['qemu-img', 'convert', '-O', 'raw', '-S', '4096', 'json:' + json.dumps(initial_view), str(d / 'initial-data.img')], check=True)
for service in ['service-manager', 'Binder.app']:
    initial = subprocess.run(['debugfs', '-R', f'cat /var/log/services/{service}.log', str(d / 'initial-data.img')], capture_output=True, check=True)
    if initial.stdout.strip():
        raise SystemExit('Acceptance requires a freshly assembled image without previous boot logs')
(d / 'initial-data.img').unlink()
report = {'command': command, 'image': str(args.image.resolve()), 'memory': args.memory, 'accelerator': 'kvm', 'cpus': 2, 'events': [], 'desktop_ready': False, 'input_verified': False}

def capture_window(name):
    tree = subprocess.check_output(['xwininfo', '-root', '-tree'], text=True)
    match = re.search('(0x[0-9a-f]+)\\s+"[^"\\n]*' + re.escape('mochiOS-boot-' + args.name) + '[^"\\n]*"', tree)
    if not match:
        raise RuntimeError('QEMU window not found')
    path = d / (name + '.png')
    from testing.x11_capture import capture
    capture(int(match[1], 16), path)
    report.setdefault('captures', []).append({'name': name, 'seconds': time.monotonic() - start})
    return path

def difference(first, second, region, threshold=3):
    from PIL import Image, ImageChops

    def guest(path):
        image = Image.open(path).convert('RGB')
        if image.width != 1280 or image.height < 800:
            raise RuntimeError(f'Unexpected QEMU window extent: {image.size}')
        return image.crop((0, image.height - 800, 1280, image.height)).crop(region)
    delta = ImageChops.difference(guest(first), guest(second))
    red, green, blue = delta.split()
    maximum = ImageChops.lighter(ImageChops.lighter(red, green), blue)
    return sum(maximum.histogram()[threshold + 1:]) / (maximum.width * maximum.height)
def clock_ink(path):
    from PIL import Image, ImageStat
    with Image.open(path) as image:
        clock = image.convert('L').crop((990, image.height-800+10, 1110, image.height-800+45))
    background = ImageStat.Stat(clock).median[0]
    return sum(count for level, count in enumerate(clock.histogram()) if abs(level-background)>40)

pointer_position = (640, 400)

def pointer(x, y, click=False):
    global pointer_position
    mice = qmp.command('human-monitor-command', {'command-line': 'info mice'})
    match = re.search('Mouse #(\\d+): QEMU PS/2 Mouse', mice)
    if not match:
        raise RuntimeError('Expected PS/2 mouse is unavailable')
    qmp.command('human-monitor-command', {'command-line': 'mouse_set ' + match[1]})
    dx, dy = (x - pointer_position[0], y - pointer_position[1])
    while dx or dy:
        step_x = max(-80, min(80, dx))
        step_y = max(-80, min(80, dy))
        qmp.command('input-send-event', {'events': [{'type': 'rel', 'data': {'axis': 'x', 'value': step_x}}, {'type': 'rel', 'data': {'axis': 'y', 'value': step_y}}]})
        dx -= step_x
        dy -= step_y
        time.sleep(0.01)
    pointer_position = (x, y)
    if click:
        qmp.command('human-monitor-command', {'command-line': 'mouse_button 1'})
        time.sleep(0.15)
        qmp.command('human-monitor-command', {'command-line': 'mouse_button 0'})
        time.sleep(0.03)
        qmp.command('input-send-event', {'events': [{'type': 'rel', 'data': {'axis': 'x', 'value': 1}}]})
        time.sleep(0.03)
        qmp.command('input-send-event', {'events': [{'type': 'rel', 'data': {'axis': 'x', 'value': -1}}]})
start = time.monotonic()
proc = subprocess.Popen(command, cwd=d, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, env=dict(os.environ, TMPDIR=str(root / 'out/tmp'), GDK_BACKEND='x11'))
qmp = None
capture_at = iter([7, 7.5, 8, 8.5, 9, 9.5, 9.8])
next_capture = next(capture_at)
interaction_stage = 0
next_probe = 0.0
reference = None
resized = False
early_pointer = False
early_key = False
pending = b''
selector = selectors.DefaultSelector()
selector.register(proc.stdout, selectors.EVENT_READ)
try:
    with (d / 'serial.log').open('wb') as raw, (d / 'timed.log').open('w') as timed:
        while time.monotonic() - start < args.seconds and proc.poll() is None:
            for key, _ in selector.select(0.02):
                chunk = os.read(key.fileobj.fileno(), 65536)
                if not chunk:
                    selector.unregister(key.fileobj)
                    continue
                raw.write(chunk)
                raw.flush()
                pending += chunk
                while b'\n' in pending:
                    line, pending = pending.split(b'\n', 1)
                    elapsed = time.monotonic() - start
                    text = line.decode(errors='replace').strip()
                    timed.write(f'{elapsed:.6f} {text}\n')
                    timed.flush()
                    report['events'].append({'seconds': elapsed, 'line': text})
                    print(f'{elapsed:.3f} {text}', flush=True)
                    if re.fullmatch('service-manager\\.service: Binder\\.app ready(?: \\[boot_ms=\\d+\\])?', text):
                        report['desktop_ready'] = True
                        report['desktop_seconds'] = elapsed
            if qmp is None and socket_path.exists():
                qmp = Qmp(socket_path)
                report['mouse_devices'] = qmp.command('human-monitor-command', {'command-line': 'info mice'})
            if args.gtk and (not resized):
                tree = subprocess.check_output(['xwininfo', '-root', '-tree'], text=True)
                match = re.search('(0x[0-9a-f]+)\\s+"[^"\\n]*' + re.escape('mochiOS-boot-' + args.name) + '[^"\\n]*"', tree)
                if match:
                    from testing.x11_capture import resize
                    resize(int(match[1], 16))
                    resized = True
            if args.early_input and qmp and not early_pointer and time.monotonic()-start >= 8.6:
                pointer(755,745)
                early_pointer = True
                report['early_pointer_sent_seconds'] = time.monotonic()-start
            if args.early_input and qmp and not early_key and time.monotonic()-start >= 8.0:
                qmp.command('send-key', {'keys':[{'type':'qcode','data':'caps_lock'}], 'hold-time':80})
                early_key = True
                report['early_key_sent_seconds'] = time.monotonic()-start
            if args.gtk and next_capture is not None and (time.monotonic() - start >= next_capture):
                capture_window(f'window-{next_capture}')
                next_capture = next(capture_at, None)
            if args.interact_at is not None and qmp:
                elapsed = time.monotonic() - start - args.interact_at
                if interaction_stage == 0 and elapsed >= 0:
                    pointer(755, 745)
                    interaction_stage = 1
                elif interaction_stage == 1 and elapsed >= 0.2:
                    capture_window('before-menu')
                    pointer(755, 745, True)
                    interaction_stage = 2
                    report['click_seconds'] = time.monotonic() - start
                elif interaction_stage in (2, 3) and elapsed >= next_probe:
                    frame = capture_window('probe')
                    reference = reference or d / 'before-menu.png'
                    changed = difference(reference, frame, (380, 120, 900, 650), 15)
                    next_probe = elapsed + 0.3
                    if interaction_stage == 2 and changed > 0.15:
                        shutil.copyfile(frame, d / 'open-menu.png')
                        report['panel_open_seconds'] = time.monotonic() - start
                        qmp.command('send-key', {'keys': [{'type': 'qcode', 'data': 'esc'}], 'hold-time': 80})
                        interaction_stage = 3
                    elif interaction_stage == 3 and changed < 0.01:
                        shutil.copyfile(frame, d / 'closed-menu.png')
                        report['functional_complete_seconds'] = time.monotonic() - start
                        if args.early_input:
                            pointer(756,745)
                            pointer(755,745)
                        else:
                            pointer(640,400)
                        interaction_stage = 4
        if qmp:
            capture_window('final-desktop')
            report['kvm'] = qmp.command('query-kvm')
            report['qemu_status'] = qmp.command('query-status')
            report['render_devices'] = []
            for descriptor in Path(f'/proc/{proc.pid}/fd').iterdir():
                try:
                    target = os.readlink(descriptor)
                    if target.startswith('/dev/dri/'):
                        report['render_devices'].append(target)
                except FileNotFoundError:
                    pass
            try:
                qmp.capture(d / 'screen.png')
            except Exception as error:
                report['capture_error'] = str(error)
            qmp.command('stop')
            report['paused_seconds'] = time.monotonic() - start
            qmp.command('human-monitor-command', {'command-line': 'qemu-io osdisk "flush"'})
            view = {'driver': 'raw', 'offset': data_partition['start'] * 512, 'size': data_partition['size'] * 512, 'file': {'driver': 'qcow2', 'file': {'driver': 'file', 'filename': str(d / 'disk.qcow2')}}}
            subprocess.run(['qemu-img', 'convert', '-U', '-O', 'raw', '-S', '4096', 'json:' + json.dumps(view), str(d / 'data.img')], check=True)
            for name in ['service-manager', 'capability', 'input', 'compositor', 'display.driver', 'drivers', 'user', 'workspace', 'Binder.app']:
                result = subprocess.run(['debugfs', '-R', f'cat /var/log/services/{name}.log', str(d / 'data.img')], capture_output=True, text=True)
                (d / (name + '.log')).write_text(result.stdout)
            manager = (d / 'service-manager.log').read_text()
            report['desktop_ready_in_persisted_log'] = any((re.fullmatch('service-manager\\.service: Binder\\.app ready(?: \\[boot_ms=\\d+\\])?', line.strip()) for line in manager.splitlines()))
            if report['desktop_ready_in_persisted_log']:
                report['desktop_ready'] = True
                report['desktop_ready_upper_bound_seconds'] = report['paused_seconds']
            report['background_presented'] = 'desktop background presented' in (d / 'Binder.app.log').read_text()
            report['input_verified'] = interaction_stage == 4
            report['resident_running'] = 'resident phase reason=Running' in manager
            report['boot_frames'] = []
            regions = [(380, 120, 900, 650), (10, 10, 45, 45), (480, 700, 805, 800), (1110, 10, 1240, 45)]
            for frame in report.get('captures', []):
                if not frame['name'].startswith('window-'):
                    continue
                path = d / (frame['name'] + '.png')
                changed = [difference(path, d / 'final-desktop.png', r) for r in regions]
                ink = clock_ink(path)
                complete = all((x < 0.01 for x in changed)) and ink >= 60 and ink >= clock_ink(d / 'final-desktop.png') * .7
                report['boot_frames'].append(dict(frame, changed_fractions=changed, clock_ink=ink, complete=complete))
            completed = [x['seconds'] for x in report['boot_frames'] if x['complete']]
            report['full_desktop_upper_bound_seconds'] = min(completed) if completed else None
            fatal = any((x.lower() in (d / 'serial.log').read_text(errors='replace').lower() for x in ['kernel panic', 'PAGE FAULT', 'Faulting user context:', 'panicked at']))
            report['under_ten_seconds'] = bool(completed and min(completed) < 10 and report['input_verified'] and report['background_presented'] and report['desktop_ready_in_persisted_log'] and (not fatal) and report['kvm'].get('enabled') and report['qemu_status'].get('running') and report['render_devices'] and report['resident_running'])
finally:
    report['elapsed_seconds'] = time.monotonic() - start
    if qmp:
        qmp.close()
    proc.terminate()
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.wait()
    report['exit_code'] = proc.returncode
    with args.image.open('rb') as source:
        report['image_sha256'] = hashlib.file_digest(source, 'sha256').hexdigest()
    control.cleanup()
    (d / 'result.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({k: v for k, v in report.items() if k not in ('events', 'command')}, indent=2), flush=True)
raise SystemExit(0 if report.get('under_ten_seconds') else 1)
