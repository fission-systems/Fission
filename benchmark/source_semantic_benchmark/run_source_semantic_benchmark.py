#!/usr/bin/env python3
from __future__ import annotations

import argparse
import os
import sys
import time
import threading
from collections import Counter
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path
from typing import Any

# Ensure repository root is in sys.path to allow imports of benchmark submodules
repo_root = str(Path(__file__).resolve().parents[2])
if repo_root not in sys.path:
    sys.path.insert(0, repo_root)

from benchmark.source_semantic_benchmark.config import (
    ROOT_DIR,
    DEFAULT_MANIFEST,
    DEFAULT_FISSION_BIN,
    DEFAULT_GHIDRA_HOME,
    DEFAULT_GHIDRA_SCRIPT_DIR,
    DEFAULT_GHIDRA_EXPORT_SCRIPT,
    DEFAULT_ARTIFACT_ROOT,
    DEFAULT_DECOMP_CACHE_FILE,
    DEFAULT_LIST_CACHE_FILE,
    DEFAULT_BEHAVIOR_CACHE_FILE,
    DEFAULT_HISTORY_FILE,
    DEFAULT_LATEST_INDEX_FILE,
    DEBUG_DECOMP_EVIDENCE_CONTRACT,
    DEFAULT_JOBS,
    CANDIDATE_TIMEOUT_MIN_SEC,
    CANDIDATE_TIMEOUT_ORACLE_MULTIPLIER,
    CONTROL_WORDS,
    KNOWN_ARCH_TAGS,
    SOURCE_EXTENSIONS,
)

from benchmark.source_semantic_benchmark.models import (
    BenchmarkEntry,
    SourceFunction,
    FissionFunction,
)

from benchmark.source_semantic_benchmark.utils import (
    rel,
    sanitize_id,
    utc_now,
    utc_timestamp_slug,
    utc_isoformat,
    load_json,
    dump_json_pretty,
    dump_json_line,
    load_json_list_or_dict,
    resolve_path,
    canonical_address,
    normalize_name,
    percent,
)

from benchmark.source_semantic_benchmark.cache import (
    file_cache_fingerprint,
    decomp_cache_key,
    list_cache_key,
    behavior_cache_key,
    load_decomp_cache,
    save_decomp_cache,
    load_list_cache,
    save_list_cache,
    load_behavior_cache,
    save_behavior_cache,
)

from benchmark.source_semantic_benchmark.parser import (
    strip_comments,
    find_matching_brace,
    find_matching_paren,
    split_params,
    classify_param,
    param_name,
    param_names,
    classify_return,
    extract_source_functions,
    extract_go_functions,
    extract_rust_functions,
    extract_c_like_functions,
    c_like_pointer_typedef_aliases,
    filter_source_functions,
    filter_inlined_static_source_functions,
)

from benchmark.source_semantic_benchmark.cli import (
    run_fission_list,
    run_fission_list_cached,
    ghidra_headless_path,
    run_ghidra_reference_export,
    ghidra_function_index,
    run_fission_decomp,
    run_fission_decomp_cached,
    run_fission_decomp_batch,
    run_command_capture,
)

from benchmark.source_semantic_benchmark.behavior import (
    default_behavior_cases,
    default_behavior_cases_for_kinds,
    explicit_behavior_cases,
    behavior_supported,
    validate_explicit_behavior_cases,
    behavior_cases_for,
    source_harness,
    candidate_harness,
    render_case_call,
    c_int_array,
    collect_observed_globals,
    render_candidate_global_decl,
    candidate_support_code_blocks,
    remove_duplicate_candidate_global_decls,
    render_explicit_case_call,
    serialize_behavior_cases,
    compile_and_run_c,
    behavior_output_lines,
    partial_behavior_progress,
    compile_and_run_c_cached,
    behavior_cache_entry_is_valid,
    candidate_timeout_sec,
    c_host_execution_probe,
    run_behavior_check,
)

from benchmark.source_semantic_benchmark.scoring import (
    row_key,
    match_function,
    select_source_functions,
    source_call_counts,
    is_function_definition_call_match,
    call_names_for_fingerprint,
    function_pointer_param_names,
    indirect_cast_call_names_for_fingerprint,
    add_call_fingerprint,
    code_fingerprint,
    add_signature_fingerprint,
    add_rendered_signature_fingerprint,
    rendered_signature_kinds,
    inline_expanded_source_fingerprint,
    expand_source_body_fingerprint,
    multiset_jaccard,
    multiset_gap_details,
    fingerprint_subset,
    static_similarity_components,
    static_similarity_gap_components,
    gap_feature_count,
    signedness_only_signature_gap,
    metric_delta,
    compare_to_baseline,
    comparison_outcome,
    improvement_summary,
    stage_first_failure,
    zero_credit_reason,
    row_zero_credit_reason,
    row_triage_priority,
    triage_row_summary,
    compare_ghidra_reference,
    canonical_sleigh_template_source,
    sleigh_template_source_gate,
    STATIC_SIMILARITY_COMPONENTS,
    cfg_topological_similarity,
    compute_readability_score,
    compute_semantic_correctness_score,
    generate_ai_remedy_hints,
)

from benchmark.source_semantic_benchmark.reporting import (
    summarize,
    render_markdown,
    metric_bucket_export,
    numeric_items,
    snapshot_baseline_artifacts,
    update_latest_index,
    load_history_records,
    history_snapshot,
    append_history_record,
    write_single_debug_bundle,
    debug_bundle_path_for_parts,
    debug_bundle_path_for_row,
    debug_triage_path_for_row,
    decomp_debug_command_for_row,
    attach_debug_repro_commands,
    top_debug_commands,
    materialize_debug_triage_for_rows,
    materialize_debug_triage,
    materialize_regression_debug_triage,
)


def discover_source_entries(manifest: dict[str, Any]) -> list[BenchmarkEntry]:
    entries: list[BenchmarkEntry] = []

    for raw in manifest.get("entries", []):
        source_path = resolve_path(raw["source_path"])
        binary_path = resolve_path(raw["binary_path"])
        language = raw.get("language") or SOURCE_EXTENSIONS.get(source_path.suffix, "")
        entries.append(
            BenchmarkEntry(
                id=raw.get("id") or sanitize_id(rel(binary_path)),
                binary_path=binary_path,
                source_path=source_path,
                language=language,
                tags=list(raw.get("tags") or []),
                weight=float(raw.get("weight", 1.0) or 1.0),
                behavior_cases=raw.get("behavior_cases"),
            )
        )

    for spec in manifest.get("discovery", []):
        root = resolve_path(spec.get("root", "benchmark/binary"))
        languages = set(spec.get("languages") or SOURCE_EXTENSIONS.values())
        require_binary = bool(spec.get("require_binary", True))
        tags = list(spec.get("tags") or [])
        for source_path in sorted(root.rglob("*")):
            if not source_path.is_file():
                continue
            language = SOURCE_EXTENSIONS.get(source_path.suffix)
            if not language or language not in languages:
                continue
            if f"{os.sep}source{os.sep}" not in str(source_path):
                continue
            binary_paths = matching_binary_paths(source_path)
            if require_binary and not binary_paths:
                continue
            if not binary_paths:
                binary_paths = [Path("")]
            for binary_path in binary_paths:
                entry_id = sanitize_id(f"{rel(binary_path)}::{rel(source_path)}")
                entries.append(
                    BenchmarkEntry(
                        id=entry_id,
                        binary_path=binary_path,
                        source_path=source_path,
                        language=language,
                        tags=tags + [language],
                        behavior_cases=spec.get("behavior_cases"),
                    )
                )

    dedup: dict[tuple[str, str], BenchmarkEntry] = {}
    for entry in entries:
        key = (str(entry.binary_path), str(entry.source_path))
        dedup.setdefault(key, entry)
    return list(dedup.values())


def filter_entries(
    entries: list[BenchmarkEntry],
    entry_ids: list[str] | None,
    required_tags: list[str] | None,
) -> list[BenchmarkEntry]:
    if entry_ids:
        wanted_ids = set(entry_ids)
        entries = [entry for entry in entries if entry.id in wanted_ids]
    if required_tags:
        wanted_tags = set(required_tags)
        entries = [
            entry
            for entry in entries
            if wanted_tags.issubset(set(entry.tags))
        ]
    return entries


def matching_binary_paths(source_path: Path) -> list[Path]:
    parts = list(source_path.parts)
    try:
        source_idx = parts.index("source")
    except ValueError:
        return []
    if source_idx + 1 >= len(parts):
        return []
    prefix = Path(*parts[:source_idx])
    language_dir = parts[source_idx + 1]
    stem = source_path.stem
    binary_root = prefix / "binary" / language_dir
    if not binary_root.exists():
        return []
    matches: list[Path] = []
    for candidate in sorted(binary_root.rglob("*")):
        if not candidate.is_file():
            continue
        if "_ghidra" in str(candidate):
            continue
        if candidate.name == stem or candidate.stem == stem:
            matches.append(candidate)
    return matches


def infer_entry_arch(entry: BenchmarkEntry) -> str:
    for tag in entry.tags:
        normalized = str(tag).strip().lower()
        if normalized in KNOWN_ARCH_TAGS:
            return "x86-64" if normalized == "x86_64" else normalized
    parts = [part.lower() for part in entry.binary_path.parts]
    try:
        binary_idx = parts.index("binary")
    except ValueError:
        binary_idx = -1
    if binary_idx >= 0 and binary_idx + 1 < len(parts):
        candidate = parts[binary_idx + 1]
        return "x86-64" if candidate == "x86_64" else candidate
    return "unknown"


def source_param_shape(param_kinds: list[str]) -> str:
    if not param_kinds:
        return "arity_0"
    counts = Counter(param_kinds)
    if len(counts) == 1:
        return f"{param_kinds[0]}_arity_{len(param_kinds)}"
    return "+".join(f"{kind}{counts[kind]}" for kind in sorted(counts))


def prewarm_decomp_cache_for_entry(
    entry: BenchmarkEntry,
    source_functions: list[SourceFunction],
    fission_funcs: list[FissionFunction],
    fission_error: str | None,
    fission_bin: Path,
    timeout_sec: int,
    include_debug_decomp: bool,
    output_dir: Path,
    cache: dict[str, dict[str, Any]],
    cache_lock: threading.Lock,
    cache_stats: Counter[str],
) -> None:
    if fission_error or len(source_functions) <= 1:
        return
    address_paths: list[tuple[str, Path | None]] = []
    address_to_cache_key: dict[str, str] = {}
    for func in source_functions:
        _status, matched, _candidates = match_function(func, fission_funcs)
        if matched is None:
            continue
        key = decomp_cache_key(entry.binary_path, matched.address, fission_bin, include_debug_decomp)
        bundle_path = (
            debug_bundle_path_for_parts(output_dir, entry.id, func.name, matched.address)
            if include_debug_decomp
            else None
        )
        with cache_lock:
            cached = cache.get(key)
        if cached is not None and (
            not include_debug_decomp
            or bundle_path is None
            or bundle_path.exists()
        ):
            continue
        canonical = canonical_address(matched.address)
        address_to_cache_key[canonical] = key
        address_paths.append((matched.address, bundle_path))
    if len(address_paths) <= 1:
        return
    batch_results = run_fission_decomp_batch(
        entry.binary_path,
        address_paths,
        fission_bin,
        timeout_sec,
        include_debug_decomp,
        output_dir,
        entry.id,
    )
    if not batch_results:
        return
    with cache_lock:
        for address, result in batch_results.items():
            key = address_to_cache_key.get(address)
            if key is None:
                continue
            cache[key] = result
            cache_stats["miss"] += 1
            cache_stats["stored"] += 1


