#!/usr/bin/env python3
"""Cross-build terminal Vim using an existing, read-only mochiOS SDK/runtime."""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import tarfile
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent


def digest(path):
    with Path(path).open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def required(path):
    path = Path(path).resolve()
    if not path.is_file() or path.stat().st_size == 0:
        raise ValueError(f"missing or empty Vim build input: {path}")
    return path


def tool(name):
    # Preserve argv[0] for multicall tools (rustup/rustc, ar/ranlib, ld.lld).
    path = Path(shutil.which(name) or name).absolute()
    required(path)
    return str(path)


def build(args):
    sdk = args.sdk.resolve()
    runtime = required(args.runtime)
    out = args.output.resolve()
    if not out.is_relative_to(ROOT / "out") or out == ROOT / "out" or out.is_relative_to(sdk):
        raise ValueError("Vim output must be a dedicated directory under out/, outside the SDK")
    for name in ("lib/crt0.o", "lib/linker.ld", "sysroot/lib/libc.a", "sysroot/lib/libm.a"):
        required(sdk / name)
    metadata = tomllib.loads((sdk / "manifest.toml").read_text()) if (sdk / "manifest.toml").exists() else {}
    compiler_runtime = {
        "libgcc": "lib/libgcc.a", "compiler-rt": "lib/libclang_rt.builtins-x86_64.a",
    }.get(metadata.get("compiler_runtime", "libgcc"))
    if compiler_runtime is None:
        raise ValueError("unsupported SDK compiler_runtime")
    builtins = required(sdk / compiler_runtime)
    cc, msign = tool(args.cc), tool(args.msign)
    if args.lld:
        lld = tool(args.lld)
    elif shutil.which("ld.lld"):
        lld = tool("ld.lld")
    else:
        sysroot = Path(subprocess.check_output([tool("rustc"), "--print", "sysroot"], text=True).strip())
        host = subprocess.check_output([tool("rustc"), "-vV"], text=True).split("host: ", 1)[1].splitlines()[0]
        lld = str(required(sysroot / "lib/rustlib" / host / "bin/rust-lld"))
    lld = [lld, *(["-flavor", "gnu"] if Path(lld).name == "rust-lld" else [])]
    if "LLD" not in subprocess.check_output([*lld, "--version"], text=True):
        raise ValueError("Vim linker backend must be ld.lld")
    if out.exists() and any(out.iterdir()) and not (out / ".vim-port").is_file():
        disabled = list(out.iterdir()) == [out / ".ready"] and (out / ".ready").read_text() == "disabled\n"
        if not disabled:
            raise ValueError(f"refusing to use an unowned output directory: {out}")
    out.mkdir(parents=True, exist_ok=True)
    (out / ".vim-port").touch()
    with (out / "build.lock").open("w") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        (out / ".ready").unlink(missing_ok=True)
        work = out / "work"
        if work.exists():
            shutil.rmtree(work)
        work.mkdir()
        downloads = args.downloads.resolve()
        downloads.mkdir(parents=True, exist_ok=True)
        pins = tomllib.loads((HERE / "sources.toml").read_text())
        for pin in pins.values():
            archive = downloads / pin["archive"]
            if not archive.exists():
                with tempfile.TemporaryDirectory(prefix="download-", dir=downloads) as temporary:
                    fetched = Path(temporary) / "source.tar.gz"
                    subprocess.run(["curl", "--fail", "--location", "--retry", "3", "--output", str(fetched), pin["url"]], check=True)
                    if digest(fetched) != pin["sha256"]:
                        raise ValueError(f"source checksum mismatch: {pin['url']}")
                    fetched.rename(archive)
            if digest(archive) != pin["sha256"]:
                raise ValueError(f"cached source checksum mismatch: {archive}")
            with tarfile.open(archive) as source:
                source.extractall(work, filter="data")

        env = os.environ.copy()
        # Host settings must not select host headers, plugins or libraries.
        for name in (
            "CC", "CPP", "CFLAGS", "CPPFLAGS", "LDFLAGS", "LIBS", "CPATH",
            "C_INCLUDE_PATH", "LIBRARY_PATH", "PKG_CONFIG_PATH", "CONFIG_SITE",
        ):
            env.pop(name, None)
        env.update(CONFIG_SITE="/dev/null", PKG_CONFIG="/usr/bin/false", AR=tool("ar"), RANLIB=tool("ranlib"))
        cflags = [
            f"--sysroot={sdk / 'sysroot'}", "-D__mochios__=1", "-D__unix__=1",
            "-D_GNU_SOURCE=1", "-mno-red-zone", "-fno-pie",
            f"-isystem{ROOT / 'user/libc-port/include'}",
            f"-isystem{sdk / 'sysroot/include'}",
        ]
        objects = []
        for name in ("newlib_reent", "newlib_ioctl"):
            obj = work / f"{name}.o"
            subprocess.run([cc, *cflags, "-O2", "-c", str(ROOT / f"user/runtime/{name}.c"), "-o", str(obj)], env=env, check=True)
            objects.append(str(obj))
        configuration = work / "toolchain.json"
        configuration.write_text(json.dumps({
            "cc": cc, "lld": lld, "cflags": cflags, "temporary": str(work),
            "ldflags": ["-m", "elf_x86_64", "-static", "-no-pie", "-z", "noexecstack", "-T", str(sdk / "lib/linker.ld"), "-L" + str(sdk / "sysroot/lib")],
            "runtime": [str(sdk / "lib/crt0.o"), str(runtime), *objects, "-lc", "-lm", str(builtins)],
        }, indent=2) + "\n")
        wrapper = work / "mochios-cc"
        wrapper.write_text("#!/bin/sh\nexec " + shlex.join([tool("python3"), str(HERE / "cc.py"), str(configuration)]) + ' "$@"\n')
        wrapper.chmod(0o755)
        env.update(CC=str(wrapper), CPP=str(wrapper) + " -E", CFLAGS="-O2 -std=gnu17")

        def run(label, command, directory):
            print(f"Vim: {label}", flush=True)
            with (out / f"{label}.log").open("w") as log:
                subprocess.run([str(arg) for arg in command], cwd=directory, env=env, stdout=log, stderr=subprocess.STDOUT, check=True)

        host = "x86_64-unknown-none"
        machine = subprocess.check_output([tool("cc"), "-dumpmachine"], text=True).strip()
        curses = work / "curses-build"
        curses.mkdir()
        run("ncurses-configure", [
            work / pins["ncurses"]["directory"] / "configure",
            f"--host={host}", f"--build={machine}", "--prefix=/system",
            "--with-termlib", "--without-shared", "--without-debug",
            "--without-ada", "--without-cxx", "--without-cxx-binding",
            "--without-progs", "--without-tests", "--without-manpages",
            "--disable-stripping", "--with-terminfo-dirs=/system/share/terminfo",
            "--with-default-terminfo-dir=/system/share/terminfo",
            "--without-dlsym", "--without-gpm", "--enable-widec",
        ], curses)
        run("ncurses-build", ["make", f"-j{args.jobs}"], curses)
        deps = work / "deps"
        run("ncurses-install", ["make", "install.libs", f"DESTDIR={deps}"], curses)
        env.update(
            CPPFLAGS=f"-I{deps}/system/include/ncursesw",
            LDFLAGS=f"-L{deps}/system/lib", vim_cv_uname_output="mochiOS",
            vim_cv_uname_r_output="26", vim_cv_uname_m_output="x86_64",
            vim_cv_stat_ignores_slash="no",
        )
        vim = work / pins["vim"]["directory"]
        run("vim-configure", [
            vim / "configure", "--cache-file=/dev/null", f"--host={host}",
            f"--build={machine}", "--prefix=/system", "--with-features=normal",
            "--enable-multibyte", "--disable-gui", "--without-x",
            "--without-wayland", "--disable-nls", "--disable-gpm",
            "--disable-selinux", "--disable-smack", "--disable-xsmp",
            "--with-tlib=tinfow",
        ], vim)
        run("vim-build", ["make", "-C", "src", f"-j{args.jobs}"], vim)
        with tempfile.TemporaryDirectory(prefix="stage-", dir=out) as temporary:
            stage = Path(temporary)
            run("vim-install", ["make", "-C", "src", "installvimbin", "installruntime", f"DESTDIR={stage}"], vim)
            run("terminfo", [tool("tic"), "-x", "-e", "xterm,xterm-256color,dumb,screen,screen-256color,tmux,tmux-256color", "-o", stage / "system/share/terminfo", work / pins["ncurses"]["directory"] / "misc/terminfo.src"], work)
            licenses = stage / "system/share/licenses/vim"
            licenses.mkdir(parents=True)
            shutil.copyfile(vim / "LICENSE", licenses / "Vim.txt")
            shutil.copyfile(work / pins["ncurses"]["directory"] / "COPYING", licenses / "ncurses.txt")
            # installvimbin strips the ELF. Sign only the final installed bytes.
            binary = stage / "system/bin/vim"
            run("sign", [msign, "elf", "sign", binary, "--adhoc"], work)
            run("verify", [msign, "elf", "verify", binary], work)
            if (out / "stage").exists():
                shutil.rmtree(out / "stage")
            stage.rename(out / "stage")
        result = {
            "sources": pins, "runtime_sha256": digest(runtime),
            "crt_sha256": digest(sdk / "lib/crt0.o"),
            "vim_sha256": digest(out / "stage/system/bin/vim"),
            "compiler_runtime": compiler_runtime,
            "linker": subprocess.check_output([*lld, "--version"], text=True).strip(),
            "sdk_regenerated": False,
        }
        (out / "build.json").write_text(json.dumps(result, indent=2) + "\n")
        (out / ".ready").write_text(result["vim_sha256"] + "\n")
        print(f"Vim staged: {out / 'stage'}", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sdk", type=Path, default=ROOT / "out/newlib-port/sdk")
    parser.add_argument("--runtime", type=Path, required=True, help="matching native runtime archive; never regenerated here")
    parser.add_argument("--output", type=Path, default=ROOT / "out/vim-port/release")
    parser.add_argument("--downloads", type=Path, default=ROOT / "out/vim-port/downloads")
    parser.add_argument("--cc", default="x86_64-elf-gcc")
    parser.add_argument("--lld", default=os.environ.get("MOCHIOS_VIM_LLD"), help="ld.lld, or the installed Rust toolchain's LLD")
    parser.add_argument("--msign", default=str(ROOT / "out/mmake/host-tools/release/msign"))
    parser.add_argument("--jobs", type=int, default=2)
    args = parser.parse_args()
    if args.jobs < 1:
        parser.error("--jobs must be positive")
    try:
        build(args)
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"Vim build failed: {error}\nSee {args.output}/*.log\n")
