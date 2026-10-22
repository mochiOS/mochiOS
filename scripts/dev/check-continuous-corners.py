#!/usr/bin/env python3
"""Verify shared continuous-corner sources and math tests without a GUI."""
from pathlib import Path
import subprocess
import tempfile

root = Path(__file__).resolve().parents[2]
protocol = root / 'user/crates/viewkit-gpu-protocol/src'
firmware = root / 'libraries/bootui/src'
for name in ('continuous_corner.rs', 'continuous_corner_data.rs'):
    if (protocol / name).read_bytes() != (firmware / name).read_bytes():
        raise SystemExit(f'BootUI copy differs: {name}; export with generate_continuous_corner.py --bootui libraries/bootui/src')
with tempfile.TemporaryDirectory(prefix='mochios-corners-') as temporary:
    for name, source in [('protocol', protocol / 'lib.rs'), ('bootui', firmware / 'lib.rs')]:
        binary = str(Path(temporary) / name)
        subprocess.run(['rustc', '--edition', '2024', '--test', str(source), '-o', binary], check=True)
        subprocess.run([binary], check=True)
