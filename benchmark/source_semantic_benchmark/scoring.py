from __future__ import annotations
import re
import json
from collections import Counter
from pathlib import Path
from typing import Any

from benchmark.source_semantic_benchmark.models import SourceFunction, FissionFunction, BenchmarkEntry
from benchmark.source_semantic_benchmark.config import (
    CALL_RE,
    CALL_EXCLUDE,
    INDIRECT_CAST_CALL_RE,
    DEREF_INDIRECT_CALL_RE,
    WORD_BOUNDARY_RE,
    CONTROL_WORDS,
    CONST_RE,
    DEFAULT_ARTIFACT_ROOT,
)
from benchmark.source_semantic_benchmark.utils import (
    percent,
    canonical_address,
    normalize_name,
    load_json,
    rel,
    utc_now,
    utc_isoformat,
    dump_json_pretty,
    dump_json_line,
)
from benchmark.source_semantic_benchmark.parser import (
    strip_comments,
    find_matching_paren,
    extract_c_like_functions,
)

STAGE_FAILURE_ORDER = ["load", "decode", "raw_pcode", "nir_build", "normalize", "structuring", "render"]

STATIC_SIMILARITY_COMPONENTS: dict[str, tuple[str, ...]] = {
    "control_flow": ("ctrl:",),
    "operator": ("op:",),
    "call": ("call:",),
    "constant": ("const:",),
    "memory": ("mem:",),
    "signature": ("sig:",),
}


def row_key(row: dict[str, Any]) -> str:
    return "::".join(
        [
            str(row.get("entry_id") or ""),
            str(row.get("source_path") or ""),
            str(row.get("function_name") or ""),
        ]
    )


def match_function(source: SourceFunction, funcs: list[FissionFunction]) -> tuple[str, FissionFunction | None, list[str]]:
    literal_exact = [f for f in funcs if f.name == source.name]
    if len(literal_exact) == 1:
        return "matched", literal_exact[0], []
    if len(literal_exact) > 1:
        return "ambiguous", None, [f"{f.address}:{f.name}" for f in literal_exact[:8]]

    src_norm = normalize_name(source.name)
    exact = [f for f in funcs if normalize_name(f.name) == src_norm]
    if len(exact) == 1:
        return "matched", exact[0], []
    if len(exact) > 1:
        return "ambiguous", None, [f"{f.address}:{f.name}" for f in exact[:8]]

    suffix = [
        f
        for f in funcs
        if normalize_name(f.name).endswith(src_norm) and src_norm and not normalize_name(f.name).startswith("sub")
    ]
    if len(suffix) == 1:
        return "matched", suffix[0], []
    if len(suffix) > 1:
        return "ambiguous", None, [f"{f.address}:{f.name}" for f in suffix[:8]]
    return "unmapped", None, []


def select_source_functions(
    source_functions: list[SourceFunction],
    fission_funcs: list[FissionFunction],
    limit: int | None,
    fission_error: str | None = None,
) -> list[SourceFunction]:
    if limit is None:
        return source_functions
    if limit <= 0:
        return []
    if fission_error or not fission_funcs:
        return source_functions[:limit]

    matched: list[SourceFunction] = []
    fallback: list[SourceFunction] = []
    for func in source_functions:
        status, matched_func, _ = match_function(func, fission_funcs)
        if status == "matched" and matched_func is not None:
            matched.append(func)
        else:
            fallback.append(func)
    return (matched + fallback)[:limit]


def source_call_counts(body: str) -> Counter[str]:
    return Counter(call_names_for_fingerprint(body))


def is_function_definition_call_match(text: str, match: re.Match[str]) -> bool:
    open_idx = text.rfind("(", match.start(), match.end())
    if open_idx < 0:
        return False
    close_idx = find_matching_paren(text, open_idx)
    if close_idx is None:
        return False
    next_idx = close_idx + 1
    while next_idx < len(text) and text[next_idx].isspace():
        next_idx += 1
    if next_idx >= len(text) or text[next_idx] != "{":
        return False
    statement_start = max(
        text.rfind(";", 0, match.start()),
        text.rfind("{", 0, match.start()),
        text.rfind("}", 0, match.start()),
    ) + 1
    prefix = text[statement_start:match.start()].strip()
    return bool(prefix)


def call_names_for_fingerprint(code: str) -> list[str]:
    stripped = strip_comments(code)
    calls: list[str] = []
    for match in CALL_RE.finditer(stripped):
        lowered = match.group(1).split("::")[-1].lower()
        if lowered in CALL_EXCLUDE:
            continue
        if stripped[match.end() :].lstrip().startswith("*"):
            continue
        if is_function_definition_call_match(stripped, match):
            continue
        calls.append(normalize_name(lowered))
    return calls


def function_pointer_param_names(func: SourceFunction | None) -> set[str]:
    if func is None:
        return set()
    return {
        normalize_name(name)
        for name, kind in zip(func.param_names, func.param_kinds, strict=False)
        if kind == "aggregate_or_pointer"
    }


def indirect_cast_call_names_for_fingerprint(code: str) -> list[str]:
    stripped = strip_comments(code)
    calls = [
        normalize_name(match.group(1))
        for match in INDIRECT_CAST_CALL_RE.finditer(stripped)
    ]
    calls.extend(
        normalize_name(match.group(1))
        for match in DEREF_INDIRECT_CALL_RE.finditer(stripped)
    )
    return calls


def add_call_fingerprint(counter: Counter[str], code: str, func: SourceFunction | None) -> None:
    pointer_params = function_pointer_param_names(func)
    for call in call_names_for_fingerprint(code):
        if call in pointer_params:
            counter["call:indirect_param"] += 1
        else:
            counter[f"call:{call}"] += 1
    for call in indirect_cast_call_names_for_fingerprint(code):
        if call in pointer_params or call.startswith("param"):
            counter["call:indirect_param"] += 1
        else:
            counter["call:indirect"] += 1


def code_fingerprint(code: str, func: SourceFunction | None = None) -> Counter[str]:
    stripped = strip_comments(code)
    counter: Counter[str] = Counter()
    signature_func = func
    if signature_func is None:
        rendered_functions = extract_c_like_functions(stripped, "c")
        if rendered_functions:
            signature_func = rendered_functions[0]
    for word in WORD_BOUNDARY_RE.findall(stripped):
        lowered = word.lower()
        if lowered in CONTROL_WORDS:
            counter[f"ctrl:{lowered}"] += 1
    for op in ["<<", ">>", "==", "!=", "<=", ">=", "&&", "||", "->", "+", "-", "*", "/", "%", "&", "|", "^", "<", ">", "="]:
        counter[f"op:{op}"] += stripped.count(op)
    for const in CONST_RE.findall(stripped):
        counter[f"const:{const.lower()}"] += 1
    add_call_fingerprint(counter, stripped, signature_func)
    counter["mem:index"] += stripped.count("[")
    counter["mem:deref_or_ptr"] += stripped.count("*")
    counter["mem:field"] += stripped.count("->") + stripped.count(".")
    if func is not None:
        add_signature_fingerprint(counter, func.return_kind, func.param_kinds)
    elif signature_func is not None:
        add_signature_fingerprint(counter, signature_func.return_kind, signature_func.param_kinds)
    return +counter


