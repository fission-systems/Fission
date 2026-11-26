#!/usr/bin/env python3
"""A per-commit ledger of main's decompiler output and cost.

Each push to main that can change the decompiler records one entry
(output-ledger.yml):
- the hash of every function's output on output_diff.py's fixed FreshBench
  M1 sample, keyed by function;
- that sample's totals (rows, rows with output, failed calls, perfect,
  distance, types perfect);
- instructions executed on speed_compare.py's sample, by this commit's CLI
  alone. Counts differ slightly between CPUs, so the runner's CPU model is
  recorded and `compare` only sets two entries' counts side by side when
  it matches.

An entry is the workflow artifact `output-ledger-<commit>`, kept 90 days. The
sweep run is a second artifact, `output-ledger-run-<commit>`. The PR
base-vs-head check uses it as the base side instead of decompiling the base
again.

    output_ledger.py record --data D --freshbench F --cli C --out DIR [--no-speed]
    output_ledger.py compare OLD.json NEW.json
    output_ledger.py history KEY [--limit N]

`history` answers "which main commit changed this function's output"
from entries alone, without building anything. KEY is the function's
key, as `compare` prints it.

Corpus binaries are only decompiled; nothing is executed.
"""

from __future__ import annotations

import argparse
import json
import os
import platform
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

import freshbench_corpus as corpus
import output_diff
import speed_compare

SCHEMA = 1
RUN_LABEL = "ledger"


def row_key(row: dict) -> str:
    """A sweep row's function. Rows carry no single id field."""
    return f"{row['arch']}/{row['compiler']}-{row['config']}/{row['project']}/{row['binary']}@{row['address']:x}"


def cpu_model() -> str:
    try:
        for line in Path("/proc/cpuinfo").read_text().splitlines():
            if line.startswith("model name"):
                return line.split(":", 1)[1].strip()
    except OSError:
        pass
    return platform.processor() or platform.machine()


def entry_from_run(run: Path, report: dict, speed: dict | None) -> dict:
    meta = json.loads((run / "meta.json").read_text())
    rows = [json.loads(line) for line in (run / "rows.jsonl").read_text().splitlines() if line.strip()]
    overall, timing = report["overall"], report["timing"]
    no_output = sum(report["failures"].values())
    return {
        "schema": SCHEMA,
        "commit": meta["git"]["commit"],
        "dirty": meta["git"].get("dirty", False),
        "scorer": meta["scorer"],
        "corpus_id": meta["corpus_id"],
        "cpu": cpu_model(),
        "summary": {
            "rows": overall["rows"],
            "rows_with_output": overall["rows"] - no_output,
            "failed_calls": timing["failed_calls"],
            "perfect": overall["perfect"],
            "distance": overall["distance"],
            "types_perfect": overall["types_perfect"],
        },
        "speed": speed,
        # The full sha256 is 64 hex digits; 16 tell 3,000 outputs apart.
        "outputs": dict(sorted((row_key(r), (r.get("code_sha256") or "")[:16]) for r in rows)),
    }


def compare(old: dict, new: dict) -> dict:
    a, b = old["outputs"], new["outputs"]
    changed = sorted(k for k in a.keys() & b.keys() if a[k] != b[k])
    summary = {k: new["summary"][k] - old["summary"][k] for k in new["summary"]}
    speed = None
    if old.get("speed") and new.get("speed"):
        speed = {
            "old": old["speed"]["instructions"],
            "new": new["speed"]["instructions"],
            "same_cpu": old["cpu"] == new["cpu"],
        }
    return {
        "old": old["commit"],
        "new": new["commit"],
        "changed": changed,
        "appeared": sorted(b.keys() - a.keys()),
        "disappeared": sorted(a.keys() - b.keys()),
        "summary_delta": summary,
        "speed": speed,
        "comparable": old["scorer"] == new["scorer"] and old["corpus_id"] == new["corpus_id"],
    }


def first_changes(entries: list[dict], key: str) -> list[dict]:
    """`entries` oldest first. Each entry where `key`'s output hash differs
    from the previous entry that had it."""
    changes, last = [], None
    for entry in entries:
        value = entry["outputs"].get(key)
        if value is None:
            continue
        if last is not None and value != last:
            changes.append({"commit": entry["commit"], "from": last, "to": value})
        last = value
    return changes


