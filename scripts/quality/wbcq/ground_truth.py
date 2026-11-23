#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Ground truth for the WBCQ P3 corpus: the minimal observable machine of each
state machine, computed without Fission.

The corpus source is compiled natively (`build_corpus.py` writes the shared
library). For each step function, breadth-first search from the zeroed context
over every input byte enumerates the reachable contexts; each context's
observable behaviour is its result per input and its successor per input. Moore
partition refinement then merges contexts no input word tells apart. The class
count is the minimal Mealy machine over the byte alphabet, the number the
quotient's state count must equal.

Inputs are bytes: every corpus machine reads `in & 0xff` or compares `in`
against small constants, so values past 255 behave like some byte. A machine
whose reachable contexts exceed --max-contexts is reported as `too_large`.

  python3 scripts/quality/wbcq/ground_truth.py ~/fission-benchmark/wbcq-p3/corpus/manifest.json
"""

from __future__ import annotations

import argparse
import ctypes
import json
from collections import deque
from pathlib import Path

INPUTS = range(256)


def explore(step, size: int, limit: int):
    start = bytes(size)
    index = {start: 0}
    contexts = [start]
    rows: list[list[tuple[int, int]]] = []
    queue = deque([start])
    buffer = ctypes.create_string_buffer(size)
    while queue:
        context = queue.popleft()
        row = []
        for value in INPUTS:
            ctypes.memmove(buffer, context, size)
            result = step(buffer, value)
            successor = buffer.raw[:size]
            if successor not in index:
                if len(contexts) >= limit:
                    return None
                index[successor] = len(contexts)
                contexts.append(successor)
                queue.append(successor)
            row.append((result, index[successor]))
        rows.append(row)
    return rows


def minimize(rows: list[list[tuple[int, int]]]) -> int:
    """Moore refinement: start from classes of equal results, split by the
    class each input leads to, until stable."""
    classes = {}
    block = [classes.setdefault(tuple(r for r, _ in row), len(classes)) for row in rows]
    while True:
        keys = {}
        refined = [
            keys.setdefault((block[s], tuple(block[t] for _, t in row)), len(keys))
            for s, row in enumerate(rows)
        ]
        if len(keys) == len(set(block)):
            return len(keys)
        block = refined


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("manifest", type=Path)
    parser.add_argument("--max-contexts", type=int, default=50_000)
    parser.add_argument("--out", type=Path)
    args = parser.parse_args()
    manifest = json.loads(args.manifest.read_text())
    library = ctypes.CDLL(manifest["native"])
    truth = {}
    for entry in manifest["functions"]:
        if entry["source"] != "state_machines.c":
            continue
        step = getattr(library, entry["name"])
        step.argtypes = [ctypes.c_void_p, ctypes.c_uint]
        step.restype = ctypes.c_uint
        rows = explore(step, entry["context_bytes"], args.max_contexts)
        if rows is None:
            truth[entry["name"]] = {"status": "too_large", "contexts_over": args.max_contexts}
        else:
            truth[entry["name"]] = {"status": "ok", "contexts": len(rows),
                                    "minimal_states": minimize(rows)}
        print(f"{entry['name']:20} {truth[entry['name']]}")
    if args.out:
        args.out.write_text(json.dumps(truth, indent=2) + "\n")


if __name__ == "__main__":
    main()
