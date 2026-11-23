#!/usr/bin/env python3
"""Refresh the workspace lock and build repository-owned MDP producers."""
import argparse
import os
from pathlib import Path
import subprocess
import sys


def build(root, jobs, run=subprocess.run):
    """Record current inputs and stop immediately if any producer fails."""
    if jobs < 1:
        raise ValueError('jobs must be a positive integer')
    root = root.resolve()
    mdp = [sys.executable, str(root / 'tools/mdp/mdp'), 'workspace']
    workspace = str(root / 'mdp.workspace.toml')
    for label, command in (
        ('Record workspace inputs', mdp + ['lock', '--file', workspace]),
        ('Build MDP producers', mdp + ['build', '--file', workspace, '-j', str(jobs)]),
    ):
        print('[build] ' + label, flush=True)
        run(command, cwd=root, check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, required=True)
    parser.add_argument('--jobs', type=int, default=int(os.environ.get('MMAKE_JOBS', '4')))
    args = parser.parse_args()
    try:
        build(args.root, args.jobs)
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        print('workspace build: ' + str(error), file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