def load_baseline_artifacts(path: Path) -> tuple[dict[str, Any], list[dict[str, Any]], Path]:
    summary_path = path
    if path.is_dir():
        summary_path = path / "source_semantic_summary.json"
    summary = load_json(summary_path)
    rows_path = summary_path.parent / "source_semantic_rows.json"
    rows = load_json(rows_path) if rows_path.exists() else []
    if not isinstance(rows, list):
        rows = []
    return summary, rows, summary_path


def find_latest_baseline_dir(
    output_dir: Path,
    manifest_name: str,
    current_row_keys: set[str],
) -> Path | None:
    root = DEFAULT_ARTIFACT_ROOT
    if not root.exists():
        return None
    output_resolved = output_dir.resolve()
    candidates: list[tuple[float, Path]] = []
    for summary_path in root.rglob("source_semantic_summary.json"):
        try:
            parent_resolved = summary_path.parent.resolve()
        except OSError:
            continue
        if parent_resolved == output_resolved:
            continue
        try:
            summary = load_json(summary_path)
        except Exception:
            continue
        if summary.get("manifest") != manifest_name:
            continue
        rows_path = summary_path.parent / "source_semantic_rows.json"
        baseline_keys: set[str] = set()
        if rows_path.exists():
            try:
                raw_rows = load_json(rows_path)
                if isinstance(raw_rows, list):
                    baseline_keys = {row_key(row) for row in raw_rows if isinstance(row, dict)}
            except Exception:
                baseline_keys = set()
        try:
            mtime = summary_path.stat().st_mtime
        except OSError:
            continue
        if current_row_keys and baseline_keys != current_row_keys:
            continue
        if not current_row_keys and summary.get("row_count") != 0:
            continue
        candidates.append((mtime, summary_path.parent))
    if not candidates:
        return None
    return max(candidates, key=lambda item: item[0])[1]


def row_for_function(
    entry: BenchmarkEntry,
    func: SourceFunction,
    source_functions_by_name: dict[str, SourceFunction],
    fission_funcs: list[FissionFunction],
    fission_error: str | None,
    fission_bin: Path,
    timeout_sec: int,
    host_execution: dict[str, Any],
    decomp_cache: dict[str, dict[str, Any]],
    decomp_cache_lock: threading.Lock,
    decomp_cache_stats: Counter[str],
    behavior_cache: dict[str, dict[str, Any]] | None,
    behavior_cache_lock: threading.Lock | None,
    behavior_cache_stats: Counter[str],
    include_debug_decomp: bool,
    output_dir: Path | None = None,
    enable_sanitizer: bool = False,
    fuzz_cases: bool = False,
) -> dict[str, Any]:
    source_fp_direct = code_fingerprint(func.body, func)
    source_fp_inline_expanded = inline_expanded_source_fingerprint(func, source_functions_by_name)
    mapping_status, matched, candidates = match_function(func, fission_funcs) if not fission_error else ("list_failed", None, [])
    decomp: dict[str, Any] = {"success": False, "failure_kind": mapping_status}
    if matched is not None:
        debug_decomp_bundle_path = (
            debug_bundle_path_for_parts(output_dir, entry.id, func.name, matched.address)
            if include_debug_decomp and output_dir is not None
            else None
        )
        decomp = run_fission_decomp_cached(
            entry.binary_path,
            matched.address,
            fission_bin,
            timeout_sec,
            include_debug_decomp,
            debug_decomp_bundle_path,
            decomp_cache,
            decomp_cache_lock,
            decomp_cache_stats,
        )
    else:
        decomp["decomp_cache_status"] = "not_requested"
    decomp_code = decomp.get("code") if decomp.get("success") else None
    source_body_lines = func.body.splitlines()
    decomp_lines = decomp_code.splitlines() if decomp_code else []
    decomp_signature = rendered_signature_kinds(decomp_code) if decomp_code else None
    decomp_return_kind = decomp_signature[0] if decomp_signature is not None else None
    decomp_param_kinds = decomp_signature[1] if decomp_signature is not None else None
    decomp_fp = code_fingerprint(decomp_code or "") if decomp_code else Counter()
    static_score_direct = multiset_jaccard(source_fp_direct, decomp_fp) if decomp_code else 0.0
    static_score_inline_expanded = (
        multiset_jaccard(source_fp_inline_expanded, decomp_fp)
        if decomp_code and source_fp_inline_expanded != source_fp_direct
        else static_score_direct
    )
    if static_score_inline_expanded > static_score_direct:
        source_fp = source_fp_inline_expanded
        static_score = static_score_inline_expanded
        static_source_variant = "same_source_inline_expanded"
    else:
        source_fp = source_fp_direct
        static_score = static_score_direct
        static_source_variant = "direct_source"
    static_components = static_similarity_components(source_fp, decomp_fp) if decomp_code else {
        name: 0.0 for name in STATIC_SIMILARITY_COMPONENTS
    }
    static_gaps = multiset_gap_details(source_fp, decomp_fp) if decomp_code else multiset_gap_details(source_fp, Counter())
    static_gap_components = (
        static_similarity_gap_components(source_fp, decomp_fp)
        if decomp_code
        else static_similarity_gap_components(source_fp, Counter())
    )
    behavior = run_behavior_check(
        entry,
        func,
        decomp_code,
        timeout_sec,
        host_execution,
        behavior_cache,
        behavior_cache_lock,
        behavior_cache_stats,
        output_dir=output_dir,
        address=matched.address if matched else None,
        enable_sanitizer=enable_sanitizer,
        fuzz_cases=fuzz_cases,
    )
    semantic_score = round(0.65 * float(behavior.get("score", 0.0)) + 0.35 * static_score, 6)
    debug_decomp = decomp.get("debug_decomp")
    semantic_correctness_score = compute_semantic_correctness_score(behavior)
    readability_score = compute_readability_score(func.body, decomp_code or "")
    ai_remedy_hints = generate_ai_remedy_hints(func.body, decomp_code, behavior)
    return {
        "candidate_decompiler": "fission",
        "entry_id": entry.id,
        "binary_path": rel(entry.binary_path),
        "binary_arch": infer_entry_arch(entry),
        "source_path": rel(entry.source_path),
        "language": entry.language,
        "tags": entry.tags,
        "function_name": func.name,
        "source_line": func.line,
        "source_signature": func.signature,
        "source_is_static": func.is_static,
        "source_return_kind": func.return_kind,
        "source_param_kinds": func.param_kinds,
        "source_param_shape": source_param_shape(func.param_kinds),
        "source_body_line_count": len(source_body_lines),
        "source_body_byte_count": len(func.body.encode("utf-8")),
        "source_static_feature_count": sum(source_fp.values()),
        "source_static_feature_count_direct": sum(source_fp_direct.values()),
        "source_static_feature_count_inline_expanded": sum(source_fp_inline_expanded.values()),
        "address": matched.address if matched else None,
        "fission_name": matched.name if matched else None,
        "mapping_status": mapping_status,
        "mapping_candidates": candidates,
        "list_error": fission_error,
        "decomp_success": bool(decomp.get("success")),
        "decomp_failure_kind": decomp.get("failure_kind"),
        "decomp_failure_detail": decomp.get("failure_detail"),
        "engine_used": decomp.get("engine_used"),
        "preview_build_stats": decomp.get("preview_build_stats"),
        "debug_decomp_bundle_path": decomp.get("debug_decomp_bundle_path"),
        "debug_decomp": debug_decomp,
        "decomp_cache_status": decomp.get("decomp_cache_status", "not_requested"),
        "decomp_wall_sec": decomp.get("wall_sec"),
        "decomp_line_count": len(decomp_lines),
        "decomp_byte_count": len(decomp_code.encode("utf-8")) if decomp_code else 0,
        "decomp_return_kind": decomp_return_kind,
        "decomp_param_kinds": decomp_param_kinds,
        "decomp_param_shape": source_param_shape(decomp_param_kinds or []) if decomp_param_kinds is not None else None,
        "decomp_static_feature_count": sum(decomp_fp.values()),
        "static_semantic_score": static_score,
        "static_semantic_score_percent": percent(static_score),
        "static_semantic_score_direct": static_score_direct,
        "static_semantic_score_inline_expanded": static_score_inline_expanded,
        "static_similarity_source_variant": static_source_variant,
        "static_similarity_components": static_components,
        "static_similarity_gaps": static_gaps,
        "static_similarity_gap_components": static_gap_components,
        "behavior": behavior,
        "semantic_score": semantic_score,
        "semantic_score_percent": percent(semantic_score),
        "semantic_correctness_score": semantic_correctness_score,
        "readability_score": readability_score,
        "ai_remedy_hints": ai_remedy_hints,
        "zero_credit_reason": zero_credit_reason(mapping_status, decomp, behavior, static_score, semantic_score),
        "stage_first_failure": stage_first_failure(debug_decomp),
    }


