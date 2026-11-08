import argparse
from datetime import datetime, timezone
import os
from pathlib import Path
import secrets
import signal
import subprocess
import sys
import time

from .evidence import digest, environment_state, save, source_state
from .model import Result, catalog
from .process import command


def main(root, arguments=None):
    parser = argparse.ArgumentParser(description="mochiOS host/KVM quality tests")
    parser.add_argument("mode", choices=("host", "kvm"))
    parser.add_argument("--list", action="store_true")
    parser.add_argument("--build", action="store_true", help="build the image and record build failure before KVM tests")
    parser.add_argument("--image", type=Path, default=root / "out/artifacts/disk.img")
    parser.add_argument("--timeout", type=int, default=480)
    args = parser.parse_args(arguments)
    plan_path = root / "tests/catalog.toml"
    if not 1 <= args.timeout <= 3600:
        parser.error("--timeout must be between 1 and 3600 seconds")
    if args.list:
        plan = catalog(plan_path)
        for case in plan["host" if args.mode == "host" else "guest"]:
            print(case["id"])
        return 0
    run_id = secrets.token_hex(16)
    parent = root / "out/tests" / args.mode
    directory = parent / (datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ-") + run_id[:8])
    directory.mkdir(parents=True, mode=0o700)
    temporary = root / "out/tests/tmp"
    temporary.mkdir(parents=True, exist_ok=True)
    environment = dict(os.environ, TMPDIR=str(temporary),
                       CARGO_TARGET_DIR=str(root / "out/tests/target/host"),
                       CARGO_PROFILE_TEST_DEBUG="0", CARGO_INCREMENTAL="0")
    # Host tests must not inherit an OS target selected by a developer's shell.
    environment.pop("CARGO_BUILD_TARGET", None)
    report = {"format": 1, "mode": args.mode, "run_id": run_id,
              "started_at": datetime.now(timezone.utc).isoformat(),
              "scope": "all registered " + args.mode + " tests"}
    plan = None
    results = []
    start = time.monotonic()
    previous_term = signal.getsignal(signal.SIGTERM)
    def interrupted(_signum, _frame):
        raise KeyboardInterrupt("test run terminated")
    signal.signal(signal.SIGTERM, interrupted)
    try:
        plan = catalog(plan_path)
        report["catalog_sha256"] = digest(plan_path)
        report["environment"] = environment_state(root)
        report["source_before"] = source_state(root)
        if args.mode == "host":
            for case in plan["host"]:
                print("[test] " + case["id"], file=sys.stderr, flush=True)
                result = command(case, root, directory, environment)
                results.append(result)
                print(f"[{result.status}] {result.id}: {result.detail}", file=sys.stderr, flush=True)
        else:
            from .kvm import run
            if args.build:
                report["source_build_start"] = report["source_before"]
                build_env = dict(os.environ, TMPDIR=str(temporary))
                built = command({"id": "build.image", "timeout": 3600, "command": ["mmake", "image"]}, root, directory, build_env)
                results.append(built)
                if built.status != "pass":
                    raise RuntimeError("OS image build failed; see build.image.log")
                report["source_before"] = source_state(root)
                report["environment"] = environment_state(root)
            image = args.image.resolve(strict=True)
            report["image"] = {"path": str(image), "size": image.stat().st_size, "sha256": digest(image)}
            results.extend(run(root, directory, image, plan["guest"], run_id, args.timeout))
            if digest(image) != report["image"]["sha256"]:
                results.append(Result("candidate.image", "error", detail="tested base image changed during execution"))
        report["source_after"] = source_state(root)
        report["source_unchanged"] = report["source_before"] == report["source_after"]
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError, KeyboardInterrupt) as error:
        results.append(Result("runner.infrastructure", "error", detail=str(error) or "interrupted"))
    finally:
        planned = plan["host" if args.mode == "host" else "guest"] if plan is not None else []
        present = {r.id for r in results}
        for case in planned:
            if case["id"] not in present:
                results.append(Result(case["id"], "error", detail="required test did not execute"))
        report["seconds"] = time.monotonic() - start
        try:
            save(directory, report, results)
        finally:
            signal.signal(signal.SIGTERM, previous_term)
    print(f"[{report['status']}] {args.mode}: {sum(r.status == 'pass' for r in results)}/{len(results)}; {directory / 'result.json'}", file=sys.stderr, flush=True)
    return 0 if report["status"] == "pass" else 1