def add_signature_fingerprint(counter: Counter[str], return_kind: str, param_kinds: list[str]) -> None:
    counter[f"sig:return:{return_kind}"] += 1
    counter[f"sig:param_count:{len(param_kinds)}"] += 1
    for kind in param_kinds:
        counter[f"sig:param:{kind}"] += 1


def add_rendered_signature_fingerprint(counter: Counter[str], code: str) -> None:
    functions = extract_c_like_functions(code, "c")
    if not functions:
        return
    rendered = functions[0]
    add_signature_fingerprint(counter, rendered.return_kind, rendered.param_kinds)


def rendered_signature_kinds(code: str) -> tuple[str, list[str]] | None:
    functions = extract_c_like_functions(strip_comments(code), "c")
    if not functions:
        return None
    rendered = functions[0]
    return rendered.return_kind, rendered.param_kinds


def inline_expanded_source_fingerprint(
    func: SourceFunction,
    functions_by_name: dict[str, SourceFunction],
    max_depth: int = 2,
) -> Counter[str]:
    return expand_source_body_fingerprint(
        func.body,
        functions_by_name,
        include_signature=func,
        max_depth=max_depth,
        visiting=(normalize_name(func.name),),
    )


def expand_source_body_fingerprint(
    body: str,
    functions_by_name: dict[str, SourceFunction],
    include_signature: SourceFunction | None,
    max_depth: int,
    visiting: tuple[str, ...],
) -> Counter[str]:
    counter = code_fingerprint(body, include_signature)
    if max_depth <= 0:
        return counter
    calls = source_call_counts(body)
    for callee_name, count in calls.items():
        if count <= 0 or callee_name in visiting:
            continue
        callee = functions_by_name.get(callee_name)
        if callee is None:
            continue
        call_key = f"call:{callee_name}"
        counter[call_key] -= min(counter.get(call_key, 0), count)
        callee_fp = expand_source_body_fingerprint(
            callee.body,
            functions_by_name,
            include_signature=None,
            max_depth=max_depth - 1,
            visiting=(*visiting, callee_name),
        )
        for key, value in callee_fp.items():
            counter[key] += value * count
    return +counter


def multiset_jaccard(left: Counter[str], right: Counter[str]) -> float:
    keys = set(left) | set(right)
    if not keys:
        return 1.0
    inter = sum(min(left[k], right[k]) for k in keys)
    union = sum(max(left[k], right[k]) for k in keys)
    return round(inter / union, 6) if union else 1.0


def multiset_gap_details(left: Counter[str], right: Counter[str], top_limit: int = 12) -> dict[str, Any]:
    keys = set(left) | set(right)
    intersection = sum(min(left[key], right[key]) for key in keys)
    union = sum(max(left[key], right[key]) for key in keys)
    missing = Counter({key: left[key] - right[key] for key in keys if left[key] > right[key]})
    extra = Counter({key: right[key] - left[key] for key in keys if right[key] > left[key]})
    missing_total = sum(missing.values())
    extra_total = sum(extra.values())
    return {
        "source_feature_total": sum(left.values()),
        "decomp_feature_total": sum(right.values()),
        "intersection_feature_total": intersection,
        "union_feature_total": union,
        "missing_feature_total": missing_total,
        "extra_feature_total": extra_total,
        "missing_feature_rate": round(missing_total / sum(left.values()), 6) if left else 0.0,
        "extra_feature_rate": round(extra_total / sum(right.values()), 6) if right else 0.0,
        "top_missing_features": [
            {"feature": feature, "count": count}
            for feature, count in missing.most_common(top_limit)
        ],
        "top_extra_features": [
            {"feature": feature, "count": count}
            for feature, count in extra.most_common(top_limit)
        ],
    }


def fingerprint_subset(fp: Counter[str], prefixes: tuple[str, ...]) -> Counter[str]:
    return Counter({key: value for key, value in fp.items() if key.startswith(prefixes)})


def static_similarity_components(source_fp: Counter[str], decomp_fp: Counter[str]) -> dict[str, float]:
    return {
        name: multiset_jaccard(fingerprint_subset(source_fp, prefixes), fingerprint_subset(decomp_fp, prefixes))
        for name, prefixes in STATIC_SIMILARITY_COMPONENTS.items()
    }


def static_similarity_gap_components(source_fp: Counter[str], decomp_fp: Counter[str]) -> dict[str, dict[str, Any]]:
    return {
        name: multiset_gap_details(
            fingerprint_subset(source_fp, prefixes),
            fingerprint_subset(decomp_fp, prefixes),
            top_limit=6,
        )
        for name, prefixes in STATIC_SIMILARITY_COMPONENTS.items()
    }


def gap_feature_count(items: Any, feature: str) -> float:
    if not isinstance(items, list):
        return 0.0
    total = 0.0
    for item in items:
        if (
            isinstance(item, dict)
            and item.get("feature") == feature
            and isinstance(item.get("count"), int | float)
        ):
            total += float(item["count"])
    return total


def signedness_only_signature_gap(details: dict[str, Any]) -> dict[str, float]:
    missing = details.get("top_missing_features")
    extra = details.get("top_extra_features")
    source_int_param_decomp_uint = min(
        gap_feature_count(missing, "sig:param:int"),
        gap_feature_count(extra, "sig:param:uint"),
    )
    source_uint_param_decomp_int = min(
        gap_feature_count(missing, "sig:param:uint"),
        gap_feature_count(extra, "sig:param:int"),
    )
    source_int_return_decomp_uint = min(
        gap_feature_count(missing, "sig:return:int"),
        gap_feature_count(extra, "sig:return:uint"),
    )
    source_uint_return_decomp_int = min(
        gap_feature_count(missing, "sig:return:uint"),
        gap_feature_count(extra, "sig:return:int"),
    )
    return {
        "param_pair_count": source_int_param_decomp_uint + source_uint_param_decomp_int,
        "return_pair_count": source_int_return_decomp_uint + source_uint_return_decomp_int,
        "source_int_param_decomp_uint_count": source_int_param_decomp_uint,
        "source_uint_param_decomp_int_count": source_uint_param_decomp_int,
        "source_int_return_decomp_uint_count": source_int_return_decomp_uint,
        "source_uint_return_decomp_int_count": source_uint_return_decomp_int,
    }


def metric_delta(current: dict[str, Any], baseline: dict[str, Any], key: str) -> dict[str, Any]:
    current_value = current.get(key)
    baseline_value = baseline.get(key)
    if key.endswith("_percent"):
        raw_key = key.removesuffix("_percent")
        if not isinstance(current_value, (int, float)) and isinstance(current.get(raw_key), (int, float)):
            current_value = percent(float(current[raw_key]))
        if not isinstance(baseline_value, (int, float)) and isinstance(baseline.get(raw_key), (int, float)):
            baseline_value = percent(float(baseline[raw_key]))
    if not isinstance(current_value, (int, float)) or not isinstance(baseline_value, (int, float)):
        return {"current": current_value, "baseline": baseline_value, "delta": None}
    return {
        "current": current_value,
        "baseline": baseline_value,
        "delta": round(float(current_value) - float(baseline_value), 6),
    }


