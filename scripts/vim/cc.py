#!/usr/bin/env python3
"""Private Autoconf C driver for the Vim port. Every link uses real LLD.

The OS's installed ld/clang drivers are separate. Unsupported port options fail
explicitly rather than falling back to a host library or linker.
"""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile


def main():
    config = json.loads(Path(sys.argv[1]).read_text())
    args = sys.argv[2:]
    cc = [config["cc"], *config["cflags"]]
    if args == ["-v"] or any(arg in args for arg in ("-c", "-E", "-S", "--version", "-dumpversion", "-dumpmachine")):
        os.execv(cc[0], [*cc, *args])

    inputs, sources, compile_flags = [], [], []
    output = "a.out"
    it = iter(args)
    for arg in it:
        if arg == "-o":
            output = next(it)
        elif arg in ("-L", "-l"):
            inputs.extend([arg, next(it)])
        elif arg.startswith(("-L", "-l")):
            inputs.append(arg)
        elif arg.startswith("-Wl,"):
            inputs.extend(arg[4:].split(","))
        elif arg in ("-static", "-no-pie"):
            inputs.append(arg)
        elif arg in ("-I", "-D", "-U", "-isystem", "-include", "--param"):
            compile_flags.extend([arg, next(it)])
        elif arg.startswith(("-I", "-D", "-U", "-O", "-g", "-W", "-f", "-m", "-std=", "--param=")) or arg in ("-pipe", "-pthread", "-v"):
            compile_flags.append(arg)
        elif arg.startswith("-"):
            raise ValueError(f"unsupported Vim port linker option: {arg}")
        elif arg.endswith((".c", ".S", ".s")):
            sources.append(arg)
        else:
            inputs.append(arg)

    with tempfile.TemporaryDirectory(prefix="cc-", dir=config["temporary"]) as temporary:
        objects = []
        for index, source in enumerate(sources):
            obj = str(Path(temporary) / f"source{index}.o")
            subprocess.run([*cc, *compile_flags, "-c", source, "-o", obj], check=True)
            objects.append(obj)
        subprocess.run([
            *config["lld"], *config["ldflags"], "-o", output, *objects,
            *inputs, "--start-group", *config["runtime"], "--end-group",
        ], check=True)


if __name__ == "__main__":
    try:
        main()
    except (ValueError, StopIteration, OSError, subprocess.CalledProcessError) as error:
        print(f"Vim port compiler: {error}", file=sys.stderr)
        sys.exit(error.returncode if isinstance(error, subprocess.CalledProcessError) else 1)
