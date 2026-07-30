#!/usr/bin/env python3
"""Stateful TLS 1.3 DeveloperCA fixture server for host-only smoke tests."""

import json
import pathlib
import signal
import socket
import ssl
import sys


STOP = False


def response(status: int, reason: str, headers: dict[str, str], body: bytes = b"") -> bytes:
    fields = {
        "Content-Length": str(len(body)),
        "Connection": "close",
        **headers,
    }
    encoded = "".join(f"{name}: {value}\r\n" for name, value in fields.items())
    return f"HTTP/1.1 {status} {reason}\r\n{encoded}\r\n".encode("ascii") + body


def read_request(connection: ssl.SSLSocket) -> tuple[str, dict[str, str]]:
    request = bytearray()
    while b"\r\n\r\n" not in request:
        block = connection.recv(4096)
        if not block:
            break
        request.extend(block)
        if len(request) > 65536:
            raise ValueError("request headers exceed fixture limit")
    lines = bytes(request).split(b"\r\n")
    first = lines[0].decode("ascii").split(" ")
    if len(first) != 3 or first[0] != "GET":
        raise ValueError("expected an HTTP GET request")
    headers: dict[str, str] = {}
    for line in lines[1:]:
        if not line:
            break
        name, separator, value = line.partition(b":")
        if not separator:
            raise ValueError("malformed request header")
        key = name.decode("ascii").lower()
        if key in headers:
            raise ValueError("duplicate request header")
        headers[key] = value.decode("ascii").strip()
    return first[1], headers


def fixture_response(
    fixture_dir: pathlib.Path,
    path: str,
    request_number: int,
    if_none_match: str,
) -> bytes:
    if path == "/v1/trust-store":
        prefix = "trust"
        current = '"trust-v2"'
    elif path == "/v1/revocations":
        prefix = "revocations"
        current = '"revocations-v2"'
    else:
        return response(404, "Not Found", {})

    sequence = (
        ("v1", f'"{prefix}-v1"'),
        ("not-modified", f'"{prefix}-v1"'),
        ("v2", current),
        ("rollback", f'"{prefix}-rollback"'),
        ("invalid-signature", f'"{prefix}-invalid"'),
    )
    phase, etag = sequence[min(request_number - 1, len(sequence) - 1)]
    expected = "" if request_number == 1 else ('"%s-v1"' % prefix if request_number <= 3 else current)
    if if_none_match != expected:
        detail = json.dumps(
            {"expected": expected, "actual": if_none_match}, separators=(",", ":")
        ).encode("ascii")
        return response(412, "Precondition Failed", {"Content-Type": "application/json"}, detail)
    if phase == "not-modified" or request_number > len(sequence):
        return response(304, "Not Modified", {"ETag": etag})
    body = (fixture_dir / f"{prefix}-{phase}.json").read_bytes()
    return response(
        200,
        "OK",
        {
            "Content-Type": "application/json",
            "Cache-Control": "public, max-age=1, must-revalidate",
            "ETag": etag,
        },
        body,
    )


def main() -> int:
    if len(sys.argv) != 5:
        print(
            f"usage: {sys.argv[0]} <port> <ready-file> <fixture-dir> <log-file>",
            file=sys.stderr,
        )
        return 2
    port = int(sys.argv[1])
    ready_file = pathlib.Path(sys.argv[2])
    fixture_dir = pathlib.Path(sys.argv[3])
    log_file = pathlib.Path(sys.argv[4])
    tls_fixtures = pathlib.Path(__file__).resolve().parents[1] / "user/crates/tls-client/test-fixtures"
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    context.minimum_version = ssl.TLSVersion.TLSv1_3
    context.maximum_version = ssl.TLSVersion.TLSv1_3
    context.num_tickets = 0
    context.load_cert_chain(
        tls_fixtures / "server.cert.pem", tls_fixtures / "server.key.pem"
    )
    counts = {"/v1/trust-store": 0, "/v1/revocations": 0}

    def stop(_signum, _frame):
        global STOP
        STOP = True

    signal.signal(signal.SIGTERM, stop)
    log_file.write_text("", encoding="ascii")
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as listener:
        listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        listener.bind(("127.0.0.1", port))
        listener.listen(8)
        listener.settimeout(0.2)
        ready_file.write_text(f"port={port}\n", encoding="ascii")
        while not STOP:
            try:
                raw, _ = listener.accept()
            except TimeoutError:
                continue
            try:
                raw.settimeout(10)
                with context.wrap_socket(raw, server_side=True) as connection:
                    path, headers = read_request(connection)
                    if path in counts:
                        counts[path] += 1
                        request_number = counts[path]
                    else:
                        request_number = 1
                    etag = headers.get("if-none-match", "")
                    with log_file.open("a", encoding="ascii") as log:
                        log.write(
                            f"tls={connection.version()} path={path} request={request_number} "
                            f"if-none-match={etag}\n"
                        )
                    connection.sendall(
                        fixture_response(fixture_dir, path, request_number, etag)
                    )
            except (ConnectionError, OSError, ssl.SSLError, ValueError) as error:
                with log_file.open("a", encoding="ascii") as log:
                    log.write(f"server-error={error}\n")
            finally:
                raw.close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