def compare_to_baseline(
    summary: dict[str, Any],
    rows: list[dict[str, Any]],
    baseline_summary: dict[str, Any],
    baseline_rows: list[dict[str, Any]],
    baseline_path: Path,
) -> dict[str, Any]:
    current_by_key = {row_key(row): row for row in rows}
    baseline_by_key = {row_key(row): row for row in baseline_rows}
    shared_keys = sorted(set(current_by_key) & set(baseline_by_key))
    new_keys = sorted(set(current_by_key) - set(baseline_by_key))
    missing_keys = sorted(set(baseline_by_key) - set(current_by_key))

    row_deltas: list[dict[str, Any]] = []
    improved = 0
    regressed = 0
    unchanged = 0
    behavior_improved = 0
    behavior_regressed = 0
    score_delta_sum_negative = 0.0
    score_delta_sum_positive = 0.0
    new_zero_score_rows = 0
    new_unmapped_rows = 0
    new_behavior_fail_rows = 0
    for key in shared_keys:
        current = current_by_key[key]
        baseline = baseline_by_key[key]
        current_score = float(current.get("semantic_score", 0.0) or 0.0)
        baseline_score = float(baseline.get("semantic_score", 0.0) or 0.0)
        delta = round(current_score - baseline_score, 6)
        if delta > 0:
            improved += 1
            score_delta_sum_positive += delta
        elif delta < 0:
            regressed += 1
            score_delta_sum_negative += delta
        else:
            unchanged += 1

        current_behavior = current.get("behavior", {}).get("status")
        baseline_behavior = baseline.get("behavior", {}).get("status")
        if current_behavior == "pass" and baseline_behavior != "pass":
            behavior_improved += 1
        elif current_behavior != "pass" and baseline_behavior == "pass":
            behavior_regressed += 1
            new_behavior_fail_rows += 1
        if current_score == 0.0 and baseline_score > 0.0:
            new_zero_score_rows += 1
        if current.get("mapping_status") != "matched" and baseline.get("mapping_status") == "matched":
            new_unmapped_rows += 1

        if delta != 0 or current_behavior != baseline_behavior:
            row_deltas.append(
                {
                    "key": key,
                    "entry_id": current.get("entry_id"),
                    "function_name": current.get("function_name"),
                    "current_score": current_score,
                    "baseline_score": baseline_score,
                    "delta": delta,
                    "current_score_percent": percent(current_score),
                    "baseline_score_percent": percent(baseline_score),
                    "delta_percent": percent(delta),
                    "current_behavior": current_behavior,
                    "baseline_behavior": baseline_behavior,
                    "current_mapping_status": current.get("mapping_status"),
                    "baseline_mapping_status": baseline.get("mapping_status"),
                    "current_decomp_failure_kind": current.get("decomp_failure_kind"),
                    "baseline_decomp_failure_kind": baseline.get("decomp_failure_kind"),
                }
            )

    row_deltas.sort(key=lambda row: (abs(float(row["delta"])), row["function_name"] or ""), reverse=True)
    top_improvements = sorted(
        (row for row in row_deltas if float(row.get("delta", 0.0) or 0.0) > 0.0),
        key=lambda row: (float(row["delta"]), row["function_name"] or ""),
        reverse=True,
    )[:10]
    top_regressions = sorted(
        (row for row in row_deltas if float(row.get("delta", 0.0) or 0.0) < 0.0),
        key=lambda row: (float(row["delta"]), row["function_name"] or ""),
    )[:10]
    metric_keys = [
        "weighted_semantic_similarity",
        "weighted_semantic_similarity_percent",
        "semantic_score_nonzero_rate",
        "function_mapping_rate",
        "decomp_success_rate",
        "candidate_compile_rate",
        "behavior_pass_rate",
        "behavior_pass_rate_total_denominator",
        "behavior_case_pass_rate",
        "behavior_pass_row_rate_executed_denominator",
        "behavior_mismatch_row_count",
        "behavior_expected_but_not_executed_row_count",
        "behavior_expected_rate",
        "behavior_executed_rate",
        "static_source_recall",
        "static_decomp_precision",
        "static_union_jaccard",
        "static_missing_feature_rate",
        "static_missing_feature_row_rate",
        "static_decomp_absent_feature_row_rate",
        "zero_static_intersection_row_rate",
        "fully_perfect_rate",
        "behavior_pass_static_perfect_rate",
        "behavior_pass_static_gap_rate",
        "static_perfect_behavior_nonpass_rate",
        "pipeline_ok_behavior_nonpass_rate",
        "rows_excluded_from_semantic_score_denominator",
        "source_extracted_function_count",
        "source_selected_function_count",
        "source_suppressed_static_inline_helper_count",
        "source_suppressed_static_inline_helper_rate",
        "lost_score_sum",
        "perfect_row_count",
        "supported_behavior_row_count",
        "row_count",
    ]
    effective = summary.get("effective_coverage") if isinstance(summary.get("effective_coverage"), dict) else {}
    behavior_eligibility = (
        summary.get("behavior_eligibility") if isinstance(summary.get("behavior_eligibility"), dict) else {}
    )
    baseline_effective = (
        baseline_summary.get("effective_coverage")
        if isinstance(baseline_summary.get("effective_coverage"), dict)
        else {}
    )
    baseline_behavior_eligibility = (
        baseline_summary.get("behavior_eligibility")
        if isinstance(baseline_summary.get("behavior_eligibility"), dict)
        else {}
    )
    metric_source = dict(summary)
    semantic_stats = summary.get("semantic_score_stats") if isinstance(summary.get("semantic_score_stats"), dict) else {}
    behavior_cases = summary.get("behavior_case_metrics") if isinstance(summary.get("behavior_case_metrics"), dict) else {}
    behavior_mismatches = (
        summary.get("behavior_mismatch_metrics")
        if isinstance(summary.get("behavior_mismatch_metrics"), dict)
        else {}
    )
    static_gaps = (
        summary.get("static_similarity_gap_totals")
        if isinstance(summary.get("static_similarity_gap_totals"), dict)
        else {}
    )
    static_gap_rows = (
        summary.get("static_gap_row_metrics")
        if isinstance(summary.get("static_gap_row_metrics"), dict)
        else {}
    )
    denominator_accounting = (
        summary.get("denominator_accounting_metrics")
        if isinstance(summary.get("denominator_accounting_metrics"), dict)
        else {}
    )
    behavior_denominators = (
        summary.get("behavior_denominator_metrics")
        if isinstance(summary.get("behavior_denominator_metrics"), dict)
        else {}
    )
    static_absence = (
        summary.get("static_absence_penalty_metrics")
        if isinstance(summary.get("static_absence_penalty_metrics"), dict)
        else {}
    )
    score_denominators = (
        summary.get("score_denominator_metrics")
        if isinstance(summary.get("score_denominator_metrics"), dict)
        else {}
    )
    readiness_metrics = (
        summary.get("semantic_readiness_metrics")
        if isinstance(summary.get("semantic_readiness_metrics"), dict)
        else {}
    )
    integrity_metrics = (
        summary.get("benchmark_integrity_metrics")
        if isinstance(summary.get("benchmark_integrity_metrics"), dict)
        else {}
    )
    source_row_selection = (
        summary.get("source_row_selection_metrics")
        if isinstance(summary.get("source_row_selection_metrics"), dict)
        else {}
    )
    metric_source.update(
        {
            "semantic_score_nonzero_rate": semantic_stats.get("nonzero_rate"),
            "behavior_pass_rate_total_denominator": behavior_eligibility.get("pass_rate_total_denominator"),
            "behavior_case_pass_rate": behavior_cases.get("case_pass_rate"),
            "behavior_pass_row_rate_executed_denominator": behavior_denominators.get(
                "pass_row_rate_executed_denominator"
            ),
            "behavior_mismatch_row_count": behavior_mismatches.get("mismatch_row_count"),
            "behavior_expected_but_not_executed_row_count": denominator_accounting.get(
                "behavior_expected_but_not_executed_row_count"
            ),
            "behavior_expected_rate": effective.get("behavior_expected_rate"),
            "behavior_executed_rate": effective.get("behavior_executed_rate"),
            "static_source_recall": static_absence.get("source_recall"),
            "static_decomp_precision": static_absence.get("decomp_precision"),
            "static_union_jaccard": static_absence.get("union_jaccard"),
            "static_missing_feature_rate": static_gaps.get("missing_feature_rate"),
            "static_missing_feature_row_rate": static_gap_rows.get("missing_feature_row_rate"),
            "static_decomp_absent_feature_row_rate": static_gap_rows.get("decomp_absent_feature_row_rate"),
            "zero_static_intersection_row_rate": static_gap_rows.get("zero_static_intersection_row_rate"),
            "fully_perfect_rate": readiness_metrics.get("fully_perfect_rate"),
            "behavior_pass_static_perfect_rate": readiness_metrics.get("behavior_pass_static_perfect_rate"),
            "behavior_pass_static_gap_rate": readiness_metrics.get("behavior_pass_static_gap_rate"),
            "static_perfect_behavior_nonpass_rate": readiness_metrics.get("static_perfect_behavior_nonpass_rate"),
            "pipeline_ok_behavior_nonpass_rate": readiness_metrics.get("pipeline_ok_behavior_nonpass_rate"),
            "rows_excluded_from_semantic_score_denominator": integrity_metrics.get(
                "rows_excluded_from_semantic_score_denominator"
            ),
            "source_extracted_function_count": source_row_selection.get("extracted_source_function_count"),
            "source_selected_function_count": source_row_selection.get("selected_source_function_count"),
            "source_suppressed_static_inline_helper_count": source_row_selection.get(
                "suppressed_static_inline_helper_count"
            ),
            "source_suppressed_static_inline_helper_rate": source_row_selection.get(
                "suppressed_static_inline_helper_rate_filtered_denominator"
            ),
            "lost_score_sum": score_denominators.get("lost_score_sum"),
        }
    )
    baseline_metric_source = dict(baseline_summary)
    baseline_semantic_stats = (
        baseline_summary.get("semantic_score_stats")
        if isinstance(baseline_summary.get("semantic_score_stats"), dict)
        else {}
    )
    baseline_behavior_cases = (
        baseline_summary.get("behavior_case_metrics")
        if isinstance(baseline_summary.get("behavior_case_metrics"), dict)
        else {}
    )
    baseline_behavior_mismatches = (
        baseline_summary.get("behavior_mismatch_metrics")
        if isinstance(baseline_summary.get("behavior_mismatch_metrics"), dict)
        else {}
    )
    baseline_static_gaps = (
        baseline_summary.get("static_similarity_gap_totals")
        if isinstance(baseline_summary.get("static_similarity_gap_totals"), dict)
        else {}
    )
    baseline_static_gap_rows = (
        baseline_summary.get("static_gap_row_metrics")
        if isinstance(baseline_summary.get("static_gap_row_metrics"), dict)
        else {}
    )
    baseline_denominator_accounting = (
        baseline_summary.get("denominator_accounting_metrics")
        if isinstance(baseline_summary.get("denominator_accounting_metrics"), dict)
        else {}
    )
    baseline_behavior_denominators = (
        baseline_summary.get("behavior_denominator_metrics")
        if isinstance(baseline_summary.get("behavior_denominator_metrics"), dict)
        else {}
    )
    baseline_static_absence = (
        baseline_summary.get("static_absence_penalty_metrics")
        if isinstance(baseline_summary.get("static_absence_penalty_metrics"), dict)
        else {}
    )
    baseline_score_denominators = (
        baseline_summary.get("score_denominator_metrics")
        if isinstance(baseline_summary.get("score_denominator_metrics"), dict)
        else {}
    )
    baseline_readiness_metrics = (
        baseline_summary.get("semantic_readiness_metrics")
        if isinstance(baseline_summary.get("semantic_readiness_metrics"), dict)
        else {}
    )
    baseline_integrity_metrics = (
        baseline_summary.get("benchmark_integrity_metrics")
        if isinstance(baseline_summary.get("benchmark_integrity_metrics"), dict)
        else {}
    )
    baseline_source_row_selection = (
        baseline_summary.get("source_row_selection_metrics")
        if isinstance(baseline_summary.get("source_row_selection_metrics"), dict)
        else {}
    )
    baseline_metric_source.update(
        {
            "semantic_score_nonzero_rate": baseline_semantic_stats.get("nonzero_rate"),
            "behavior_pass_rate_total_denominator": baseline_behavior_eligibility.get("pass_rate_total_denominator"),
            "behavior_case_pass_rate": baseline_behavior_cases.get("case_pass_rate"),
            "behavior_pass_row_rate_executed_denominator": baseline_behavior_denominators.get(
                "pass_row_rate_executed_denominator"
            ),
            "behavior_mismatch_row_count": baseline_behavior_mismatches.get("mismatch_row_count"),
            "behavior_expected_but_not_executed_row_count": baseline_denominator_accounting.get(
                "behavior_expected_but_not_executed_row_count"
            ),
            "behavior_expected_rate": baseline_effective.get("behavior_expected_rate"),
            "behavior_executed_rate": baseline_effective.get("behavior_executed_rate"),
            "static_source_recall": baseline_static_absence.get("source_recall"),
            "static_decomp_precision": baseline_static_absence.get("decomp_precision"),
            "static_union_jaccard": baseline_static_absence.get("union_jaccard"),
            "static_missing_feature_rate": baseline_static_gaps.get("missing_feature_rate"),
            "static_missing_feature_row_rate": baseline_static_gap_rows.get("missing_feature_row_rate"),
            "static_decomp_absent_feature_row_rate": baseline_static_gap_rows.get("decomp_absent_feature_row_rate"),
            "zero_static_intersection_row_rate": baseline_static_gap_rows.get("zero_static_intersection_row_rate"),
            "fully_perfect_rate": baseline_readiness_metrics.get("fully_perfect_rate"),
            "behavior_pass_static_perfect_rate": baseline_readiness_metrics.get("behavior_pass_static_perfect_rate"),
            "behavior_pass_static_gap_rate": baseline_readiness_metrics.get("behavior_pass_static_gap_rate"),
            "static_perfect_behavior_nonpass_rate": baseline_readiness_metrics.get(
                "static_perfect_behavior_nonpass_rate"
            ),
            "pipeline_ok_behavior_nonpass_rate": baseline_readiness_metrics.get("pipeline_ok_behavior_nonpass_rate"),
            "rows_excluded_from_semantic_score_denominator": baseline_integrity_metrics.get(
                "rows_excluded_from_semantic_score_denominator"
            ),
            "source_extracted_function_count": baseline_source_row_selection.get("extracted_source_function_count"),
            "source_selected_function_count": baseline_source_row_selection.get("selected_source_function_count"),
            "source_suppressed_static_inline_helper_count": baseline_source_row_selection.get(
                "suppressed_static_inline_helper_count"
            ),
            "source_suppressed_static_inline_helper_rate": baseline_source_row_selection.get(
                "suppressed_static_inline_helper_rate_filtered_denominator"
            ),
            "lost_score_sum": baseline_score_denominators.get("lost_score_sum"),
        }
    )
    return {
        "baseline_summary_path": rel(baseline_path),
        "shared_row_count": len(shared_keys),
        "new_row_count": len(new_keys),
        "missing_row_count": len(missing_keys),
        "improved_row_count": improved,
        "regressed_row_count": regressed,
        "unchanged_row_count": unchanged,
        "behavior_improved_row_count": behavior_improved,
        "behavior_regressed_row_count": behavior_regressed,
        "regression_severity": {
            "score_delta_sum_negative": round(score_delta_sum_negative, 6),
            "score_delta_sum_positive": round(score_delta_sum_positive, 6),
            "new_zero_score_rows": new_zero_score_rows,
            "new_unmapped_rows": new_unmapped_rows,
            "new_behavior_fail_rows": new_behavior_fail_rows,
        },
        "metric_deltas": {key: metric_delta(metric_source, baseline_metric_source, key) for key in metric_keys},
        "top_row_deltas": row_deltas[:20],
        "top_improvements": top_improvements,
        "top_regressions": top_regressions,
        "new_rows": [current_by_key[key].get("function_name") for key in new_keys[:20]],
        "missing_rows": [baseline_by_key[key].get("function_name") for key in missing_keys[:20]],
    }


