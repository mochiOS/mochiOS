#!/usr/bin/env python3
"""Export a generic Rust application build description from the development SDK."""
import argparse
import json
import pathlib
import shlex
import tomllib


def export(root, sdk, output):
    """Describe the existing SDK; this does not build or install any application."""
    root, sdk = root.resolve(), sdk.resolve()
    overlay = root / 'out/rust-std/sysroot-overlay'
    required = [sdk / 'sysroot/lib/libc.a', sdk / 'lib/crt0.o', sdk / 'share/x86_64-unknown-mochios.json', overlay / 'bin/rustc']
    for path in required:
        if not path.is_file():
            raise ValueError(f'SDK input is missing: {path}; build runtime and rust-sysroot first')
    patches = []
    for line in (root / 'build/mmake/toolchain.mk').read_text().splitlines():
        if line.startswith('CARGO_PATCHES ') and (' := ' in line or ' += ' in line):
            args = shlex.split(line.split('= ', 1)[1].replace('$(ROOT)', str(root)))
            if len(args) % 2 or any(args[i] != '--config' for i in range(0, len(args), 2)):
                raise ValueError('Unsupported SDK Cargo configuration')
            patches.extend(args[1::2])
    flags = ['--remap-path-prefix', '/home/jnix=/build', '--remap-path-prefix', f'{root}=/src/mochios']
    for value in ['linker=x86_64-elf-gcc', f'link-arg=--sysroot={sdk}/sysroot', f'link-arg=-L{sdk}/sysroot/lib',
                  'link-arg=-static', 'link-arg=-nostdlib', 'link-arg=-nostartfiles', f'link-arg=-Wl,-T,{sdk}/lib/linker.ld',
                  'link-arg=-Wl,-no-pie', 'link-arg=-Wl,-z,noexecstack', 'link-arg=-Wl,--start-group',
                  f'link-arg={sdk}/lib/crt0.o', f'link-arg={sdk}/lib/libmochi_user_newlib_runtime.a',
                  'link-arg=-lc', 'link-arg=-lm', 'link-arg=-lgcc', 'link-arg=-Wl,--end-group']:
        flags += ['-C', value]
    version = tomllib.loads((root / 'version.toml').read_text())
    result = dict(format=1, sdk=str(sdk), cargo=str(overlay / 'bin/cargo'), rustc=str(overlay / 'bin/rustc'),
                  rustdoc=str(overlay / 'bin/rustdoc'), target=str(sdk / 'share/x86_64-unknown-mochios.json'),
                  target_dir=str(root / 'out/rust-std/target'), rustflags=flags, cargo_config=patches,
                  appcore=str(root / 'tools/devkit/crates/appcore'),
                  fonts={name: str(root / 'libraries/fonts/out/fonts' / file) for name, file in
                         [('VIEWKIT_UI_FONT_PATH', 'InterVariable.ttf'), ('VIEWKIT_MONOSPACE_FONT_PATH', 'UDEVGothic-Regular.ttf'), ('VIEWKIT_JAPANESE_FONT_PATH', 'IBMPlexSansJP-Regular.ttf')]},
                  versions={'MOCHIOS_VERSION': str(version.get('release', '26.0.0')), 'MOCHIOS_BUILD_NUMBER': str(version.get('build', 0))})
    output.parent.mkdir(parents=True, exist_ok=True)
    text = json.dumps(result, indent=2) + '\n'
    if not output.exists() or output.read_text() != text:
        temporary = output.with_suffix('.json.new')
        temporary.write_text(text)
        temporary.replace(output)
    print(output)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=pathlib.Path, required=True)
    parser.add_argument('--sdk', type=pathlib.Path, required=True)
    parser.add_argument('--output', type=pathlib.Path, required=True)
    args = parser.parse_args()
    export(args.root, args.sdk, args.output)
