#!/usr/bin/env bash
set -euo pipefail

[[ $# -eq 2 ]] || { echo "usage: $0 <project-root> <mmake-output>" >&2; exit 2; }
root=$1
mmake_out=$2
toolchain=$(tr -d '[:space:]' < "$root/build/rust-std-toolchain")
overlay=$root/out/rust-std/sysroot-overlay
temporary=$overlay.new
base=$(rustc "+$toolchain" --print sysroot)

rm -rf "$temporary"
mkdir -p "$temporary/bin" "$temporary/lib/rustlib/src/rust/library" "$temporary/lib/rustlib/src/rust/src/llvm-project"

for path in "$base/bin"/*; do
    name=${path##*/}
    [[ $name == rustc || $name == rustdoc ]] || ln -s "$path" "$temporary/bin/$name"
done
for tool in rustc rustdoc; do
    cat > "$temporary/bin/$tool" <<WRAPPER
#!/usr/bin/env bash
set -euo pipefail
exec "$base/bin/$tool" --sysroot "$overlay" "\$@"
WRAPPER
    chmod 0755 "$temporary/bin/$tool"
done

for path in "$base/lib"/*; do
    name=${path##*/}
    [[ $name == rustlib ]] || ln -s "$path" "$temporary/lib/$name"
done
for path in "$base/lib/rustlib"/*; do
    name=${path##*/}
    [[ $name == src ]] || ln -s "$path" "$temporary/lib/rustlib/$name"
done

fork_library=$root/libraries/rust/library
for path in "$fork_library"/*; do
    name=${path##*/}
    [[ $name == backtrace || $name == vendor ]] && continue
    if [[ $name == std ]]; then
        mkdir -p "$temporary/lib/rustlib/src/rust/library/std"
        cp -a --symbolic-link "$path/." "$temporary/lib/rustlib/src/rust/library/std/"
    else
        ln -s "$path" "$temporary/lib/rustlib/src/rust/library/$name"
    fi
done
ln -s "$root/libraries/rust/src/mochios-backtrace" "$temporary/lib/rustlib/src/rust/library/backtrace"
ln -s "$root/libraries/rust/src/mochios-libunwind" "$temporary/lib/rustlib/src/rust/src/llvm-project/libunwind"

vendor=$root/libraries/rust/vendor/rustc-literal-escaper
[[ -d $vendor ]] || { echo "fatal: missing vendored rustc-literal-escaper" >&2; exit 1; }
mkdir -p "$temporary/lib/rustlib/src/rust/vendor" "$temporary/lib/rustlib/src/rust/library/vendor"
ln -s "$vendor" "$temporary/lib/rustlib/src/rust/vendor/rustc-literal-escaper"
ln -s "$vendor" "$temporary/lib/rustlib/src/rust/library/vendor/rustc-literal-escaper"

touch "$temporary/.ready"
rm -rf "$overlay"
mv "$temporary" "$overlay"
