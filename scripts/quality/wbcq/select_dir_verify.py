#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Select step-shaped functions from the `dir-verify` GNU programs for the
WBCQ P4 reach measurement (RFC #1093).

The `dir-verify` binaries are stripped, so the selection P3 used -- a DWARF
signature `<integer> f(<struct> *ctx, <integer> in)` -- has nothing to read in
them. Each one is matched to the unstripped build with a byte-identical
`.text` (same section address, size and hash), and that build's DWARF names
the candidates. Only the DWARF is read from the match; addresses are those of
the matched `.text`, which are the stripped binary's own. A binary with no
byte-identical match is skipped and reported.

The selection reads only the signature, as `select_decbench.py` does: the
body (calls, globals, loops) is not filtered. Every candidate is kept (no
sampling); one copy per (binary, function).

  python3 scripts/quality/wbcq/select_dir_verify.py ~/fission-benchmark/dir-verify/binaries \\
      --debug-root ~/fission-benchmark/decbench-data/binaries \\
      --out ~/fission-benchmark/wbcq-p4/dir_verify_candidates.json
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

from elftools.elf.elffile import ELFFile

from select_decbench import candidates


def text_key(path: Path) -> tuple[int, int, str] | None:
    try:
        with path.open("rb") as handle:
            elf = ELFFile(handle)
            if elf["e_machine"] != "EM_X86_64":
                return None
            text = elf.get_section_by_name(".text")
            if text is None:
                return None
            return (text["sh_addr"], text["sh_size"], hashlib.sha256(text.data()).hexdigest())
    except Exception:  # not an ELF pyelftools can read
        return None


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("binaries", type=Path, help="directory of stripped *.elf binaries")
    parser.add_argument("--debug-root", type=Path, required=True,
                        help="tree of unstripped builds to find byte-identical .text in")
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    stripped = sorted(args.binaries.expanduser().glob("*.elf"))
    wanted = {}
    for path in stripped:
        key = text_key(path)
        if key is not None:
            wanted.setdefault(key, []).append(path)
    matches: dict[Path, Path] = {}
    for candidate in sorted(args.debug_root.expanduser().rglob("*")):
        if not candidate.is_file() or candidate.stat().st_size < 1024:
            continue
        key = text_key(candidate)
        if key in wanted:
            for path in wanted[key]:
                matches.setdefault(path, candidate)
        if len(matches) == len(stripped):
            break
    rows = []
    skipped = []
    for path in stripped:
        debug = matches.get(path)
        if debug is None:
            skipped.append(path.name)
            continue
        found = candidates(debug)
        seen = set()
        for row in found:
            if row["address"] in seen:
                continue
            seen.add(row["address"])
            config, _, rest = path.stem.partition("_")
            rows.append({"config": config, "project": rest.split("_")[0], "opt": config,
                         "binary": str(path), "debug_info_from": str(debug), **row})
        print(f"{path.name:36} <- {debug}  {len(seen)} step-shaped")
    for name in skipped:
        print(f"{name:36} no byte-identical unstripped build; skipped")
    args.out.write_text(json.dumps({"population": len(rows), "sample": rows,
                                    "skipped": skipped}, indent=2) + "\n")
    print(f"{len(rows)} step-shaped functions in {len(stripped) - len(skipped)} binaries")


if __name__ == "__main__":
    main()
