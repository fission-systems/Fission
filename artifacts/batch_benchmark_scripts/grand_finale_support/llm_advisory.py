from __future__ import annotations

import json
import os
import time
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any

DEFAULT_LM_STUDIO_BASE_URL = "http://127.0.0.1:1234/v1"
DEFAULT_LM_STUDIO_MODEL = "nvidia/nemotron-3-nano-4b"
DEFAULT_LM_STUDIO_TIMEOUT_SECS = 30
DEFAULT_LM_STUDIO_MAX_TOKENS = 700
DEFAULT_LM_STUDIO_TEMPERATURE = 0.1
PROMPT_VERSION = "v1"
MAX_ROW_EXAMPLES = 5
MAX_FAMILY_ROWS = 8
ENV_ENABLE = "FISSION_BENCHMARK_LLM_ENABLE"
ENV_BASE_URL = "FISSION_BENCHMARK_LLM_BASE_URL"
ENV_MODEL = "FISSION_BENCHMARK_LLM_MODEL"
ENV_TIMEOUT = "FISSION_BENCHMARK_LLM_TIMEOUT_SECS"
ENV_MAX_TOKENS = "FISSION_BENCHMARK_LLM_MAX_TOKENS"
ENV_TEMPERATURE = "FISSION_BENCHMARK_LLM_TEMPERATURE"


def llm_advisory_enabled() -> bool:
    return _env_truthy(os.getenv(ENV_ENABLE))


def maybe_generate_benchmark_llm_advisory(
    *,
    output_dir: Path,
    summary_json_path: Path,
    delta_json_path: Path | None = None,
    regression_gate_json_path: Path | None = None,
) -> dict[str, Any] | None:
    if not llm_advisory_enabled():
        return None

    output_dir = output_dir.resolve()
    summary_md_path = output_dir / "benchmark_llm_summary.md"
    summary_meta_path = output_dir / "benchmark_llm_summary.json"
    input_json_path = output_dir / "benchmark_llm_input.json"
    generated_at = time.strftime("%Y-%m-%d %H:%M:%S")

    metadata: dict[str, Any] = {
        "generated_at": generated_at,
        "prompt_version": PROMPT_VERSION,
        "model": os.getenv(ENV_MODEL, DEFAULT_LM_STUDIO_MODEL),
        "endpoint": _normalize_base_url(os.getenv(ENV_BASE_URL, DEFAULT_LM_STUDIO_BASE_URL)),
        "success": False,
        "input_file_path": str(input_json_path),
        "output_file_path": str(summary_md_path),
        "summary_json_source": str(summary_json_path.resolve()),
    }
    if delta_json_path is not None:
        metadata["delta_json_source"] = str(delta_json_path.resolve())
    if regression_gate_json_path is not None:
        metadata["regression_gate_json_source"] = str(regression_gate_json_path.resolve())

    try:
        summary_payload = _load_json(summary_json_path)
        delta_payload = _load_optional_json(delta_json_path)
        regression_gate_payload = _load_optional_json(regression_gate_json_path)

        llm_input = build_benchmark_llm_input(
            summary_payload=summary_payload,
            summary_json_path=summary_json_path,
            delta_payload=delta_payload,
            delta_json_path=delta_json_path,
            regression_gate_payload=regression_gate_payload,
            regression_gate_json_path=regression_gate_json_path,
        )
        with input_json_path.open("w", encoding="utf-8") as handle:
            json.dump(llm_input, handle, indent=2)

        markdown = _generate_llm_markdown(llm_input)
        summary_md_path.write_text(markdown.rstrip() + "\n", encoding="utf-8")

        metadata["success"] = True
        metadata["summary_kind"] = llm_input.get("summary_kind")
        metadata["same_axis_comparable"] = llm_input.get("same_axis_comparable")
    except Exception as exc:
        metadata["error"] = str(exc)
        summary_md_path.write_text(
            "\n".join(
                [
                    "# Benchmark LLM Advisory Summary",
                    "",
                    "## Status",
                    "",
                    "- Advisory generation failed.",
                    f"- Reason: `{exc}`",
                    "",
                    "Deterministic benchmark artifacts remain canonical.",
                ]
            )
            + "\n",
            encoding="utf-8",
        )

    with summary_meta_path.open("w", encoding="utf-8") as handle:
        json.dump(metadata, handle, indent=2)
    return metadata


