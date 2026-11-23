#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Run the WBCQ P3 evaluation (RFC #1093) over the built corpus.

For every (ISA, variant, function) of the manifest `build_corpus.py` wrote:

- `p3_eval` (example of fission-quotient): the white-box quotient, a replay of
  long random words against it on the emulator, and the black-box ablation;
- `fission_cli decomp`: the HIR baseline, measured as text (lines, gotos,
  conditionals, loops).

Rows go to <out>/rows.jsonl, one per run; a row already there is not run
again. `summarize` turns rows plus `ground_truth.py`'s output into the tables
of the evaluation report.

  python3 scripts/quality/wbcq/evaluate.py run --corpus ~/fission-benchmark/wbcq-p3/corpus \\
      --out ~/fission-benchmark/wbcq-p3/results --eval target/release/examples/p3_eval \\
      --cli target/release/fission_cli -j 4
  python3 scripts/quality/wbcq/evaluate.py summarize --out ~/fission-benchmark/wbcq-p3/results \\
      --truth ~/fission-benchmark/wbcq-p3/ground_truth.json

Real binaries (`select_decbench.py` candidates):

  python3 scripts/quality/wbcq/evaluate.py decbench \\
      --candidates ~/fission-benchmark/wbcq-p3/decbench_candidates.json \\
      --out ~/fission-benchmark/wbcq-p3/decbench --eval target/release/examples/p3_eval -j 8
  python3 scripts/quality/wbcq/evaluate.py summarize-decbench --out ~/fission-benchmark/wbcq-p3/decbench
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import threading
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from statistics import median

VARIANT_ORDER = ("O0", "O1", "O1-fla", "O1-bcf-fla")


def hir_metrics(cli: str, binary: str, name: str) -> dict:
    out = subprocess.run([cli, "--no-db", "decomp", binary, "--name", name, "--json"],
                         capture_output=True, text=True, timeout=600)
    if out.returncode != 0:
        return {"error": out.stderr.strip().splitlines()[-1:] or ["failed"]}
    code = json.loads(out.stdout)[0].get("code", "")
    body = [line for line in code.splitlines() if line.strip()]
    count = lambda pattern: len(re.findall(pattern, code))
    return {
        "lines": len(body),
        "gotos": count(r"\bgoto\b"),
        "ifs": count(r"\bif\s*\("),
        "loops": count(r"\b(while|for)\s*\(") + count(r"\bdo\s*\{"),
        "switches": count(r"\bswitch\s*\("),
        "cases": count(r"\bcase\b"),
    }


def run_one(task: dict, args) -> dict:
    started = time.time()
    row = {**task}
    try:
        out = subprocess.run(
            [args.eval, task["binary"], task["function"], str(task["context_bytes"])],
            capture_output=True, text=True, timeout=args.timeout)
        if out.returncode != 0:
            row["error"] = (out.stderr.strip().splitlines() or ["failed"])[-1]
        else:
            row["eval"] = json.loads(out.stdout.strip().splitlines()[-1])
    except subprocess.TimeoutExpired:
        row["error"] = f"timeout after {args.timeout}s"
    try:
        row["hir"] = hir_metrics(args.cli, task["binary"], task["function"])
    except subprocess.TimeoutExpired:
        row["hir"] = {"error": "timeout"}
    row["wall_s"] = round(time.time() - started, 1)
    return row


def run(args) -> None:
    manifest = json.loads((args.corpus / "manifest.json").read_text())
    args.out.mkdir(parents=True, exist_ok=True)
    rows_path = args.out / "rows.jsonl"
    done = set()
    if rows_path.exists():
        for line in rows_path.read_text().splitlines():
            row = json.loads(line)
            done.add((row["isa"], row["variant"], row["function"]))
    tasks = []
    for key, binary in manifest["binaries"].items():
        isa, variant = key.split("/")
        for entry in manifest["functions"]:
            if (isa, variant, entry["name"]) in done:
                continue
            if args.only and entry["name"] not in args.only:
                continue
            tasks.append({"isa": isa, "variant": variant, "binary": binary,
                          "function": entry["name"], "context_bytes": entry["context_bytes"],
                          "expect": entry["expect"]})
    lock = threading.Lock()
    with ThreadPoolExecutor(max_workers=args.jobs) as pool, rows_path.open("a") as sink:
        for row in pool.map(lambda t: run_one(t, args), tasks):
            with lock:
                sink.write(json.dumps(row) + "\n")
                sink.flush()
            status = row.get("eval", {}).get("white_box", {}).get("status", {}).get("kind", row.get("error"))
            print(f"{row['isa']:8} {row['variant']:11} {row['function']:20} {status} ({row['wall_s']}s)",
                  flush=True)


