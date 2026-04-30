#!/usr/bin/env python3
"""Collect binary samples, hash them, and emit a Fission corpus manifest.

The output intentionally stays compatible with the existing full benchmark
`entries[].binary_path` contract. Hashes and provenance are additive metadata.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import sys
import tempfile
import urllib.parse
import urllib.request
from dataclasses import dataclass
from pathlib import Path
from typing import Any


REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_OUTPUT = REPO_ROOT / "benchmark/config/benchmark_corpus/realworld_samples.json"
DEFAULT_STORE = REPO_ROOT / "benchmark/binary/realworld"
MAX_MAGIC = 8


@dataclass(frozen=True)
class SampleSource:
    path: Path
    source: str
    source_kind: str


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def guess_format(path: Path) -> str:
    try:
        data = path.read_bytes()[:MAX_MAGIC]
    except OSError:
        return "unknown"
    if data.startswith(b"MZ"):
        return "pe_or_mz"
    if data.startswith(b"\x7fELF"):
        return "elf"
    if data.startswith((b"\xfe\xed\xfa\xcf", b"\xcf\xfa\xed\xfe")):
        return "macho64"
    if data.startswith((b"\xfe\xed\xfa\xce", b"\xce\xfa\xed\xfe")):
        return "macho32"
    if data.startswith((b"\xca\xfe\xba\xbe", b"\xbe\xba\xfe\xca")):
        return "macho_fat_or_java"
    if data.startswith(b":"):
        return "intel_hex"
    if data.startswith(b"S0") or data.startswith(b"S1") or data.startswith(b"S2") or data.startswith(b"S3"):
        return "motorola_srec"
    return "unknown"


def stable_id(path: Path, digest: str, prefix: str) -> str:
    stem = "".join(ch.lower() if ch.isalnum() else "-" for ch in path.stem).strip("-")
    stem = "-".join(part for part in stem.split("-") if part)
    return f"{prefix}-{stem or 'sample'}-{digest[:12]}"


def iter_inputs(inputs: list[Path]) -> list[SampleSource]:
    rows: list[SampleSource] = []
    for item in inputs:
        resolved = item.expanduser().resolve()
        if resolved.is_file():
            rows.append(SampleSource(resolved, str(resolved), "file"))
            continue
        if resolved.is_dir():
            for path in sorted(p for p in resolved.rglob("*") if p.is_file()):
                rows.append(SampleSource(path, str(resolved), "directory"))
            continue
        raise FileNotFoundError(f"input path does not exist: {item}")
    return rows


def read_url_list(path: Path) -> list[str]:
    urls: list[str] = []
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        urls.append(line)
    return urls


def download_urls(urls: list[str], download_dir: Path, timeout: float) -> list[SampleSource]:
    download_dir.mkdir(parents=True, exist_ok=True)
    rows: list[SampleSource] = []
    for index, url in enumerate(urls, start=1):
        parsed_name = Path(urllib.parse.urlparse(url).path).name or f"download-{index}.bin"
        target = download_dir / parsed_name
        with urllib.request.urlopen(url, timeout=timeout) as response:
            with tempfile.NamedTemporaryFile(dir=str(download_dir), delete=False) as tmp:
                shutil.copyfileobj(response, tmp)
                tmp_path = Path(tmp.name)
        tmp_path.replace(target)
        rows.append(SampleSource(target.resolve(), url, "url"))
    return rows


def copy_into_store(source: Path, store: Path, digest: str) -> Path:
    suffix = source.suffix
    target_dir = store / digest[:2] / digest[2:4]
    target_dir.mkdir(parents=True, exist_ok=True)
    target = target_dir / f"{source.stem}-{digest[:16]}{suffix}"
    if not target.exists():
        shutil.copy2(source, target)
    return target.resolve()


def make_entry(
    sample: SampleSource,
    binary_path: Path,
    digest: str,
    role: str,
    tags: list[str],
    seed_limit: int | None,
    id_prefix: str,
) -> dict[str, Any]:
    size = binary_path.stat().st_size
    guessed_format = guess_format(binary_path)
    entry_id = stable_id(binary_path, digest, id_prefix)
    entry_tags = sorted(set(tags + ["realworld", guessed_format]))
    entry: dict[str, Any] = {
        "id": entry_id,
        "binary_path": str(binary_path),
        "ghidra_project_key": entry_id,
        "role": role,
        "tags": entry_tags,
        "weight": 1,
        "metadata": {
            "sha256": digest,
            "size_bytes": size,
            "format_guess": guessed_format,
            "source": sample.source,
            "source_kind": sample.source_kind,
            "original_path": str(sample.path),
        },
    }
    if seed_limit is not None:
        entry["seed_limit"] = seed_limit
    return entry


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", action="append", type=Path, default=[], help="Input file or directory. Repeatable.")
    parser.add_argument("--url-list", type=Path, help="Text file containing one URL per line.")
    parser.add_argument("--download-dir", type=Path, default=DEFAULT_STORE / "_downloads")
    parser.add_argument("--download-timeout", type=float, default=60.0)
    parser.add_argument("--copy-to", type=Path, help="Copy samples into this store before manifesting.")
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--name", default="realworld-samples")
    parser.add_argument("--role", default="realworld_loader_smoke")
    parser.add_argument("--tag", action="append", default=[], help="Extra manifest tag. Repeatable.")
    parser.add_argument("--seed-limit", type=int)
    parser.add_argument("--id-prefix", default="realworld")
    parser.add_argument("--repo-relative", action="store_true", help="Write binary_path relative to repo root when possible.")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if not args.input and not args.url_list:
        raise SystemExit("provide at least one --input or --url-list")

    samples = iter_inputs(args.input)
    if args.url_list:
        samples.extend(download_urls(read_url_list(args.url_list), args.download_dir, args.download_timeout))

    entries: list[dict[str, Any]] = []
    seen: set[str] = set()
    for sample in samples:
        digest = sha256_file(sample.path)
        if digest in seen:
            continue
        seen.add(digest)
        manifest_path = copy_into_store(sample.path, args.copy_to, digest) if args.copy_to else sample.path
        output_path = manifest_path
        if args.repo_relative:
            try:
                output_path = output_path.resolve().relative_to(REPO_ROOT)
            except ValueError:
                pass
        entries.append(
            make_entry(
                sample=sample,
                binary_path=output_path,
                digest=digest,
                role=args.role,
                tags=list(args.tag),
                seed_limit=args.seed_limit,
                id_prefix=args.id_prefix,
            )
        )

    entries.sort(key=lambda row: (row["metadata"]["sha256"], row["binary_path"]))
    payload = {
        "name": args.name,
        "suite_tier": "advisory",
        "gate_mode": "advisory",
        "notes": "Generated by scripts/corpus/hash_and_manifest.py. Hash/provenance metadata is additive.",
        "entries": entries,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(json.dumps({"output": str(args.output), "entry_count": len(entries)}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
