#!/usr/bin/env bash
# Build target C++ libraries without regenerating or writing into the SDK.
set -euo pipefail

if [[ $# -lt 1 || $# -gt 2 ]]; then
    echo "usage: $0 LLVM_18_1_8_SOURCE [BUILD_DIRECTORY]" >&2
    echo "The supplied disposable LLVM source tree receives the newlib patch." >&2
    exit 2
fi

root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
source_tree=$(realpath -- "$1")
sdk=$(realpath -- "${MOCHIOS_SDK:-$root/out/newlib-port/sdk}")
build_directory=${2:-$root/out/llvm-port/build-runtimes}
cc=${CC:-clang}
cxx=${CXX:-clang++}
jobs=${JOBS:-4}

for input in "$sdk/sysroot/include/stdio.h" "$sdk/sysroot/lib/libc.a" \
    "$source_tree/runtimes/CMakeLists.txt"; do
    [[ -f "$input" ]] || { echo "required input is missing: $input" >&2; exit 1; }
done
for version in 'LLVM_VERSION_MAJOR 18' 'LLVM_VERSION_MINOR 1' 'LLVM_VERSION_PATCH 8'; do
    grep -Fq "$version" "$source_tree/llvm/CMakeLists.txt" || {
        echo "expected LLVM 18.1.8 sources: $source_tree" >&2
        exit 1
    }
done

for patch_name in libcxx-18-newlib-regex-masks.patch libcxxabi-18-pthread-thread-atexit.patch; do
    patch_file="$root/scripts/llvm/patches/$patch_name"
    if patch --batch --dry-run --forward -p1 -d "$source_tree" < "$patch_file" >/dev/null 2>&1; then
        patch --batch --forward -p1 -d "$source_tree" < "$patch_file"
    elif ! patch --batch --dry-run --reverse -p1 -d "$source_tree" < "$patch_file" >/dev/null 2>&1; then
        echo "LLVM runtime patch does not match the supplied source: $patch_name" >&2
        exit 1
    fi
done

flags="-D__mochios__=1 -D_GNU_SOURCE=1 -mno-red-zone -fno-pie"
flags+=" -isystem\"$root/user/libc-port/include\" -isystem\"$sdk/sysroot/include\""
# The runtime does not export __cxa_thread_atexit_impl. Static-library-only
# configure probes cannot detect a missing definition; use libc++abi's real
# pthread-key fallback instead of their cached false positive.
cmake -S "$source_tree/runtimes" -B "$build_directory" -G 'Unix Makefiles' \
    -DCMAKE_SYSTEM_NAME=Generic -DCMAKE_SYSTEM_PROCESSOR=x86_64 \
    -DCMAKE_TRY_COMPILE_TARGET_TYPE=STATIC_LIBRARY \
    "-DCMAKE_C_COMPILER=$cc" "-DCMAKE_CXX_COMPILER=$cxx" "-DCMAKE_ASM_COMPILER=$cc" \
    -DCMAKE_C_COMPILER_TARGET=x86_64-unknown-none-elf \
    -DCMAKE_CXX_COMPILER_TARGET=x86_64-unknown-none-elf \
    -DCMAKE_ASM_COMPILER_TARGET=x86_64-unknown-none-elf \
    "-DCMAKE_SYSROOT=$sdk/sysroot" \
    "-DCMAKE_FIND_ROOT_PATH=$sdk/sysroot" \
    -DCMAKE_FIND_ROOT_PATH_MODE_LIBRARY=ONLY -DCMAKE_FIND_ROOT_PATH_MODE_INCLUDE=ONLY \
    -DCMAKE_FIND_ROOT_PATH_MODE_PACKAGE=ONLY -DCMAKE_FIND_ROOT_PATH_MODE_PROGRAM=NEVER \
    "-DCMAKE_C_FLAGS=$flags" "-DCMAKE_CXX_FLAGS=$flags" \
    -DCMAKE_BUILD_TYPE=Release "-DCMAKE_INSTALL_PREFIX=$build_directory/install" \
    '-DLLVM_ENABLE_RUNTIMES=libcxx;libcxxabi;libunwind' \
    -DLIBCXX_ENABLE_SHARED=OFF -DLIBCXXABI_ENABLE_SHARED=OFF -DLIBUNWIND_ENABLE_SHARED=OFF \
    -DLIBCXX_HAS_PTHREAD_API=ON -DLIBCXXABI_HAS_PTHREAD_API=ON \
    -DLIBCXX_ENABLE_FILESYSTEM=ON -DLIBCXX_ENABLE_LOCALIZATION=ON \
    -DLIBCXX_INCLUDE_TESTS=OFF -DLIBCXX_INCLUDE_BENCHMARKS=OFF \
    -DLIBCXXABI_INCLUDE_TESTS=OFF -DLIBUNWIND_INCLUDE_TESTS=OFF \
    -DLIBUNWIND_IS_BAREMETAL=ON \
    -DLIBCXX_HAS_PTHREAD_LIB=OFF -DLIBCXX_HAS_RT_LIB=OFF -DLIBCXX_HAS_ATOMIC_LIB=OFF \
    -DLIBCXXABI_HAS_PTHREAD_LIB=OFF -DLIBCXXABI_HAS_DL_LIB=OFF \
    -DLIBCXXABI_HAS_CXA_THREAD_ATEXIT_IMPL=OFF \
    -DLIBUNWIND_HAS_PTHREAD_LIB=OFF -DLIBUNWIND_HAS_DL_LIB=OFF
cmake --build "$build_directory" --parallel "$jobs"
