import os
import selectors
import signal
import subprocess
import time

from .model import Result


def command(case, root, directory, environment):
    """Bound both runtime and output, and stop the owned process group."""
    log = directory / (case["id"] + ".log")
    start = time.monotonic()
    process = None
    status, detail = "error", ""
    try:
        process = subprocess.Popen(case["command"], cwd=root, env=environment,
                                   stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                   start_new_session=True)
        selector = selectors.DefaultSelector()
        selector.register(process.stdout, selectors.EVENT_READ)
        length = 0
        with log.open("wb") as output:
            while selector.get_map() or process.poll() is None:
                if time.monotonic() - start > case["timeout"]:
                    raise TimeoutError("host test timed out")
                for key, _ in selector.select(.05):
                    data = os.read(key.fd, 65536)
                    if not data:
                        selector.unregister(key.fileobj)
                        continue
                    length += len(data)
                    if length > 16 * 1024 * 1024:
                        raise ValueError("host test exceeded the 16 MiB log limit")
                    output.write(data)
                    output.flush()
        status = "pass" if process.wait() == 0 else "fail"
        detail = "" if status == "pass" else f"command exited {process.returncode}"
    except (OSError, TimeoutError, ValueError) as error:
        detail = str(error)
    finally:
        if process is not None:
            try:
                os.killpg(process.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            try:
                process.wait(timeout=2)
            except subprocess.TimeoutExpired:
                pass
            # The command may have exited while one of its descendants still
            # holds the output pipe or ignores SIGTERM. Own the whole session.
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.wait()
            if process.stdout is not None:
                process.stdout.close()
        if "selector" in locals():
            selector.close()
    return Result(case["id"], status, time.monotonic() - start, detail, str(log))