def comparison_outcome(comparison: dict[str, Any]) -> dict[str, Any]:
    weighted_delta = (
        comparison.get("metric_deltas", {})
        .get("weighted_semantic_similarity_percent", {})
        .get("delta")
    )
    behavior_improved = int(comparison.get("behavior_improved_row_count") or 0)
    behavior_regressed = int(comparison.get("behavior_regressed_row_count") or 0)
    improved = int(comparison.get("improved_row_count") or 0)
    regressed = int(comparison.get("regressed_row_count") or 0)
    shape_changed = bool(comparison.get("new_row_count") or comparison.get("missing_row_count"))
    if shape_changed:
        direction = "mixed"
    elif isinstance(weighted_delta, (int, float)) and weighted_delta > 0 and behavior_regressed == 0:
        direction = "improved"
    elif isinstance(weighted_delta, (int, float)) and weighted_delta < 0 and behavior_improved == 0:
        direction = "regressed"
    elif improved == 0 and regressed == 0 and behavior_improved == 0 and behavior_regressed == 0:
        direction = "unchanged"
    else:
        direction = "mixed"
    delta_text = "n/a" if not isinstance(weighted_delta, (int, float)) else f"{weighted_delta:+.3f}%"
    return {
        "direction": direction,
        "weighted_semantic_similarity_percent_delta": weighted_delta,
        "headline": (
            f"{direction}: weighted semantic similarity {delta_text}, "
            f"rows +{improved}/-{regressed}, behavior +{behavior_improved}/-{behavior_regressed}"
        ),
    }