def row_for_ghidra_function(
    entry: BenchmarkEntry,
    func: SourceFunction,
    source_functions_by_name: dict[str, SourceFunction],
    ghidra_funcs: list[FissionFunction],
    ghidra_by_address: dict[str, dict[str, Any]],
    ghidra_error: str | None,
    timeout_sec: int,
    host_execution: dict[str, Any],
    behavior_cache: dict[str, dict[str, Any]] | None,
    behavior_cache_lock: threading.Lock | None,
    behavior_cache_stats: Counter[str],
    output_dir: Path | None = None,
    enable_sanitizer: bool = False,
    fuzz_cases: bool = False,
) -> dict[str, Any]:
    source_fp_direct = code_fingerprint(func.body, func)
    source_fp_inline_expanded = inline_expanded_source_fingerprint(func, source_functions_by_name)
    mapping_status, matched, candidates = match_function(func, ghidra_funcs) if not ghidra_error else ("list_failed", None, [])
    decomp: dict[str, Any] = {"success": False, "failure_kind": mapping_status}
    if matched is not None:
        raw = ghidra_by_address.get(matched.address, {})
        decomp = {
            "success": bool(raw.get("success")) and bool(str(raw.get("code") or "").strip()),
            "code": raw.get("code") or "",
            "failure_kind": None if raw.get("success") else "decompile_error",
            "failure_detail": raw.get("error"),
            "wall_sec": raw.get("decompile_sec"),
            "engine_used": "ghidra",
        }
        if not decomp["success"] and not decomp.get("failure_detail"):
            decomp["failure_detail"] = "empty_output"
    decomp_code = decomp.get("code") if decomp.get("success") else None
    source_body_lines = func.body.splitlines()
    decomp_lines = decomp_code.splitlines() if decomp_code else []
    decomp_signature = rendered_signature_kinds(decomp_code) if decomp_code else None
    decomp_return_kind = decomp_signature[0] if decomp_signature is not None else None
    decomp_param_kinds = decomp_signature[1] if decomp_signature is not None else None
    decomp_fp = code_fingerprint(decomp_code or "") if decomp_code else Counter()
    static_score_direct = multiset_jaccard(source_fp_direct, decomp_fp) if decomp_code else 0.0
    static_score_inline_expanded = (
        multiset_jaccard(source_fp_inline_expanded, decomp_fp)
        if decomp_code and source_fp_inline_expanded != source_fp_direct
        else static_score_direct
    )
    if static_score_inline_expanded > static_score_direct:
        source_fp = source_fp_inline_expanded
        static_score = static_score_inline_expanded
        static_source_variant = "same_source_inline_expanded"
    else:
        source_fp = source_fp_direct
        static_score = static_score_direct
        static_source_variant = "direct_source"
    static_components = static_similarity_components(source_fp, decomp_fp) if decomp_code else {
        name: 0.0 for name in STATIC_SIMILARITY_COMPONENTS
    }
    static_gaps = multiset_gap_details(source_fp, decomp_fp) if decomp_code else multiset_gap_details(source_fp, Counter())
    static_gap_components = (
        static_similarity_gap_components(source_fp, decomp_fp)
        if decomp_code
        else static_similarity_gap_components(source_fp, Counter())
    )
    behavior_output_dir = output_dir / "ghidra_reference_behavior" if output_dir is not None else None
    behavior = run_behavior_check(
        entry,
        func,
        decomp_code,
        timeout_sec,
        host_execution,
        behavior_cache,
        behavior_cache_lock,
        behavior_cache_stats,
        output_dir=behavior_output_dir,
        address=matched.address if matched else None,
        enable_sanitizer=enable_sanitizer,
        fuzz_cases=fuzz_cases,
    )
    semantic_score = round(0.65 * float(behavior.get("score", 0.0)) + 0.35 * static_score, 6)
    semantic_correctness_score = compute_semantic_correctness_score(behavior)
    readability_score = compute_readability_score(func.body, decomp_code or "")
    ai_remedy_hints = generate_ai_remedy_hints(func.body, decomp_code, behavior)
    return {
        "candidate_decompiler": "ghidra",
        "entry_id": entry.id,
        "binary_path": rel(entry.binary_path),
        "binary_arch": infer_entry_arch(entry),
        "source_path": rel(entry.source_path),
        "language": entry.language,
        "tags": entry.tags,
        "function_name": func.name,
        "source_line": func.line,
        "source_signature": func.signature,
        "source_is_static": func.is_static,
        "source_return_kind": func.return_kind,
        "source_param_kinds": func.param_kinds,
        "source_param_shape": source_param_shape(func.param_kinds),
        "source_body_line_count": len(source_body_lines),
        "source_body_byte_count": len(func.body.encode("utf-8")),
        "source_static_feature_count": sum(source_fp.values()),
        "source_static_feature_count_direct": sum(source_fp_direct.values()),
        "source_static_feature_count_inline_expanded": sum(source_fp_inline_expanded.values()),
        "address": matched.address if matched else None,
        "fission_name": matched.name if matched else None,
        "mapping_status": mapping_status,
        "mapping_candidates": candidates,
        "list_error": ghidra_error,
        "decomp_success": bool(decomp.get("success")),
        "decomp_failure_kind": decomp.get("failure_kind"),
        "decomp_failure_detail": decomp.get("failure_detail"),
        "engine_used": decomp.get("engine_used"),
        "preview_build_stats": None,
        "debug_decomp_bundle_path": None,
        "debug_decomp": None,
        "decomp_cache_status": "not_requested",
        "decomp_wall_sec": decomp.get("wall_sec"),
        "decomp_line_count": len(decomp_lines),
        "decomp_byte_count": len(decomp_code.encode("utf-8")) if decomp_code else 0,
        "decomp_return_kind": decomp_return_kind,
        "decomp_param_kinds": decomp_param_kinds,
        "decomp_param_shape": source_param_shape(decomp_param_kinds or []) if decomp_param_kinds is not None else None,
        "decomp_static_feature_count": sum(decomp_fp.values()),
        "static_semantic_score": static_score,
        "static_semantic_score_percent": percent(static_score),
        "static_semantic_score_direct": static_score_direct,
        "static_semantic_score_inline_expanded": static_score_inline_expanded,
        "static_similarity_source_variant": static_source_variant,
        "static_similarity_components": static_components,
        "static_similarity_gaps": static_gaps,
        "static_similarity_gap_components": static_gap_components,
        "behavior": behavior,
        "semantic_score": semantic_score,
        "semantic_score_percent": percent(semantic_score),
        "semantic_correctness_score": semantic_correctness_score,
        "readability_score": readability_score,
        "ai_remedy_hints": ai_remedy_hints,
        "zero_credit_reason": zero_credit_reason(mapping_status, decomp, behavior, static_score, semantic_score),
        "stage_first_failure": None,
    }