def run_decbench(args) -> None:
    candidates = json.loads(args.candidates.read_text())["sample"]
    args.out.mkdir(parents=True, exist_ok=True)
    rows_path = args.out / "rows.jsonl"
    done = set()
    if rows_path.exists():
        for line in rows_path.read_text().splitlines():
            row = json.loads(line)
            done.add((row["binary"], row["address"]))

    def one(task: dict) -> dict:
        started = time.time()
        row = {**task}
        try:
            out = subprocess.run(
                [args.eval, task["binary"], task["address"], str(task["context_bytes"]),
                 "--input-arg", "1", "--input-bits", str(task["input_bits"]),
                 "--output-bits", str(task["output_bits"]), "--replay-words", "100"],
                capture_output=True, text=True, timeout=args.timeout)
            if out.returncode != 0:
                row["error"] = (out.stderr.strip().splitlines() or ["failed"])[-1]
            else:
                row["eval"] = json.loads(out.stdout.strip().splitlines()[-1])
        except subprocess.TimeoutExpired:
            row["error"] = f"timeout after {args.timeout}s"
        row["wall_s"] = round(time.time() - started, 1)
        return row

    tasks = [c for c in candidates if (c["binary"], c["address"]) not in done]
    with ThreadPoolExecutor(max_workers=args.jobs) as pool, rows_path.open("a") as sink:
        for row in pool.map(one, tasks):
            sink.write(json.dumps(row) + "\n")
            sink.flush()
            status = row.get("eval", {}).get("white_box", {}).get("status", {}).get("kind",
                                                                                   row.get("error"))
            print(f"{row['project']:18} {row['opt']:12} {row['function'][:32]:32} {status} "
                  f"({row['wall_s']}s)", flush=True)


def reason_bucket(reason: str) -> str:
    """Group unsupported reasons by what the slice did, not where."""
    text = re.sub(r"^at 0x[0-9a-f]+: ", "", reason)
    text = re.sub(r"0x[0-9a-f]+", "ADDR", text)
    text = re.sub(r"\d+", "N", text)
    if "depends on values outside the boundary" in text:
        return text.split(":")[0] + ": undeclared live-in"
    return text[:80]


def summarize_decbench(args) -> None:
    rows = [json.loads(line) for line in (args.out / "rows.jsonl").read_text().splitlines()]
    lines: list[str] = []
    emit = lines.append

    def kind(row):
        return (row.get("eval", {}).get("white_box", {}).get("status", {}).get("kind")
                or "harness_error")

    emit(f"## Real binaries: {len(rows)} step-shaped DecBench functions")
    emit("")
    configs = sorted({r["config"] for r in rows})
    kinds = sorted({kind(r) for r in rows})
    emit("| status | " + " | ".join(configs) + " | all |")
    emit("|---|" + "---|" * (len(configs) + 1))
    for k in kinds:
        cells = [sum(1 for r in rows if r["config"] == c and kind(r) == k) for c in configs]
        total = sum(cells)
        emit(f"| {k} | " + " | ".join(f"{n}" for n in cells) + f" | {total} ({pct(total, len(rows))}) |")
    emit("")
    degenerate = sum(1 for r in rows if kind(r) in ("partial_frontier", "empirically_consistent"))
    machines = [r for r in rows if kind(r) in ("proved_under_model", "empirically_consistent")]
    emit(f"Degenerate (partial frontier or solver-undecided): {degenerate} ({pct(degenerate, len(rows))}). "
         f"Machines (proved or empirical): {len(machines)} ({pct(len(machines), len(rows))}).")
    emit("")
    reasons: dict[str, int] = {}
    for row in rows:
        if kind(row) == "unsupported_semantics":
            bucket = reason_bucket(row["eval"]["white_box"]["status"]["reason"])
            reasons[bucket] = reasons.get(bucket, 0) + 1
        elif kind(row) == "harness_error":
            bucket = "harness: " + reason_bucket(row.get("error", "?"))
            reasons[bucket] = reasons.get(bucket, 0) + 1
    emit("Why no machine (unsupported and harness errors), by reason:")
    emit("")
    for bucket, n in sorted(reasons.items(), key=lambda kv: -kv[1])[:20]:
        emit(f"- {n} x {bucket}")
    emit("")
    if machines:
        emit("| project | opt | function | states | transitions | CFG blocks/edges | pruned CFG | replay agree |")
        emit("|---|---|---|---|---|---|---|---|")
        for row in machines:
            w = row["eval"]["white_box"]
            m = w["metrics"]
            replay = row["eval"].get("replay") or {}
            emit(f"| {row['project']} | {row['opt']} | `{row['function']}` | {w['states']} | "
                 f"{w['transitions']} | {m['pcode_blocks']}/{m['pcode_edges']} | "
                 f"{m['feasible_blocks']}/{m['feasible_edges']} | "
                 f"{replay.get('agree', '-')}/{replay.get('words', '-')} |")
        emit("")
        trivial = sum(1 for r in machines if r["eval"]["white_box"]["states"] == 1)
        emit(f"Single-state machines (the result is a function of the input alone): "
             f"{trivial} of {len(machines)}.")
        emit("")
    (args.out / "summary.md").write_text("\n".join(lines) + "\n")
    print("\n".join(lines))