def improvement_summary(comparison: dict[str, Any]) -> dict[str, Any]:
    metric_deltas = comparison.get("metric_deltas") if isinstance(comparison.get("metric_deltas"), dict) else {}

    def delta_for(key: str) -> float | None:
        metric = metric_deltas.get(key)
        if not isinstance(metric, dict):
            return None
        delta = metric.get("delta")
        return float(delta) if isinstance(delta, int | float) else None

    improved_metrics: list[dict[str, Any]] = []
    regressed_metrics: list[dict[str, Any]] = []
    for key in [
        "weighted_semantic_similarity_percent",
        "semantic_score_nonzero_rate",
        "function_mapping_rate",
        "decomp_success_rate",
        "candidate_compile_rate",
        "behavior_pass_rate",
        "behavior_case_pass_rate",
        "perfect_row_count",
        "supported_behavior_row_count",
    ]:
        delta = delta_for(key)
        if delta is None or delta == 0:
            continue
        metric = {
            "metric": key,
            "delta": delta,
            "current": metric_deltas.get(key, {}).get("current"),
            "baseline": metric_deltas.get(key, {}).get("baseline"),
        }
        if delta > 0:
            improved_metrics.append(metric)
        else:
            regressed_metrics.append(metric)

    return {
        "headline": comparison_outcome(comparison)["headline"],
        "improved_metric_count": len(improved_metrics),
        "regressed_metric_count": len(regressed_metrics),
        "improved_metrics": improved_metrics,
        "regressed_metrics": regressed_metrics,
        "top_improved_functions": [
            {
                "function_name": row.get("function_name"),
                "delta_percent": row.get("delta_percent"),
                "baseline_score_percent": row.get("baseline_score_percent"),
                "current_score_percent": row.get("current_score_percent"),
                "baseline_behavior": row.get("baseline_behavior"),
                "current_behavior": row.get("current_behavior"),
            }
            for row in (comparison.get("top_improvements") or [])[:10]
        ],
        "top_regressed_functions": [
            {
                "function_name": row.get("function_name"),
                "delta_percent": row.get("delta_percent"),
                "baseline_score_percent": row.get("baseline_score_percent"),
                "current_score_percent": row.get("current_score_percent"),
                "baseline_behavior": row.get("baseline_behavior"),
                "current_behavior": row.get("current_behavior"),
            }
            for row in (comparison.get("top_regressions") or [])[:10]
        ],
    }


def stage_first_failure(debug_decomp: Any) -> str | None:
    if not isinstance(debug_decomp, dict):
        return None
    stage_status = debug_decomp.get("stage_status")
    if not isinstance(stage_status, dict):
        return None
    for stage in STAGE_FAILURE_ORDER:
        status = stage_status.get(stage)
        if status not in {None, "ok"}:
            return f"{stage}:{status}"
    return None


def zero_credit_reason(
    mapping_status: str,
    decomp: dict[str, Any],
    behavior: dict[str, Any],
    static_score: float,
    semantic_score: float,
) -> str | None:
    if semantic_score > 0.0:
        return None
    if mapping_status != "matched":
        return mapping_status
    if not decomp.get("success"):
        return f"decomp:{decomp.get('failure_kind', 'unknown')}"
    behavior_status = behavior.get("status", "unknown")
    if behavior_status not in {"unsupported_signature", "pass"}:
        return f"behavior:{behavior_status}"
    if static_score == 0.0:
        return "static_zero"
    return "weighted_zero"


def row_zero_credit_reason(row: dict[str, Any]) -> str:
    explicit = row.get("zero_credit_reason")
    if explicit:
        return str(explicit)
    mapping_status = str(row.get("mapping_status") or "unknown")
    if mapping_status != "matched":
        return mapping_status
    if not row.get("decomp_success"):
        return f"decomp:{row.get('decomp_failure_kind', 'unknown')}"
    behavior_status = row.get("behavior", {}).get("status", "unknown")
    if behavior_status not in {"unsupported_signature", "pass"}:
        return f"behavior:{behavior_status}"
    if float(row.get("static_semantic_score", 0.0) or 0.0) == 0.0:
        return "static_zero"
    return "weighted_zero"