def run_benchmark(args: argparse.Namespace) -> int:
    start = time.perf_counter()
    created_at = utc_now()
    manifest_path = resolve_path(args.manifest)
    manifest = load_json(manifest_path)
    manifest_name = manifest.get("name", manifest_path.stem)
    run_id = f"{sanitize_id(manifest_name)}-{utc_timestamp_slug(created_at)}"
    entries = discover_source_entries(manifest)
    entries = filter_entries(entries, args.entry_id, args.tag)
    if args.limit_binaries is not None:
        entries = entries[: args.limit_binaries]

    output_dir = resolve_path(args.output_dir) if args.output_dir else DEFAULT_ARTIFACT_ROOT / run_id
    output_dir.mkdir(parents=True, exist_ok=True)
    fission_bin = resolve_path(args.fission_bin)
    ghidra_home = resolve_path(args.ghidra_home) if args.include_ghidra_reference else None
    host_execution = c_host_execution_probe(args.timeout_sec)

    rows: list[dict[str, Any]] = []
    ghidra_rows: list[dict[str, Any]] = []
    ghidra_reference_exports: list[dict[str, Any]] = []
    jobs = max(1, int(args.jobs or 1))
    decomp_cache_path = None if args.no_decomp_cache else resolve_path(args.decomp_cache_file)
    decomp_cache: dict[str, dict[str, Any]] = load_decomp_cache(decomp_cache_path)
    decomp_cache_lock = threading.Lock()
    decomp_cache_stats: Counter[str] = Counter()
    decomp_cache_initial_entry_count = len(decomp_cache)
    list_cache_path = None if args.no_list_cache else resolve_path(args.list_cache_file)
    list_cache: dict[str, dict[str, Any]] = load_list_cache(list_cache_path)
    list_cache_lock = threading.Lock()
    list_cache_stats: Counter[str] = Counter()
    list_cache_initial_entry_count = len(list_cache)
    behavior_cache_path = None if args.no_behavior_cache else resolve_path(args.behavior_cache_file)
    behavior_cache: dict[str, dict[str, Any]] | None = load_behavior_cache(behavior_cache_path)
    behavior_cache_lock = threading.Lock()
    behavior_cache_stats: Counter[str] = Counter()
    behavior_cache_initial_entry_count = len(behavior_cache or {})
    source_row_selection_entries: list[dict[str, Any]] = []
    suppressed_static_inline_rows: list[dict[str, Any]] = []
    phase_list_sec = 0.0
    phase_decomp_sec = 0.0
    phase_behavior_sec = 0.0
    total_entry_count = len(entries)
    total_fn_processed = 0
    _stderr = sys.stderr
    for entry_index, entry in enumerate(entries):
        entry_start = time.perf_counter()
        entry_label = entry.binary_path.name
        all_source_functions = extract_source_functions(entry.source_path, entry.language)
        source_functions = filter_source_functions(all_source_functions, args.function_name)
        source_functions_by_name = {
            normalize_name(func.name): func
            for func in all_source_functions
        }
        list_start = time.perf_counter()
        fission_funcs, fission_error = run_fission_list_cached(
            entry.binary_path,
            fission_bin,
            args.timeout_sec,
            list_cache,
            list_cache_lock,
            list_cache_stats,
        )
        list_elapsed = time.perf_counter() - list_start
        phase_list_sec += list_elapsed
        source_functions, suppressed_for_entry = filter_inlined_static_source_functions(
            source_functions,
            all_source_functions,
            fission_funcs,
            explicit_function_filter=bool(args.function_name),
            fission_error=fission_error,
        )
        for suppressed in suppressed_for_entry:
            suppressed_static_inline_rows.append(
                {
                    "entry_id": entry.id,
                    "source_path": rel(entry.source_path),
                    "binary_path": rel(entry.binary_path),
                    **suppressed,
                }
            )
        source_functions = select_source_functions(
            source_functions,
            fission_funcs,
            args.limit_functions,
            fission_error,
        )
        source_row_selection_entries.append(
            {
                "entry_id": entry.id,
                "source_path": rel(entry.source_path),
                "binary_path": rel(entry.binary_path),
                "extracted_source_function_count": len(all_source_functions),
                "extracted_static_source_function_count": sum(1 for func in all_source_functions if func.is_static),
                "filtered_source_function_count": len(filter_source_functions(all_source_functions, args.function_name)),
                "suppressed_static_inline_helper_count": len(suppressed_for_entry),
                "selected_source_function_count": len(source_functions),
                "listed_binary_function_count": len(fission_funcs),
                "list_error": fission_error,
            }
        )
        ghidra_funcs: list[FissionFunction] = []
        ghidra_by_address: dict[str, dict[str, Any]] = {}
        ghidra_error: str | None = None
        if args.include_ghidra_reference and ghidra_home is not None:
            ghidra_export = run_ghidra_reference_export(
                entry.binary_path,
                ghidra_home,
                args.timeout_sec,
                output_dir,
                entry.id,
            )
            ghidra_reference_exports.append(
                {
                    "entry_id": entry.id,
                    "binary_path": rel(entry.binary_path),
                    "success": ghidra_export.get("success"),
                    "failure_kind": ghidra_export.get("failure_kind"),
                    "failure_detail": ghidra_export.get("failure_detail"),
                    "function_count": ghidra_export.get("function_count", 0),
                    "artifact_path": ghidra_export.get("artifact_path"),
                    "wall_sec": ghidra_export.get("wall_sec"),
                    "command": ghidra_export.get("command"),
                }
            )
            if ghidra_export.get("success"):
                ghidra_funcs, ghidra_by_address = ghidra_function_index(ghidra_export.get("functions") or [])
            else:
                ghidra_error = str(ghidra_export.get("failure_kind") or "ghidra_reference_failed")
            for func in source_functions:
                ghidra_rows.append(
                    row_for_ghidra_function(
                        entry,
                        func,
                        source_functions_by_name,
                        ghidra_funcs,
                        ghidra_by_address,
                        ghidra_error,
                        args.timeout_sec,
                        host_execution,
                        behavior_cache,
                        behavior_cache_lock,
                        behavior_cache_stats,
                        output_dir,
                        enable_sanitizer=args.enable_sanitizer,
                        fuzz_cases=args.fuzz_cases,
                    )
                )
        prewarm_decomp_cache_for_entry(
            entry,
            source_functions,
            fission_funcs,
            fission_error,
            fission_bin,
            args.timeout_sec,
            args.include_debug_decomp,
            output_dir,
            decomp_cache,
            decomp_cache_lock,
            decomp_cache_stats,
        )
        fn_count = len(source_functions)
        print(f"[{entry_index + 1}/{total_entry_count}] {entry_label}: {fn_count} functions  list={list_elapsed:.2f}s", file=_stderr)
        if jobs == 1 or fn_count <= 1:
            for fn_idx, func in enumerate(source_functions):
                fn_start = time.perf_counter()
                row = row_for_function(
                    entry,
                    func,
                    source_functions_by_name,
                    fission_funcs,
                    fission_error,
                    fission_bin,
                    args.timeout_sec,
                    host_execution,
                    decomp_cache,
                    decomp_cache_lock,
                    decomp_cache_stats,
                    behavior_cache,
                    behavior_cache_lock,
                    behavior_cache_stats,
                    args.include_debug_decomp,
                    output_dir,
                    enable_sanitizer=args.enable_sanitizer,
                    fuzz_cases=args.fuzz_cases,
                )
                rows.append(row)
                fn_elapsed = time.perf_counter() - fn_start
                d_sec = float(row.get("decomp_wall_sec") or 0.0)
                b_sec = float((row.get("behavior") or {}).get("wall_sec") or 0.0)
                phase_decomp_sec += d_sec
                phase_behavior_sec += b_sec
                total_fn_processed += 1
                sem_pct = row.get("semantic_score_percent", "?")
                beh_st = (row.get("behavior") or {}).get("status", "?")
                addr = row.get("address") or "unmapped"
                print(
                    f"  [{fn_idx + 1}/{fn_count}] {func.name} @ {addr}"
                    f"  decomp={d_sec:.2f}s  behavior={b_sec:.2f}s  total={fn_elapsed:.2f}s"
                    f"  semantic={sem_pct}  behavior_status={beh_st}",
                    file=_stderr,
                )
            entry_elapsed = time.perf_counter() - entry_start
            entry_pass = sum(1 for r in rows[-fn_count:] if (r.get("behavior") or {}).get("status") == "pass")
            print(
                f"[{entry_index + 1}/{total_entry_count}] ✓ {entry_label}"
                f"  {entry_pass}/{fn_count} pass  entry_time={entry_elapsed:.2f}s",
                file=_stderr,
            )
            continue

        entry_rows: list[tuple[int, dict[str, Any]]] = []
        with ThreadPoolExecutor(max_workers=jobs) as executor:
            futures = {
                executor.submit(
                    row_for_function,
                    entry,
                    func,
                    source_functions_by_name,
                    fission_funcs,
                    fission_error,
                    fission_bin,
                    args.timeout_sec,
                    host_execution,
                    decomp_cache,
                    decomp_cache_lock,
                    decomp_cache_stats,
                    behavior_cache,
                    behavior_cache_lock,
                    behavior_cache_stats,
                    args.include_debug_decomp,
                    output_dir,
                    enable_sanitizer=args.enable_sanitizer,
                    fuzz_cases=args.fuzz_cases,
                ): index
                for index, func in enumerate(source_functions)
            }
            for future in as_completed(futures):
                idx = futures[future]
                row = future.result()
                entry_rows.append((idx, row))
                d_sec = float(row.get("decomp_wall_sec") or 0.0)
                b_sec = float((row.get("behavior") or {}).get("wall_sec") or 0.0)
                phase_decomp_sec += d_sec
                phase_behavior_sec += b_sec
                total_fn_processed += 1
                fn_name = row.get("function_name", "?")
                addr = row.get("address") or "unmapped"
                sem_pct = row.get("semantic_score_percent", "?")
                beh_st = (row.get("behavior") or {}).get("status", "?")
                print(
                    f"  [{len(entry_rows)}/{fn_count}] {fn_name} @ {addr}"
                    f"  decomp={d_sec:.2f}s  behavior={b_sec:.2f}s"
                    f"  semantic={sem_pct}  behavior_status={beh_st}",
                    file=_stderr,
                )
        sorted_entry_rows = sorted(entry_rows, key=lambda item: item[0])
        rows.extend(row for _index, row in sorted_entry_rows)
        entry_elapsed = time.perf_counter() - entry_start
        entry_pass = sum(1 for _, r in sorted_entry_rows if (r.get("behavior") or {}).get("status") == "pass")
        print(
            f"[{entry_index + 1}/{total_entry_count}] ✓ {entry_label}"
            f"  {entry_pass}/{fn_count} pass  entry_time={entry_elapsed:.2f}s",
            file=_stderr,
        )

    attach_debug_repro_commands(rows, fission_bin, output_dir)
    summary = summarize(rows, manifest_name, entries)
    summary["run_id"] = run_id
    summary["created_at_utc"] = utc_isoformat(created_at)
    summary["artifact_dir"] = rel(output_dir)
    summary["jobs"] = jobs
    summary["decomp_cache_file"] = rel(decomp_cache_path) if decomp_cache_path is not None else None
    summary["list_cache_file"] = rel(list_cache_path) if list_cache_path is not None else None
    summary["behavior_cache_file"] = rel(behavior_cache_path) if behavior_cache_path is not None else None
    summary["history_file"] = rel(DEFAULT_HISTORY_FILE)
    summary["latest_index_file"] = rel(DEFAULT_LATEST_INDEX_FILE)
    summary["decomp_cache_initial_entry_count"] = decomp_cache_initial_entry_count
    summary["decomp_cache_entry_count"] = len(decomp_cache)
    summary["decomp_cache_hit_count"] = int(decomp_cache_stats.get("hit", 0))
    summary["decomp_cache_miss_count"] = int(decomp_cache_stats.get("miss", 0))
    summary["decomp_cache_stored_count"] = int(decomp_cache_stats.get("stored", 0))
    summary["list_cache_initial_entry_count"] = list_cache_initial_entry_count
    summary["list_cache_entry_count"] = len(list_cache)
    summary["list_cache_hit_count"] = int(list_cache_stats.get("hit", 0))
    summary["list_cache_miss_count"] = int(list_cache_stats.get("miss", 0))
    summary["list_cache_stored_count"] = int(list_cache_stats.get("stored", 0))
    summary["behavior_cache_initial_entry_count"] = behavior_cache_initial_entry_count
    summary["behavior_cache_entry_count"] = len(behavior_cache or {})
    summary["behavior_cache_hit_count"] = int(behavior_cache_stats.get("hit", 0))
    summary["behavior_cache_miss_count"] = int(behavior_cache_stats.get("miss", 0))
    summary["behavior_cache_stored_count"] = int(behavior_cache_stats.get("stored", 0))
    extracted_source_function_count = sum(
        int(entry.get("extracted_source_function_count") or 0)
        for entry in source_row_selection_entries
    )
    filtered_source_function_count = sum(
        int(entry.get("filtered_source_function_count") or 0)
        for entry in source_row_selection_entries
    )
    selected_source_function_count = sum(
        int(entry.get("selected_source_function_count") or 0)
        for entry in source_row_selection_entries
    )
    suppressed_static_inline_count = len(suppressed_static_inline_rows)
    summary["source_row_selection_metrics"] = {
        "extracted_source_function_count": extracted_source_function_count,
        "extracted_static_source_function_count": sum(
            int(entry.get("extracted_static_source_function_count") or 0)
            for entry in source_row_selection_entries
        ),
        "filtered_source_function_count": filtered_source_function_count,
        "selected_source_function_count": selected_source_function_count,
        "semantic_score_denominator_row_count": summary.get("row_count"),
        "explicit_function_filter_active": bool(args.function_name),
        "limit_functions": args.limit_functions,
        "suppressed_static_inline_helper_count": suppressed_static_inline_count,
        "suppressed_static_inline_helper_rate_filtered_denominator": round(
            suppressed_static_inline_count / filtered_source_function_count,
            6,
        ) if filtered_source_function_count else 0.0,
        "suppressed_static_inline_policy": (
            "static source helpers reachable from matched source functions but absent from binary symbols "
            "are excluded from benchmark rows unless explicitly selected; they remain available for "
            "same-source inline-expanded fingerprints"
        ),
        "entries": source_row_selection_entries,
        "suppressed_static_inline_helpers": suppressed_static_inline_rows,
    }
    decomp_cache_requests = summary["decomp_cache_hit_count"] + summary["decomp_cache_miss_count"]
    list_cache_requests = summary["list_cache_hit_count"] + summary["list_cache_miss_count"]
    behavior_cache_requests = summary["behavior_cache_hit_count"] + summary["behavior_cache_miss_count"]
    summary["cache_efficiency_metrics"] = {
        "decomp_cache_request_count": decomp_cache_requests,
        "decomp_cache_hit_rate": round(summary["decomp_cache_hit_count"] / decomp_cache_requests, 6)
        if decomp_cache_requests
        else 0.0,
        "decomp_cache_stored_count": summary["decomp_cache_stored_count"],
        "list_cache_request_count": list_cache_requests,
        "list_cache_hit_rate": round(summary["list_cache_hit_count"] / list_cache_requests, 6)
        if list_cache_requests
        else 0.0,
        "list_cache_stored_count": summary["list_cache_stored_count"],
        "behavior_cache_request_count": behavior_cache_requests,
        "behavior_cache_hit_rate": round(summary["behavior_cache_hit_count"] / behavior_cache_requests, 6)
        if behavior_cache_requests
        else 0.0,
        "behavior_cache_stored_count": summary["behavior_cache_stored_count"],
    }
    summary["wall_sec"] = round(time.perf_counter() - start, 6)
    summary["phase_timings"] = {
        "list_sec": round(phase_list_sec, 6),
        "decomp_sec": round(phase_decomp_sec, 6),
        "behavior_sec": round(phase_behavior_sec, 6),
    }
    if args.require_sleigh_template_source:
        summary["sleigh_template_source_gate"] = sleigh_template_source_gate(
            summary,
            args.require_sleigh_template_source,
        )
    ghidra_summary: dict[str, Any] | None = None
    ghidra_comparison: dict[str, Any] | None = None
    if args.include_ghidra_reference:
        ghidra_summary = summarize(ghidra_rows, manifest_name, entries)
        ghidra_summary["candidate_decompiler"] = "ghidra"
        ghidra_summary["artifact_dir"] = rel(output_dir)
        ghidra_summary["ghidra_home"] = rel(ghidra_home) if ghidra_home is not None else None
        ghidra_summary["reference_exports"] = ghidra_reference_exports
        ghidra_summary["reference_export_success_count"] = sum(
            1 for export in ghidra_reference_exports if export.get("success")
        )
        ghidra_summary["reference_export_failure_count"] = sum(
            1 for export in ghidra_reference_exports if not export.get("success")
        )
        ghidra_comparison = compare_ghidra_reference(summary, rows, ghidra_summary, ghidra_rows)
        summary["ghidra_reference"] = {
            "contract": "reference lane only; Ghidra is not the oracle",
            "summary_path": rel(output_dir / "ghidra_source_semantic_summary.json"),
            "rows_path": rel(output_dir / "ghidra_source_semantic_rows.json"),
            "comparison_path": rel(output_dir / "ghidra_source_semantic_comparison.json"),
            "ghidra_home": rel(ghidra_home) if ghidra_home is not None else None,
            "reference_export_success_count": ghidra_summary["reference_export_success_count"],
            "reference_export_failure_count": ghidra_summary["reference_export_failure_count"],
            "reference_exports": ghidra_reference_exports,
            "weighted_semantic_similarity_percent": ghidra_summary.get("weighted_semantic_similarity_percent"),
            "behavior_status_counts": ghidra_summary.get("behavior_status_counts"),
            "comparison": ghidra_comparison,
        }
    history = history_snapshot(DEFAULT_HISTORY_FILE, summary)
    if history is not None:
        summary["history"] = history
    save_decomp_cache(decomp_cache_path, decomp_cache)
    save_list_cache(list_cache_path, list_cache)
    save_behavior_cache(behavior_cache_path, behavior_cache or {})
    baseline_path: Path | None = None
    if not args.no_baseline_compare:
        baseline_path = resolve_path(args.baseline_dir) if args.baseline_dir else find_latest_baseline_dir(
            output_dir,
            summary["manifest"],
            {row_key(row) for row in rows},
        )
    if baseline_path is not None:
        try:
            baseline_summary, baseline_rows, baseline_summary_path = load_baseline_artifacts(baseline_path)
            summary["comparison"] = compare_to_baseline(
                summary,
                rows,
                baseline_summary,
                baseline_rows,
                baseline_summary_path,
            )
            summary["comparison_outcome"] = comparison_outcome(summary["comparison"])
            summary["improvement_summary"] = improvement_summary(summary["comparison"])
            if not args.no_baseline_snapshot:
                summary["baseline_snapshot"] = snapshot_baseline_artifacts(
                    output_dir,
                    baseline_summary_path,
                    baseline_summary,
                    baseline_rows,
                    summary["comparison"],
                )
        except Exception as exc:
            summary["comparison_error"] = {
                "baseline": str(baseline_path),
                "error": str(exc),
            }
    if args.materialize_debug_triage:
        triage_rows = materialize_debug_triage(
            rows,
            fission_bin,
            output_dir,
            args.timeout_sec,
            args.debug_triage_limit,
        )
        summary["debug_triage"] = triage_rows
        summary["debug_triage_count"] = len(triage_rows)
    if args.materialize_regression_debug_triage and isinstance(summary.get("comparison"), dict):
        regression_triage_rows = materialize_regression_debug_triage(
            rows,
            summary["comparison"],
            fission_bin,
            output_dir,
            args.timeout_sec,
            args.regression_debug_triage_limit,
        )
        summary["regression_debug_triage"] = regression_triage_rows
        summary["regression_debug_triage_count"] = len(regression_triage_rows)
    debug_commands = top_debug_commands(rows)
    if debug_commands:
        summary["debug_repro_commands"] = debug_commands
    (output_dir / "source_semantic_rows.json").write_text(
        dump_json_pretty(rows), encoding="utf-8"
    )
    (output_dir / "source_semantic_summary.json").write_text(
        dump_json_pretty(summary), encoding="utf-8"
    )
    if "comparison" in summary:
        (output_dir / "source_semantic_comparison.json").write_text(
            dump_json_pretty(summary["comparison"]), encoding="utf-8"
        )
    if ghidra_summary is not None:
        (output_dir / "ghidra_source_semantic_rows.json").write_text(
            dump_json_pretty(ghidra_rows), encoding="utf-8"
        )
        (output_dir / "ghidra_source_semantic_summary.json").write_text(
            dump_json_pretty(ghidra_summary), encoding="utf-8"
        )
    if ghidra_comparison is not None:
        (output_dir / "ghidra_source_semantic_comparison.json").write_text(
            dump_json_pretty(ghidra_comparison), encoding="utf-8"
        )
    (output_dir / "source_semantic_summary.md").write_text(render_markdown(summary, rows), encoding="utf-8")
    gate = summary.get("sleigh_template_source_gate")
    gate_failed = isinstance(gate, dict) and gate.get("status") == "failed"
    if not gate_failed:
        append_history_record(DEFAULT_HISTORY_FILE, summary)
        update_latest_index(DEFAULT_LATEST_INDEX_FILE, summary)
    # ── Human-friendly console summary on stderr ──
    wall = summary.get("wall_sec", 0.0)
    pt = summary.get("phase_timings", {})
    row_count = summary.get("row_count", 0)
    mapped_count = summary.get("effective_coverage", {}).get("mapped_rows", 0)
    decomp_ok_count = summary.get("effective_coverage", {}).get("decompiled_rows", 0)
    compile_ok_count = summary.get("admission_gate_metrics", {}).get("counts", {}).get("candidate_compiled_rows", 0)
    behavior_pass_count = summary.get("admission_gate_metrics", {}).get("counts", {}).get("behavior_pass_rows", 0)
    avg_semantic = summary.get("weighted_semantic_similarity_percent", 0.0)
    avg_static = (summary.get("score_component_metrics", {}).get("static_score_sum", 0.0) / max(row_count, 1)) * 100.0
    strict_success = summary.get("behavior_pass_rate", 0.0) * 100.0
    d_hit = int(decomp_cache_stats.get("hit", 0))
    l_hit = int(list_cache_stats.get("hit", 0))
    b_hit = int(behavior_cache_stats.get("hit", 0))
    decomp_avg = pt.get("decomp_sec", 0.0) / max(row_count, 1)
    behav_avg = pt.get("behavior_sec", 0.0) / max(row_count, 1)
    sep = "═" * 58
    thin = "─" * 58
    print(f"\n{sep}", file=_stderr)
    print(f"  Source Semantic Benchmark — {manifest_name}", file=_stderr)
    print(f"{sep}", file=_stderr)
    print(f"  Rows       : {row_count}", file=_stderr)
    print(f"  Mapped     : {mapped_count} / {row_count}  ({mapped_count / max(row_count, 1) * 100:.1f}%)", file=_stderr)
    print(f"  Decomp OK  : {decomp_ok_count} / {row_count}  ({decomp_ok_count / max(row_count, 1) * 100:.1f}%)", file=_stderr)
    print(f"  Compile OK : {compile_ok_count} / {row_count}  ({compile_ok_count / max(row_count, 1) * 100:.1f}%)", file=_stderr)
    print(f"  Behavior   : {behavior_pass_count} / {row_count}  ({strict_success:.1f}%)", file=_stderr)
    print(f"  {thin}", file=_stderr)
    print(f"  Semantic   : {avg_semantic:.2f}%  (weighted avg)", file=_stderr)
    print(f"  Strict OK  : {strict_success:.2f}%", file=_stderr)
    print(f"  {thin}", file=_stderr)
    print(f"  Timings:", file=_stderr)
    print(f"    Total      : {wall:.2f}s", file=_stderr)
    print(f"    List       : {pt.get('list_sec', 0.0):.2f}s", file=_stderr)
    print(f"    Decomp     : {pt.get('decomp_sec', 0.0):.2f}s  (avg {decomp_avg:.2f}s/fn)", file=_stderr)
    print(f"    Behavior   : {pt.get('behavior_sec', 0.0):.2f}s  (avg {behav_avg:.2f}s/fn)", file=_stderr)
    print(f"  Cache hits : decomp={d_hit}  list={l_hit}  behavior={b_hit}", file=_stderr)
    print(f"{sep}\n", file=_stderr)

    if args.format == "markdown":
        print(render_markdown(summary, rows))
    else:
        print(dump_json_pretty(summary), end="")
    return 1 if gate_failed else 0


