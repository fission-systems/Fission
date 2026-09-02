#!/usr/bin/env python3
"""Score DecBench's sample-set and refuse a regression.

WHY THIS EXISTS
---------------
`golden_corpus_check.py` diffs decompiled *text* and the Rust suite checks
behaviour. Neither scores what the leaderboard scores, and the two
populations move independently: a change can leave every fixture green and
still cost perfect matches on the 250 functions the ranking is computed
over. Glaurung's own holdout tool exists for exactly this -- their
`byte_match` fell 0.2392 -> 0.2005 across four commits with the whole suite
passing, because nothing measured the scored population.

This gate closes that. It decompiles the eval kit's stripped, anonymized
binaries -- the same artifacts the published rows were produced from --
scores GED against DecBench's published source CFGs and types against the
unstripped twin's DWARF, and compares the perfect counts to a checked-in
baseline.

SAFETY
------
Several sample-set binaries are compiled-from-source malware (theZoo
corpus). Nothing here executes one: decompilation is static analysis, and
the kit's own README says to treat every binary as hostile.

PREREQUISITES
-------------
Both live outside this repository and the gate skips cleanly without them:

  - the eval kit at `vendor/decbench-evalkit/decbench-evalkit-sample-set`
  - `fission-benchmark` with `decbench-data/` and a `pyjoern` environment

USAGE
-----
    scripts/quality/decbench_sample_set_gate.py check          # fail on a drop
    scripts/quality/decbench_sample_set_gate.py check --quick  # a fixed slice
    scripts/quality/decbench_sample_set_gate.py update         # accept as floor
"""

from __future__ import annotations

import argparse
import collections
import json
import subprocess
import sys
import time
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent.parent
KIT = REPO / "vendor" / "decbench-evalkit" / "decbench-evalkit-sample-set"
DEFAULT_BENCHMARK_ROOT = Path.home() / "fission-benchmark"
BASELINE = Path(__file__).resolve().parent / "decbench_sample_set_baseline.json"
LAYERS = ("nir", "hir")
JOERN_CHUNK = 60
# Enough binaries to catch a broad regression in a couple of minutes; the
# full run is what a submission is measured on.
QUICK_BINARIES = 60


def resolve_cli(explicit: str | None) -> Path | None:
    """The newest `fission_cli` build, not the first profile that exists.

    Preferring a fixed profile order silently scores a stale binary: the
    first run of this gate picked a `quick-release` build seven hours older
    than `release` and reported 47 of 69 functions decompiled against the
    250 of 250 the current build produces. A baseline captured that way is
    worse than none.
    """
    if explicit:
        path = Path(explicit)
        return path if path.is_file() else None
    builds = [
        candidate
        for profile in ("release", "quick-release", "debug")
        if (candidate := REPO / "target" / profile / "fission_cli").is_file()
    ]
    if not builds:
        return None
    newest = max(builds, key=lambda p: p.stat().st_mtime)
    for other in builds:
        if other != newest and other.stat().st_mtime < newest.stat().st_mtime:
            print(f"note: ignoring older build {other.relative_to(REPO)}", file=sys.stderr)
    return newest


def load_scoring(benchmark_root: Path):
    """Import DecBench's own metric code, or explain why we cannot."""
    sys.path.insert(0, str(benchmark_root))
    sys.path.insert(0, str(benchmark_root / "runner"))
    sys.path.insert(0, str(REPO / "vendor" / "decbench"))
    from ged import compute_ged, extract_decompiled_cfgs, load_published_source_cfgs
    from decbench.decompilers.raw.fission_raw import _variables
    from decbench.metrics.type_match import TypeMatchMetric, extract_ground_truth_types

    return {
        "compute_ged": compute_ged,
        "extract_decompiled_cfgs": extract_decompiled_cfgs,
        "load_published_source_cfgs": load_published_source_cfgs,
        "variables": _variables,
        "TypeMatchMetric": TypeMatchMetric,
        "extract_ground_truth_types": extract_ground_truth_types,
    }


