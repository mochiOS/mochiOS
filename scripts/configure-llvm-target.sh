#!/usr/bin/env bash
# Configure target LLVM with real compile/link probes and a static mochiOS ABI.
set -euo pipefail
if [[ $# -lt 1 || $# -gt 2 ]]; then
    echo "usage: $0 LLVM_SOURCE [LLVM_OUTPUT_DIRECTORY]" >&2
    exit 2
fi
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
source_tree=$(realpath -- "$1")
output=$(realpath -m -- "${2:-$root/out/llvm-port}")
sdk=$(realpath -- "${MOCHIOS_SDK:-$root/out/newlib-port/sdk}")
runtime=$(realpath -- "${MOCHIOS_RUNTIME_ARCHIVE:-$sdk/lib/libmochi_user_newlib_runtime.a}")
cc=$(command -v "${CC:-clang}")
cxx=$(command -v "${CXX:-clang++}")
lld=${MOCHIOS_LLD:-$(command -v ld.lld || true)}
[[ -n "$lld" ]] || { echo 'set MOCHIOS_LLD to a host ld.lld or rust-lld executable' >&2; exit 1; }
lld=$(realpath -- "$lld")
for input in "$source_tree/llvm/CMakeLists.txt" "$runtime" "$lld" \
    "$output/build-host-tools/bin/llvm-tblgen" "$output/build-host-tools/bin/clang-tblgen" \
    "$output/build-runtimes/lib/libc++.a" "$output/build-runtimes/lib/libc++abi.a" \
    "$output/build-runtimes/lib/libunwind.a" "$sdk/lib/libgcc.a" \
    "$sdk/sysroot/lib/libc.a" "$sdk/sysroot/lib/libm.a"; do
    [[ -s "$input" ]] || { echo "required input is missing: $input" >&2; exit 1; }
done
for patch_name in llvm-18-mochios-path.patch clang-18-mochios-toolchain.patch; do
    patch_file="$root/scripts/llvm/patches/$patch_name"
    if patch --batch --dry-run --forward -p1 -d "$source_tree" < "$patch_file" >/dev/null 2>&1; then
        patch --batch --forward -p1 -d "$source_tree" < "$patch_file"
    elif ! patch --batch --dry-run --reverse -p1 -d "$source_tree" < "$patch_file" >/dev/null 2>&1; then
        echo "LLVM patch does not match the supplied source: $patch_name" >&2
        exit 1
    fi
done
mkdir -p "$output/cmake/Platform" "$output/bin"
"$cc" --target=x86_64-unknown-none-elf -c "$root/user/runtime/crt0.S" -o "$output/crt0.o"

python3 - "$root" "$output" "$sdk" "$runtime" "$cc" "$cxx" "$lld" <<'PY'
import os
from pathlib import Path
import shlex
import sys

root, output, sdk, runtime, cc, cxx, lld = map(Path, sys.argv[1:])
platform = output / "cmake/Platform"
(platform / "mochiOS-Initialize.cmake").write_text("set(UNIX 1)\n")
(platform / "mochiOS.cmake").write_text(
    'set(UNIX 1)\nset(CMAKE_DL_LIBS "")\n'
    'set_property(GLOBAL PROPERTY TARGET_SUPPORTS_SHARED_LIBS FALSE)\n'
    'include(Platform/UnixPaths)\n'
)
linker = output / "bin/ld.lld"
if linker.is_symlink():
    linker.unlink()
elif linker.exists():
    raise SystemExit(f"refusing to replace a non-symlink linker: {linker}")
linker.symlink_to(lld)

def words(values):
    return shlex.join(map(str, values))

def setting(name, value):
    value = str(value).replace("\\", "\\\\").replace('"', '\\"')
    return f'set({name} "{value}")\n'

cflags = words([
    "-D__mochios__=1", "-D_GNU_SOURCE=1", "-mno-red-zone", "-fno-pie",
    "-isystem" + str(root / "user/libc-port/include"),
    "-isystem" + str(sdk / "sysroot/include"),
])
cxxflags = words([
    "-nostdinc++", "-isystem" + str(output / "build-runtimes/include/c++/v1"),
]) + " " + cflags
libraries = [output / "crt0.o", output / "build-runtimes/lib/libc++.a",
             output / "build-runtimes/lib/libc++abi.a", output / "build-runtimes/lib/libunwind.a",
             runtime]
if extra := os.environ.get("MOCHIOS_REENT_OBJECT"):
    extra = Path(extra).resolve(strict=True)
    libraries.append(extra)
libraries += [sdk / "sysroot/lib/libc.a", sdk / "sysroot/lib/libm.a", sdk / "lib/libgcc.a"]
settings = {
    "CMAKE_SYSTEM_NAME": "mochiOS", "CMAKE_SYSTEM_PROCESSOR": "x86_64",
    "CMAKE_C_COMPILER": cc, "CMAKE_CXX_COMPILER": cxx, "CMAKE_ASM_COMPILER": cc,
    "CMAKE_C_COMPILER_TARGET": "x86_64-unknown-none-elf",
    "CMAKE_CXX_COMPILER_TARGET": "x86_64-unknown-none-elf",
    "CMAKE_ASM_COMPILER_TARGET": "x86_64-unknown-none-elf",
    "CMAKE_SYSROOT": sdk / "sysroot", "CMAKE_FIND_ROOT_PATH": sdk / "sysroot",
    "CMAKE_FIND_ROOT_PATH_MODE_LIBRARY": "ONLY", "CMAKE_FIND_ROOT_PATH_MODE_INCLUDE": "ONLY",
    "CMAKE_FIND_ROOT_PATH_MODE_PACKAGE": "ONLY", "CMAKE_FIND_ROOT_PATH_MODE_PROGRAM": "NEVER",
    "CMAKE_C_FLAGS_INIT": cflags, "CMAKE_CXX_FLAGS_INIT": cxxflags,
    "CMAKE_LINKER": linker,
    "CMAKE_EXE_LINKER_FLAGS_INIT": words([
        "-static", "--no-pie", "-T", root / "user/runtime/linker.ld", "-z", "noexecstack",
        "-L" + str(sdk / "sysroot/lib"),
    ]),
    "CMAKE_C_STANDARD_LIBRARIES": words(["--start-group", *libraries, "--end-group"]),
    "CMAKE_CXX_STANDARD_LIBRARIES": words(["--start-group", *libraries, "--end-group"]),
    "CMAKE_C_LINK_EXECUTABLE": "<CMAKE_LINKER> <LINK_FLAGS> <OBJECTS> -o <TARGET> <LINK_LIBRARIES>",
    "CMAKE_CXX_LINK_EXECUTABLE": "<CMAKE_LINKER> <LINK_FLAGS> <OBJECTS> -o <TARGET> <LINK_LIBRARIES>",
}
module_path = str(output / "cmake").replace('"', '\\"')
(output / "cmake/mochios-toolchain.cmake").write_text(
    f'list(PREPEND CMAKE_MODULE_PATH "{module_path}")\n'
    + "".join(setting(name, value) for name, value in settings.items())
)
PY

cmake --fresh -S "$source_tree/llvm" -B "$output/build-target" -G 'Unix Makefiles' \
    "-DCMAKE_TOOLCHAIN_FILE=$output/cmake/mochios-toolchain.cmake" \
    '-DLLVM_ENABLE_PROJECTS=clang;lld' -DLLVM_TARGETS_TO_BUILD=X86 -DCMAKE_BUILD_TYPE=Release \
    -DLLVM_INCLUDE_TESTS=OFF -DCLANG_INCLUDE_TESTS=OFF -DLLVM_INCLUDE_BENCHMARKS=OFF \
    -DLLVM_INCLUDE_EXAMPLES=OFF -DLLVM_ENABLE_ZLIB=OFF -DLLVM_ENABLE_ZSTD=OFF \
    -DLLVM_ENABLE_TERMINFO=OFF -DLLVM_ENABLE_LIBXML2=OFF \
    -DLLVM_HOST_TRIPLE=x86_64-unknown-mochios -DLLVM_DEFAULT_TARGET_TRIPLE=x86_64-unknown-mochios \
    "-DLLVM_NATIVE_TOOL_DIR=$output/build-host-tools/bin"
