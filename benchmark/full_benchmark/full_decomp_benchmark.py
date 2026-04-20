#!/usr/bin/env python3
"""CLI entry for whole-binary Fission vs Ghidra benchmark; logic lives in grand_finale_support.benchmark_core."""

from __future__ import annotations

import sys

from grand_finale_support.benchmark_core import main


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:
        print(f"[-] {exc}", file=sys.stderr)
        raise SystemExit(1)
