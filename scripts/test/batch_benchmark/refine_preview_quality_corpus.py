#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
from pathlib import Path

from grand_finale_support.corpus_candidates import (
    candidate_passes_explicit_quality_prefilter,
    candidate_passes_heuristic_quality_prefilter,
    curated_quality_entry,
    preview_hint_total,
    run_candidate_inventory,
)


ROOT_DIR = Path(__file__).resolve().parents[3]
DEFAULT_FISSION_BIN = ROOT_DIR / "target" / "release" / "fission_cli"
DEFAULT_CORPUS_FILE = ROOT_DIR / "scripts" / "test" / "batch_benchmark" / "corpora" / "preview_quality_corpus.json"
DEFAULT_CANDIDATES_FILE = ROOT_DIR / "scripts" / "test" / "batch_benchmark" / "corpora" / "preview_quality_candidates.json"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Build curated preview quality corpus candidates.")
    parser.add_argument("binaries", nargs="+", help="Target binaries to inventory")
    parser.add_argument("--fission-bin", type=Path, default=DEFAULT_FISSION_BIN)
    parser.add_argument("--corpus-file", type=Path, default=DEFAULT_CORPUS_FILE)
    parser.add_argument("--candidates-file", type=Path, default=DEFAULT_CANDIDATES_FILE)
    parser.add_argument("--timeout-ms", type=int, default=10000)
    parser.add_argument("--candidate-limit", type=int)
    parser.add_argument("--per-binary-limit", type=int, default=4)
    parser.add_argument(
        "--manual-explicit-seed",
        action="append",
        default=[],
        help="Extra binary@0xaddr seeds to force-inventory into the explicit-facts pool",
    )
    parser.add_argument(
        "--manual-heuristic-seed",
        action="append",
        default=[],
        help="Extra binary@0xaddr seeds to force-inventory into the heuristic-surface pool",
    )
    return parser.parse_args()


def candidate_sort_key(entry: dict) -> tuple[int, int, int, int]:
    return (
        int(entry.get("quality_potential_score", 0) or 0),
        int(entry.get("fact_density_score", 0) or 0),
        preview_hint_total(entry),
        -int(entry.get("pcode_op_count", 0) or 0),
    )


def load_timeout_rescue(corpus_path: Path) -> dict:
    if not corpus_path.exists():
        return {}
    try:
        data = json.loads(corpus_path.read_text())
    except Exception:
        return {}
    timeout_rescue = data.get("timeout_rescue", {})
    return timeout_rescue if isinstance(timeout_rescue, dict) else {}


def parse_manual_seed(seed: str) -> tuple[str, str]:
    binary, _, address = seed.partition("@")
    if not binary or not address:
        raise SystemExit(f"invalid --manual-seed format: {seed} (expected /path/to/bin@0xaddr)")
    return binary, address


def main() -> int:
    args = parse_args()
    if not args.fission_bin.exists():
        raise SystemExit(f"Fission binary not found: {args.fission_bin}")

    all_candidates: list[dict] = []
    curated_explicit_entries: list[dict] = []
    curated_heuristic_entries: list[dict] = []

    binary_paths = [Path(item).resolve() for item in args.binaries]
    for binary_path in binary_paths:
        report = run_candidate_inventory(
            ROOT_DIR,
            binary_path,
            args.fission_bin,
            timeout_ms=args.timeout_ms,
            limit=args.candidate_limit,
        )
        candidates = report.get("candidates", [])
        all_candidates.extend(candidates)
        explicit_primary = sorted(
            [entry for entry in candidates if candidate_passes_explicit_quality_prefilter(entry)],
            key=candidate_sort_key,
            reverse=True,
        )
        curated_explicit_entries.extend(
            curated_quality_entry(entry) for entry in explicit_primary[: args.per_binary_limit]
        )

        heuristic_primary = sorted(
            [entry for entry in candidates if candidate_passes_heuristic_quality_prefilter(entry)],
            key=candidate_sort_key,
            reverse=True,
        )
        selected = heuristic_primary[: args.per_binary_limit]
        curated_heuristic_entries.extend(curated_quality_entry(entry) for entry in selected)

    for seed in args.manual_explicit_seed:
        binary_str, address = parse_manual_seed(seed)
        binary_path = Path(binary_str).resolve()
        report = run_candidate_inventory(
            ROOT_DIR,
            binary_path,
            args.fission_bin,
            timeout_ms=args.timeout_ms,
            address=address,
        )
        candidates = report.get("candidates", [])
        all_candidates.extend(candidates)
        curated_explicit_entries.extend(curated_quality_entry(entry) for entry in candidates)

    for seed in args.manual_heuristic_seed:
        binary_str, address = parse_manual_seed(seed)
        binary_path = Path(binary_str).resolve()
        report = run_candidate_inventory(
            ROOT_DIR,
            binary_path,
            args.fission_bin,
            timeout_ms=args.timeout_ms,
            address=address,
        )
        candidates = report.get("candidates", [])
        all_candidates.extend(candidates)
        curated_heuristic_entries.extend(curated_quality_entry(entry) for entry in candidates)

    def dedupe(entries: list[dict]) -> list[dict]:
        deduped: list[dict] = []
        seen = set()
        for entry in entries:
            key = (entry["binary"], entry["address"])
            if key in seen:
                continue
            seen.add(key)
            deduped.append(entry)
        return deduped

    deduped_explicit = dedupe(curated_explicit_entries)
    explicit_keys = {(entry["binary"], entry["address"]) for entry in deduped_explicit}
    deduped_heuristic = [
        entry for entry in dedupe(curated_heuristic_entries) if (entry["binary"], entry["address"]) not in explicit_keys
    ]

    args.candidates_file.parent.mkdir(parents=True, exist_ok=True)
    args.candidates_file.write_text(json.dumps({"candidates": all_candidates}, indent=2))

    curated_corpus = {
        "timeout_rescue": load_timeout_rescue(args.corpus_file),
        "quality_explicit_facts": deduped_explicit,
        "quality_heuristic_surface": deduped_heuristic,
    }
    args.corpus_file.write_text(json.dumps(curated_corpus, indent=2))
    print(f"[+] Wrote candidates JSON to {args.candidates_file}")
    print(f"[+] Wrote curated corpus JSON to {args.corpus_file}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
