import json
import os
from pathlib import Path
import selectors
import select
import shutil
import socket
import subprocess
import time

from .model import GuestSession, Result, decode_frame
from .qmp import Qmp
from .evidence import digest


class VM:
    def __init__(self, root, directory, image, timeout):
        self.root, self.directory = root, directory
        self.process = self.qmp = self.log = None
        self.started = time.monotonic()
        self.deadline = self.started + timeout
        self.details = {}
        for tool in ("qemu-system-x86_64", "qemu-img"):
            if not shutil.which(tool):
                raise RuntimeError(f"missing tool: {tool}")
        # Neither missing KVM nor unavailable GL may silently select a weaker
        # test environment and produce release-quality evidence.
        with open("/dev/kvm", "rb+"):
            pass
        code = Path(os.environ.get("MOCHIOS_TEST_OVMF_CODE", "/usr/share/OVMF/OVMF_CODE_4M.secboot.fd"))
        variables = Path(os.environ.get("MOCHIOS_TEST_OVMF_VARS", "/usr/share/OVMF/OVMF_VARS_4M.snakeoil.fd"))
        if not code.is_file() or not variables.is_file() or not image.is_file():
            raise RuntimeError("missing OS image or OVMF firmware")
        display = os.environ.get("MOCHIOS_TEST_DISPLAY", "gtk")
        if display not in ("gtk", "egl-headless"):
            raise ValueError("MOCHIOS_TEST_DISPLAY must be gtk or egl-headless")
        if display == "gtk" and not (os.environ.get("DISPLAY") or os.environ.get("WAYLAND_DISPLAY")):
            raise RuntimeError("GTK needs a desktop; select MOCHIOS_TEST_DISPLAY=egl-headless on a GL-capable headless host")
        subprocess.run(["qemu-img", "create", "-q", "-f", "qcow2", "-F", "raw", "-b", str(image), str(directory / "disk.qcow2")], check=True)
        shutil.copyfile(variables, directory / "vars.fd")
        arguments = [
            "-machine", "q35,accel=kvm,smm=on", "-m", "1G", "-smp", "2", "-cpu", "host",
            "-rtc", "base=utc,clock=host", "-no-reboot", "-no-shutdown",
            "-boot", "order=c,menu=off,strict=on",
            "-drive", f"if=pflash,format=raw,readonly=on,file={code}",
            "-drive", f"if=pflash,format=raw,file={directory / 'vars.fd'}",
            "-global", "driver=cfi.pflash01,property=secure,value=on",
            "-drive", f"id=osdisk,if=none,format=qcow2,file={directory / 'disk.qcow2'}",
            "-device", "virtio-blk-pci,disable-modern=on,drive=osdisk,bootindex=1",
            "-object", "rng-random,id=rng0,filename=/dev/urandom", "-device", "virtio-rng-pci,rng=rng0",
            "-netdev", "user,id=net0", "-device", "virtio-net-pci,disable-legacy=on,netdev=net0,mac=52:54:00:12:34:56",
            "-device", "virtio-gpu-gl-pci,id=virtio-gpu,disable-legacy=on",
            "-device", "qemu-xhci,id=xhci", "-device", "usb-tablet,bus=xhci.0",
            "-display", "gtk,gl=on,show-cursor=off" if display == "gtk" else "egl-headless",
            "-serial", f"file:{directory / 'serial.log'}", "-monitor", "none",
            "-qmp", "unix:qmp.sock,server=on,wait=off",
        ]
        if display == "egl-headless":
            arguments += ["-vnc", "unix:" + str(directory / "vnc.sock")]
        self.details = {"arguments": arguments, "cpus": 2, "memory_mib": 1024, "graphics": "virgl", "display": display, "stages": {}}
        self.details["firmware_sha256"] = {str(code): digest(code), str(variables): digest(variables)}
        self.log = (directory / "qemu.log").open("wb")
        self.process = subprocess.Popen(["qemu-system-x86_64", *arguments], cwd=directory, stdout=self.log, stderr=subprocess.STDOUT)
        self.details["pid"] = self.process.pid

    def check(self):
        if time.monotonic() >= self.deadline:
            raise TimeoutError("KVM suite timed out")
        if self.process.poll() is not None:
            raise RuntimeError("QEMU exited; see qemu.log")
        serial = self.serial()
        for name, marker in {"kernel": "kernel: start", "desktop": "Binder.app ready",
                             "resident": "resident requests enabled", "input": "IME initialization finished"}.items():
            if marker in serial and name not in self.details["stages"]:
                self.details["stages"][name] = round(time.monotonic() - self.started, 3)
        for marker in ("PAGE FAULT", "kernel panic", "panicked at", "Faulting user context:"):
            if marker.lower() in serial.lower():
                raise RuntimeError(f"fatal guest error: {marker}")

    def serial(self):
        path = self.directory / "serial.log"
        return path.read_text(errors="replace") if path.exists() else ""

    def wait(self, predicate, label, timeout=60):
        deadline = min(self.deadline, time.monotonic() + timeout)
        while time.monotonic() < deadline:
            self.check()
            if predicate():
                return
            time.sleep(.1)
        raise TimeoutError("timed out waiting for " + label)

    def boot(self):
        self.wait(lambda: (self.directory / "qmp.sock").exists(), "QMP")
        self.qmp = Qmp(self.directory / "qmp.sock")
        self.details["kvm"] = self.qmp.command("query-kvm")
        if self.details["kvm"] != {"enabled": True, "present": True}:
            raise RuntimeError("QEMU did not enable KVM")
        self.wait(lambda: all(marker in self.serial() for marker in (
            "Binder.app ready", "resident requests enabled", "execution presenter ready", "IME initialization finished")), "desktop and input readiness")
        nodes = []
        for entry in Path(f"/proc/{self.process.pid}/fd").iterdir():
            try:
                name = os.readlink(entry)
                if name.startswith("/dev/dri/"):
                    nodes.append(name)
            except FileNotFoundError:
                pass
        if not nodes:
            raise RuntimeError("QEMU has no DRM render device; GL environment was not verified")
        self.details["render_devices"] = nodes
        self.qmp.capture(self.directory / "desktop.png")

    def terminal(self):
        self.qmp.key("alt", True)
        self.qmp.tap("spc")
        self.qmp.key("alt", False)
        self.wait(lambda: self.qmp.frame(self.directory / ".frame.ppm").launcher(), "application launcher", 15)
        self.qmp.text("terminal")
        time.sleep(.5)
        self.qmp.tap("ret")
        self.wait(lambda: self.qmp.frame(self.directory / ".frame.ppm").terminal(), "Terminal", 30)
        time.sleep(1)
        self.qmp.capture(self.directory / "terminal.png")

    def close(self):
        if self.qmp is not None:
            try:
                self.qmp.close()
            except OSError as error:
                self.details["control_close_error"] = str(error)
        if self.process is not None:
            if self.process.poll() is None:
                self.process.terminate()
                try:
                    self.process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    self.process.kill()
                    self.process.wait()
            self.details["exit"] = self.process.returncode
            self.details["stopped"] = True
        if self.log is not None:
            self.log.close()
        (self.directory / ".frame.ppm").unlink(missing_ok=True)
        (self.directory / "vm.json").write_text(json.dumps(self.details, indent=2) + "\n")