def build_benchmark_llm_input(
    *,
    summary_payload: dict[str, Any],
    summary_json_path: Path,
    delta_payload: dict[str, Any] | None,
    delta_json_path: Path | None,
    regression_gate_payload: dict[str, Any] | None,
    regression_gate_json_path: Path | None,
) -> dict[str, Any]:
    if "summary" in summary_payload:
        return _build_single_benchmark_llm_input(
            summary_payload=summary_payload,
            summary_json_path=summary_json_path,
            delta_payload=delta_payload,
            delta_json_path=delta_json_path,
            regression_gate_payload=regression_gate_payload,
            regression_gate_json_path=regression_gate_json_path,
        )
    if "corpus_summary" in summary_payload:
        return _build_corpus_benchmark_llm_input(
            summary_payload=summary_payload,
            summary_json_path=summary_json_path,
        )
    raise ValueError(f"Unsupported benchmark summary shape: {summary_json_path}")


def _build_single_benchmark_llm_input(
    *,
    summary_payload: dict[str, Any],
    summary_json_path: Path,
    delta_payload: dict[str, Any] | None,
    delta_json_path: Path | None,
    regression_gate_payload: dict[str, Any] | None,
    regression_gate_json_path: Path | None,
) -> dict[str, Any]:
    summary = summary_payload.get("summary", {})
    quality = ((summary.get("quality") or {}).get("pyghidra_vs_fission") or {})
    engines = summary.get("engines", {}) or {}
    fission_engine = engines.get("fission", {}) or {}
    row_gate = (
        regression_gate_payload.get("row_fidelity_gate", {})
        if isinstance(regression_gate_payload, dict)
        else {}
    )
    same_axis_comparable = bool(regression_gate_payload)
    incomparable_reason = None if same_axis_comparable else "no baseline regression artifact present"

    top_regressions = []
    if isinstance(delta_payload, dict):
        top_regressions = _extract_top_degraded_rows(delta_payload.get("degraded_functions"))
    if not top_regressions and isinstance(regression_gate_payload, dict):
        top_regressions = _extract_top_degraded_rows(
            regression_gate_payload.get("top_degraded_functions")
        )

    top_recoveries = _extract_row_gate_improvements(row_gate)
    row_examples = _extract_single_row_examples(
        summary_payload=summary_payload,
        delta_payload=delta_payload,
        regression_gate_payload=regression_gate_payload,
    )

    return {
        "summary_kind": "single_benchmark",
        "policy_context": {
            "same_axis_comparable": same_axis_comparable,
            "incomparable_reason": incomparable_reason,
            "release_advisory_only": True,
            "gate_decision_must_remain_deterministic": True,
        },
        "run_metadata": {
            "artifact_path": str(summary_json_path.resolve()),
            "benchmark_summary_markdown_path": str(summary_json_path.with_suffix(".md").resolve()),
            "binary": summary.get("binary"),
            "generated_at": summary.get("generated_at"),
            "run_profile": None,
            "function_count": fission_engine.get("function_count"),
            "baseline_artifact_path": (
                str(regression_gate_json_path.resolve()) if regression_gate_json_path else None
            ),
            "delta_artifact_path": str(delta_json_path.resolve()) if delta_json_path else None,
        },
        "core_metrics": {
            "avg_normalized_similarity": {
                "current": _safe_float(quality.get("avg_normalized_similarity")),
                "previous": _extract_metric_previous(
                    delta_payload, "avg_normalized_similarity_pct"
                ),
                "delta": _extract_metric_delta(delta_payload, "avg_normalized_similarity_pct"),
            },
            "aggregate_normalized_similarity": _safe_float(
                quality.get("aggregate_normalized_similarity")
            ),
            "both_success_count": _safe_int(quality.get("both_success_count")),
            "shared_count": _safe_int(quality.get("shared_count")),
            "failed_rows": row_gate.get("failed_targets", []) if isinstance(row_gate, dict) else [],
            "recovered_rows": [item.get("address") for item in top_recoveries if item.get("address")],
        },
        "family_summaries": {
            "dominant_failure_families": _extract_regression_reason_families(regression_gate_payload),
            "dominant_recovered_families": _extract_recovery_reason_families(top_recoveries),
            "known_counters": {
                "goto_total": _safe_int(fission_engine.get("goto_total")),
                "top_level_label_total": _safe_int(fission_engine.get("top_level_label_total")),
                "materialization_stabilized_count": _safe_int(
                    fission_engine.get("materialization_stabilized_count")
                ),
                "proof_payload_direct_emit_count": _safe_int(
                    fission_engine.get("proof_payload_direct_emit_count")
                ),
                "guarded_tail_promoted_count": _safe_int(
                    fission_engine.get("guarded_tail_promoted_count")
                ),
            },
        },
        "top_regressions": top_regressions,
        "top_recoveries": top_recoveries,
        "row_examples": row_examples,
    }


