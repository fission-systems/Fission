#!/usr/bin/env python3
"""The FreshBench M1 corpus, fetched file by file and hash-checked.

CI jobs that decompile real binaries take them from FreshBench M1
(docs/BENCHMARK_FRESHBENCH.md, ADR 0019).

- The manifest comes from the FreshBench repository at a pinned commit. Its
  sha256 is checked against the one FreshBench publishes.
- Each file comes from the Hugging Face dataset at a pinned revision. Its size
  and sha256 are checked against the manifest.
- Files land under `<root>/m1/<path>`, the layout `freshbench sweep run
  <root>/m1` reads.
- Corpus binaries are only decompiled (static); none is executed.

Move FRESHBENCH_COMMIT, REVISION and MANIFEST_SHA256 together: a new revision
is a new corpus.
"""

from __future__ import annotations

import gzip
import hashlib
import json
import os
import shutil
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from urllib.parse import quote
from urllib.request import urlopen

FRESHBENCH_REPO = "fission-systems/freshbench"
# Also the scorer: CI builds `freshbench` from this commit.
FRESHBENCH_COMMIT = "31f3358c4d0de8a4c53162bc4546b4d8e0b63c78"
DATASET = "sjkim1127/freshbench-v2"
REVISION = "15b6f2c52c379cf9361f22edaa75886602e5e1ff"
# Uncompressed `corpus/m1/manifest.jsonl.gz`, as corpus/m1/README.md states.
MANIFEST_SHA256 = "d64f8af35369769b0fe0b49a3b15f7165bf800c2f1997d2f1e0eca7671318318"
REPO = Path(__file__).resolve().parents[2]
ATTEMPTS = 4


def _download(url: str, target: Path) -> None:
    target.parent.mkdir(parents=True, exist_ok=True)
    partial = target.with_name(target.name + ".part")
    for attempt in range(ATTEMPTS):
        try:
            with urlopen(url, timeout=120) as response, open(partial, "wb") as out:
                shutil.copyfileobj(response, out, 1 << 20)
            partial.replace(target)
            return
        except OSError:
            if attempt == ATTEMPTS - 1:
                raise
            time.sleep(2 ** attempt)


def digest(path: Path) -> str:
    hasher = hashlib.sha256()
    with open(path, "rb") as stream:
        for block in iter(lambda: stream.read(1 << 20), b""):
            hasher.update(block)
    return hasher.hexdigest()


def manifest(root: Path) -> list[dict]:
    """Every manifest entry: path (relative to m1/), sha256, size, kind,
    project, arch, compiler, config."""
    cached = root / "manifest.jsonl.gz"
    if not cached.exists():
        _download(
            f"https://raw.githubusercontent.com/{FRESHBENCH_REPO}/{FRESHBENCH_COMMIT}/corpus/m1/manifest.jsonl.gz",
            cached,
        )
    raw = gzip.decompress(cached.read_bytes())
    if hashlib.sha256(raw).hexdigest() != MANIFEST_SHA256:
        cached.unlink()
        raise SystemExit("FreshBench M1 manifest hash differs from the pinned one")
    return [json.loads(line) for line in raw.decode().splitlines() if line.strip()]


def build_of(entry: dict) -> str:
    """`gcc-O2`, `i686-gcc-O0`, ...: the path's second component for
    binaries, stripped binaries and truth."""
    return entry["path"].split("/")[1]


def fetch(entry: dict, root: Path) -> Path:
    """The file for `entry` under `<root>/m1/`, downloaded once and checked."""
    target = root / "m1" / entry["path"]
    if not (target.exists() and target.stat().st_size == entry["size"] and digest(target) == entry["sha256"]):
        _download(
            f"https://huggingface.co/datasets/{DATASET}/resolve/{REVISION}/m1/{quote(entry['path'])}",
            target,
        )
        if target.stat().st_size != entry["size"] or digest(target) != entry["sha256"]:
            raise SystemExit(f"FreshBench file differs from the manifest: {entry['path']}")
    return target


def fetch_all(entries: list[dict], root: Path, workers: int = 8) -> list[Path]:
    with ThreadPoolExecutor(workers) as pool:
        return list(pool.map(lambda entry: fetch(entry, root), entries))


def stripped(entries: list[dict]) -> list[dict]:
    """The decompiler inputs."""
    return [entry for entry in entries if entry["kind"] == "stripped"]


def scoring_files(entries: list[dict], units: set[tuple[str, str]]) -> list[dict]:
    """Everything `freshbench sweep run` reads for the given (build, project)
    units: stripped and unstripped binaries, the targets, and the project's
    preprocessed sources for that architecture."""
    archs = {
        (entry["arch"], entry["project"])
        for entry in entries
        if entry["kind"] == "ground_truth" and (build_of(entry), entry["project"]) in units
    }
    chosen = []
    for entry in entries:
        kind = entry["kind"]
        if kind in ("stripped", "binary", "ground_truth"):
            if (build_of(entry), entry["project"]) in units:
                chosen.append(entry)
        elif kind == "source" and (entry["arch"], entry["project"]) in archs:
            chosen.append(entry)
    return chosen


def telemetry_free_env() -> dict[str, str]:
    """`freshbench sweep run` refuses to measure with error reporting on."""
    env = dict(os.environ)
    for name in ("FRESHBENCH_SENTRY", "FISSION_SENTRY", "SENTRY_DSN"):
        env.pop(name, None)
    return env