def run(root, directory, image, cases, run_id, timeout):
    results = []
    session = GuestSession(run_id, cases)
    vm = None
    start = time.monotonic()
    try:
        with socket.socket() as listener:
            listener.bind(("127.0.0.1", 0))
            listener.listen(1)
            listener.setblocking(False)
            port = listener.getsockname()[1]
            vm = VM(root, directory, image, timeout)
            vm.boot()
            results.append(Result("kvm.boot", "pass", time.monotonic()-start))
            vm.terminal()
            results.append(Result("desktop.terminal", "pass", time.monotonic()-start))
            vm.qmp.text(f"os-test {run_id} {port}")
            vm.qmp.tap("ret")
            vm.wait(lambda: bool(select.select([listener], [], [], 0)[0]), "guest test connection", 40)
            connection, _ = listener.accept()
            with connection, selectors.DefaultSelector() as selector, (directory / "guest.jsonl").open("wb") as log:
                connection.setblocking(False)
                selector.register(connection, selectors.EVENT_READ)
                pending = b""
                phase_deadline = time.monotonic() + 10
                while True:
                    vm.check()
                    if time.monotonic() > phase_deadline:
                        raise TimeoutError("guest result timeout: " + (session.active or "protocol completion"))
                    if not selector.select(.1):
                        continue
                    data = connection.recv(65536)
                    if not data:
                        if pending:
                            raise ValueError("truncated guest result frame")
                        session.complete()
                        break
                    log.write(data)
                    log.flush()
                    pending += data
                    while b"\n" in pending:
                        raw, pending = pending.split(b"\n", 1)
                        session.accept(decode_frame(raw + b"\n"))
                        phase_deadline = time.monotonic() + (next(c["timeout"] for c in cases if c["id"] == session.active) + 5 if session.active else 10)
                    if len(pending) > 65536:
                        raise ValueError("guest result exceeds the frame limit")
            results.extend(session.results)
            vm.qmp.capture(directory / "completed.png")
            results.append(Result("kvm.report-complete", "pass", time.monotonic()-start))
    except (OSError, ValueError, RuntimeError, TimeoutError, subprocess.SubprocessError) as error:
        results.extend(session.results)
        results.append(Result("kvm.infrastructure", "error", time.monotonic()-start, str(error)))
        if vm and vm.qmp:
            try:
                vm.qmp.capture(directory / "failure.png")
            except (OSError, ValueError, RuntimeError):
                pass
    finally:
        if vm is not None:
            vm.close()
    present = {r.id for r in results}
    for case in cases:
        if case["id"] not in present:
            results.append(Result(case["id"], "error", detail="required guest test did not complete"))
    return results
