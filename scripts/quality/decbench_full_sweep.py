#!/usr/bin/env python3
"""Static DecBench full sweep and exact-row CI comparison."""

from __future__ import annotations

import argparse
import collections
import hashlib
from importlib.metadata import version
import json
import math
import os
import re
import shutil
import signal
import subprocess
import tempfile
from pathlib import Path, PurePosixPath
from urllib.parse import quote
from urllib.request import urlopen

DATASET = "noelo-lab/decbench-dataset"
REVISION = "815eb96f130b6d2ef7fa600e04669bf20139bf27"
DECBENCH = "b65837037c7e93dbd3db01e72fa8d46641686dcb"
JOERN = "a2c16ece800f0843df21f6bb09efed2d0430a83a"
MANIFEST = "configs/full/manifest.json"
REPO = Path(__file__).resolve().parents[2]
BATCH = 128


def fetch(name: str, root: Path) -> Path:
    path = PurePosixPath(name)
    if path.is_absolute() or ".." in path.parts:
        raise ValueError(f"invalid dataset path {name!r}")
    dest = root.joinpath(*path.parts)
    if not dest.is_file():
        dest.parent.mkdir(parents=True, exist_ok=True)
        with urlopen(
            f"https://huggingface.co/datasets/{DATASET}/resolve/{REVISION}/{quote(name)}",
            timeout=120,
        ) as response, dest.with_suffix(dest.suffix + ".partial").open("wb") as out:
            shutil.copyfileobj(response, out)
        dest.with_suffix(dest.suffix + ".partial").replace(dest)
    return dest


def digest(path: Path) -> str:
    with path.open("rb") as data:
        return hashlib.file_digest(data, "sha256").hexdigest()


def identities(entry: dict):
    return [(entry["binary_path"], name) for name in entry["functions"]]


def manifest(root: Path) -> list[dict]:
    data = json.loads(fetch(MANIFEST, root).read_text())
    entries = data["binaries"]
    rows = [key for entry in entries for key in identities(entry)]
    if (
        data["config"] != "full"
        or data["dataset_repo"] != DATASET
        or len(entries) != 770
        or len(rows) != 96103
        or len(set(rows)) != len(rows)
        or len({entry["binary_path"] for entry in entries}) != len(entries)
    ):
        raise ValueError("unexpected full DecBench manifest or membership")
    return entries


def partition(entries: list[dict], count: int) -> list[list[dict]]:
    if not 1 <= count <= len(entries):
        raise ValueError("invalid shard count")
    buckets: list[list[dict]] = [[] for _ in range(count)]
    sizes = [0] * count
    for entry in sorted(entries, key=lambda row: (-len(row["functions"]), row["binary_path"])):
        shard = sizes.index(min(sizes))
        buckets[shard].append(entry)
        sizes[shard] += len(entry["functions"])
    return buckets


def strip_and_resolve(original: Path, stripped: Path, names: list[str]) -> dict[str, int]:
    from decbench.evalkit.export import _verify_strip
    from decbench.evalkit.resolve import name_to_addr

    with original.open("rb") as data:
        magic = data.read(4)
    flags = ["--strip-all"]
    commands = ["llvm-strip"]
    if magic == b"\x7fELF":
        flags += ["-R", ".comment", "-R", ".note.gnu.build-id"]
    elif magic.startswith(b"MZ"):
        commands += [
            tool for tool in ("i686-w64-mingw32-strip", "x86_64-w64-mingw32-strip")
            if shutil.which(tool)
        ]
    for command in commands:
        shutil.copyfile(original, stripped)
        result = subprocess.run([command, *flags, str(stripped)], capture_output=True, text=True, timeout=60)
        if result.returncode == 0:
            break
    else:
        raise RuntimeError(f"could not strip {original.name}: {result.stderr}")
    _verify_strip(original, stripped)

    targets = name_to_addr(original, names=set(names), stems=None)
    if set(names) - targets.keys():
        result = subprocess.run(
            ["llvm-nm", "--defined-only", str(original)], capture_output=True, text=True, timeout=60,
        )
        if result.returncode and result.stdout == "" and "no symbols" not in result.stderr:
            raise RuntimeError(f"could not read symbols for {original.name}: {result.stderr}")
        symbols = collections.defaultdict(set)
        for line in result.stdout.splitlines():
            fields = line.split()
            if len(fields) >= 3 and fields[1] in {"T", "t"}:
                symbol = fields[2]
                plain = symbol.split("@", 1)[0]
                for alias in {symbol, plain, plain.removeprefix("_")}:
                    symbols[alias].add(int(fields[0], 16))
        for name in set(names) - targets.keys():
            if len(symbols[name]) == 1:
                targets[name] = next(iter(symbols[name]))
    return targets


