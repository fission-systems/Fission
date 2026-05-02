#!/usr/bin/env python3
"""Pick a decomp address for PE CLI smoke.

MinGW statically linked CRT exposes symbols like ``__mingw_*`` that are not imports
but may lack SLA pcode templates; prefer ``test_functions.c`` symbols first.
"""

from __future__ import annotations

import json
import sys

# Historical fallback used when list ordering differed (documented in CI guide).
DEFAULT_FALLBACK = "0x140001470"
PREFERRED_NAMES = ("add", "main")


def main() -> None:
    data = json.load(sys.stdin)
    if not isinstance(data, list) or len(data) == 0:
        raise SystemExit("expected non-empty JSON array from fission_cli list --json")

    for want in PREFERRED_NAMES:
        for func in data:
            if func.get("is_import"):
                continue
            if not isinstance(func.get("address"), str):
                continue
            if func.get("name") == want:
                print(func["address"])
                return

    addr = next(
        (
            func["address"]
            for func in data
            if not func.get("is_import")
            and isinstance(func.get("address"), str)
            and not str(func.get("name", "")).startswith("__")
        ),
        DEFAULT_FALLBACK,
    )
    print(addr)


if __name__ == "__main__":
    main()
