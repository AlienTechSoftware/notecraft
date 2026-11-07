#!/usr/bin/env python3
"""Compatibility entry point for the v0.0.9 artwork generator."""

from pathlib import Path
from runpy import run_path


if __name__ == "__main__":
    run_path(Path(__file__).resolve().parents[1] / "0.0.9.py", run_name="__main__")
