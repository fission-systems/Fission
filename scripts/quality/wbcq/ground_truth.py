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

Machines that make calls (`call_machines.c`, P4) run against
`call_env_native.c`: each callee records the call and returns a value the
explorer scripts, the way the engine treats a call's return as an input the
environment chooses. A symbol is then an input byte and one return value per
call the step may make (manifest `calls`), drawn from RETURNS -- zero, one,
and an all-ones int, which every corpus caller tells apart only as zero or
not, or passes through. An observation is the result (or that the step did
not return) together with the calls made, each as (callee, argument). After a
step that does not return the machine is halted: every later step observes
only that.

Machines whose context holds pointers (`region_machines.c`, P4 step 2) get a
buffer per pointer field holding the manifest's named table
(`region_tables.py`), the same bytes the engine is given; a context is then
its bytes together with every buffer's.

  python3 scripts/quality/wbcq/ground_truth.py ~/fission-benchmark/wbcq-p3/corpus/manifest.json
"""

from __future__ import annotations

import argparse
import ctypes
import itertools
import json
from collections import deque
from pathlib import Path

from region_tables import region_bytes

INPUTS = range(256)
RETURNS = (0, 1, 0xFFFFFFFF)
HALTED = b"halted"


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


def explore_calls(library, step, size: int, calls: int, limit: int):
    """`explore` for a step that makes up to `calls` calls: the symbol is
    (input, return of each call), the observation includes the calls."""
    run = library.wbcq_run
    run.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.c_uint,
                    ctypes.POINTER(ctypes.c_uint)]
    run.restype = ctypes.c_int
    returns = (ctypes.c_longlong * 8).in_dll(library, "wbcq_returns")
    returns_len = ctypes.c_int.in_dll(library, "wbcq_returns_len")
    made = ctypes.c_int.in_dll(library, "wbcq_calls")
    call_fn = (ctypes.c_int * 8).in_dll(library, "wbcq_call_fn")
    call_arg = (ctypes.c_longlong * 8).in_dll(library, "wbcq_call_arg")
    step_ptr = ctypes.cast(step, ctypes.c_void_p)
    symbols = [(value, scripted) for value in INPUTS
               for scripted in itertools.product(RETURNS, repeat=calls)]
    start = bytes(size)
    index = {start: 0}
    contexts = [start]
    rows: list[list[tuple[object, int]]] = []
    queue = deque([start])
    buffer = ctypes.create_string_buffer(size)
    result = ctypes.c_uint()
    while queue:
        context = queue.popleft()
        row = []
        for value, scripted in symbols:
            if context == HALTED:
                successor, observed = HALTED, ("halted",)
            else:
                ctypes.memmove(buffer, context, size)
                for n, ret in enumerate(scripted):
                    returns[n] = ctypes.c_longlong(ret).value if ret < 1 << 63 else ret - (1 << 64)
                returns_len.value = len(scripted)
                halted = run(step_ptr, buffer, value, ctypes.byref(result))
                if made.value > len(scripted):
                    raise SystemExit(f"{step}: a step made {made.value} calls, more than "
                                     f"the manifest's {len(scripted)}")
                trace = tuple((call_fn[n], call_arg[n]) for n in range(made.value))
                if halted:
                    successor, observed = HALTED, ("no_return", trace)
                else:
                    successor, observed = buffer.raw[:size], ("returned", result.value, trace)
            if successor not in index:
                if len(contexts) >= limit:
                    return None
                index[successor] = len(contexts)
                contexts.append(successor)
                queue.append(successor)
            row.append((observed, index[successor]))
        rows.append(row)
    return rows


def explore_regions(step, size: int, regions: list[tuple[int, bytes]], limit: int):
    """`explore` for a step whose context holds pointers (`region_machines.c`,
    P4 step 2): each pointer field gets a buffer of its own holding the
    named table, and a context's identity is its bytes together with every
    buffer's. The pointers themselves never change."""
    buffers = [ctypes.create_string_buffer(len(initial)) for _, initial in regions]
    pointer = ctypes.sizeof(ctypes.c_void_p)
    context = bytearray(size)
    for (field, _), buffer in zip(regions, buffers):
        context[field:field + pointer] = ctypes.addressof(buffer).to_bytes(pointer, "little")
    start = (bytes(context), *(initial for _, initial in regions))
    index = {start: 0}
    contexts = [start]
    rows: list[list[tuple[int, int]]] = []
    queue = deque([start])
    ctx = ctypes.create_string_buffer(size)
    while queue:
        state = queue.popleft()
        row = []
        for value in INPUTS:
            ctypes.memmove(ctx, state[0], size)
            for buffer, content in zip(buffers, state[1:]):
                ctypes.memmove(buffer, content, len(content))
            result = step(ctx, value)
            successor = (ctx.raw[:size], *(buffer.raw[:len(content)]
                                           for buffer, content in zip(buffers, state[1:])))
            if successor[0] != state[0] and any(
                    successor[0][f:f + pointer] != state[0][f:f + pointer] for f, _ in regions):
                raise SystemExit(f"{step}: a step moved a region pointer")
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
    calls_library = ctypes.CDLL(manifest["native_calls"]) if "native_calls" in manifest else None
    truth = {}
    for entry in manifest["functions"]:
        if entry["source"] == "region_machines.c":
            step = getattr(library, entry["name"])
            step.argtypes = [ctypes.c_void_p, ctypes.c_uint]
            step.restype = ctypes.c_uint
            regions = [(region["field"], region_bytes(region["table"]))
                       for region in entry["regions"]]
            rows = explore_regions(step, entry["context_bytes"], regions, args.max_contexts)
        elif entry["source"] == "state_machines.c":
            step = getattr(library, entry["name"])
            step.argtypes = [ctypes.c_void_p, ctypes.c_uint]
            step.restype = ctypes.c_uint
            rows = explore(step, entry["context_bytes"], args.max_contexts)
        elif entry["source"] == "call_machines.c" and calls_library is not None:
            step = getattr(calls_library, entry["name"])
            rows = explore_calls(calls_library, step, entry["context_bytes"],
                                 entry.get("calls", 0), args.max_contexts)
        else:
            continue
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