def source_cfg_name(fn: str, src) -> str | None:
    """The name the published source CFG files this function under.

    A symbol table carries the decorated form (i386 PE `stdcall` becomes
    `_Name@bytes`, several toolchains prefix an underscore) and the source
    CFG the plain one; matching literally drops those functions instead of
    scoring them.
    """
    if fn in src:
        return fn
    stripped = fn.split("@", 1)[0]
    for candidate in (stripped, stripped.lstrip("_"), fn.lstrip("_")):
        if candidate in src:
            return candidate
    return None


def declared_name(text: str, addr: int) -> str:
    """The name in the emitted declarator, which is not always `sub_<addr>`."""
    import re

    pattern = re.compile(r"^[^\n;{}]*?\b([A-Za-z_]\w*)\s*\(", re.M)
    for line in text.splitlines():
        stripped = line.lstrip()
        if "(" not in line or stripped.startswith(("//", "#", "typedef")):
            continue
        match = pattern.match(line)
        if match and (line.rstrip().endswith(")") or line.rstrip().endswith("{")):
            return match.group(1)
    return f"sub_{addr:x}"


def node_count(cfg) -> int | None:
    """Nodes in a CFG, whichever shape the scoring API hands back.

    GED here is purely topological, so node-count mismatch is the first thing
    to look at when a function is far from perfect -- but the gate only ever
    reported totals, which cannot say *which* function moved or why.
    """
    for attr in ("nodes", "blocks"):
        value = getattr(cfg, attr, None)
        if value is not None:
            try:
                return len(value)
            except TypeError:
                pass
    if isinstance(cfg, dict):
        for attr in ("nodes", "blocks"):
            if attr in cfg:
                try:
                    return len(cfg[attr])
                except TypeError:
                    pass
    try:
        return cfg.number_of_nodes()
    except Exception:
        return None