def decompile(cli: Path, binary: Path, addresses: list[int], temp: Path):
    records: dict[int, dict] = {}
    failures: dict[int, str] = {}
    crashes = 0
    address_file = temp / "addresses"
    env = dict(os.environ, FISSION_BENCHMARK_NO_CACHE="1", DECBENCH_NO_CACHE="1")

    def batch(values: list[int]) -> None:
        nonlocal crashes
        address_file.write_text("".join(f"0x{addr:x}\n" for addr in values))
        with subprocess.Popen(
            [str(cli), "--resource-root", str(REPO / "utils"), "decomp", str(binary),
             "--json", "--no-header", "--no-warnings", "--no-db", "--addresses-file", str(address_file)],
            cwd=temp, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True,
        ) as proc:
            try:
                stdout, _ = proc.communicate(timeout=90 + 2 * len(values))
                status = "crash" if proc.returncode < 0 else "cli_error" if proc.returncode else ""
            except subprocess.TimeoutExpired:
                try:
                    os.killpg(proc.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                proc.communicate()
                status = "timeout"
        if status == "crash":
            crashes += 1
        if not status:
            try:
                parsed = json.loads(stdout)
                if not isinstance(parsed, list):
                    raise ValueError("decomp did not return an array")
                for item in parsed:
                    raw = item.get("address")
                    if isinstance(raw, str):
                        records[int(raw, 16)] = item
                    elif isinstance(raw, int):
                        records[raw] = item
                return
            except (ValueError, AttributeError):
                status = "invalid_output"
        if len(values) == 1:
            failures[values[0]] = status
        else:
            mid = len(values) // 2
            batch(values[:mid])
            batch(values[mid:])

    for start in range(0, len(addresses), BATCH):
        batch(addresses[start:start+BATCH])
    return records, failures, crashes


def score(entry: dict, root: Path, cli: Path, api: dict) -> tuple[list[dict], int]:
    from decbench.models.decompilation import FunctionDecompilation
    from decbench_sample_set_gate import rename_declared_function, source_cfg_name

    path = entry["binary_path"]
    cfg_path = entry["source_cfg_path"]
    if not path.startswith("binaries/") or not cfg_path.startswith("pipeline_data/source_cfgs/"):
        raise ValueError(f"unexpected dataset asset paths for {path}")
    original = fetch(path, root)
    if original.stat().st_size != entry["size"] or digest(original) != entry["sha256"]:
        raise ValueError(f"dataset binary hash or size differs for {path}")
    cfg_file = fetch(cfg_path, root)
    ground_truth = api["extract_ground_truth_types"](original)
    source = api["load_published_source_cfgs"](str(cfg_file))
    if not source:
        raise ValueError(f"no readable source CFGs for {path}")
    with tempfile.TemporaryDirectory(prefix="decbench-static-") as temp:
        temp = Path(temp)
        stripped = temp / "binary"
        targets = strip_and_resolve(original, stripped, entry["functions"])
        records, failures, crashes = decompile(cli, stripped, sorted(set(targets.values())), temp)

    rows = []
    for start in range(0, len(entry["functions"]), BATCH):
        functions = entry["functions"][start:start+BATCH]
        fragments = {}
        pending = []
        for index, name in enumerate(functions):
            addr = targets.get(name)
            record = records.get(addr, {})
            code = record.get("code")
            row = {"binary": path, "function": name, "address": addr}
            if addr is None:
                row["status"] = "unresolved"
            elif addr in failures:
                row["status"] = failures[addr]
            elif record.get("error") or not isinstance(code, str) or not code.strip():
                row["status"] = "no_output"
            else:
                row["status"] = "ok"
                row["code_sha256"] = hashlib.sha256(code.encode()).hexdigest()
                label = f"decbench_{index}"
                pending.append((row, label, record))
                fragments[label] = rename_declared_function(code, addr, label)
            rows.append(row)
        cfgs = api["extract_decompiled_cfgs"](fragments)
        metric = api["TypeMatchMetric"]()
        for row, label, record in pending:
            name = row["function"]
            key = source_cfg_name(name, source)
            if key is not None and label in cfgs:
                result = api["compute_ged"](source[key], cfgs[label])
                if "ged" in result and math.isfinite(result["ged"]):
                    row["ged"] = result["ged"]
            if ground_truth.get(name):
                code = record["code"]
                result = metric.compute_for_function(
                    FunctionDecompilation(
                        name=name, address=row["address"], decompiled_code=code,
                        line_count=code.count("\n") + 1, variables=api["variables"](record),
                    ),
                    ground_truth_vars=ground_truth[name],
                )
                if result.value is not None and math.isfinite(result.value):
                    row["type_match"] = result.value
    return rows, crashes


def run(args) -> None:
    from decbench_scoring import load

    os.environ["FISSION_BENCHMARK_NO_CACHE"] = "1"
    os.environ["DECBENCH_NO_CACHE"] = "1"
    api = load(REPO)
    items = partition(manifest(args.data), args.shards)[args.shard]
    if args.only:
        items = [entry for entry in items if args.only in entry["binary_path"]]
    if not items:
        raise ValueError("empty shard selection")
    args.out.mkdir(parents=True, exist_ok=True)
    crashes = 0
    with (args.out / "rows.jsonl").open("w") as out:
        for entry in items:
            rows, signals = score(entry, args.data, args.cli.resolve(), api)
            crashes += signals
            for row in rows:
                out.write(json.dumps(row, sort_keys=True) + "\n")
            out.flush()
            print(f"{entry['binary_path']}: {len(rows)} rows", flush=True)
    provenance = {
        "dataset_revision": REVISION, "decbench": DECBENCH, "rust_joern": JOERN,
        "scorer_sha256": digest(Path(__file__).with_name("decbench_scoring.py")),
        "sweep_sha256": digest(Path(__file__)),
        "metric_requirements_sha256": digest(REPO / "scripts/quality/decbench-ci-requirements.txt"),
        "metric_packages": {
            package: version(package) for package in
            ("cfgutils", "networkx", "lief", "pyelftools", "tree-sitter", "tree-sitter-c")
        },
        "helpers_sha256": digest(Path(__file__).with_name("decbench_sample_set_gate.py")),
    }
    (args.out / "meta.json").write_text(json.dumps({
        "provenance": provenance, "cli_sha256": digest(args.cli),
        "sha": os.environ.get("GITHUB_SHA"), "shard": args.shard, "crashes": crashes,
    }))


def read_rows(paths):
    rows = {}
    for path in paths:
        with path.open() as stream:
            for line in stream:
                row = json.loads(line)
                key = (row["binary"], row["function"])
                if key in rows:
                    raise ValueError(f"duplicate row {key}")
                rows[key] = row
    return rows


def summary(rows):
    ged = [row["ged"] for row in rows.values() if "ged" in row]
    types = [row["type_match"] for row in rows.values() if "type_match" in row]
    return {
        "status": dict(collections.Counter(row["status"] for row in rows.values())),
        "ged_scored": len(ged), "ged_perfect": ged.count(0), "ged_total_distance": sum(ged),
        "type_scored": len(types), "type_perfect": types.count(1),
        "type_mean": sum(types) / len(types) if types else None, "byte_match": "not_run",
    }


def report(args) -> None:
    directories = sorted(args.input.glob("shard-*"))
    metadata = [json.loads((directory / "meta.json").read_text()) for directory in directories]
    if sorted(item["shard"] for item in metadata) != list(range(args.shards)):
        raise ValueError("missing or duplicate shard")
    first = metadata[0]
    if any(any(item[key] != first[key] for key in ("provenance", "sha", "cli_sha256")) for item in metadata):
        raise ValueError("shards used different inputs")
    rows = read_rows(directory / "rows.jsonl" for directory in directories)
    expected = {key for entry in manifest(args.data) for key in identities(entry)}
    if rows.keys() != expected:
        raise ValueError("result row identities differ from the pinned full manifest")
    current = summary(rows)
    if not current["status"].get("ok") or not current["ged_scored"] or not current["type_scored"]:
        raise ValueError("full run produced no usable decompilation or scoring data")
    args.out.mkdir(parents=True, exist_ok=True)
    crashes = sum(item["crashes"] for item in metadata)
    regressions = 0
    compared = False
    report_data = {"candidate": current, "meta": first, "crashes": crashes}
    with (args.out / "rows.jsonl").open("w") as out:
        for key in sorted(rows):
            out.write(json.dumps(rows[key], sort_keys=True) + "\n")
    if args.baseline and (args.baseline / "summary.json").exists():
        base_data = json.loads((args.baseline / "summary.json").read_text())
        if base_data["meta"]["provenance"] == first["provenance"]:
            base = read_rows([args.baseline / "rows.jsonl"])
            if base.keys() != rows.keys():
                raise ValueError("compatible baseline has different row identities")
            compared = True
            report_data["baseline"] = base_data["candidate"]
            report_data["baseline_sha"] = base_data["meta"]["sha"]
            with (args.out / "changed.jsonl").open("w") as out:
                for key, before in base.items():
                    after = rows[key]
                    if before != after:
                        out.write(json.dumps({"before": before, "after": after}, sort_keys=True) + "\n")
                    regressions += before["status"] == "ok" and after["status"] != "ok"
    report_data.update(compared=compared, regressions=regressions)
    data = json.dumps(report_data, indent=2, sort_keys=True)
    (args.out / "summary.json").write_text(data + "\n")
    message = (
        f"DecBench full: {len(rows):,} rows · {current['ged_perfect']:,}/{current['ged_scored']:,} "
        f"GED perfect · total distance {current['ged_total_distance']:,} · type mean {current['type_mean']:.4f}\n"
        f"Process crashes: {crashes}. Previous successful rows now failing: {regressions}. "
        + ("Compared against compatible main baseline." if compared else "No compatible main baseline.")
    )
    print(message)
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a") as out:
            out.write(message + "\n")
    if crashes or regressions:
        raise SystemExit(1)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--data", type=Path, required=True)
    parser.add_argument("--shards", type=int, default=24)
    sub = parser.add_subparsers(dest="mode", required=True)
    worker = sub.add_parser("run")
    worker.add_argument("--cli", type=Path, required=True)
    worker.add_argument("--shard", type=int, required=True)
    worker.add_argument("--only", help="Select a binary path substring for a local smoke run")
    worker.add_argument("--out", type=Path, required=True)
    worker.set_defaults(command=run)
    collect = sub.add_parser("report")
    collect.add_argument("--input", type=Path, required=True)
    collect.add_argument("--out", type=Path, required=True)
    collect.add_argument("--baseline", type=Path)
    collect.set_defaults(command=report)
    args = parser.parse_args()
    if args.mode == "run" and not 0 <= args.shard < args.shards:
        parser.error("--shard must be in [0, --shards)")
    args.command(args)


if __name__ == "__main__":
    main()