def pct(part: float, whole: float) -> str:
    return "n/a" if not whole else f"{100.0 * part / whole:.0f}%"


def summarize(args) -> None:
    rows = [json.loads(line) for line in (args.out / "rows.jsonl").read_text().splitlines()]
    truth = json.loads(args.truth.read_text()) if args.truth else {}
    lines: list[str] = []
    emit = lines.append

    def white(row):
        return row.get("eval", {}).get("white_box", {})

    def kind(row):
        return white(row).get("status", {}).get("kind") or ("error: " + row.get("error", "?"))

    functions = []
    for row in rows:
        if row["function"] not in functions:
            functions.append(row["function"])
    variants = sorted({(r["isa"], r["variant"]) for r in rows},
                      key=lambda iv: (iv[0], VARIANT_ORDER.index(iv[1])))

    # 1. Status and state counts against the ground truth.
    emit("## Status and state counts")
    emit("")
    emit("Each cell: status, then states learned. `=` marks a count equal to the "
         "ground truth (minimal machine of the natively compiled source).")
    emit("")
    emit("| function | truth | " + " | ".join(f"{i} {v}" for i, v in variants) + " |")
    emit("|---|---|" + "---|" * len(variants))
    short = {"proved_under_model": "P", "empirically_consistent": "E", "partial_frontier": "F",
             "refuted": "R", "unsupported_semantics": "U"}
    for function in functions:
        t = truth.get(function, {})
        truth_states = t.get("minimal_states")
        cells = []
        for isa, variant in variants:
            row = next((r for r in rows if (r["isa"], r["variant"], r["function"]) ==
                        (isa, variant, function)), None)
            if row is None:
                cells.append("-")
                continue
            k = kind(row)
            states = white(row).get("states")
            mark = "=" if truth_states is not None and states == truth_states else ""
            cells.append(f"{short.get(k, k)} {states}{mark}")
        emit(f"| `{function}` | {truth_states if truth_states is not None else '-'} | " +
             " | ".join(cells) + " |")
    emit("")
    emit("P proved_under_model, E empirically_consistent, F partial_frontier, R refuted, "
         "U unsupported_semantics.")
    emit("")

    # 2. Complexity: raw CFG, pruned CFG (H0 baseline), quotient, HIR text.
    emit("## Structural complexity (machines only: P or E)")
    emit("")
    emit("| function | ISA | variant | CFG blocks/edges | pruned CFG blocks/edges | quotient states/transitions | node reduction vs CFG | vs pruned | HIR lines | HIR gotos |")
    emit("|---|---|---|---|---|---|---|---|---|---|")
    reductions_raw, reductions_pruned, edge_raw, edge_pruned = [], [], [], []
    for row in rows:
        if kind(row) not in ("proved_under_model", "empirically_consistent"):
            continue
        m = white(row)["metrics"]
        s, t = m["quotient_states"], m["quotient_transitions"]
        rr = 1 - s / m["pcode_blocks"] if m["pcode_blocks"] else 0
        rp = 1 - s / m["feasible_blocks"] if m["feasible_blocks"] else 0
        er = 1 - t / m["pcode_edges"] if m["pcode_edges"] else 0
        ep = 1 - t / m["feasible_edges"] if m["feasible_edges"] else 0
        reductions_raw.append(rr)
        reductions_pruned.append(rp)
        edge_raw.append(er)
        edge_pruned.append(ep)
        hir = row.get("hir", {})
        emit(f"| `{row['function']}` | {row['isa']} | {row['variant']} | "
             f"{m['pcode_blocks']}/{m['pcode_edges']} | {m['feasible_blocks']}/{m['feasible_edges']} | "
             f"{s}/{t} | {rr:.0%} | {rp:.0%} | {hir.get('lines', '-')} | {hir.get('gotos', '-')} |")
    emit("")
    if reductions_raw:
        emit(f"Median node reduction: {median(reductions_raw):.0%} vs CFG, "
             f"{median(reductions_pruned):.0%} vs pruned CFG; median edge reduction: "
             f"{median(edge_raw):.0%} vs CFG, {median(edge_pruned):.0%} vs pruned CFG "
             f"({len(reductions_raw)} machines).")
        emit("")

    # 3. Replay.
    emit("## Independent replay")
    emit("")
    total = agree = result = data = 0
    differing = []
    for row in rows:
        replay = row.get("eval", {}).get("replay")
        if not replay:
            continue
        total += replay["words"]
        agree += replay["agree"]
        result += replay["result_differs"]
        data += replay["data_differs"]
        if replay["agree"] != replay["words"]:
            differing.append(f"{row['isa']}/{row['variant']}/{row['function']} ({kind(row)})")
    emit(f"Random words: {agree}/{total} agree ({pct(agree, total)}); result differs {result}, "
         f"data differs {data}.")
    c_total = sum(r.get("eval", {}).get("replay", {}).get("conformance_words", 0)
                  for r in rows if r.get("eval", {}).get("replay"))
    c_agree = sum(r.get("eval", {}).get("replay", {}).get("conformance_agree", 0)
                  for r in rows if r.get("eval", {}).get("replay"))
    emit("")
    emit(f"W-method conformance (access word x alphabet x distinguishing suffix): "
         f"{c_agree}/{c_total} agree ({pct(c_agree, c_total)}).")
    for row in rows:
        replay = row.get("eval", {}).get("replay") or {}
        if replay.get("conformance_words", 0) != replay.get("conformance_agree", 0):
            differing.append(f"{row['isa']}/{row['variant']}/{row['function']} conformance")
    if differing:
        emit("")
        emit("Machines with a differing word: " + ", ".join(differing))
    emit("")

    # 4. Black box.
    emit("## Black-box ablation (no summaries, no solver)")
    emit("")
    emit("| function | truth | white-box states (median) | black-box states (median) | black-box = truth | white-box = truth |")
    emit("|---|---|---|---|---|---|")
    for function in functions:
        group = [r for r in rows if r["function"] == function and "eval" in r]
        if not group:
            continue
        t = truth.get(function, {}).get("minimal_states")
        ws = [white(r).get("states", 0) for r in group]
        bs = [r["eval"]["black_box"].get("states", 0) or 0 for r in group]
        weq = sum(1 for r in group if t is not None and white(r).get("states") == t
                  and kind(r) in ("proved_under_model", "empirically_consistent"))
        beq = sum(1 for b in bs if t is not None and b == t)
        emit(f"| `{function}` | {t if t is not None else '-'} | {median(ws)} | {median(bs)} | "
             f"{beq}/{len(group)} | {weq}/{len(group)} |")
    emit("")

    # 5. Status distribution and time.
    emit("## Status distribution and cost")
    emit("")
    by_kind: dict[str, int] = {}
    for row in rows:
        by_kind[kind(row)] = by_kind.get(kind(row), 0) + 1
    for k, n in sorted(by_kind.items(), key=lambda kv: -kv[1]):
        emit(f"- {k}: {n}")
    times = [white(r).get("elapsed_ms", 0) / 1000 for r in rows if "eval" in r]
    if times:
        emit("")
        emit(f"White-box time per function: median {median(times):.1f}s, max {max(times):.1f}s.")
    emit("")
    (args.out / "summary.md").write_text("\n".join(lines) + "\n")
    print("\n".join(lines))


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = parser.add_subparsers(dest="command", required=True)
    r = sub.add_parser("run")
    r.add_argument("--corpus", type=Path, required=True)
    r.add_argument("--out", type=Path, required=True)
    r.add_argument("--eval", required=True)
    r.add_argument("--cli", required=True)
    r.add_argument("-j", "--jobs", type=int, default=4)
    r.add_argument("--timeout", type=int, default=1800)
    r.add_argument("--only", nargs="*")
    s = sub.add_parser("summarize")
    s.add_argument("--out", type=Path, required=True)
    s.add_argument("--truth", type=Path)
    d = sub.add_parser("decbench")
    d.add_argument("--candidates", type=Path, required=True)
    d.add_argument("--out", type=Path, required=True)
    d.add_argument("--eval", required=True)
    d.add_argument("-j", "--jobs", type=int, default=4)
    d.add_argument("--timeout", type=int, default=600)
    ds = sub.add_parser("summarize-decbench")
    ds.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    args.out = args.out.expanduser()
    if args.command == "run":
        args.corpus = args.corpus.expanduser()
        run(args)
    elif args.command == "decbench":
        args.candidates = args.candidates.expanduser()
        run_decbench(args)
    elif args.command == "summarize-decbench":
        summarize_decbench(args)
    else:
        summarize(args)


if __name__ == "__main__":
    main()
