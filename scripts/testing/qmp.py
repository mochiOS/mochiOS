import json
import socket
import time


class Qmp:
    def __init__(self, path):
        self.socket = socket.socket(socket.AF_UNIX)
        self.socket.settimeout(5)
        self.socket.connect(str(path))
        self.stream = self.socket.makefile("rwb", buffering=0)
        greeting = self.stream.readline(65537)
        if len(greeting) > 65536 or "QMP" not in json.loads(greeting):
            raise ValueError("invalid QMP greeting")
        self.sequence = 0
        self.command("qmp_capabilities")

    def command(self, name, arguments=None):
        self.sequence += 1
        request = {"execute": name, "id": self.sequence}
        if arguments is not None:
            request["arguments"] = arguments
        self.stream.write((json.dumps(request) + "\n").encode())
        while True:
            raw = self.stream.readline(65537)
            if not raw or len(raw) > 65536:
                raise ValueError("QMP disconnected or oversized response")
            reply = json.loads(raw)
            if reply.get("id") != self.sequence:
                continue
            if "error" in reply:
                raise RuntimeError(str(reply["error"]))
            return reply["return"]

    def key(self, key, down):
        self.command("input-send-event", {"events": [{"type": "key", "data": {
            "down": down, "key": {"type": "qcode", "data": key}}}]})

    def tap(self, key):
        self.key(key, True)
        time.sleep(.06)
        self.key(key, False)
        time.sleep(.16)

    def text(self, value):
        mapping = {" ": "spc", "-": "minus", ".": "dot", "/": "slash"}
        for character in value:
            if character not in mapping and not (character.isascii() and (character.islower() or character.isdigit())):
                raise ValueError("unsupported test keyboard character")
            self.tap(mapping.get(character, character))

    def capture(self, path):
        self.command("screendump", {"filename": str(path), "format": "png"})

    def frame(self, path):
        self.command("screendump", {"filename": str(path)})
        return Frame(path.read_bytes())

    def close(self):
        self.stream.close()
        self.socket.close()


class Frame:
    def __init__(self, data):
        fields = data.split(b"\n", 3)
        if len(fields) != 4 or fields[0] != b"P6" or fields[2] != b"255":
            raise ValueError("unexpected QEMU PPM frame")
        self.width, self.height = map(int, fields[1].split())
        self.data = fields[3]
        if not (0 < self.width <= 8192 and 0 < self.height <= 8192) or len(self.data) != self.width * self.height * 3:
            raise ValueError("invalid QEMU frame extent")

    def pixel(self, x, y):
        offset = (int(y * self.height) * self.width + int(x * self.width)) * 3
        return self.data[offset:offset + 3]

    def launcher(self):
        return min(self.pixel(.25, .13)) > 230 and max(self.pixel(.15, .5)) < 225

    def terminal(self):
        return max(self.pixel(.3, .25)) < 80 and max(self.pixel(.4, .4)) < 80
