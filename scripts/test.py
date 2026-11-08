#!/usr/bin/env python3
from pathlib import Path
from testing.cli import main

if __name__ == "__main__":
    raise SystemExit(main(Path(__file__).resolve().parents[1]))
