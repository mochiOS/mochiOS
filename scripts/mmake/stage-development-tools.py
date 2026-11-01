#!/usr/bin/env python3
"""Install supplied native tools and SDK inputs; never build or regenerate them."""
import argparse
import hashlib
import os
from pathlib import Path
import shutil
import struct
import tempfile
import tomllib


TOOLS = ("clang", "ld.lld", "ld", "msign")
# Explicit requirements of the native compiler pipeline verified from msh.
# This package has its own identity; no trusted shell/prompt role is assigned.
CAPABILITIES = (
    "fs.read.all", "fs.write.tmp", "fs.write.user", "ipc.client",
    "ipc.server", "process.spawn", "system.random.read",
)
SDK_FILES = (
    "lib/crt0.o", "lib/linker.ld", "lib/libmochi_user_newlib_runtime.a",
    "sysroot/lib/libc.a", "sysroot/lib/libm.a",
)


def required(path: Path) -> Path:
    if not path.is_file() or path.stat().st_size == 0:
        raise ValueError(f"required development input is missing or empty: {path}")
    return path


def executable(path: Path) -> Path:
    required(path)
    with path.open("rb") as stream:
        header = stream.read(64)
        if len(header) != 64 or header[:7] != b"\x7fELF\x02\x01\x01":
            raise ValueError(f"expected a native ELF64 executable: {path}")
        kind, machine = struct.unpack_from("<HH", header, 16)
        offset = struct.unpack_from("<Q", header, 32)[0]
        stride, count = struct.unpack_from("<HH", header, 54)
        if kind != 2 or machine != 62 or stride < 56 or not 0 < count <= 4096:
            raise ValueError(f"expected a static x86_64 executable: {path}")
        if offset + stride * count > path.stat().st_size:
            raise ValueError(f"truncated ELF program headers: {path}")
        loaded = False
        for index in range(count):
            stream.seek(offset + index * stride)
            segment = struct.unpack("<I", stream.read(4))[0]
            if segment == 3:
                raise ValueError(f"native tool must not require an ELF interpreter: {path}")
            loaded |= segment == 1
        if not loaded:
            raise ValueError(f"native tool has no loadable segment: {path}")
    # This verifies the build profile, not the producer's identity or authority.
    return path


def headers(source: Path, destination: Path) -> dict[Path, Path]:
    if not source.is_dir():
        raise ValueError(f"required development header directory is missing: {source}")
    result = {}
    base = source.resolve()
    for path in sorted(source.rglob("*")):
        if path.is_symlink():
            if path.is_dir() or not path.resolve().is_relative_to(base):
                raise ValueError(f"header link escapes its input tree or names a directory: {path}")
        if path.is_file():
            result[destination / path.relative_to(source)] = path
        elif not path.is_dir():
            raise ValueError(f"unsupported development header: {path}")
    if not result:
        raise ValueError(f"development header directory is empty: {source}")
    return result


def package_manifest(prefix: Path) -> str:
    """Bind the actual staged tools, without changing the supplied SDK metadata."""
    lines = [
        "format = 1", "", "[package]", 'id = "org.mochios.development-tools"',
        'name = "Development Tools"', 'version = "1"', 'kind = "binary"',
        'provenance = "built-in"', 'vendor = "mochiOS Project"',
        'architecture = "x86_64"', 'abi = "mochios-1"',
    ]
    for name in TOOLS:
        path = executable(prefix / "bin" / name)
        digest = hashlib.sha256()
        size = 0
        with path.open("rb") as stream:
            for chunk in iter(lambda: stream.read(1024 * 1024), b""):
                digest.update(chunk)
                size += len(chunk)
        lines.extend([
            "", "[[binary]]", f'path = "/system/developer/bin/{name}"',
            'kind = "application"', f'file = "{name}"', "requires = [",
            *(f'    "{capability}",' for capability in CAPABILITIES), "]",
            "", "[[file]]", f'id = "{name}"', f'path = "/system/developer/bin/{name}"',
            f'digest = "sha256:{digest.hexdigest()}"', f"size = {size}", 'mode = "0755"',
        ])
    return "\n".join(lines) + "\n"


def stage_tools(stage: Path, sdk: Path, llvm: Path, rust_tools: Path) -> None:
    manifest = sdk / "manifest.toml"
    metadata = tomllib.loads(manifest.read_text()) if manifest.exists() else {}
    if metadata.get("architecture", "x86_64") != "x86_64":
        raise ValueError("native development SDK architecture must be x86_64")
    runtime = metadata.get("compiler_runtime", "libgcc")
    runtimes = {
        "libgcc": "lib/libgcc.a",
        "compiler-rt": "lib/libclang_rt.builtins-x86_64.a",
    }
    if not isinstance(runtime, str) or runtime not in runtimes:
        raise ValueError(f"unsupported SDK compiler_runtime: {runtime}")
    inputs = {Path(name): required(sdk / name) for name in (*SDK_FILES, runtimes[runtime])}
    if manifest.exists():
        inputs[Path("manifest.toml")] = required(manifest)
    inputs.update(headers(sdk / "sysroot/include", Path("sysroot/include")))
    inputs.update(headers(llvm / "lib/clang/18/include", Path("lib/clang/18/include")))
    for name, source in {
        "clang": llvm / "bin/clang-18", "ld.lld": llvm / "bin/lld",
        "ld": rust_tools / "ld", "msign": rust_tools / "msign",
    }.items():
        inputs[Path("bin") / name] = executable(source)

    destination = stage / "system/developer"
    package = stage / "system/packages/development-tools"
    aliases = {stage / "system/bin" / name: "../developer/bin/" + name for name in TOOLS}
    for path in (destination, package, *aliases):
        if os.path.lexists(path):
            raise ValueError(f"refusing to replace existing development path: {path}")
    for directory in (destination.parent, package.parent, stage / "system/bin"):
        existed = directory.exists()
        directory.mkdir(parents=True, exist_ok=True)
        if not existed:
            directory.chmod(0o755)
    installed = []
    published = []
    try:
        with tempfile.TemporaryDirectory(prefix=".developer-", dir=destination.parent) as temporary:
            pending = Path(temporary) / "sdk"
            pending.mkdir()
            for relative, source in inputs.items():
                target = pending / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(source, target)
                target.chmod(0o755 if relative.parent == Path("bin") else 0o644)
            for directory in (pending, *(p for p in pending.rglob("*") if p.is_dir())):
                directory.chmod(0o755)
            pending_package = Path(temporary) / "package"
            pending_package.mkdir(mode=0o755)
            pending_package.chmod(0o755)
            declaration = pending_package / "manifest.toml"
            declaration.write_text(package_manifest(pending), encoding="utf-8")
            declaration.chmod(0o644)
            pending.rename(destination)
            published.append(destination)
            pending_package.rename(package)
            published.append(package)
        for path, target in aliases.items():
            path.symlink_to(target)
            installed.append(path)
    except BaseException:
        for path in installed:
            path.unlink()
        for path in reversed(published):
            shutil.rmtree(path)
        raise
    print(f"Installed {len(inputs)} supplied development files and {len(aliases)} command links")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("stage", type=Path)
    parser.add_argument("--sdk", required=True, type=Path)
    parser.add_argument("--llvm", required=True, type=Path)
    parser.add_argument("--rust-tools", required=True, type=Path)
    options = parser.parse_args()
    try:
        stage_tools(options.stage, options.sdk, options.llvm, options.rust_tools)
    except (OSError, ValueError) as error:
        parser.exit(1, f"development tools: {error}\n")


if __name__ == "__main__":
    main()
