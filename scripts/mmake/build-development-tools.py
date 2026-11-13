#!/usr/bin/env python3
"""Prepare native development tools only when enabled in the OS configuration."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import tarfile
import tempfile

VERSION = '18.1.8'
ARCHIVE = f'llvm-project-{VERSION}.src.tar.xz'
SHA256 = '0b58557a6d32ceee97c8d533a59b9212d87e0fc4d2833924eb6c611247db2f2a'
URL = f'https://github.com/llvm/llvm-project/releases/download/llvmorg-{VERSION}/{ARCHIVE}'


def configuration(path):
    values = {}
    for line in path.read_text().splitlines():
        if line.strip() and not line.startswith('#'):
            key, value = line.split('=', 1)
            values[key] = shlex.split(value)[0]
    enabled = values.get('USER_INSTALL_DEVELOPER_TOOLS', 'n')
    if enabled not in ('y', 'n'):
        raise ValueError('USER_INSTALL_DEVELOPER_TOOLS must be y or n')
    jobs = int(values.get('USER_DEVELOPER_TOOLS_JOBS', '2'))
    if jobs < 1:
        raise ValueError('USER_DEVELOPER_TOOLS_JOBS must be a positive integer')
    return enabled == 'y', jobs


def write_json(path, value):
    text = json.dumps(value, sort_keys=True, separators=(',', ':')) + '\n'
    if path.exists() and path.read_text() == text:
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix('.new')
    temporary.write_text(text)
    temporary.replace(path)


def files(*paths):
    result = []
    for path in paths:
        if path.is_dir():
            result.extend(p for p in sorted(path.rglob('*')) if p.is_file())
        else:
            result.append(path)
    return result


def identity(path):
    metadata = path.stat()
    if not path.is_file():
        raise ValueError(f'Missing development tool input: {path}')
    return [str(path), metadata.st_dev, metadata.st_ino, metadata.st_size, metadata.st_mtime_ns]


def outputs(paths):
    for path in paths:
        if path.is_dir() and not files(path):
            raise ValueError(f'Empty development tool output directory: {path}')
        if path.is_file() and not path.stat().st_size:
            raise ValueError(f'Empty development tool output: {path}')
    return [identity(p) for p in files(*paths)]


class Builder:
    def __init__(self, root, jobs, cargo):
        self.root, self.jobs, self.cargo = root, jobs, cargo
        self.output = root / 'out/llvm-port'
        self.sdk = root / 'out/newlib-port/sdk'
        self.environment = os.environ.copy()
        self.environment['TMPDIR'] = str(self.output / 'tmp')
        (self.output / 'tmp').mkdir(parents=True, exist_ok=True)

    def run(self, command):
        print('[development-tools] ' + shlex.join(map(str, command)), flush=True)
        subprocess.run(list(map(str, command)), env=self.environment, check=True)

    def source(self):
        source = self.output / 'src' / f'llvm-project-{VERSION}.src'
        marker = source / '.mochios-source.json'
        if marker.exists() and json.loads(marker.read_text()) == {'sha256': SHA256}:
            if all((source / name).is_dir() for name in ('llvm', 'clang', 'lld', 'runtimes')):
                return source
        if source.exists():
            raise ValueError(f'Refusing to replace an unmanaged or incomplete LLVM source tree: {source}')
        archive = self.output / ARCHIVE
        if not archive.exists():
            temporary = archive.with_suffix('.download')
            self.run(['curl', '--fail', '--location', '--retry', '3', '--output', temporary, URL])
            with temporary.open('rb') as stream:
                if hashlib.file_digest(stream, 'sha256').hexdigest() != SHA256:
                    temporary.unlink()
                    raise ValueError('LLVM source checksum mismatch')
            temporary.replace(archive)
        with archive.open('rb') as stream:
            if hashlib.file_digest(stream, 'sha256').hexdigest() != SHA256:
                raise ValueError(f'LLVM source checksum mismatch: {archive}')
        source.parent.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix='.llvm-source-', dir=source.parent) as temporary:
            with tarfile.open(archive) as bundle:
                bundle.extractall(temporary, filter='data')
            extracted = Path(temporary) / source.name
            if not all((extracted / name).is_dir() for name in ('llvm', 'clang', 'lld', 'runtimes')):
                raise ValueError('LLVM source archive has an unexpected layout')
            write_json(extracted / marker.name, {'sha256': SHA256})
            extracted.rename(source)
        return source

    def stage(self, name, inputs, products, action, options=()):
        state = self.output / 'state' / (name + '.json')
        key = {'inputs': [identity(p) for p in files(*inputs)], 'options': options}
        # Cache scheduling-independent commands; -j changes never rebuild LLVM.
        signature = hashlib.sha256(json.dumps(key, sort_keys=True).encode()).hexdigest()
        try:
            old = json.loads(state.read_text())
            if old == {'signature': signature, 'outputs': outputs(products)}:
                print(f'[development-tools] {name}: unchanged', flush=True)
                return
        except (OSError, ValueError):
            pass
        # Never retain a success receipt after a failed build or missing output.
        state.unlink(missing_ok=True)
        action()
        write_json(state, {'signature': signature, 'outputs': outputs(products)})

    def build(self):
        if not self.cargo:
            raise ValueError('The native Cargo build command is required when tools are enabled')
        cc = shutil.which(os.environ.get('CC', 'clang'))
        cxx = shutil.which(os.environ.get('CXX', 'clang++'))
        if not cc or not cxx:
            raise ValueError('Host clang and clang++ are required to cross-build LLVM')
        for name in ('cmake', 'make', 'patch', 'curl'):
            if not shutil.which(name):
                raise ValueError(f'Required host tool is missing: {name}')
        lld = os.environ.get('MOCHIOS_LLD') or shutil.which('ld.lld')
        if not lld:
            base = subprocess.check_output(['rustc', '+nightly-2026-05-14', '--print', 'sysroot'], text=True).strip()
            host = next(line.split(': ', 1)[1] for line in subprocess.check_output(
                ['rustc', '+nightly-2026-05-14', '-vV'], text=True).splitlines() if line.startswith('host: '))
            lld = str(Path(base) / 'lib/rustlib' / host / 'bin/rust-lld')
        lld = Path(lld).resolve(strict=True)
        self.environment.update(CC=cc, CXX=cxx, JOBS=str(self.jobs), MOCHIOS_LLD=str(lld), MOCHIOS_SDK=str(self.sdk))
        # Do not inherit one-off runtime overrides from a prior manual port build.
        for name in ('MOCHIOS_RUNTIME_ARCHIVE', 'MOCHIOS_REENT_OBJECT'):
            self.environment.pop(name, None)
        source = self.source()
        scripts = self.root / 'scripts'
        patches = scripts / 'llvm/patches'
        own_inputs = [Path(__file__).resolve(), source / '.mochios-source.json']
        compilers = [Path(cc).resolve(), Path(cxx).resolve(), lld]
        sdk = files(self.sdk / 'lib', self.sdk / 'sysroot')
        host = self.output / 'build-host-tools'
        generators = [host / 'bin/llvm-tblgen', host / 'bin/clang-tblgen']

        def host_tools():
            self.run(['cmake', '--fresh', '-S', source / 'llvm', '-B', host, '-G', 'Unix Makefiles',
                      '-DLLVM_ENABLE_PROJECTS=clang;lld', '-DLLVM_TARGETS_TO_BUILD=X86',
                      '-DCMAKE_BUILD_TYPE=Release', f'-DCMAKE_C_COMPILER={cc}', f'-DCMAKE_CXX_COMPILER={cxx}',
                      '-DLLVM_INCLUDE_TESTS=OFF', '-DCLANG_INCLUDE_TESTS=OFF', '-DLLVM_INCLUDE_BENCHMARKS=OFF',
                      '-DLLVM_ENABLE_ZLIB=OFF', '-DLLVM_ENABLE_ZSTD=OFF', '-DLLVM_ENABLE_TERMINFO=OFF', '-DLLVM_ENABLE_LIBXML2=OFF'])
            self.run(['cmake', '--build', host, '--parallel', self.jobs, '--target', 'llvm-tblgen', 'clang-tblgen'])
        self.stage('host-tools', own_inputs + compilers, generators, host_tools)
        runtime = self.output / 'build-runtimes'
        libraries = [runtime / 'lib' / name for name in ('libc++.a', 'libc++abi.a', 'libunwind.a')]
        runtime_inputs = own_inputs + compilers + sdk + files(self.root / 'user/libc-port/include') + [
            scripts / 'build-llvm-runtimes.sh', patches / 'libcxx-18-newlib-regex-masks.patch',
            patches / 'libcxxabi-18-pthread-thread-atexit.patch']
        self.stage('runtimes', runtime_inputs, libraries + [runtime / 'include/c++/v1'],
                   lambda: self.run(['bash', scripts / 'build-llvm-runtimes.sh', source, runtime]))
        target = self.output / 'build-target'
        llvm_products = [target / 'bin/clang-18', target / 'bin/lld', target / 'lib/clang/18/include']

        def llvm():
            self.run(['bash', scripts / 'configure-llvm-target.sh', source, self.output])
            self.run(['cmake', '--build', target, '--parallel', self.jobs, '--target', 'clang', 'lld', 'clang-resource-headers'])
        target_inputs = runtime_inputs + generators + libraries + [scripts / 'configure-llvm-target.sh',
            patches / 'llvm-18-mochios-path.patch', patches / 'clang-18-mochios-toolchain.patch',
            self.root / 'user/runtime/crt0.S', self.root / 'user/runtime/linker.ld']
        self.stage('llvm', target_inputs, llvm_products, llvm)
        native = self.root / 'out/rust-std/target/x86_64-unknown-mochios/release'
        native_products = [native / 'ld', native / 'msign']
        rust_inputs = own_inputs + sdk + files(self.root / 'user/crates', self.root / 'core/crates/abi',
            self.root / 'services/mboot-protocol', self.root / 'libraries/libc/src',
            self.root / 'user/targets',
            self.root / 'tools/devkit/crates/msign', self.root / 'tools/devkit/crates/linker-driver',
            self.root / 'tools/devkit/crates/file-publication') + [
                self.root / 'tools/devkit/Cargo.toml', self.root / 'tools/devkit/Cargo.lock',
                self.root / 'user/Cargo.toml', self.root / 'user/Cargo.lock',
                self.root / 'core/Cargo.toml', self.root / 'services/Cargo.toml',
                self.root / 'libraries/libc/Cargo.toml', self.root / 'libraries/libc/build.rs',
                self.root / 'build/rust-std-toolchain',
                self.root / 'out/mmake/fingerprints/rust.sha256',
                self.root / 'out/rust-std/sysroot-overlay/.ready',
                self.root / 'out/rust-std/sysroot-overlay/bin/rustc',
                self.root / 'out/rust-std/sysroot-overlay/bin/cargo']
        cargo = [f'CARGO_BUILD_JOBS={self.jobs}' if arg.startswith('CARGO_BUILD_JOBS=') else arg for arg in self.cargo]
        command = cargo + ['--manifest-path', str(self.root / 'tools/devkit/Cargo.toml'),
            '--config', f"patch.\"https://github.com/mochiOS/syscalls\".mochios-certificate.path='{self.root / 'user/crates/certificate'}'",
            '--no-default-features', '--package', 'mochios-linker-driver', '--package', 'msign', '--bins']
        options = [arg for arg in command if not arg.startswith('CARGO_BUILD_JOBS=')]
        self.stage('native-signing-tools', rust_inputs, native_products, lambda: self.run(command), options)
        # Same format can be validated cheaply before every rootfs staging run.
        return outputs(llvm_products + native_products)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, required=True)
    parser.add_argument('cargo', nargs=argparse.REMAINDER)
    args = parser.parse_args()
    root = args.root.resolve()
    enabled, jobs = configuration(root / '.config')
    receipt = root / 'out/mmake/components/development-tools.json'
    command = args.cargo[1:] if args.cargo[:1] == ['--'] else args.cargo
    products = Builder(root, jobs, command).build() if enabled else []
    write_json(receipt, {'enabled': enabled, 'outputs': products})


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        raise SystemExit(f'development tools: {error}')