def score(cli: Path, benchmark_root: Path, quick: bool, dump: list | None = None) -> dict:
    api = load_scoring(benchmark_root)
    data = benchmark_root / "decbench-data"
    kit = json.load(open(KIT / "functions.json"))
    manifest = json.load(open(data / "configs" / "sample-set" / "manifest.json"))
    dataset_path = {
        (b["opt"], b["project"], b["binary"]): b["binary_path"] for b in manifest["binaries"]
    }

    entries = sorted(kit["public"].items())
    if quick:
        entries = entries[:QUICK_BINARIES]

    units, src_cfgs, gt_types = [], {}, {}
    scratch = Path(subprocess.run(["mktemp", "-d"], capture_output=True, text=True).stdout.strip())
    started = time.time()
    for anon, addrs_hex in entries:
        meta = kit["private"][anon]
        key = (meta["opt"], meta["project"], meta["binary"])
        original = data / dataset_path[key] if key in dataset_path else None
        addrs = [int(a, 0) for a in addrs_hex]

        names = {}
        if original and original.is_file():
            nm = subprocess.run(
                ["llvm-nm", "--defined-only", str(original)], capture_output=True, text=True
            ).stdout
            by_addr = {}
            for line in nm.splitlines():
                parts = line.split()
                if len(parts) >= 3 and parts[1] in "tT":
                    by_addr[int(parts[0], 16)] = parts[2]
            names = {a: by_addr.get(a) for a in addrs}

        addr_file = scratch / "addrs.txt"
        addr_file.write_text("".join(f"0x{a:x}\n" for a in addrs))
        records = {}
        try:
            run = subprocess.run(
                [str(cli), "decomp", str(KIT / "binaries" / anon), "--layer", "nir", "--json",
                 "--no-header", "--no-warnings", "--addresses-file", str(addr_file)],
                capture_output=True, text=True, timeout=900,
            )
            records = {
                int(e["address"], 16): e
                for e in json.loads(run.stdout[run.stdout.index("["):])
                if e.get("code")
            }
        except Exception:
            pass

        if key not in src_cfgs:
            try:
                src_cfgs[key] = api["load_published_source_cfgs"](
                    data / "pipeline_data" / "source_cfgs" / key[0] / key[1] / f"{key[2]}.json"
                )
            except Exception:
                src_cfgs[key] = {}
            try:
                gt_types[key] = (
                    api["extract_ground_truth_types"](original)
                    if original and original.is_file()
                    else {}
                )
            except Exception:
                gt_types[key] = {}

        for a in addrs:
            units.append({"anon": anon, "addr": a, "key": key,
                          "fn": names.get(a), "rec": records.get(a)})

    # Every CFG in a handful of batched joern calls -- one call per binary
    # per layer spends nearly all of its time starting a JVM.
    cfgs = {layer: {} for layer in LAYERS}
    for layer in LAYERS:
        batch = {}
        for index, unit in enumerate(units):
            if not (unit["rec"] and unit["fn"]):
                continue
            text = unit["rec"].get(f"code_{layer}") or unit["rec"]["code"]
            unique = f"{unit['fn']}_u{index}"
            batch[unique] = text.replace(declared_name(text, unit["addr"]), unique)
            unit[f"key_{layer}"] = unique
        names_l = list(batch)
        for i in range(0, len(names_l), JOERN_CHUNK):
            part = {n: batch[n] for n in names_l[i:i + JOERN_CHUNK]}
            try:
                cfgs[layer].update(api["extract_decompiled_cfgs"](part))
            except Exception:
                pass
        for missing in [n for n in names_l if n not in cfgs[layer]]:
            try:
                cfgs[layer].update(api["extract_decompiled_cfgs"]({missing: batch[missing]}))
            except Exception:
                pass

    metric = api["TypeMatchMetric"]()
    from decbench.models.decompilation import FunctionDecompilation

    counts = collections.Counter()
    for unit in units:
        rec, fn = unit["rec"], unit["fn"]
        counts["functions"] += 1
        if rec:
            counts["decompiled"] += 1
        if not (rec and fn):
            continue
        src = src_cfgs.get(unit["key"], {})
        gt = gt_types.get(unit["key"], {})
        src_name = source_cfg_name(fn, src) if src else None
        for layer in LAYERS:
            unique = unit.get(f"key_{layer}")
            if src_name and unique and unique in cfgs[layer]:
                try:
                    result = api["compute_ged"](src[src_name], cfgs[layer][unique])
                    if "ged" in result:
                        counts[f"ged_{layer}_scored"] += 1
                        if result["ged"] == 0:
                            counts[f"ged_{layer}_perfect"] += 1
                        if dump is not None:
                            dump.append({
                                "anon": unit["anon"],
                                "addr": f"0x{unit['addr']:x}",
                                "fn": fn,
                                "opt": unit["key"][0],
                                "project": unit["key"][1],
                                "layer": layer,
                                "ged": result["ged"],
                                "src_nodes": node_count(src[src_name]),
                                "out_nodes": node_count(cfgs[layer][unique]),
                            })
                except Exception:
                    pass
            if gt.get(fn):
                code = rec.get(f"code_{layer}") or rec["code"]
                decompiled = FunctionDecompilation(
                    name=fn, address=unit["addr"], decompiled_code=code,
                    line_count=code.count("\n") + 1, variables=api["variables"](rec),
                )
                try:
                    value = metric.compute_for_function(decompiled, ground_truth_vars=gt[fn]).value
                except Exception:
                    continue
                counts[f"type_{layer}_scored"] += 1
                if value == 1.0:
                    counts[f"type_{layer}_perfect"] += 1

    counts["elapsed_sec"] = round(time.time() - started)
    return dict(counts)


# Counts that must not fall. Denominators are reported but not gated: a
# function the harness could not score is a coverage question, and gating on
# it would fail a run for a joern hiccup.
GATED = [f"{metric}_{layer}_perfect" for metric in ("ged", "type") for layer in LAYERS]


