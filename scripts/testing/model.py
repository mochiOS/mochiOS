from dataclasses import asdict, dataclass
import json
import re
import tomllib


@dataclass
class Result:
    id: str
    status: str
    seconds: float = 0
    detail: str = ""
    log: str = ""

    def record(self):
        return asdict(self)


def catalog(path):
    data = tomllib.loads(path.read_text())
    if type(data.get("version")) is not int or data["version"] != 1:
        raise ValueError("unsupported test catalog version")
    for mode in ("host", "guest"):
        rows = data.get(mode, [])
        if not isinstance(rows, list) or not rows:
            raise ValueError(f"empty {mode} test plan")
        seen = set()
        for row in rows:
            if not isinstance(row, dict):
                raise ValueError(f"invalid {mode} test entry")
            name, timeout = row.get("id", ""), row.get("timeout", 0)
            if not isinstance(name, str) or not re.fullmatch(r"[a-z0-9][a-z0-9.-]{0,63}", name) or name in seen:
                raise ValueError(f"invalid or duplicate {mode} test ID: {name}")
            seen.add(name)
            if type(timeout) is not int or not 0 < timeout <= 1800:
                raise ValueError(f"invalid timeout: {name}")
            if mode == "host":
                command = row.get("command")
                if not isinstance(command, list) or not command or any(
                    not isinstance(arg, str) or not arg or "\0" in arg for arg in command
                ):
                    raise ValueError(f"invalid host command: {name}")
    return data


def decode_frame(raw):
    if len(raw) > 65536 or not raw.endswith(b"\n"):
        raise ValueError("incomplete or oversized guest frame")

    def unique(pairs):
        out = {}
        for key, value in pairs:
            if key in out:
                raise ValueError("duplicate JSON field")
            out[key] = value
        return out

    value = json.loads(raw, object_pairs_hook=unique)
    if not isinstance(value, dict):
        raise ValueError("guest frame must be an object")
    return value


class GuestSession:
    """The host inventory is authoritative; missing/extra/repeated cases fail."""

    def __init__(self, run_id, cases):
        self.run_id = run_id
        self.plan = [case["id"] for case in cases]
        if not self.plan or len(set(self.plan)) != len(self.plan):
            raise ValueError("invalid guest plan")
        self.started = False
        self.finished = False
        self.active = None
        self.results = []

    def accept(self, frame):
        if type(frame.get("protocol")) is not int or frame["protocol"] != 1 or frame.get("run_id") != self.run_id:
            raise ValueError("wrong guest protocol or run ID")
        if self.finished:
            raise ValueError("guest data after final result")
        event = frame.get("event")
        if event == "begin" and not self.started:
            if frame.get("cases") != self.plan:
                raise ValueError("guest binary does not match the required test inventory")
            self.started = True
        elif event == "start" and self.started and self.active is None and len(self.results) < len(self.plan):
            if frame.get("id") != self.plan[len(self.results)]:
                raise ValueError("unexpected guest test order")
            self.active = frame["id"]
        elif event == "result" and self.active is not None:
            elapsed, detail = frame.get("elapsed_ms"), frame.get("detail")
            if frame.get("id") != self.active or frame.get("status") not in ("pass", "fail"):
                raise ValueError("invalid guest case result")
            if type(elapsed) is not int or not 0 <= elapsed <= (1 << 64) - 1 or not isinstance(detail, str) or len(detail) > 8192:
                raise ValueError("invalid guest result fields")
            self.results.append(Result(self.active, frame["status"], elapsed / 1000, detail))
            self.active = None
        elif event == "end" and self.started and self.active is None:
            passed = sum(r.status == "pass" for r in self.results)
            if len(self.results) != len(self.plan) or type(frame.get("passed")) is not int or type(frame.get("failed")) is not int:
                raise ValueError("missing guest test results")
            if (frame["passed"], frame["failed"]) != (passed, len(self.plan) - passed):
                raise ValueError("guest summary contradicts case results")
            self.finished = True
        else:
            raise ValueError("unexpected guest test event")

    def complete(self):
        if not self.finished:
            raise ValueError("guest disconnected without a complete final result")
        return self.results


class SerialReport:
    """Transport framing is separate from ordinary, potentially noisy logs."""

    RECORD = b"MOCHIOS_TEST_JSON "
    FINISH = b"MOCHIOS_TEST_FINISH "

    def __init__(self, session):
        self.session = session
        self.finished = False

    def accept_line(self, raw):
        if not raw.startswith((self.RECORD, self.FINISH)):
            return None
        if self.finished:
            raise ValueError("test data after serial completion")
        if raw.startswith(self.RECORD):
            frame = decode_frame(raw[len(self.RECORD):])
            self.session.accept(frame)
            return frame
        if not raw.endswith(b"\n") or len(raw) > 256:
            raise ValueError("incomplete or oversized serial completion")
        fields = raw[len(self.FINISH):].split()
        results = self.session.complete()
        expected = b"0" if all(r.status == "pass" for r in results) else b"1"
        if fields != [self.session.run_id.encode(), expected]:
            raise ValueError("serial completion contradicts the run ID or results")
        self.finished = True
        return None
