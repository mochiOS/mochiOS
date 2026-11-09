#!/usr/bin/env python3
"""Resolve a local mochiOS exception report using exact archived ELF hashes."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess


def identifier(value):
    if not isinstance(value, str) or len(value) != 64 or any(c not in '0123456789abcdef' for c in value):
        raise ValueError('invalid image SHA-256')
    return value


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('report', type=Path)
    parser.add_argument('--symbols', type=Path, required=True)
    parser.add_argument('--addr2line', default='addr2line')
    parser.add_argument('--json', action='store_true')
    args = parser.parse_args()
    if args.report.stat().st_size > 64 * 1024:
        raise ValueError('report exceeds the local report size limit')
    report = json.loads(args.report.read_text())
    if report.get('schema_version') != 1 or report.get('evidence') != 'native':
        raise ValueError('unsupported report schema/evidence')
    if report.get('kind') != 'exception_termination' or not report.get('exception'):
        raise ValueError(f'no native exception location: kind={report.get("kind")}, exit_code={report.get("exit_code")}')
    pc = int(report['exception']['instruction'], 16)
    if not 0 < pc < 0x800000000000:
        raise ValueError('invalid user instruction address')
    modules = (report.get('image') or {}).get('modules', [])
    candidates = [module for module in modules if any(
        region['flags'] & 1 and region['start'] <= pc < region['end'] for region in module['ranges'])]
    if len(candidates) > 1:
        raise ValueError('ambiguous captured module ranges')
    if candidates:
        module = candidates[0]
        source = identifier(module['source_sha256'])
        bias = module['load_bias']
        if not isinstance(bias, int) or not 0 <= bias <= pc:
            raise ValueError('invalid captured relocation bias')
        address = pc - bias
        label = module['path']
    else:
        source = identifier(report['image_sha256'])
        address = pc
        label = '<native image>'
    directory = args.symbols / 'objects' / source
    metadata = json.loads((directory / 'image.json').read_text())
    if metadata.get('schema_version') != 1 or metadata['image_sha256'] != source:
        raise ValueError('symbol object identity mismatch')
    if metadata['architecture'] != report['architecture']:
        raise ValueError('symbol architecture mismatch')
    if not candidates and metadata['elf_type'] != 2:
        raise ValueError('relocatable image has no captured placement; refusing to guess its bias')
    if not any(region['flags'] & 1 and region['address'] <= address < region['address'] + region['size']
               for region in metadata['load_segments']):
        raise ValueError('instruction does not belong to an executable source segment')
    debug = directory / 'image.debug'
    with debug.open('rb') as stream:
        actual = hashlib.file_digest(stream, 'sha256').hexdigest()
    if actual != identifier(metadata['debug_sha256']):
        raise ValueError('debug information SHA-256 mismatch')
    result = subprocess.run([args.addr2line, '-C', '-f', '-i', '-e', str(debug.resolve()), hex(address)],
                            check=True, text=True, capture_output=True, timeout=30)
    lines = result.stdout.strip().splitlines()
    frames = [dict(function=lines[i], location=lines[i + 1]) for i in range(0, len(lines) - 1, 2)]
    output = dict(module=label, source_sha256=source, instruction=hex(pc),
                  source_address=hex(address), has_line_information=metadata['has_line_information'], frames=frames)
    if args.json:
        print(json.dumps(output, ensure_ascii=False, indent=2))
    else:
        print(f'{label}: {hex(pc)} -> {source} @ {hex(address)}')
        for frame in frames:
            print(f'  {frame["function"]} — {frame["location"]}')
    if not frames or all(frame['function'] == '??' and frame['location'].startswith('??') for frame in frames):
        raise ValueError('no symbol/line information for this address')


if __name__ == '__main__':
    try:
        main()
    except (ValueError, OSError, KeyError, TypeError, subprocess.SubprocessError) as error:
        raise SystemExit(f'symbolize-crash: {error}')