def row_triage_priority(row: dict[str, Any]) -> tuple[int, float, int, str]:
    score = float(row.get("semantic_score", 0.0) or 0.0)
    behavior = row.get("behavior") if isinstance(row.get("behavior"), dict) else {}
    static_gaps = row.get("static_similarity_gaps") if isinstance(row.get("static_similarity_gaps"), dict) else {}
    missing_total = int(static_gaps.get("missing_feature_total") or 0)
    if row.get("mapping_status") != "matched":
        severity = 0
    elif not row.get("decomp_success"):
        severity = 1
    elif behavior.get("status") not in {"pass", "unsupported_signature"}:
        severity = 2
    elif missing_total > 0:
        severity = 3
    else:
        severity = 4
    return (severity, score, -missing_total, str(row.get("function_name") or ""))


def triage_row_summary(row: dict[str, Any]) -> dict[str, Any]:
    behavior = row.get("behavior") if isinstance(row.get("behavior"), dict) else {}
    static_gaps = row.get("static_similarity_gaps") if isinstance(row.get("static_similarity_gaps"), dict) else {}
    debug_decomp = row.get("debug_decomp") if isinstance(row.get("debug_decomp"), dict) else {}
    return {
        "entry_id": row.get("entry_id"),
        "function_name": row.get("function_name"),
        "address": row.get("address"),
        "semantic_score_percent": row.get("semantic_score_percent"),
        "static_semantic_score_percent": row.get("static_semantic_score_percent"),
        "static_similarity_source_variant": row.get("static_similarity_source_variant"),
        "static_semantic_score_direct_percent": percent(float(row.get("static_semantic_score_direct", 0.0) or 0.0)),
        "static_semantic_score_inline_expanded_percent": percent(
            float(row.get("static_semantic_score_inline_expanded", 0.0) or 0.0)
        ),
        "source_static_feature_count_direct": row.get("source_static_feature_count_direct"),
        "source_static_feature_count_inline_expanded": row.get("source_static_feature_count_inline_expanded"),
        "mapping_status": row.get("mapping_status"),
        "decomp_success": bool(row.get("decomp_success")),
        "decomp_failure_kind": row.get("decomp_failure_kind"),
        "behavior_status": behavior.get("status"),
        "case_pass_count": behavior.get("case_pass_count"),
        "case_fail_count": behavior.get("case_fail_count"),
        "first_mismatch_index": behavior.get("first_mismatch_index"),
        "zero_credit_reason": row_zero_credit_reason(row)
        if float(row.get("semantic_score", 0.0) or 0.0) == 0.0
        else row.get("zero_credit_reason"),
        "stage_first_failure": row.get("stage_first_failure"),
        "debug_owner_buckets": debug_decomp.get("owner_buckets") if isinstance(debug_decomp, dict) else None,
        "missing_feature_total": static_gaps.get("missing_feature_total"),
        "extra_feature_total": static_gaps.get("extra_feature_total"),
        "top_missing_features": (static_gaps.get("top_missing_features") or [])[:5],
        "top_extra_features": (static_gaps.get("top_extra_features") or [])[:5],
        "debug_decomp_bundle_path": row.get("debug_decomp_bundle_path"),
        "behavior_artifact_dir": behavior.get("artifact_dir"),
    }


def compare_ghidra_reference(
    fission_summary: dict[str, Any],
    fission_rows: list[dict[str, Any]],
    ghidra_summary: dict[str, Any],
    ghidra_rows: list[dict[str, Any]],
) -> dict[str, Any]:
    fission_by_key = {row_key(row): row for row in fission_rows}
    ghidra_by_key = {row_key(row): row for row in ghidra_rows}
    shared_keys = sorted(set(fission_by_key) & set(ghidra_by_key))
    fission_only = sorted(set(fission_by_key) - set(ghidra_by_key))
    ghidra_only = sorted(set(ghidra_by_key) - set(fission_by_key))
    buckets: Counter[str] = Counter()
    deltas: list[dict[str, Any]] = []
    for key in shared_keys:
        fission = fission_by_key[key]
        ghidra = ghidra_by_key[key]
        fission_score = float(fission.get("semantic_score", 0.0) or 0.0)
        ghidra_score = float(ghidra.get("semantic_score", 0.0) or 0.0)
        delta = round(fission_score - ghidra_score, 6)
        fission_behavior = fission.get("behavior", {}).get("status")
        ghidra_behavior = ghidra.get("behavior", {}).get("status")
        if fission.get("mapping_status") != "matched" and ghidra.get("mapping_status") == "matched":
            bucket = "fission_failed_only"
        elif fission.get("mapping_status") == "matched" and ghidra.get("mapping_status") != "matched":
            bucket = "ghidra_failed_only"
        elif fission_score >= 0.999 and ghidra_score >= 0.999:
            bucket = "both_good"
        elif fission_score <= 0.0 and ghidra_score <= 0.0:
            bucket = "both_bad"
        elif delta > 0:
            bucket = "fission_ahead"
        elif delta < 0:
            bucket = "ghidra_ahead"
        else:
            bucket = "tied"
        buckets[bucket] += 1
        deltas.append(
            {
                "key": key,
                "entry_id": fission.get("entry_id"),
                "function_name": fission.get("function_name"),
                "fission_score": fission_score,
                "ghidra_score": ghidra_score,
                "delta": delta,
                "fission_score_percent": percent(fission_score),
                "ghidra_score_percent": percent(ghidra_score),
                "delta_percent": percent(delta),
                "fission_behavior": fission_behavior,
                "ghidra_behavior": ghidra_behavior,
                "fission_mapping_status": fission.get("mapping_status"),
                "ghidra_mapping_status": ghidra.get("mapping_status"),
                "bucket": bucket,
            }
        )
    deltas.sort(key=lambda row: (abs(float(row.get("delta", 0.0) or 0.0)), row.get("function_name") or ""), reverse=True)
    return {
        "contract": "reference lane only; Ghidra is compared against source and Fission but is not used as the oracle",
        "row_count": len(shared_keys),
        "fission_only_row_count": len(fission_only),
        "ghidra_only_row_count": len(ghidra_only),
        "fission_weighted_semantic_similarity_percent": fission_summary.get("weighted_semantic_similarity_percent"),
        "ghidra_weighted_semantic_similarity_percent": ghidra_summary.get("weighted_semantic_similarity_percent"),
        "weighted_semantic_similarity_delta_percent": round(
            float(fission_summary.get("weighted_semantic_similarity_percent", 0.0) or 0.0)
            - float(ghidra_summary.get("weighted_semantic_similarity_percent", 0.0) or 0.0),
            6,
        ),
        "bucket_counts": dict(sorted(buckets.items())),
        "top_fission_ahead": [row for row in deltas if float(row.get("delta", 0.0) or 0.0) > 0.0][:12],
        "top_ghidra_ahead": [row for row in deltas if float(row.get("delta", 0.0) or 0.0) < 0.0][:12],
        "top_absolute_deltas": deltas[:20],
    }


def canonical_sleigh_template_source(source: str) -> str:
    if source in {"spec_derived", "SpecDerived"}:
        return "sla_construct_tpl"
    return source


