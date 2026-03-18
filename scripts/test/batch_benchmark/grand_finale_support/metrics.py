from __future__ import annotations

import json
import re
import statistics
from collections import Counter
from pathlib import Path
from typing import Any


def normalize_address(address: str) -> str:
    return address.lower().replace("0x", "").lstrip("0") or "0"


def load_struct_pointer_aliases(base_types_json: Path) -> dict[str, str]:
    items = json.loads(base_types_json.read_text())
    aliases: dict[str, str] = {}
    for item in items:
        name = item.get("name", "")
        if item.get("is_pointer") and name.startswith("LP") and len(name) > 2:
            aliases[name] = name[2:]
    return aliases


def compute_timing_stats(samples_sec: list[float]) -> dict[str, Any]:
    ms = [round(sample * 1000.0, 3) for sample in samples_sec]
    if not ms:
        return {
            "runs": 0,
            "min_ms": 0.0,
            "max_ms": 0.0,
            "avg_ms": 0.0,
            "median_ms": 0.0,
            "p95_ms": 0.0,
        }

    sorted_ms = sorted(ms)
    if len(sorted_ms) == 1:
        p95_ms = sorted_ms[0]
    else:
        index = min(max(int(round(0.95 * (len(sorted_ms) - 1))), 0), len(sorted_ms) - 1)
        p95_ms = sorted_ms[index]

    return {
        "runs": len(ms),
        "min_ms": round(min(ms), 3),
        "max_ms": round(max(ms), 3),
        "avg_ms": round(statistics.fmean(ms), 3),
        "median_ms": round(statistics.median(ms), 3),
        "p95_ms": round(p95_ms, 3),
    }


def count_regex(pattern: str, text: str) -> int:
    return len(re.findall(pattern, text, flags=re.MULTILINE))


def detect_embedded_failure(code: str) -> tuple[str, str] | None:
    stripped = code.lstrip()
    if stripped.startswith("// Decompilation failed:"):
        first = stripped.splitlines()[0].replace("// Decompilation failed:", "").strip()
        return classify_failure_kind(first), first
    if stripped.startswith("// Error:"):
        first = stripped.splitlines()[0].replace("// Error:", "").strip()
        return classify_failure_kind(first), first
    if stripped.startswith("// Assembly fallback:"):
        first = stripped.splitlines()[0].replace("// Assembly fallback:", "").strip()
        return classify_failure_kind(first), first
    return None


def classify_failure_kind(message: str | None) -> str:
    if not message:
        return "other"
    lower = message.lower()
    if "timeout" in lower:
        return "timeout"
    if "out of memory" in lower or "oom" in lower:
        return "oom"
    if "control flow" in lower or "followflow" in lower:
        return "control_flow"
    if "ptrsub" in lower or "printer" in lower or "print" in lower:
        return "printer"
    if (
        "duplicate variablepiece" in lower
        or "high-level" in lower
        or "structure" in lower
        or "type" in lower
        or "union" in lower
    ):
        return "type"
    return "other"


def extract_fallback_kind(reason: str | None) -> str | None:
    if not reason:
        return None
    prefix, _, _ = reason.partition(":")
    normalized = prefix.strip().lower()
    if normalized in {
        "preview_timeout",
        "preview_unsupported",
        "native_pcode_failure",
        "legacy_fallback",
        "assembly_fallback",
    }:
        return normalized
    return None


def collect_type_preservation_metrics(code: str, struct_ptr_aliases: dict[str, str]) -> dict[str, int]:
    hits: Counter[str] = Counter()
    signature = code.split("{", 1)[0]
    for alias, struct_name in struct_ptr_aliases.items():
        patterns = [
            rf"\b{re.escape(alias)}\b",
            rf"\b{re.escape(struct_name)}\s*\*",
            rf"\bstruct\s+{re.escape(struct_name)}\s*\*",
        ]
        if any(re.search(pattern, signature) for pattern in patterns):
            hits[alias] = 1
    return dict(hits)


