#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
from pathlib import Path

from grand_finale_support.corpus_candidates import (
    aligned_explicit_candidate_entry,
    blocked_explicit_candidate_entry,
    candidate_passes_explicit_quality_prefilter,
    candidate_passes_heuristic_quality_prefilter,
    candidate_sort_key,
    curated_quality_entry,
    explicit_fact_total,
    run_candidate_inventory,
)
from grand_finale_support.inventory_reader import load_source_inventory


ROOT_DIR = Path(__file__).resolve().parents[3]
DEFAULT_FISSION_BIN = ROOT_DIR / "target" / "release" / "fission_cli"
DEFAULT_CORPUS_FILE = ROOT_DIR / "scripts" / "test" / "batch_benchmark" / "corpora" / "preview_quality_corpus.json"
DEFAULT_CANDIDATES_FILE = ROOT_DIR / "scripts" / "test" / "batch_benchmark" / "corpora" / "preview_quality_candidates.json"
DEFAULT_SOURCE_INVENTORY_FILE = ROOT_DIR / "scripts" / "test" / "batch_benchmark" / "corpora" / "preview_explicit_source_inventory.json"
DEFAULT_BLOCKED_FILE = ROOT_DIR / "scripts" / "test" / "batch_benchmark" / "corpora" / "preview_explicit_blocked_candidates.json"
DEFAULT_ALIGNED_CANDIDATES_FILE = ROOT_DIR / "scripts" / "test" / "batch_benchmark" / "corpora" / "preview_explicit_aligned_candidate_report.json"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Build curated preview quality corpus candidates.")
    parser.add_argument("binaries", nargs="+", help="Target binaries to inventory")
    parser.add_argument("--fission-bin", type=Path, default=DEFAULT_FISSION_BIN)
    parser.add_argument("--corpus-file", type=Path, default=DEFAULT_CORPUS_FILE)
    parser.add_argument("--candidates-file", type=Path, default=DEFAULT_CANDIDATES_FILE)
    parser.add_argument("--blocked-file", type=Path, default=DEFAULT_BLOCKED_FILE)
    parser.add_argument("--aligned-candidates-file", type=Path, default=DEFAULT_ALIGNED_CANDIDATES_FILE)
    parser.add_argument("--source-inventory-file", type=Path, default=DEFAULT_SOURCE_INVENTORY_FILE)
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


def merge_counts(target: dict[str, int], source: dict[str, int] | None) -> None:
    if not source:
        return
    for key, value in source.items():
        target[key] = target.get(key, 0) + int(value or 0)