def sleigh_template_source_gate(summary: dict[str, Any], required_source: str) -> dict[str, Any]:
    raw_template_totals = summary.get("debug_template_source_totals")
    if not isinstance(raw_template_totals, dict):
        raw_template_totals = {}
    template_totals: dict[str, int] = {}
    for source, value in raw_template_totals.items():
        if isinstance(value, int | float):
            canonical = canonical_sleigh_template_source(str(source))
            template_totals[canonical] = template_totals.get(canonical, 0) + int(value)
    stage_counts = summary.get("debug_stage_status_counts")
    if not isinstance(stage_counts, dict):
        stage_counts = {}
    quality_totals = summary.get("debug_quality_evidence_totals")
    if not isinstance(quality_totals, dict):
        quality_totals = {}
    sleigh_health = summary.get("sleigh_lift_health_metrics")
    if not isinstance(sleigh_health, dict):
        sleigh_health = {}
    nir_stats = summary.get("nir_build_stats_metrics")
    if not isinstance(nir_stats, dict):
        nir_stats = {}
    nir_numeric_totals = nir_stats.get("numeric_totals")
    if not isinstance(nir_numeric_totals, dict):
        nir_numeric_totals = {}

    failures: list[str] = []
    row_count = int(summary.get("row_count", 0) or 0)
    mapping_counts = summary.get("mapping_status_counts")
    if not isinstance(mapping_counts, dict):
        mapping_counts = {}
    mapped_row_count = int(mapping_counts.get("matched", row_count) or 0)
    unmapped_row_count = max(0, row_count - mapped_row_count)
    decode_ok = int(stage_counts.get("decode:ok", 0) or 0)
    raw_pcode_ok = int(stage_counts.get("raw_pcode:ok", 0) or 0)
    invalid_pcode_shape_count = int(quality_totals.get("invalid_pcode_shape_count", 0) or 0)
    raw_pcode_compat_import_count = int(
        sleigh_health.get(
            "raw_pcode_compat_import_total",
            nir_numeric_totals.get("raw_pcode_compat_import_count", 0),
        )
        or 0
    )
    total_templates = sum(
        int(value) for value in template_totals.values() if isinstance(value, int | float)
    )
    failed_sleigh_stages = {
        stage: int(value)
        for stage, value in stage_counts.items()
        if (
            stage.startswith("decode:")
            or stage.startswith("raw_pcode:")
        )
        and stage not in {"decode:ok", "raw_pcode:ok"}
        and isinstance(value, int | float)
        and int(value) != 0
    }
    unexpected_sources = {
        source: int(value)
        for source, value in template_totals.items()
        if source != required_source and isinstance(value, int | float) and int(value) != 0
    }

    if mapped_row_count > 0 and total_templates == 0:
        failures.append(
            "SLEIGH template source gate requires debug_template_source_totals; run with --include-debug-decomp"
        )
    if raw_pcode_ok > 0 and total_templates < raw_pcode_ok:
        failures.append(
            f"SLEIGH template source evidence must cover every raw_pcode:ok row ({total_templates}/{raw_pcode_ok})"
        )
    if mapped_row_count > 0 and decode_ok != mapped_row_count:
        failures.append(
            f"SLEIGH decode must be ok for every mapped row ({decode_ok}/{mapped_row_count})"
        )
    if mapped_row_count > 0 and raw_pcode_ok != mapped_row_count:
        failures.append(
            f"SLEIGH raw_pcode must be ok for every mapped row ({raw_pcode_ok}/{mapped_row_count})"
        )
    if unexpected_sources:
        failures.append(
            f"SLEIGH template sources must be only {required_source!r} "
            f"(unexpected {unexpected_sources})"
        )
    if failed_sleigh_stages:
        failures.append(f"SLEIGH decode/raw_pcode stages must be ok (got {failed_sleigh_stages})")
    if invalid_pcode_shape_count != 0:
        failures.append(f"SLEIGH invalid_pcode_shape_count must be 0 (got {invalid_pcode_shape_count})")
    if raw_pcode_compat_import_count != 0:
        failures.append(
            "SLEIGH raw_pcode_compat_import_count must be 0 "
            f"(got {raw_pcode_compat_import_count})"
        )

    return {
        "required_source": required_source,
        "status": "passed" if not failures else "failed",
        "failures": failures,
        "template_source_totals": dict(sorted(template_totals.items())),
        "template_source_count": total_templates,
        "row_count": row_count,
        "mapped_row_count": mapped_row_count,
        "unmapped_row_count": unmapped_row_count,
        "decode_ok_rows": decode_ok,
        "raw_pcode_ok_rows": raw_pcode_ok,
        "invalid_pcode_shape_count": invalid_pcode_shape_count,
        "raw_pcode_compat_import_count": raw_pcode_compat_import_count,
    }


def complexity_bucket(value: float) -> str:
    if value <= 5:
        return "tiny"
    if value <= 15:
        return "small"
    if value <= 40:
        return "medium"
    return "large"


def feature_gap_bucket(value: float) -> str:
    if value <= 0:
        return "none"
    if value <= 5:
        return "small"
    if value <= 20:
        return "medium"
    return "large"


def cost_bucket(seconds: float) -> str:
    if seconds <= 0.1:
        return "fast"
    if seconds <= 1.0:
        return "normal"
    if seconds <= 5.0:
        return "slow"
    return "very_slow"


def behavior_failure_owner(status: str) -> str:
    if status.startswith("oracle_"):
        return "oracle"
    if status.startswith("candidate_"):
        return "candidate"
    if status in {"decomp_failed", "unsupported_signature", "host_execution_unavailable"}:
        return status
    if status == "mismatch":
        return "semantic_mismatch"
    if status == "pass":
        return "pass"
    return "unknown"


def behavior_detail_signature(detail: Any) -> str:
    if not isinstance(detail, str) or not detail.strip():
        return "none"
    lines = [line.strip() for line in detail.splitlines() if line.strip()]
    if not lines:
        return "none"
    def stable_detail_line(line: str) -> str:
        line = re.sub(r".*/(candidate|oracle)\.c:", r"\1.c:", line)
        line = re.sub(r"/(?:private/)?(?:tmp|var)/[^\s:]+", "<tmp>", line)
        return re.sub(r"\s+", " ", line)[-240:]

    for line in lines:
        if "error:" in line or "undefined reference" in line or "undeclared" in line:
            return stable_detail_line(line)
    return stable_detail_line(lines[-1])


def furthest_ok_stage(stage_status: Any) -> str:
    if not isinstance(stage_status, dict):
        return "missing"
    furthest = "none"
    for stage in STAGE_FAILURE_ORDER:
        status = stage_status.get(stage)
        if status is None:
            continue
        if status != "ok":
            break
        furthest = stage
    return furthest


def extract_nesting_profile(code: str) -> list[tuple[str, int]]:
    stripped = strip_comments(code)
    profile = []
    depth = 0
    pattern = re.compile(r'\b(if|while|for|switch|case|break|continue|goto)\b|[{}]')
    for match in pattern.finditer(stripped):
        tok = match.group(0)
        if tok == '{':
            depth += 1
        elif tok == '}':
            depth = max(0, depth - 1)
        else:
            profile.append((tok, depth))
    return profile