def collect_code_metrics(code: str, struct_ptr_aliases: dict[str, str]) -> dict[str, Any]:
    must_emit_labels = set(
        re.findall(r"\bgoto\s+([A-Za-z_]\w*)\s*;", code, flags=re.MULTILINE)
    )
    metrics = {
        "goto_count": count_regex(r"\bgoto\s+[A-Za-z_]\w*\s*;", code),
        "top_level_label_count": count_regex(r"^block_[0-9a-fA-F]+:\s*$", code),
        "must_emit_label_count": len(must_emit_labels),
        "switch_count": count_regex(r"\bswitch\s*\(", code),
        "for_count": count_regex(r"\bfor\s*\(", code),
        "do_while_count": count_regex(r"\bdo\s*\{", code),
        "while_count": count_regex(r"\bwhile\s*\(", code),
        "unique_surface_count": count_regex(r"\bunique0x[0-9a-fA-F]+\b", code),
        "field_access_count": count_regex(r"->\w+(?:/\* @[^*]+ \*/)?", code),
        "offset_index_count": count_regex(r"\b\w+\[\s*(?:0x[0-9a-fA-F]+|\d+)\s*\]", code),
        "empty_if_count": len(
            re.findall(r"if\s*\([^)]*\)\s*\{\s*\n\s*\}", code, flags=re.MULTILINE)
        ),
        "constant_if_count": count_regex(r"\bif\s*\(\s*(?:0|1)\s*\)\s*\{", code),
    }
    metrics["single_pred_non_emitted_boundary_count"] = max(
        int(metrics["top_level_label_count"]) - int(metrics["must_emit_label_count"]),
        0,
    )

    type_hits: Counter[str] = Counter()
    for alias in struct_ptr_aliases:
        count = count_regex(rf"\b{re.escape(alias)}\b", code)
        if count:
            type_hits[alias] = count
    metrics["type_hits"] = dict(type_hits)
    metrics["type_preservation_hits"] = collect_type_preservation_metrics(code, struct_ptr_aliases)

    residue_patterns = {
        "uVar": r"\buVar\d+\b",
        "iVar": r"\biVar\d+\b",
        "xVar": r"\bxVar\d+\b",
        "bVar": r"\bbVar\d+\b",
        "uStack": r"\buStack_[0-9a-fA-F]+\b",
        "xStack": r"\bxStack_[0-9a-fA-F]+\b",
        "axStack": r"\baxStack_[0-9a-fA-F]+\b",
        "raw_pointer_fallback": r"\(\((?:uint8_t|byte|uint1)\s*\*\)[^)]+\+\s*[^)]+\)",
        "assembly_fallback": r"^// Assembly fallback:",
        "redundant_return_temp": r"^\s*[A-Za-z_][A-Za-z0-9_]*\s*=\s*[^;]+;\s*\n\s*return\s+[A-Za-z_][A-Za-z0-9_]*;\s*$",
    }
    metrics["residue_families"] = {
        family: count_regex(pattern, code) for family, pattern in residue_patterns.items()
    }

    metrics["fallback_counts"] = {
        "raw_pointer_fallback": metrics["residue_families"]["raw_pointer_fallback"],
        "assembly_fallback": metrics["residue_families"]["assembly_fallback"],
    }

    metrics["cast_chain_count"] = count_regex(
        r"\([A-Za-z_][A-Za-z0-9_\s\*]+\)\s*\([A-Za-z_][A-Za-z0-9_\s\*]+\)",
        code,
    )
    metrics["helper_call_counts"] = {
        "FLUSH_BITS": count_regex(r"\bFLUSH_BITS\s*\(", code),
        "WRITE_BITS": count_regex(r"\bWRITE_BITS\s*\(", code),
        "EMIT_CODE": count_regex(r"\bEMIT_CODE\s*\(", code),
    }
    metrics["helper_call_total"] = sum(metrics["helper_call_counts"].values())

    residue_names = re.findall(
        r"\b(?:[uibax]Var\d+|(?:u|x|ax)Stack_[0-9a-fA-F]+)\b",
        code,
    )
    metrics["residue_names"] = dict(Counter(residue_names).most_common(25))

    single_assign_lhs = re.findall(
        r"^\s*((?:[uibax]Var\d+|(?:u|x|ax)Stack_[0-9a-fA-F]+))\s*=\s*[^;]+;\s*$",
        code,
        flags=re.MULTILINE,
    )
    metrics["single_assign_temps"] = dict(Counter(single_assign_lhs).most_common(25))
    metrics["temp_surface_count"] = sum(
        int(metrics["residue_families"].get(key, 0))
        for key in ("uVar", "iVar", "xVar", "bVar", "uStack", "xStack", "axStack")
    )
    return metrics


def compute_residue_score(entry: dict[str, Any]) -> int:
    metrics = entry.get("metrics", {})
    families = metrics.get("residue_families", {})
    score = 0
    for key in ("uVar", "iVar", "xVar", "bVar", "uStack", "xStack", "axStack"):
        score += int(families.get(key, 0))
    score += int(families.get("raw_pointer_fallback", 0)) * 2
    score += int(families.get("redundant_return_temp", 0)) * 2
    score += sum(int(v) for v in metrics.get("single_assign_temps", {}).values())
    return score


def collect_top_residue_offenders(
    entries: dict[str, dict[str, Any]],
    limit: int = 5,
) -> list[dict[str, Any]]:
    offenders: list[dict[str, Any]] = []
    for entry in entries.values():
        if not entry.get("success"):
            continue
        metrics = entry.get("metrics", {})
        residue_names = metrics.get("residue_names", {})
        single_assign = metrics.get("single_assign_temps", {})
        raw_pointer_fallback = int(metrics.get("fallback_counts", {}).get("raw_pointer_fallback", 0))
        residue_score = compute_residue_score(entry)
        if residue_score <= 0 and raw_pointer_fallback <= 0:
            continue
        offenders.append(
            {
                "address": entry.get("address", ""),
                "name": entry.get("name", ""),
                "residue_score": residue_score,
                "raw_pointer_fallback": raw_pointer_fallback,
                "single_assign_temp_total": sum(int(v) for v in single_assign.values()),
                "top_residue_names": dict(Counter(residue_names).most_common(5)),
            }
        )

    offenders.sort(
        key=lambda item: (
            -int(item["residue_score"]),
            -int(item["raw_pointer_fallback"]),
            -int(item["single_assign_temp_total"]),
            item["address"],
        )
    )
    return offenders[:limit]