def _build_corpus_benchmark_llm_input(
    *,
    summary_payload: dict[str, Any],
    summary_json_path: Path,
) -> dict[str, Any]:
    corpus = summary_payload.get("corpus_summary", {}) or {}
    binaries = summary_payload.get("binaries", []) or []
    row_gate_per_binary = summary_payload.get("row_fidelity_gate_per_binary", {}) or {}
    comparable_rows = [
        gate
        for gate in row_gate_per_binary.values()
        if isinstance(gate, dict) and gate.get("status") not in {"no_baseline", "report_only"}
    ]
    same_axis_comparable = bool(comparable_rows)
    incomparable_reason = None if same_axis_comparable else "no comparable corpus baseline details present"

    return {
        "summary_kind": "corpus_benchmark",
        "policy_context": {
            "same_axis_comparable": same_axis_comparable,
            "incomparable_reason": incomparable_reason,
            "release_advisory_only": True,
            "gate_decision_must_remain_deterministic": True,
        },
        "run_metadata": {
            "artifact_path": str(summary_json_path.resolve()),
            "benchmark_summary_markdown_path": str(summary_json_path.with_suffix(".md").resolve()),
            "generated_at": summary_payload.get("generated_at"),
            "run_profile": None,
            "function_count": _safe_int(corpus.get("shared_function_count")),
            "baseline_artifact_path": None,
        },
        "core_metrics": {
            "avg_normalized_similarity": {
                "current": _safe_float(corpus.get("weighted_avg_normalized_similarity")),
                "previous": None,
                "delta": None,
            },
            "release_candidate_count": _safe_int(corpus.get("release_candidate_count")),
            "release_eligible_count": _safe_int(corpus.get("release_eligible_count")),
            "failed_rows": _extract_corpus_failed_targets(row_gate_per_binary),
            "recovered_rows": [],
        },
        "family_summaries": {
            "dominant_failure_families": _top_count_rows(
                summary_payload.get("failure_family_distribution"), MAX_FAMILY_ROWS
            ),
            "dominant_recovered_families": [],
            "known_counters": {
                "binary_count": _safe_int(corpus.get("binary_count")),
                "release_candidate_count": _safe_int(corpus.get("release_candidate_count")),
                "release_eligible_count": _safe_int(corpus.get("release_eligible_count")),
                "direct_success_non_worse_count": _safe_int(
                    corpus.get("direct_success_non_worse_count")
                ),
            },
        },
        "top_regressions": _extract_corpus_top_regressions(
            summary_payload.get("cross_binary_degraded_watchlist")
        ),
        "top_recoveries": [],
        "row_examples": _extract_corpus_row_examples(binaries),
    }


