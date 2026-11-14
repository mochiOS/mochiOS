#!/usr/bin/env python3
"""Compose prebuilt MDPs; never invoke a repository build."""
import os
from pathlib import Path
import subprocess

root = Path(__file__).resolve().parents[2]
raise SystemExit(subprocess.call([
    str(root / 'tools/mdp/mdp'), 'assemble',
    '--lock', os.environ.get('MDP_ASSEMBLY_LOCK', str(root / 'mdp.assembly.lock')),
    '--output', os.environ.get('MDP_OUTPUT', str(root / 'out/mdp/assembled')),
]))