def render(scores: dict) -> str:
    lines = [f"  functions {scores.get('functions', 0)}  decompiled {scores.get('decompiled', 0)}"]
    for layer in LAYERS:
        for metric, label in (("ged", "GED  "), ("type", "Types")):
            perfect = scores.get(f"{metric}_{layer}_perfect", 0)
            scored = scores.get(f"{metric}_{layer}_scored", 0)
            rate = perfect / scored * 100 if scored else 0.0
            lines.append(f"  {layer.upper()} {label} perfect {perfect:3d}/{scored:3d} = {rate:5.1f}%")
    return "\n".join(lines)


def cmd_check(args: argparse.Namespace) -> int:
    cli = resolve_cli(args.cli)
    if cli is None:
        print("skip: no fission_cli build found", file=sys.stderr)
        return 0
    if not KIT.is_dir():
        print(f"skip: eval kit not at {KIT}", file=sys.stderr)
        return 0
    if not BASELINE.is_file():
        print(f"error: no baseline at {BASELINE} -- run `update` first", file=sys.stderr)
        return 1
    baseline = json.load(open(BASELINE))
    key = "quick" if args.quick else "full"
    if key not in baseline:
        print(f"error: baseline has no `{key}` entry -- run `update{' --quick' if args.quick else ''}`",
              file=sys.stderr)
        return 1
    rows: list | None = [] if args.dump else None
    scores = score(cli, Path(args.benchmark_root), args.quick, rows)
    if args.dump and rows is not None:
        Path(args.dump).write_text(json.dumps(rows, indent=1))
        print(f"wrote {len(rows)} per-function rows to {args.dump}")
    print(f"[{key}] current ({scores['elapsed_sec']}s)")
    print(render(scores))
    print(f"[{key}] baseline")
    print(render(baseline[key]))

    drops = [
        (name, baseline[key].get(name, 0), scores.get(name, 0))
        for name in GATED
        if scores.get(name, 0) < baseline[key].get(name, 0)
    ]
    if drops:
        print("\nREGRESSION", file=sys.stderr)
        for name, was, now in drops:
            print(f"  {name}: {was} -> {now}", file=sys.stderr)
        print("\nIf the drop is understood and accepted, re-run with `update`.", file=sys.stderr)
        return 1
    gains = [(n, baseline[key].get(n, 0), scores.get(n, 0))
             for n in GATED if scores.get(n, 0) > baseline[key].get(n, 0)]
    if gains:
        print("\nimproved (run `update` to raise the floor)")
        for name, was, now in gains:
            print(f"  {name}: {was} -> {now}")
    return 0


def cmd_update(args: argparse.Namespace) -> int:
    cli = resolve_cli(args.cli)
    if cli is None:
        print("error: no fission_cli build found", file=sys.stderr)
        return 1
    scores = score(cli, Path(args.benchmark_root), args.quick)
    baseline = json.load(open(BASELINE)) if BASELINE.is_file() else {}
    baseline["quick" if args.quick else "full"] = scores
    BASELINE.write_text(json.dumps(baseline, indent=2, sort_keys=True) + "\n")
    print(f"wrote {BASELINE}")
    print(render(scores))
    return 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    sub = parser.add_subparsers(dest="command", required=True)
    common = argparse.ArgumentParser(add_help=False)
    common.add_argument("--cli", default=None, help="Path to fission_cli")
    common.add_argument("--benchmark-root", default=str(DEFAULT_BENCHMARK_ROOT))
    common.add_argument("--quick", action="store_true",
                        help=f"Score the first {QUICK_BINARIES} binaries only")

    p_check = sub.add_parser("check", parents=[common], help="Fail if a perfect count fell")
    p_check.add_argument("--dump", default=None, metavar="FILE",
                         help="Write per-function GED and node counts as JSON")
    p_check.set_defaults(func=cmd_check)
    p_update = sub.add_parser("update", parents=[common], help="Accept current scores as the floor")
    p_update.set_defaults(func=cmd_update)
    return parser


def main() -> int:
    args = build_parser().parse_args()
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
