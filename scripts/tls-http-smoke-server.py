#!/usr/bin/env python3
"""Deterministic TLS 1.3 HTTP server for mochiOS smoke tests only."""

import pathlib
import select
import signal
import socket
import ssl
import sys
import threading


ROOT = pathlib.Path(__file__).resolve().parents[1]
FIXTURES = ROOT / "user" / "crates" / "tls-client" / "test-fixtures"
STOP = threading.Event()


def tls_context(certificate: str, private_key: str) -> ssl.SSLContext:
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    context.minimum_version = ssl.TLSVersion.TLSv1_3
    context.maximum_version = ssl.TLSVersion.TLSv1_3
    context.num_tickets = 0
    context.load_cert_chain(FIXTURES / certificate, FIXTURES / private_key)
    return context


def complete_tls_records(buffer: bytearray):
    while len(buffer) >= 5:
        length = int.from_bytes(buffer[3:5], "big")
        if length > 18432:
            raise ValueError("TLS record exceeds test proxy limit")
        record_length = 5 + length
        if len(buffer) < record_length:
            return
        yield bytearray(buffer[:record_length])
        del buffer[:record_length]


def serve_tampered_record_proxy(
    port: int,
    backend_port: int,
    ready: threading.Event,
) -> None:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as listener:
        listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        listener.bind(("127.0.0.1", port))
        listener.listen(4)
        listener.settimeout(0.2)
        ready.set()
        while not STOP.is_set():
            try:
                client, _ = listener.accept()
            except TimeoutError:
                continue
            try:
                with client, socket.create_connection(
                    ("127.0.0.1", backend_port), timeout=10
                ) as backend:
                    buffers = {client: bytearray(), backend: bytearray()}
                    destinations = {client: backend, backend: client}
                    open_inputs = {client, backend}
                    client_application_records = 0
                    tampered = False
                    while open_inputs and not STOP.is_set():
                        readable, _, _ = select.select(list(open_inputs), [], [], 0.2)
                        for source in readable:
                            block = source.recv(65536)
                            if not block:
                                open_inputs.remove(source)
                                try:
                                    destinations[source].shutdown(socket.SHUT_WR)
                                except OSError:
                                    pass
                                continue
                            buffers[source].extend(block)
                            for record in complete_tls_records(buffers[source]):
                                content_type = record[0]
                                if source is client and content_type == 23:
                                    client_application_records += 1
                                elif (
                                    source is backend
                                    and content_type == 23
                                    and client_application_records >= 2
                                    and not tampered
                                ):
                                    record[-1] ^= 0x01
                                    tampered = True
                                    print(
                                        "tls-http-smoke-server: "
                                        f"port={port} record=tampered",
                                        flush=True,
                                    )
                                destinations[source].sendall(record)
                    if not tampered:
                        print(
                            "tls-http-smoke-server: "
                            f"port={port} record=not-tampered",
                            flush=True,
                        )
            except (ConnectionError, OSError, ValueError) as error:
                print(
                    "tls-http-smoke-server: "
                    f"port={port} expected-proxy-error={error}",
                    flush=True,
                )


def serve_stalled_handshake(port: int, ready: threading.Event) -> None:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as listener:
        listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        listener.bind(("127.0.0.1", port))
        listener.listen(4)
        listener.settimeout(0.2)
        ready.set()
        while not STOP.is_set():
            try:
                client, _ = listener.accept()
            except TimeoutError:
                continue
            with client:
                while not STOP.wait(0.2):
                    pass


def read_request(connection: ssl.SSLSocket) -> bytes:
    request = bytearray()
    while b"\r\n\r\n" not in request:
        block = connection.recv(4096)
        if not block:
            break
        request.extend(block)
        if len(request) > 65536:
            raise ValueError("request headers exceed test server limit")
    return bytes(request)