def cfg_topological_similarity(source_code: str, decomp_code: str) -> float:
    src_profile = extract_nesting_profile(source_code)
    decomp_profile = extract_nesting_profile(decomp_code)
    
    if not src_profile and not decomp_profile:
        return 1.0
        
    src_kws = Counter(kw for kw, _ in src_profile)
    decomp_kws = Counter(kw for kw, _ in decomp_profile)
    kw_similarity = multiset_jaccard(src_kws, decomp_kws)
    
    src_nodes = Counter(src_profile)
    decomp_nodes = Counter(decomp_profile)
    node_similarity = multiset_jaccard(src_nodes, decomp_nodes)
    
    max_src_depth = max((d for _, d in src_profile), default=0)
    max_decomp_depth = max((d for _, d in decomp_profile), default=0)
    if max_src_depth == 0 and max_decomp_depth == 0:
        depth_similarity = 1.0
    else:
        depth_similarity = 1.0 - abs(max_src_depth - max_decomp_depth) / max(1, max_src_depth, max_decomp_depth)
        
    return round(0.4 * kw_similarity + 0.4 * node_similarity + 0.2 * depth_similarity, 6)


def temp_var_suppression_score(decomp_code: str) -> float:
    pattern = re.compile(r'\b(?:[ui]|pv|l|s)?Var\d+\b|\b(?:tmp|temp)\d*\b')
    matches = pattern.findall(decomp_code)
    unique_temps = len(set(matches))
    if unique_temps == 0:
        return 1.0
    return max(0.0, round(1.0 - (unique_temps * 0.1), 4))


def line_efficiency_score(source_code: str, decomp_code: str) -> float:
    def clean_lines(code: str) -> list[str]:
        stripped = strip_comments(code)
        return [l.strip() for l in stripped.splitlines() if l.strip()]
    src_lines = len(clean_lines(source_code))
    decomp_lines = len(clean_lines(decomp_code))
    if src_lines == 0:
        return 1.0 if decomp_lines == 0 else 0.0
    ratio = decomp_lines / src_lines
    return max(0.0, round(1.0 - abs(ratio - 1.0), 4))


def compute_readability_score(source_code: str, decomp_code: str) -> float:
    cfg_sim = cfg_topological_similarity(source_code, decomp_code)
    temp_supp = temp_var_suppression_score(decomp_code)
    line_eff = line_efficiency_score(source_code, decomp_code)
    return round(0.5 * cfg_sim + 0.3 * temp_supp + 0.2 * line_eff, 6)


def compute_semantic_correctness_score(behavior: dict[str, Any]) -> float:
    status = behavior.get("status")
    if status == "pass":
        return 1.0
    elif status == "mismatch":
        return float(behavior.get("case_pass_rate", 0.0))
    else:
        return 0.0


def generate_ai_remedy_hints(
    source_code: str,
    decomp_code: str | None,
    behavior: dict[str, Any],
) -> dict[str, Any]:
    cfg_similarity = cfg_topological_similarity(source_code, decomp_code or "")
    temp_supp = temp_var_suppression_score(decomp_code or "")
    line_efficiency = line_efficiency_score(source_code, decomp_code or "")
    
    readability_score = compute_readability_score(source_code, decomp_code or "")
    semantic_correctness_score = compute_semantic_correctness_score(behavior)
    
    if semantic_correctness_score >= 1.0 and readability_score >= 0.8:
        discrepancy_type = "perfect_alignment"
        ai_guidance = "Success: The decompiled code is both semantically correct and clean/readable. No actions needed."
    elif semantic_correctness_score >= 1.0 and readability_score < 0.8:
        discrepancy_type = "semantic_match_readability_gap"
        ai_guidance = (
            "Warning: The decompiled code has correct semantic behavior (passes all test/fuzz cases) "
            "but suffers from poor readability. Focus on AST structuring, variable renaming, copy propagation, "
            "and reducing redundant control flow / loops to match the readability of the source."
        )
    elif semantic_correctness_score < 1.0 and readability_score >= 0.8:
        discrepancy_type = "readability_ok_semantic_bug"
        ai_guidance = (
            "Critical: The decompiled code is clean and highly readable, but is semantically INCORRECT "
            "(fails test/fuzz cases or has sanitizer/memory errors). This is a silent logic bug "
            "(e.g., incorrect branch conditions, sign extension errors, off-by-one bounds, or wrong variable mappings). "
            "Do NOT waste effort on code formatting; debug the data-flow, SSA definitions, and expression folding logic."
        )
    else:
        discrepancy_type = "both_failed"
        ai_guidance = (
            "Critical: The decompiled code is both semantically incorrect and unreadable. "
            "Check for failure in control-flow recovery and major AST construction bugs."
        )
        
    hints = {
        "status": behavior.get("status", "unknown"),
        "has_sanitizer_error": behavior.get("status") == "run_sanitizer_error",
        "sanitizer_details": None,
        "cfg_similarity": cfg_similarity,
        "cfg_mismatches": [],
        "excessive_nesting": False,
        "suggested_actions": [],
        "readability_score": readability_score,
        "semantic_correctness_score": semantic_correctness_score,
        "readability_vs_correctness_assessment": {
            "discrepancy_type": discrepancy_type,
            "ai_guidance": ai_guidance
        }
    }
    
    if behavior.get("status") == "run_sanitizer_error":
        detail = behavior.get("detail", "")
        san_match = re.search(r'(?:AddressSanitizer|UndefinedBehaviorSanitizer|runtime error):\s*(.*)', detail)
        hints["sanitizer_details"] = san_match.group(0) if san_match else detail[:200]
        hints["suggested_actions"].append("Investigate memory safety, buffer overflows, or undefined behavior in pointer arithmetic.")
        
    if behavior.get("status") == "run_tle":
        hints["suggested_actions"].append("Check for infinite loops or incorrect loop exit conditions in structured control flow.")
        
    if behavior.get("status") == "run_mle":
        hints["suggested_actions"].append("Check for unbounded memory allocation or infinite recursion causing stack overflow.")

    if decomp_code:
        src_profile = extract_nesting_profile(source_code)
        decomp_profile = extract_nesting_profile(decomp_code)
        
        src_kws = Counter(kw for kw, _ in src_profile)
        decomp_kws = Counter(kw for kw, _ in decomp_profile)
        
        all_kws = set(src_kws.keys()) | set(decomp_kws.keys())
        for kw in sorted(all_kws):
            diff = decomp_kws[kw] - src_kws[kw]
            if diff > 0:
                hints["cfg_mismatches"].append(f"extra_{kw}_count: {diff}")
            elif diff < 0:
                hints["cfg_mismatches"].append(f"missing_{kw}_count: {abs(diff)}")
                
        max_src_depth = max((d for _, d in src_profile), default=0)
        max_decomp_depth = max((d for _, d in decomp_profile), default=0)
        if max_decomp_depth > max_src_depth + 1:
            hints["excessive_nesting"] = True
            hints["suggested_actions"].append(f"Decompiled code has excessive nesting depth ({max_decomp_depth}) compared to source ({max_src_depth}). Simplify block structuring.")
            
        if temp_supp < 0.8:
            hints["suggested_actions"].append("Reduce temporary variables by applying aggressive variable copy propagation.")
            
        if line_efficiency < 0.7:
            hints["suggested_actions"].append("Improve line conciseness; decompiled code is either too verbose or missing logic.")
            
        if cfg_similarity < 0.9:
            hints["suggested_actions"].append("Refactor control flow structuring algorithms to match the source control graph structure.")
            
    else:
        hints["suggested_actions"].append("Fix decompiler crash or listing failure to produce decompiled C code.")
        
    return hints