def run_self_test() -> int:
    import tempfile
    sample = """
static int helper(int x) { return x + 1; }
int add(int a, int b) { return a + b; }
int max(int a, int b) { if (a > b) return a; return b; }
"""
    with tempfile.TemporaryDirectory(prefix="source-semantic-selftest-") as tmp:
        path = Path(tmp) / "sample.c"
        path.write_text(sample, encoding="utf-8")
        funcs = extract_source_functions(path, "c")
        assert [f.name for f in funcs] == ["helper", "add", "max"]
        assert funcs[0].is_static
        assert funcs[1].return_kind == "int"
        assert funcs[1].param_kinds == ["int", "int"]
        funcs_by_name = {normalize_name(f.name): f for f in funcs}
        caller = SourceFunction(
            name="caller",
            signature="int caller(int a, int b)",
            body="return add(a, b);",
            return_kind="int",
            param_kinds=["int", "int"],
            param_names=["a", "b"],
            line=1,
        )
        expanded_fp = inline_expanded_source_fingerprint(caller, funcs_by_name)
        assert expanded_fp["call:add"] == 0
        assert expanded_fp["op:+"] >= 1
        assert multiset_jaccard(code_fingerprint(funcs[1].body, funcs[1]), code_fingerprint(funcs[1].body, funcs[1])) == 1.0
        missing_feature_score = multiset_jaccard(
            code_fingerprint("if (a > b) return helper(a + b);", funcs[1]),
            code_fingerprint("return a;", funcs[1]),
        )
        assert 0.0 < missing_feature_score < 1.0, missing_feature_score
        gap_details = multiset_gap_details(
            code_fingerprint("if (a > b) return helper(a + b);", funcs[1]),
            code_fingerprint("return a;", funcs[1]),
        )
        assert gap_details["missing_feature_total"] > 0
        assert gap_details["union_feature_total"] >= gap_details["intersection_feature_total"]
        rendered_sig_fp = code_fingerprint("uint test_switch(uint param_1) { return param_1; }")
        assert rendered_sig_fp["sig:return:int"] == 1
        assert rendered_sig_fp["sig:param_count:1"] == 1
        assert rendered_sig_fp["sig:param:uint"] == 1
        assert rendered_sig_fp["call:testswitch"] == 0
        rendered_call_fp = code_fingerprint(
            "uint caller(uint param_1) { return helper(param_1); }"
        )
        assert rendered_call_fp["call:caller"] == 0
        assert rendered_call_fp["call:helper"] == 1
        function_pointer_source = SourceFunction(
            name="apply_op",
            signature="u32 apply_op(op_func f, u32 a, u32 b)",
            body="return f(a, b);",
            return_kind="int",
            param_kinds=["aggregate_or_pointer", "uint", "uint"],
            param_names=["f", "a", "b"],
            line=1,
        )
        function_pointer_source_fp = code_fingerprint(
            function_pointer_source.body,
            function_pointer_source,
        )
        function_pointer_decomp_fp = code_fingerprint(
            "uint apply_op(void * param_1, uint param_2, uint param_3) { "
            "return ((uint (*)(uint, uint))param_1)(param_2, param_3); }"
        )
        assert function_pointer_source_fp["call:indirect_param"] == 1
        assert function_pointer_source_fp["call:f"] == 0
        assert function_pointer_decomp_fp["call:indirect_param"] == 1
        status, matched, _ = match_function(funcs[1], [FissionFunction("0x1000", "add [export]")])
        assert status == "matched"
        assert matched is not None
        status, matched, candidates = match_function(
            SourceFunction(
                name="main",
                signature="int main()",
                body="return 0;",
                return_kind="int",
                param_kinds=[],
                param_names=[],
                line=1,
            ),
            [
                FissionFunction("0x1000", "__main"),
                FissionFunction("0x2000", "main"),
            ],
        )
        assert status == "matched"
        assert matched is not None
        assert matched.address == "0x2000"
        assert candidates == []
        main_func = SourceFunction(
            name="main",
            signature="int main()",
            body="return 0;",
            return_kind="int",
            param_kinds=[],
            param_names=[],
            line=1,
        )
        main_entry = BenchmarkEntry(
            id="main-smoke",
            binary_path=Path("/tmp/main.exe"),
            source_path=path,
            language="c",
            tags=[],
        )
        supported, reason = behavior_supported(main_entry, main_func, None)
        assert supported, reason
        main_source_harness = source_harness(path, main_func, [()])
        assert "source_original_main()" in main_source_harness
        assert "main()" not in main_source_harness.split("#undef main", 1)[1].split("source_original_main()", 1)[0]
        main_candidate_harness = candidate_harness("uint main(void) { return 0; }", main_func, [()], path)
        assert "source_original_main" in main_candidate_harness
        assert "#define main fission_candidate_main" in main_candidate_harness
        assert "fission_candidate_main()" in main_candidate_harness
        limited = select_source_functions(
            [
                SourceFunction(
                    name="helper",
                    signature="static int helper(int x)",
                    body="return x + 1;",
                    return_kind="int",
                    param_kinds=["int"],
                    param_names=["x"],
                    line=1,
                    is_static=True,
                ),
                SourceFunction(
                    name="entry",
                    signature="int entry(int x)",
                    body="return helper(x);",
                    return_kind="int",
                    param_kinds=["int"],
                    param_names=["x"],
                    line=2,
                ),
            ],
            [FissionFunction("0x3000", "entry")],
            1,
        )
        assert [func.name for func in limited] == ["entry"]
        static_funcs = [
            SourceFunction(
                name="helper",
                signature="static int helper(int x)",
                body="return x + 1;",
                return_kind="int",
                param_kinds=["int"],
                param_names=["x"],
                line=1,
                is_static=True,
            ),
            SourceFunction(
                name="entry",
                signature="int entry(int x)",
                body="return helper(x);",
                return_kind="int",
                param_kinds=["int"],
                param_names=["x"],
                line=2,
            ),
        ]
        filtered_static, suppressed_static = filter_inlined_static_source_functions(
            static_funcs,
            static_funcs,
            [FissionFunction("0x3000", "entry")],
            explicit_function_filter=False,
        )
        assert [func.name for func in filtered_static] == ["entry"]
        assert [row["function_name"] for row in suppressed_static] == ["helper"]
        explicit_static, explicit_suppressed = filter_inlined_static_source_functions(
            [static_funcs[0]],
            static_funcs,
            [FissionFunction("0x3000", "entry")],
            explicit_function_filter=True,
        )
        assert [func.name for func in explicit_static] == ["helper"]
        assert explicit_suppressed == []
        entries = [
            BenchmarkEntry(
                id="x86-smoke",
                binary_path=Path("/tmp/x86.exe"),
                source_path=Path("/tmp/x86.c"),
                language="c",
                tags=["smoke", "x86-64"],
            ),
            BenchmarkEntry(
                id="aarch64-control",
                binary_path=Path("/tmp/aarch64.o"),
                source_path=Path("/tmp/aarch64.c"),
                language="c",
                tags=["smoke", "aarch64", "control-flow"],
            ),
        ]
        assert [entry.id for entry in filter_entries(entries, ["aarch64-control"], None)] == [
            "aarch64-control"
        ]
        assert [entry.id for entry in filter_entries(entries, None, ["smoke", "aarch64"])] == [
            "aarch64-control"
        ]
        assert [
            func.name for func in filter_source_functions(funcs, ["max"])
        ] == ["max"]
        assert classify_return("u64 wide(unsigned int seed)", "wide", "unsigned int seed", "c") == "int"
        assert classify_return("uint64_t wide(unsigned int seed)", "wide", "unsigned int seed", "c") == "int"
        assert classify_return("longlong wide(longlong seed)", "wide", "longlong seed", "c") == "int"
        assert classify_param("ulonglong count", "c") == "uint"
        assert classify_param("ushort flags", "c") == "uint"
        assert classify_param("uchar byte", "c") == "uint"
        summary = summarize(
            [
                {
                    "language": "c",
                    "tags": ["clang", "O2"],
                    "entry_id": "selftest",
                    "mapping_status": "matched",
                    "decomp_success": True,
                    "behavior": {"status": "pass", "case_count": 2, "case_pass_count": 2, "case_fail_count": 0},
                    "semantic_score": 1.0,
                    "static_semantic_score": 1.0,
                    "static_semantic_score_direct": 1.0,
                    "static_semantic_score_inline_expanded": 1.0,
                    "static_similarity_source_variant": "direct_source",
                    "source_body_line_count": 1,
                    "source_body_byte_count": 12,
                    "source_return_kind": "int",
                    "source_param_kinds": ["int", "int"],
                    "decomp_return_kind": "int",
                    "decomp_param_kinds": ["uint", "uint"],
                    "source_static_feature_count_direct": 2,
                    "source_static_feature_count_inline_expanded": 2,
                    "decomp_line_count": 2,
                    "decomp_byte_count": 24,
                    "readability_score": 0.8,
                    "semantic_correctness_score": 1.0,
                    "ai_remedy_hints": {
                        "has_sanitizer_error": False,
                        "status": "pass",
                        "excessive_nesting": False,
                        "cfg_similarity": 1.0,
                        "cfg_mismatches": [],
                        "suggested_actions": [],
                    },
                    "static_similarity_gaps": {
                        "source_feature_total": 2,
                        "decomp_feature_total": 2,
                        "intersection_feature_total": 2,
                        "union_feature_total": 2,
                        "missing_feature_total": 0,
                        "extra_feature_total": 0,
                    },
                    "static_similarity_gap_components": {
                        "signature": {
                            "source_feature_total": 2,
                            "decomp_feature_total": 2,
                            "intersection_feature_total": 0,
                            "union_feature_total": 4,
                            "missing_feature_total": 2,
                            "extra_feature_total": 2,
                            "top_missing_features": [{"feature": "sig:param:int", "count": 2}],
                            "top_extra_features": [{"feature": "sig:param:uint", "count": 2}],
                        }
                    },
                    "preview_build_stats": {
                        "validated_pcode_op_count": 10,
                        "replacement_plan_rejected_missing_merge_count": 2,
                    },
                    "debug_decomp": {
                        "stage_status": {
                            "decode": "ok",
                            "raw_pcode": "ok",
                            "nir_build": "ok",
                            "normalize": "ok",
                            "structuring": "ok",
                            "render": "ok",
                        },
                        "quality_evidence": {
                            "region_emit_ready_failed_count": 1,
                        },
                    },
                },
                {
                    "language": "c",
                    "tags": ["mingw-gcc", "O0"],
                    "entry_id": "selftest",
                    "mapping_status": "unmapped",
                    "decomp_success": False,
                    "behavior": {"status": "decomp_failed", "score": 0.0, "case_count": 1},
                    "semantic_score": 0.0,
                    "static_semantic_score": 0.0,
                    "static_semantic_score_direct": 0.0,
                    "static_semantic_score_inline_expanded": 0.0,
                    "static_similarity_source_variant": "direct_source",
                    "source_body_line_count": 1,
                    "source_body_byte_count": 12,
                    "source_return_kind": "int",
                    "source_param_kinds": ["int"],
                    "decomp_return_kind": None,
                    "decomp_param_kinds": None,
                    "source_static_feature_count_direct": 2,
                    "source_static_feature_count_inline_expanded": 2,
                    "decomp_line_count": 0,
                    "decomp_byte_count": 0,
                    "readability_score": 0.0,
                    "semantic_correctness_score": 0.0,
                    "ai_remedy_hints": {
                        "has_sanitizer_error": True,
                        "status": "run_tle",
                        "excessive_nesting": True,
                        "cfg_similarity": 0.2,
                        "cfg_mismatches": ["loop_depth_mismatch"],
                        "suggested_actions": ["Simplify control flow structure"],
                    },
                    "static_similarity_gaps": {
                        "source_feature_total": 2,
                        "decomp_feature_total": 0,
                        "intersection_feature_total": 0,
                        "union_feature_total": 2,
                        "missing_feature_total": 2,
                        "extra_feature_total": 0,
                        "top_missing_features": [{"feature": "op:+", "count": 1}],
                    },
                    "static_similarity_gap_components": {},
                },
            ],
            "selftest",
            [],
        )
        assert summary["row_count"] == 2
        assert summary["function_mapping_rate"] == 0.5
        assert summary["decomp_success_rate"] == 0.5
        assert summary["weighted_semantic_similarity"] == 0.5
        assert summary["effective_coverage"]["mapped_rows"] == 1
        assert summary["zero_credit_breakdown"]["unmapped"] == 1
        assert "control_flow" in summary["static_similarity_component_averages"]
        assert summary["static_similarity_gap_totals"]["missing_feature_total"] == 2
        assert summary["static_similarity_gap_totals"]["top_missing_features"][0]["feature"] == "op:+"
        assert summary["behavior_case_metrics"]["case_pass_count"] == 2
        assert summary["score_distribution"]["perfect"] == 1
        assert summary["score_distribution"]["zero"] == 1
        assert summary["readability_score_stats"]["avg"] == 0.4
        assert summary["semantic_correctness_score_stats"]["avg"] == 0.5
        assert summary["compiler_option_matrix"]["Clang"]["O2"]["avg_readability_score"] == 0.8
        assert summary["compiler_option_matrix"]["GCC"]["O0"]["avg_semantic_correctness_score"] == 0.0
        assert summary["ai_remedy_summary"]["total_sanitizer_errors"] == 1
        assert summary["ai_remedy_summary"]["total_tle_errors"] == 1
        assert summary["ai_remedy_summary"]["excessive_nesting_count"] == 1
        assert summary["ai_remedy_summary"]["avg_cfg_similarity"] == 0.6
        assert summary["semantic_score_stats"]["nonzero_count"] == 1
        assert summary["scoring_contract"]["semantic_score_denominator"] == "all manifest rows"
        assert summary["score_component_metrics"]["behavior_component_score_sum"] == 0.65
        assert summary["score_component_metrics"]["static_component_score_sum"] == 0.35
        assert summary["score_weight_sensitivity_metrics"]["scenario_scores"]["behavior_100_static_000"]["weighted_score_percent"] == 50.0
        assert summary["score_weight_sensitivity_metrics"]["scenario_scores"]["behavior_000_static_100"]["weighted_score_percent"] == 50.0
        assert summary["score_weight_sensitivity_metrics"]["behavior_static_equal_row_count"] == 2
        assert summary["component_loss_hot_row_metrics"]["top_total_component_loss_rows"][0]["total_component_loss"] == 1.0
        assert summary["score_denominator_metrics"]["score_denominator_row_count"] == 2
        assert summary["score_denominator_metrics"]["lost_score_sum"] == 1.0
        assert summary["semantic_loss_metrics"]["lost_score_by_zero_credit_reason"]["unmapped"] == 1.0
        assert summary["semantic_readiness_metrics"]["fully_perfect_rows"] == 1
        assert summary["semantic_readiness_metrics"]["behavior_pass_static_perfect_rows"] == 1
        assert summary["semantic_readiness_metrics"]["pipeline_ok_behavior_nonpass_rows"] == 0
        assert summary["benchmark_integrity_metrics"]["rows_excluded_from_semantic_score_denominator"] == 0
        assert summary["benchmark_integrity_metrics"]["missing_source_features_penalized"] is True
        assert summary["behavior_mismatch_metrics"]["mismatch_row_count"] == 0
        assert summary["behavior_case_metrics"]["compared_case_count"] == 3
        assert summary["behavior_denominator_metrics"]["case_denominator_count"] == 3
        assert summary["behavior_timeout_progress_metrics"]["partial_timeout_row_count"] == 0
        assert summary["behavior_case_metrics"]["partial_progress_row_count"] == 0
        assert summary["denominator_accounting_metrics"]["unmapped_row_count"] == 1
        assert summary["denominator_accounting_metrics"]["semantic_score_denominator_row_count"] == 2
        assert summary["static_gap_row_metrics"]["missing_feature_row_count"] == 1
        assert summary["static_absence_penalty_metrics"]["missing_feature_total"] == 2.0
        assert summary["static_absence_penalty_metrics"]["rows_with_no_decomp_features_despite_source"] == 1
        assert summary["static_component_absence_matrix_metrics"]["signature"]["both_present_row_count"] == 1
        assert (
            summary["static_component_absence_matrix_metrics"]["signature"][
                "zero_intersection_source_present_row_count"
            ]
            == 1
        )
        assert summary["source_decomp_size_metrics"]["decomp_to_source_line_ratio_distribution"]["max"] == 2.0
        assert summary["static_source_variant_metrics"]["variant_counts"]["direct_source"] == 2
        assert summary["score_by_behavior_status"]["pass"]["count"] == 1
        assert summary["behavior_status_by_zero_credit_reason"]["unmapped"]["decomp_failed"] == 1
        assert "top_decompile_wall_rows" in summary["cost_hot_rows"]
        assert "debug_coverage_metrics" in summary
        assert summary["triage_priority_rows"][0]["function_name"] is None
        assert "decompile_avg_sec" in summary["harness_cost_metrics"]
        assert "decompile_p95_sec" in summary["harness_cost_metrics"]
        assert summary["pipeline_stage_metrics"]["decode"]["ok_count"] == 1
        assert summary["nir_build_stats_metrics"]["stats_row_count"] == 1
        assert summary["nir_build_stats_metrics"]["debt_metric_totals"]["replacement_plan_rejected_missing_merge_count"] == 2.0
        assert summary["nir_debt_correlation_metrics"]["debt_row_count"] == 1
        assert summary["debug_quality_evidence_totals"]["region_emit_ready_failed_count"] == 1.0
        assert summary["behavior_distance_metrics"]["case_pass_rate_distribution"]["count"] == 0
        assert summary["improvement_axis_metrics"]["nir_telemetry_debt"]["row_count"] == 1
        assert summary["improvement_axis_metrics"]["mapping"]["lost_score_sum"] == 1.0
        assert summary["admission_gate_metrics"]["counts"]["manifest_rows"] == 2
        assert summary["admission_gate_metrics"]["counts"]["raw_pcode_ok_rows"] == 1
        assert summary["quality_gate_funnel_metrics"]["drop_rows_from_previous_gate"]["manifest_rows->mapped_rows"] == 1
        assert summary["stage_transition_metrics"]["furthest_ok_stage_counts"]["render"] == 1
        assert summary["sleigh_lift_health_metrics"]["decode_ok_rows"] == 1
        assert summary["sleigh_lift_health_metrics"]["raw_pcode_ok_rows"] == 1
        assert summary["sleigh_lift_health_metrics"]["raw_pcode_compat_import_total"] == 0.0
        assert summary["behavior_failure_diagnostics"]["owner_counts"]["decomp_failed"] == 1
        assert summary["semantic_quality_quadrant_metrics"]["dynamic_pass|static_perfect"]["row_count"] == 1
        assert (
            summary["semantic_quality_quadrant_metrics"][
                "dynamic_harness_or_decomp_blocked|static_no_decomp_features"
            ]["row_count"]
            == 1
        )
        assert summary["outcome_matrix_metrics"]["outcome_count"] == 2
        assert summary["coverage_blind_spot_metrics"]["counts"]["unmapped_source_function"] == 1
        assert summary["coverage_blind_spot_metrics"]["counts"]["source_features_without_decomp_features"] == 1
        assert summary["static_gap_density_metrics"]["gap_bucket_rows"]["missing:small|extra:none"]["row_count"] == 1
        assert summary["static_gap_hot_row_metrics"]["top_missing_feature_rows"][0]["missing_feature_total"] == 2.0
        assert summary["static_component_precision_recall_metrics"]["signature"]["precision"] == 0.0
        assert summary["behavior_partial_progress_metrics"]["row_count"] == 0
        assert summary["focus_area_metrics"]["nir_builder_dataflow"]["row_count"] == 1
        assert summary["focus_area_metrics"]["mapping_name_recovery"]["lost_score_sum"] == 1.0
        assert summary["roadmap_priority_metrics"]["priority_order"][0] == "p1_sleigh_lift_correctness"
        assert summary["roadmap_priority_metrics"]["buckets"]["p3_structuring_hard_cases"]["row_count"] == 1
        assert summary["roadmap_priority_metrics"]["buckets"]["p4_fid_name_recovery"]["lost_score_sum"] == 1.0
        assert "signature_gap_rows" in summary["type_data_gap_metrics"]
        assert summary["signedness_only_signature_gap_metrics"]["row_count"] == 1
        assert summary["signedness_only_signature_gap_metrics"]["param_pair_count"] == 2.0
        assert summary["signature_kind_confusion_metrics"]["return_pair_counts"]["int->missing"] == 1
        assert summary["signature_kind_confusion_metrics"]["param_pair_counts"]["int->missing"] == 1
        assert summary["signature_kind_confusion_metrics"]["param_pair_counts"]["int->uint"] == 2
        assert summary["signature_kind_confusion_metrics"]["param_arity_mismatch_row_count"] == 1
        aliases = c_like_pointer_typedef_aliases(
            "typedef unsigned int (*op_func)(unsigned int, unsigned int);\n"
            "typedef unsigned char *byte_ptr;\n"
        )
        assert aliases == {"op_func", "byte_ptr"}
        parsed = extract_c_like_functions(
            "typedef unsigned int (*op_func)(unsigned int, unsigned int);\n"
            "unsigned int apply_op(op_func f, unsigned int a) { return f(a, a); }\n",
            "c",
        )
        assert parsed[0].param_kinds == ["aggregate_or_pointer", "uint"]
        assert "uint" not in call_names_for_fingerprint(
            "return ((uint (*)(uint, uint))param_1)(param_2, param_3);"
        )
        fp_cases = [
            {
                "args": ["op_add", 2],
                "candidate_support_code": "uint op_add(uint a, uint b) { return a + b; }",
            }
        ]
        valid, reason = validate_explicit_behavior_cases(parsed[0], fp_cases)
        assert valid, reason
        rendered = render_explicit_case_call(parsed[0], fp_cases[0], 0)
        assert "apply_op(op_add, 2)" in rendered
        assert candidate_support_code_blocks(fp_cases) == [
            "uint op_add(uint a, uint b) { return a + b; }"
        ]
        progress = partial_behavior_progress(
            {"stdout": "ret=0\nret=1\nret=1\nret=5\n"},
            {"partial_stdout": "ret=0\nret=1\nret=1\n"},
            [(0,), (1,), (2,), (5,)],
        )
        assert progress["case_pass_count"] == 3
        assert progress["case_fail_count"] == 1
        assert progress["compared_case_count"] == 4
        assert progress["first_mismatch_index"] == 3
        assert progress["candidate_missing_line_count"] == 1
        run_failed_progress = partial_behavior_progress(
            {"stdout": "0\n1\n1\n5\n55\n-1\n"},
            {"status": "run_failed", "partial_stdout": "0\n1\n1\n5\n"},
            [(0,), (1,), (2,), (5,), (10,), (-1,)],
        )
        assert run_failed_progress["case_pass_count"] == 4
        assert run_failed_progress["case_fail_count"] == 2
        assert run_failed_progress["first_mismatch_index"] == 4
        assert not behavior_cache_entry_is_valid({"status": "run_failed", "detail": "0\n1\n"})
        assert behavior_cache_entry_is_valid({"status": "run_failed", "partial_stdout": ""})
        assert candidate_timeout_sec(20, {"run_sec": 0.01}) == CANDIDATE_TIMEOUT_MIN_SEC
        assert candidate_timeout_sec(20, {"run_sec": 0.25}) == 3
        assert candidate_timeout_sec(20, {"run_sec": 0.32}) == 4
        cache_key = behavior_cache_key("int main(void){return 0;}", "/bin/clang", 7)
        assert cache_key.startswith("source-semantic-behavior-v2|")
        assert "timeout_sec=7" in cache_key
        assert "control_flow_gap_rows" in summary["structuring_gap_metrics"]
        assert summary["fid_name_recovery_metrics"]["name_or_mapping_gap_row_count"] == 1
        assert "unknown" in summary["architecture_support_metrics"]
        assert summary["complexity_quality_metrics"]["by_source_feature_bucket"]["tiny"]["row_count"] == 2
        assert "decompile_wall_by_stage_first_failure" in summary["stage_cost_correlation_metrics"]
        assert "score_by_decompile_cost_bucket" in summary["stage_cost_correlation_metrics"]
        void_func = SourceFunction(
            name="touch",
            signature="void touch(unsigned int seed)",
            body="control_sink = seed;",
            return_kind="void",
            param_kinds=["uint"],
            param_names=["seed"],
            line=1,
        )
        global_cases = [
            {
                "args": [7],
                "globals": [{"name": "control_sink", "ctype": "unsigned int", "reset": 0}],
            }
        ]
        valid, reason = validate_explicit_behavior_cases(void_func, global_cases)
        assert valid, reason
        rendered = render_explicit_case_call(void_func, global_cases[0], 0)
        assert "control_sink = 0;" in rendered
        assert "control_sink=%lld" in rendered
        assert "touch(7);" in rendered
        deduped_candidate = candidate_harness(
            "uint control_sink;\nuint math_sink;\nvoid touch(unsigned int seed) { control_sink = seed; }",
            void_func,
            global_cases,
        )
        assert "volatile unsigned int control_sink = 0;" in deduped_candidate
        assert "\nuint control_sink;\n" not in deduped_candidate
        assert "\nuint math_sink;\n" in deduped_candidate
        gate = sleigh_template_source_gate(
            {
                "row_count": 1,
                "debug_stage_status_counts": {"decode:ok": 1, "raw_pcode:ok": 1},
                "debug_quality_evidence_totals": {"invalid_pcode_shape_count": 0},
                "debug_template_source_totals": {"sla_construct_tpl": 2},
            },
            "sla_construct_tpl",
        )
        assert gate["status"] == "passed"
        assert gate["raw_pcode_compat_import_count"] == 0
        gate = sleigh_template_source_gate(
            {
                "row_count": 1,
                "debug_stage_status_counts": {"decode:ok": 1, "raw_pcode:ok": 1},
                "debug_quality_evidence_totals": {"invalid_pcode_shape_count": 0},
                "debug_template_source_totals": {"spec_derived": 2},
            },
            "sla_construct_tpl",
        )
        assert gate["status"] == "passed"
        assert gate["template_source_totals"] == {"sla_construct_tpl": 2}
        gate = sleigh_template_source_gate(
            {
                "row_count": 1,
                "debug_stage_status_counts": {"decode:ok": 1, "raw_pcode:ok": 1},
                "debug_quality_evidence_totals": {"invalid_pcode_shape_count": 0},
                "debug_template_source_totals": {"compatibility_lowered": 1},
            },
            "sla_construct_tpl",
        )
        assert gate["status"] == "failed"
        assert "compatibility_lowered" in gate["failures"][0]
        gate = sleigh_template_source_gate(
            {
                "row_count": 1,
                "debug_stage_status_counts": {"decode:ok": 1, "raw_pcode:failed": 1},
                "debug_quality_evidence_totals": {"invalid_pcode_shape_count": 0},
                "debug_template_source_totals": {"sla_construct_tpl": 1},
            },
            "sla_construct_tpl",
        )
        assert gate["status"] == "failed"
        assert any("raw_pcode:failed" in failure for failure in gate["failures"])
        gate = sleigh_template_source_gate(
            {
                "row_count": 2,
                "debug_stage_status_counts": {"decode:ok": 1, "raw_pcode:ok": 1},
                "debug_quality_evidence_totals": {"invalid_pcode_shape_count": 0},
                "debug_template_source_totals": {"sla_construct_tpl": 1},
            },
            "sla_construct_tpl",
        )
        assert gate["status"] == "failed"
        assert any(
            "decode must be ok for every mapped row (1/2)" in failure
            for failure in gate["failures"]
        )
        gate = sleigh_template_source_gate(
            {
                "row_count": 2,
                "mapping_status_counts": {"matched": 1, "unmapped": 1},
                "debug_stage_status_counts": {"decode:ok": 1, "raw_pcode:ok": 1},
                "debug_quality_evidence_totals": {"invalid_pcode_shape_count": 0},
                "debug_template_source_totals": {"sla_construct_tpl": 1},
            },
            "sla_construct_tpl",
        )
        assert gate["status"] == "passed"
        assert gate["mapped_row_count"] == 1
        assert gate["unmapped_row_count"] == 1
        gate = sleigh_template_source_gate(
            {
                "row_count": 2,
                "debug_stage_status_counts": {"decode:ok": 2, "raw_pcode:ok": 2},
                "debug_quality_evidence_totals": {"invalid_pcode_shape_count": 0},
                "debug_template_source_totals": {"sla_construct_tpl": 1},
            },
            "sla_construct_tpl",
        )
        assert gate["status"] == "failed"
        assert any("template source evidence must cover every raw_pcode:ok row (1/2)" in failure for failure in gate["failures"])
        gate = sleigh_template_source_gate(
            {
                "row_count": 1,
                "debug_stage_status_counts": {"decode:ok": 1, "raw_pcode:ok": 1},
                "debug_quality_evidence_totals": {"invalid_pcode_shape_count": 1},
                "debug_template_source_totals": {"sla_construct_tpl": 1},
            },
            "sla_construct_tpl",
        )
        assert gate["status"] == "failed"
        assert any("invalid_pcode_shape_count" in failure for failure in gate["failures"])
        gate = sleigh_template_source_gate(
            {
                "row_count": 1,
                "debug_stage_status_counts": {"decode:ok": 1, "raw_pcode:ok": 1},
                "debug_quality_evidence_totals": {"invalid_pcode_shape_count": 0},
                "debug_template_source_totals": {"sla_construct_tpl": 1},
                "nir_build_stats_metrics": {
                    "numeric_totals": {"raw_pcode_compat_import_count": 1}
                },
            },
            "sla_construct_tpl",
        )
        assert gate["status"] == "failed"
        assert any("raw_pcode_compat_import_count" in failure for failure in gate["failures"])
        gate = sleigh_template_source_gate(
            {
                "row_count": 1,
                "debug_stage_status_counts": {},
                "debug_template_source_totals": {},
            },
            "sla_construct_tpl",
        )
        assert gate["status"] == "failed"
        assert "--include-debug-decomp" in gate["failures"][0]
    print("self-test ok")
    return 0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Benchmark Fission pseudocode against original source semantics. "
            "Optional Ghidra runs are reference lanes, not oracles."
        )
    )
    parser.add_argument("--manifest", default=str(DEFAULT_MANIFEST), help="Source semantic manifest JSON")
    parser.add_argument("--fission-bin", default=str(DEFAULT_FISSION_BIN), help="Path to fission_cli")
    parser.add_argument(
        "--include-ghidra-reference",
        action="store_true",
        help="Also run Ghidra headless and score Ghidra decompiler output against the same source rows as a reference lane",
    )
    parser.add_argument(
        "--ghidra-home",
        default=str(DEFAULT_GHIDRA_HOME),
        help="Ghidra checkout/install root used by --include-ghidra-reference",
    )
    parser.add_argument(
        "--output-dir",
        help="Output artifact directory; defaults to a timestamped directory under benchmark/artifacts/source_semantic_benchmark",
    )
    parser.add_argument(
        "--entry-id",
        action="append",
        help="Run only the manifest entry with this exact id; repeat to include multiple entries",
    )
    parser.add_argument(
        "--tag",
        action="append",
        help="Run only manifest entries containing this tag; repeat to require all listed tags",
    )
    parser.add_argument(
        "--function-name",
        action="append",
        help="Run only source functions with this exact name; repeat to include multiple functions",
    )
    parser.add_argument("--limit-binaries", type=int, help="Limit discovered manifest entries")
    parser.add_argument("--limit-functions", type=int, help="Limit source functions per entry")
    parser.add_argument("--timeout-sec", type=int, default=30, help="Per-command timeout")
    parser.add_argument(
        "--baseline-dir",
        help="Compare against a previous artifact directory or source_semantic_summary.json; defaults to latest matching prior run",
    )
    parser.add_argument(
        "--no-baseline-compare",
        action="store_true",
        help="Disable automatic comparison against previous source-semantic artifacts",
    )
    parser.add_argument(
        "--no-baseline-snapshot",
        action="store_true",
        help="Do not copy the selected baseline summary/rows/comparison into the current artifact directory",
    )
    parser.add_argument(
        "--include-debug-decomp",
        action="store_true",
        help="Pass fission_cli decomp --debug-decomp and attach compact stage/owner evidence to each row",
    )
    parser.add_argument(
        "--require-sleigh-template-source",
        choices=["sla_construct_tpl"],
        help=(
            "Fail the run unless all debug SLEIGH template-source evidence uses this source. "
            "Requires --include-debug-decomp for rows with raw_pcode:ok."
        ),
    )
    parser.add_argument(
        "--materialize-debug-triage",
        action="store_true",
        help="Run fission_cli decomp/disasm/xrefs/function-facts for the lowest-scoring rows and save captures",
    )
    parser.add_argument(
        "--debug-triage-limit",
        type=int,
        default=8,
        help="Maximum non-perfect rows to materialize with --materialize-debug-triage",
    )
    parser.add_argument(
        "--materialize-regression-debug-triage",
        action="store_true",
        help="Run fission_cli debug surfaces for rows that regressed versus the selected baseline",
    )
    parser.add_argument(
        "--regression-debug-triage-limit",
        type=int,
        default=8,
        help="Maximum regressed rows to materialize with --materialize-regression-debug-triage",
    )
    parser.add_argument(
        "--decomp-cache-file",
        default=str(DEFAULT_DECOMP_CACHE_FILE),
        help="Persistent decompile-result cache file keyed by input binary and fission_cli build metadata",
    )
    parser.add_argument(
        "--no-decomp-cache",
        action="store_true",
        help="Disable the persistent decompile-result cache; the in-run memory cache remains enabled",
    )
    parser.add_argument(
        "--list-cache-file",
        default=str(DEFAULT_LIST_CACHE_FILE),
        help="Persistent fission_cli list-result cache file keyed by input binary and fission_cli build metadata",
    )
    parser.add_argument(
        "--no-list-cache",
        action="store_true",
        help="Disable the persistent fission_cli list-result cache",
    )
    parser.add_argument(
        "--behavior-cache-file",
        default=str(DEFAULT_BEHAVIOR_CACHE_FILE),
        help="Persistent behavior harness cache file keyed by C harness contents and compiler metadata",
    )
    parser.add_argument(
        "--no-behavior-cache",
        action="store_true",
        help="Disable persistent behavior harness compile/run cache",
    )
    parser.add_argument(
        "--jobs",
        type=int,
        default=DEFAULT_JOBS,
        help=f"Run source-function rows in parallel per binary entry (default: {DEFAULT_JOBS}; use 1 for serial)",
    )
    parser.add_argument("--self-test", action="store_true", help="Run lightweight parser/scoring self-test")
    parser.add_argument(
        "--format",
        choices=["json", "markdown"],
        default="json",
        help="Output format for the final summary on stdout (default: json)",
    )
    parser.add_argument(
        "--enable-sanitizer",
        action="store_true",
        help="Enable ASan and UBSan flags for host validation compilation",
    )
    parser.add_argument(
        "--fuzz-cases",
        action="store_true",
        help="Enable fuzzing of dynamic behavior test inputs when explicit cases are absent",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.self_test:
        return run_self_test()
    return run_benchmark(args)


if __name__ == "__main__":
    raise SystemExit(main())