def record(args) -> int:
    entries = corpus.manifest(args.data)
    chosen = output_diff.units(entries, args.data, args.budget)
    corpus.fetch_all(corpus.scoring_files(entries, chosen), args.data)
    runs = Path(tempfile.mkdtemp(prefix="ledger-runs-"))
    os.environ["FISSION_SLEIGH_CACHE_DIR"] = tempfile.mkdtemp(prefix="ledger-sleigh-cache-")
    output_diff.sweep(args.freshbench, args.data, args.cli.resolve(), corpus.REPO, RUN_LABEL, runs)
    run = runs / RUN_LABEL
    report = json.loads(subprocess.run(
        [str(args.freshbench), "sweep", "report", str(run), "--json"],
        check=True, capture_output=True, text=True, env=corpus.telemetry_free_env()).stdout)

    speed = None
    if not args.no_speed:
        binaries = sorted(corpus.stripped(entries), key=lambda e: e["sha256"])[: speed_compare.BINARIES]
        per_binary = {}
        cache = Path(tempfile.mkdtemp(prefix="ledger-speed-cache-"))
        for entry in binaries:
            binary = corpus.fetch(entry, args.data)
            per_binary[entry["path"]] = speed_compare.cost(args.cli, binary, corpus.REPO / "utils", cache=cache)
        speed = {"instructions": sum(per_binary.values()), "binaries": per_binary}

    entry = entry_from_run(run, report, speed)
    args.out.mkdir(parents=True, exist_ok=True)
    (args.out / "entry.json").write_text(json.dumps(entry, indent=1) + "\n")
    shutil.copytree(run, args.out / "run", dirs_exist_ok=True)
    s = entry["summary"]
    text = (f"### Output ledger: `{entry['commit'][:12]}`\n\n"
            f"{len(chosen)} units, {s['rows']} functions: {s['rows_with_output']} with output, "
            f"{s['failed_calls']} failed calls, {s['perfect']} perfect, distance {s['distance']:g}, "
            f"{s['types_perfect']} types perfect.\n")
    if speed:
        text += f"\nInstructions on the speed sample: {speed['instructions']:,} ({entry['cpu']}).\n"
    print(text)
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a") as out:
            out.write(text)
    return 0


def gh_json(*args: str) -> list | dict:
    return json.loads(subprocess.run(["gh", *args], check=True, capture_output=True, text=True).stdout)


def fetch_entries(limit: int) -> list[dict]:
    """The ledger entries of the last `limit` recorded main commits, oldest first."""
    runs = gh_json("run", "list", "--workflow", "output-ledger.yml", "--branch", "main", "--status", "success",
                   "--limit", str(limit), "--json", "databaseId,headSha")
    entries = []
    with tempfile.TemporaryDirectory() as temp:
        for run in reversed(runs):
            target = Path(temp) / run["headSha"]
            subprocess.run(["gh", "run", "download", str(run["databaseId"]), "-n",
                            f"output-ledger-{run['headSha']}", "-D", str(target)],
                           check=True, capture_output=True)
            entries.append(json.loads((target / "entry.json").read_text()))
    return entries


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    r = sub.add_parser("record")
    r.add_argument("--data", type=Path, required=True)
    r.add_argument("--freshbench", type=Path, required=True)
    r.add_argument("--cli", type=Path, required=True)
    r.add_argument("--out", type=Path, required=True)
    r.add_argument("--no-speed", action="store_true", help="skip the cachegrind sample (needs valgrind)")
    r.add_argument("--budget", type=int, default=output_diff.BUDGET,
                   help="sample size in functions; base-vs-head reuses only the default")
    c = sub.add_parser("compare")
    c.add_argument("old", type=Path)
    c.add_argument("new", type=Path)
    h = sub.add_parser("history")
    h.add_argument("key")
    h.add_argument("--limit", type=int, default=50)
    args = parser.parse_args()

    if args.command == "record":
        return record(args)
    if args.command == "compare":
        result = compare(json.loads(args.old.read_text()), json.loads(args.new.read_text()))
        print(json.dumps(result, indent=1))
        return 0
    entries = fetch_entries(args.limit)
    if not any(args.key in e["outputs"] for e in entries):
        raise SystemExit(f"{args.key} is in none of the last {len(entries)} entries")
    changes = first_changes(entries, args.key)
    print(json.dumps({"entries": len(entries), "from": entries[0]["commit"], "to": entries[-1]["commit"],
                      "changes": changes}, indent=1))
    return 0


if __name__ == "__main__":
    sys.exit(main())