def _generate_llm_markdown(llm_input: dict[str, Any]) -> str:
    base_url = _normalize_base_url(os.getenv(ENV_BASE_URL, DEFAULT_LM_STUDIO_BASE_URL))
    model = os.getenv(ENV_MODEL, DEFAULT_LM_STUDIO_MODEL)
    timeout_secs = _safe_int(os.getenv(ENV_TIMEOUT), DEFAULT_LM_STUDIO_TIMEOUT_SECS)
    max_tokens = _safe_int(os.getenv(ENV_MAX_TOKENS), DEFAULT_LM_STUDIO_MAX_TOKENS)
    temperature = _safe_float(os.getenv(ENV_TEMPERATURE), DEFAULT_LM_STUDIO_TEMPERATURE)
    url = _chat_completions_url(base_url)

    payload = {
        "model": model,
        "temperature": temperature,
        "max_tokens": max_tokens,
        "messages": [
            {
                "role": "system",
                "content": (
                    "You are summarizing a deterministic benchmark artifact. "
                    "Use only the provided JSON fields. "
                    "Do not invent metrics, rows, or reasons. "
                    "Do not override or reinterpret the deterministic gate verdict. "
                    "Produce Markdown with exactly these sections in order: "
                    "## Same-Axis Comparability, "
                    "## Top Regressions, "
                    "## Top Recoveries, "
                    "## Dominant Failure Families, "
                    "## Likely Next Owner, "
                    "## Release Advisory Note."
                ),
            },
            {
                "role": "user",
                "content": (
                    "Summarize this benchmark artifact in concise Markdown.\n\n"
                    "```json\n"
                    + json.dumps(llm_input, indent=2, ensure_ascii=False)
                    + "\n```"
                ),
            },
        ],
    }
    request = urllib.request.Request(
        url,
        data=json.dumps(payload).encode("utf-8"),
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    try:
        with urllib.request.urlopen(request, timeout=timeout_secs) as response:
            raw_body = response.read().decode("utf-8")
    except urllib.error.URLError as exc:
        raise RuntimeError(f"LM Studio request failed: {exc}") from exc
    except TimeoutError as exc:
        raise RuntimeError("LM Studio request timed out") from exc

    try:
        response_payload = json.loads(raw_body)
    except json.JSONDecodeError as exc:
        raise RuntimeError("LM Studio returned non-JSON response") from exc

    content = _extract_chat_completion_content(response_payload)
    if not content.strip():
        raise RuntimeError("LM Studio returned empty summary content")
    return content


def _extract_chat_completion_content(payload: dict[str, Any]) -> str:
    choices = payload.get("choices")
    if not isinstance(choices, list) or not choices:
        raise RuntimeError("LM Studio response missing choices")
    message = choices[0].get("message", {})
    content = message.get("content")
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        parts: list[str] = []
        for item in content:
            if isinstance(item, dict) and isinstance(item.get("text"), str):
                parts.append(item["text"])
        return "".join(parts)
    raise RuntimeError("LM Studio response missing message content")


def _extract_single_row_examples(
    *,
    summary_payload: dict[str, Any],
    delta_payload: dict[str, Any] | None,
    regression_gate_payload: dict[str, Any] | None,
) -> list[dict[str, Any]]:
    examples: list[dict[str, Any]] = []

    if isinstance(delta_payload, dict):
        examples.extend(_extract_top_degraded_rows(delta_payload.get("degraded_functions")))
    if isinstance(regression_gate_payload, dict):
        row_gate = regression_gate_payload.get("row_fidelity_gate", {})
        if isinstance(row_gate, dict):
            for row in row_gate.get("rows", [])[:MAX_ROW_EXAMPLES]:
                if not isinstance(row, dict):
                    continue
                examples.append(
                    {
                        "address": row.get("address"),
                        "role": row.get("role"),
                        "status": row.get("status"),
                        "previous_normalized_similarity": row.get("previous_normalized_similarity"),
                        "current_normalized_similarity": row.get("current_normalized_similarity"),
                        "normalized_similarity_delta": row.get("normalized_similarity_delta"),
                        "reason_tags": row.get("failure_reasons", []),
                    }
                )
    low_rows = (
        ((summary_payload.get("summary") or {}).get("samples") or {}).get(
            "pyghidra_vs_fission_lowest_similarity", []
        )
    )
    for row in low_rows[:MAX_ROW_EXAMPLES]:
        if not isinstance(row, dict):
            continue
        examples.append(
            {
                "address": row.get("address"),
                "function_name": row.get("fission_name") or row.get("pyghidra_name"),
                "normalized_similarity": row.get("normalized_similarity"),
                "reason_tags": _row_reason_tags(row),
            }
        )
    return _dedupe_row_examples(examples, MAX_ROW_EXAMPLES)


def _extract_corpus_row_examples(binaries: list[dict[str, Any]]) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    for row in binaries[:MAX_ROW_EXAMPLES]:
        if not isinstance(row, dict):
            continue
        rows.append(
            {
                "binary_id": row.get("id"),
                "role": row.get("role"),
                "avg_normalized_similarity": row.get("avg_normalized_similarity"),
                "reason": (row.get("eligibility") or {}).get("reason"),
                "row_fidelity_gate_status": row.get("row_fidelity_gate_status"),
            }
        )
    return rows


def _extract_top_degraded_rows(container: Any) -> list[dict[str, Any]]:
    if not isinstance(container, dict):
        return []
    degraded = container.get("top_degraded", [])
    if not isinstance(degraded, list):
        return []
    rows: list[dict[str, Any]] = []
    for row in degraded[:MAX_ROW_EXAMPLES]:
        if not isinstance(row, dict):
            continue
        rows.append(
            {
                "address": row.get("address"),
                "function_name": row.get("fission_name") or row.get("pyghidra_name"),
                "previous_normalized_similarity": row.get("previous_normalized_similarity"),
                "current_normalized_similarity": row.get("current_normalized_similarity"),
                "normalized_similarity_delta": row.get("normalized_similarity_delta"),
                "reason_tags": row.get("reason_tags", []),
            }
        )
    return rows


def _extract_row_gate_improvements(row_gate: Any) -> list[dict[str, Any]]:
    if not isinstance(row_gate, dict):
        return []
    rows = row_gate.get("rows", [])
    if not isinstance(rows, list):
        return []
    improvements: list[dict[str, Any]] = []
    for row in rows:
        if not isinstance(row, dict) or row.get("status") != "improved":
            continue
        improvements.append(
            {
                "address": row.get("address"),
                "role": row.get("role"),
                "previous_normalized_similarity": row.get("previous_normalized_similarity"),
                "current_normalized_similarity": row.get("current_normalized_similarity"),
                "normalized_similarity_delta": row.get("normalized_similarity_delta"),
                "reason_tags": row.get("failure_reasons", []),
            }
        )
    return improvements[:MAX_ROW_EXAMPLES]


def _extract_regression_reason_families(report: Any) -> list[dict[str, Any]]:
    if not isinstance(report, dict):
        return []
    row_gate = report.get("row_fidelity_gate", {})
    rows = row_gate.get("rows", []) if isinstance(row_gate, dict) else []
    counts: dict[str, int] = {}
    regressions = report.get("regressions", [])
    for item in regressions if isinstance(regressions, list) else []:
        if isinstance(item, str):
            counts[item] = counts.get(item, 0) + 1
    for row in rows if isinstance(rows, list) else []:
        if not isinstance(row, dict):
            continue
        for reason in row.get("failure_reasons", []):
            if isinstance(reason, str):
                counts[reason] = counts.get(reason, 0) + 1
    return _top_count_rows(counts, MAX_FAMILY_ROWS)


def _extract_recovery_reason_families(rows: list[dict[str, Any]]) -> list[dict[str, Any]]:
    counts: dict[str, int] = {}
    for row in rows:
        for reason in row.get("reason_tags", []):
            if isinstance(reason, str):
                counts[reason] = counts.get(reason, 0) + 1
    return _top_count_rows(counts, MAX_FAMILY_ROWS)


def _extract_corpus_failed_targets(row_gate_per_binary: Any) -> list[str]:
    failed: list[str] = []
    if not isinstance(row_gate_per_binary, dict):
        return failed
    for binary_id, gate in row_gate_per_binary.items():
        if not isinstance(gate, dict):
            continue
        for address in gate.get("failed_targets", []):
            failed.append(f"{binary_id}:{address}")
    return failed


def _extract_corpus_top_regressions(cross_binary: Any) -> list[dict[str, Any]]:
    if not isinstance(cross_binary, list):
        return []
    rows: list[dict[str, Any]] = []
    for row in cross_binary[:MAX_ROW_EXAMPLES]:
        if not isinstance(row, dict):
            continue
        rows.append(
            {
                "binary_id": row.get("binary_id"),
                "address": row.get("address"),
                "function_name": row.get("fission_name") or row.get("pyghidra_name"),
                "previous_normalized_similarity": row.get("previous_normalized_similarity"),
                "current_normalized_similarity": row.get("current_normalized_similarity"),
                "normalized_similarity_delta": row.get("normalized_similarity_delta"),
                "reason_tags": row.get("reason_tags", []),
            }
        )
    return rows


def _extract_metric_previous(delta_payload: Any, key: str) -> float | None:
    row = _find_metric_row(delta_payload, key)
    return _safe_float(row.get("previous")) if row else None


def _extract_metric_delta(delta_payload: Any, key: str) -> float | None:
    row = _find_metric_row(delta_payload, key)
    return _safe_float(row.get("delta")) if row else None


def _find_metric_row(delta_payload: Any, key: str) -> dict[str, Any] | None:
    if not isinstance(delta_payload, dict):
        return None
    for row in delta_payload.get("metrics", []):
        if isinstance(row, dict) and row.get("key") == key:
            return row
    return None


def _row_reason_tags(row: dict[str, Any]) -> list[str]:
    tags = []
    if row.get("fission_has_unresolved_unsupported_indirect"):
        tags.append("unsupported_indirect")
    if row.get("fission_has_preserved_indirect_surface"):
        tags.append("preserved_indirect_surface")
    if row.get("fission_has_dispatcher_recovery"):
        tags.append("dispatcher_recovery")
    if row.get("fission_has_indirect_target_proof"):
        tags.append("indirect_target_proof")
    return tags


def _dedupe_row_examples(rows: list[dict[str, Any]], limit: int) -> list[dict[str, Any]]:
    seen: set[str] = set()
    deduped: list[dict[str, Any]] = []
    for row in rows:
        key = str(row.get("address") or row.get("binary_id") or row)
        if key in seen:
            continue
        seen.add(key)
        deduped.append(row)
        if len(deduped) >= limit:
            break
    return deduped


def _top_count_rows(values: Any, limit: int) -> list[dict[str, Any]]:
    if not isinstance(values, dict):
        return []
    rows = [{"name": key, "count": _safe_int(value)} for key, value in values.items()]
    rows.sort(key=lambda item: (-item["count"], item["name"]))
    return rows[:limit]


def _env_truthy(value: str | None) -> bool:
    if value is None:
        return False
    return value.strip().lower() in {"1", "true", "yes", "on"}


def _normalize_base_url(base_url: str) -> str:
    return base_url.rstrip("/")


def _chat_completions_url(base_url: str) -> str:
    normalized = _normalize_base_url(base_url)
    if normalized.endswith("/chat/completions"):
        return normalized
    if normalized.endswith("/v1"):
        return normalized + "/chat/completions"
    return normalized + "/v1/chat/completions"


def _load_json(path: Path) -> dict[str, Any]:
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def _load_optional_json(path: Path | None) -> dict[str, Any] | None:
    if path is None or not path.is_file():
        return None
    return _load_json(path)


def _safe_float(value: Any, default: float = 0.0) -> float:
    try:
        if value is None:
            return default
        return float(value)
    except (TypeError, ValueError):
        return default


def _safe_int(value: Any, default: int = 0) -> int:
    try:
        if value is None:
            return default
        return int(value)
    except (TypeError, ValueError):
        return default