def response_for(request: bytes, port: int) -> bytes:
    first_line = request.split(b"\r\n", 1)[0]
    parts = first_line.split(b" ")
    path = parts[1] if len(parts) == 3 else b"/invalid"
    if path == b"/content-length":
        body = b'{"status":"ok","framing":"content-length"}\n'
        return (
            b"HTTP/1.1 200 OK\r\n"
            b"Content-Type: application/json\r\n"
            + f"Content-Length: {len(body)}\r\n".encode("ascii")
            + b"Connection: close\r\n\r\n"
            + body
        )
    if path == b"/chunked":
        return (
            b"HTTP/1.1 200 OK\r\n"
            b"Content-Type: text/plain\r\n"
            b"Transfer-Encoding: chunked\r\n"
            b"Connection: close\r\n\r\n"
            b"8;fixture=yes\r\nmochiOS \r\n"
            b"D\r\nchunked smoke\r\n"
            b"1\r\n\n\r\n"
            b"0\r\nX-Smoke-Trailer: complete\r\n\r\n"
        )
    if path == b"/header-overflow":
        return (
            b"HTTP/1.1 200 OK\r\nX-Overflow: "
            + b"a" * 17000
            + b"\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        )
    if path == b"/bad-content-length":
        return (
            b"HTTP/1.1 200 OK\r\nContent-Length: invalid\r\n"
            b"Connection: close\r\n\r\n"
        )
    if path == b"/bad-chunk":
        return (
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n"
            b"Connection: close\r\n\r\nnot-hex\r\n"
        )
    if path == b"/redirect-http":
        return (
            b"HTTP/1.1 302 Found\r\n"
            + f"Location: http://tls.test.mochios:{port}/content-length\r\n".encode(
                "ascii"
            )
            + b"Content-Length: 0\r\nConnection: close\r\n\r\n"
        )
    if path == b"/body-overflow":
        return (
            b"HTTP/1.1 200 OK\r\nContent-Length: 1048577\r\n"
            b"Connection: close\r\n\r\n"
        )
    return b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"


def request_path(request: bytes) -> str:
    first_line = request.split(b"\r\n", 1)[0]
    parts = first_line.split(b" ")
    if len(parts) != 3:
        return "/invalid"
    return parts[1].decode("ascii", errors="replace")


def serve(
    port: int,
    context: ssl.SSLContext,
    trusted_http: bool,
    ready: threading.Event,
) -> None:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as listener:
        listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        listener.bind(("127.0.0.1", port))
        listener.listen(8)
        listener.settimeout(0.2)
        ready.set()
        while not STOP.is_set():
            try:
                raw, _ = listener.accept()
            except TimeoutError:
                continue
            try:
                raw.settimeout(10)
                with context.wrap_socket(raw, server_side=True) as connection:
                    path = "/handshake-only"
                    if trusted_http:
                        request = read_request(connection)
                        if request:
                            path = request_path(request)
                            print(
                                f"tls-http-smoke-server: port={port} path={path} request=received",
                                flush=True,
                            )
                            connection.sendall(response_for(request, port))
                    try:
                        connection.unwrap()
                        print(
                            f"tls-http-smoke-server: port={port} path={path} close-notify=complete",
                            flush=True,
                        )
                    except (ConnectionError, OSError, ssl.SSLError):
                        pass
            except (ConnectionError, OSError, ssl.SSLError, ValueError) as error:
                print(f"tls-http-smoke-server: port={port} expected-peer-error={error}")
            finally:
                raw.close()


def main() -> int:
    if len(sys.argv) != 3:
        print(f"usage: {sys.argv[0]} <base-port> <ready-file>", file=sys.stderr)
        return 2
    base_port = int(sys.argv[1])
    if base_port < 1 or base_port > 65530:
        raise ValueError("base port must leave room for six listeners")
    ready_file = pathlib.Path(sys.argv[2])
    listeners = (
        (base_port, tls_context("server.cert.pem", "server.key.pem"), True),
        (
            base_port + 1,
            tls_context("untrusted-server.cert.pem", "untrusted-server.key.pem"),
            False,
        ),
        (
            base_port + 2,
            tls_context("expired-server.cert.pem", "expired-server.key.pem"),
            False,
        ),
    )
    signal.signal(signal.SIGTERM, lambda _signum, _frame: STOP.set())
    ready_events = [threading.Event() for _listener in listeners]
    threads = [
        threading.Thread(target=serve, args=(*listener, ready), daemon=True)
        for listener, ready in zip(listeners, ready_events)
    ]
    tamper_ready = threading.Event()
    threads.append(
        threading.Thread(
            target=serve_tampered_record_proxy,
            args=(base_port + 3, base_port, tamper_ready),
            daemon=True,
        )
    )
    ready_events.append(tamper_ready)
    stalled_ready = threading.Event()
    threads.append(
        threading.Thread(
            target=serve_stalled_handshake,
            args=(base_port + 5, stalled_ready),
            daemon=True,
        )
    )
    ready_events.append(stalled_ready)
    for thread in threads:
        thread.start()
    if not all(ready.wait(5) for ready in ready_events):
        raise RuntimeError("TLS listener did not become ready")
    ready_file.write_text(
        f"trusted={base_port}\nuntrusted={base_port + 1}\n"
        f"expired={base_port + 2}\ntampered={base_port + 3}\n"
        f"stalled={base_port + 5}\n",
        encoding="ascii",
    )
    print(
        "tls-http-smoke-server: "
        f"trusted={base_port} untrusted={base_port + 1} "
        f"expired={base_port + 2} tampered={base_port + 3} "
        f"stalled={base_port + 5}",
        flush=True,
    )
    while not STOP.wait(0.2):
        if any(not thread.is_alive() for thread in threads):
            raise RuntimeError("TLS listener terminated")
    for thread in threads:
        thread.join(timeout=2)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