def main() -> int:
    args = parse_args()
    if not args.fission_bin.exists():
        raise SystemExit(f"Fission binary not found: {args.fission_bin}")

    all_candidates: list[dict] = []
    curated_explicit_entries: list[dict] = []
    curated_heuristic_entries: list[dict] = []
    blocked_explicit_entries: list[dict] = []
    aligned_candidates: list[dict] = []
    inventory_summaries: list[dict] = []
    source_inventory = load_source_inventory(args.source_inventory_file)

    binary_paths = [Path(item).resolve() for item in args.binaries]
    for binary_path in binary_paths:
        report = run_candidate_inventory(
            ROOT_DIR,
            binary_path,
            args.fission_bin,
            timeout_ms=args.timeout_ms,
            limit=args.candidate_limit,
        )
        inventory_summaries.append(report.get("summary", {}))
        candidates = report.get("candidates", [])
        all_candidates.extend(candidates)
        source_meta = source_inventory.get(str(binary_path)) or source_inventory.get(binary_path.name) or source_inventory.get(binary_path.stem)

        explicit_primary = sorted(
            [entry for entry in candidates if candidate_passes_explicit_quality_prefilter(entry, source_meta)],
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
        curated_heuristic_entries.extend(
            curated_quality_entry(entry) for entry in heuristic_primary[: args.per_binary_limit]
        )

        aligned_candidates.extend(
            aligned_explicit_candidate_entry(entry, source_meta)
            for entry in candidates
            if source_meta and source_meta.get("admission_alignment") == "aligned"
        )

        blocked_explicit_entries.extend(
            blocked_explicit_candidate_entry(entry, source_meta)
            for entry in candidates
            if explicit_fact_total(entry) > 0 and not candidate_passes_explicit_quality_prefilter(entry, source_meta)
        )

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
        inventory_summaries.append(report.get("summary", {}))
        candidates = report.get("candidates", [])
        all_candidates.extend(candidates)
        source_meta = source_inventory.get(str(binary_path)) or source_inventory.get(binary_path.name) or source_inventory.get(binary_path.stem)
        curated_explicit_entries.extend(
            curated_quality_entry(entry)
            for entry in candidates
            if candidate_passes_explicit_quality_prefilter(entry, source_meta)
        )

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
        inventory_summaries.append(report.get("summary", {}))
        candidates = report.get("candidates", [])
        all_candidates.extend(candidates)
        curated_heuristic_entries.extend(curated_quality_entry(entry) for entry in candidates)

    deduped_explicit = dedupe(curated_explicit_entries)
    explicit_keys = {(entry["binary"], entry["address"]) for entry in deduped_explicit}
    deduped_heuristic = [
        entry for entry in dedupe(curated_heuristic_entries) if (entry["binary"], entry["address"]) not in explicit_keys
    ]
    deduped_blocked = dedupe(blocked_explicit_entries)
    deduped_aligned = dedupe(aligned_candidates)

    block_reason_counts: dict[str, int] = {}
    for entry in deduped_blocked:
        reason = entry.get("block_reason") or "strict_filter_reject"
        block_reason_counts[reason] = block_reason_counts.get(reason, 0) + 1

    inventory_summary_totals = {
        "functions_total": 0,
        "rows_emitted": 0,
        "direct_success_count": 0,
        "preview_failure_count": 0,
        "panic_recovered_count": 0,
        "explicit_fact_nonzero_count": 0,
        "strict_explicit_candidate_count": 0,
        "heuristic_surface_candidate_count": 0,
        "inventory_surface_gap_count": 0,
        "aligned_with_zero_explicit_count": 0,
        "source_presence_counts": {},
        "explicit_breakdown_totals": {},
        "failure_kind_counts": {},
        "row_error_kind_counts": {},
    }
    for summary in inventory_summaries:
        inventory_summary_totals["functions_total"] += int(summary.get("functions_total", 0) or 0)
        inventory_summary_totals["rows_emitted"] += int(summary.get("rows_emitted", 0) or 0)
        inventory_summary_totals["direct_success_count"] += int(summary.get("direct_success_count", 0) or 0)
        inventory_summary_totals["preview_failure_count"] += int(summary.get("preview_failure_count", 0) or 0)
        inventory_summary_totals["panic_recovered_count"] += int(summary.get("panic_recovered_count", 0) or 0)
        inventory_summary_totals["explicit_fact_nonzero_count"] += int(summary.get("explicit_fact_nonzero_count", 0) or 0)
        inventory_summary_totals["strict_explicit_candidate_count"] += int(summary.get("strict_explicit_candidate_count", 0) or 0)
        inventory_summary_totals["heuristic_surface_candidate_count"] += int(summary.get("heuristic_surface_candidate_count", 0) or 0)
        inventory_summary_totals["inventory_surface_gap_count"] += int(summary.get("inventory_surface_gap_count", 0) or 0)
        inventory_summary_totals["aligned_with_zero_explicit_count"] += int(summary.get("aligned_with_zero_explicit_count", 0) or 0)
        merge_counts(inventory_summary_totals["source_presence_counts"], summary.get("source_presence_counts"))
        merge_counts(inventory_summary_totals["explicit_breakdown_totals"], summary.get("explicit_breakdown_totals"))
        merge_counts(inventory_summary_totals["failure_kind_counts"], summary.get("failure_kind_counts"))
        merge_counts(inventory_summary_totals["row_error_kind_counts"], summary.get("row_error_kind_counts"))

    args.candidates_file.parent.mkdir(parents=True, exist_ok=True)
    args.candidates_file.write_text(json.dumps({"candidates": all_candidates}, indent=2))
    args.aligned_candidates_file.write_text(
        json.dumps({"aligned_candidates": sorted(deduped_aligned, key=candidate_sort_key, reverse=True)}, indent=2)
    )
    args.blocked_file.write_text(
        json.dumps(
            {
                "blocked_candidates": sorted(deduped_blocked, key=candidate_sort_key, reverse=True),
                "block_reason_counts": block_reason_counts,
                "inventory_summary_totals": inventory_summary_totals,
            },
            indent=2,
        )
    )

    curated_corpus = {
        "timeout_rescue": load_timeout_rescue(args.corpus_file),
        "quality_explicit_facts": deduped_explicit,
        "quality_heuristic_surface": deduped_heuristic,
    }
    args.corpus_file.write_text(json.dumps(curated_corpus, indent=2))
    print(f"[+] Wrote inventory-backed candidates JSON to {args.candidates_file}")
    print(f"[+] Wrote aligned explicit candidate report to {args.aligned_candidates_file}")
    print(f"[+] Wrote blocked explicit candidate report to {args.blocked_file}")
    print(f"[+] Wrote curated corpus JSON to {args.corpus_file}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
