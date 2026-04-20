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


GENERIC_LOCAL_PREFIXES = ("local_", "slot_", "uVar", "iVar", "puVar", "bVar", "cVar", "auVar", "temp_")
RAW_TYPE_NAMES = {
    "undefined",
    "int",
    "uint",
    "short",
    "ushort",
    "char",
    "uchar",
    "longlong",
    "ulonglong",
    "float",
    "double",
}


def _is_generic_function_name(name: str) -> bool:
    return name.startswith("FUN_") or name.startswith("sub_")


def _is_generic_param_name(name: str) -> bool:
    return name.startswith("param_")


def _is_generic_local_name(name: str) -> bool:
    return any(name.startswith(prefix) for prefix in GENERIC_LOCAL_PREFIXES)


def _is_raw_surface_type(type_name: str) -> bool:
    normalized = type_name.strip()
    if not normalized:
        return True
    if normalized in RAW_TYPE_NAMES:
        return True
    if re.fullmatch(r"fission_agg\d+", normalized):
        return True
    return False


def _split_type_and_name(fragment: str) -> tuple[str, str] | None:
    fragment = fragment.strip()
    if not fragment or fragment == "void":
        return None
    match = re.match(r"(.+?)\s+([A-Za-z_][A-Za-z0-9_]*)$", fragment)
    if not match:
        return None
    type_name = match.group(1).strip()
    name = match.group(2).strip()
    return type_name, name


def _strip_block_comments_for_scan(code: str) -> str:
    return re.sub(r"/\*.*?\*/", "", code, flags=re.DOTALL)


def approx_max_brace_nesting(code: str) -> int:
    """Rough max `{` nesting depth; skips `//`, block comments, and quoted strings on a line."""
    text = _strip_block_comments_for_scan(code)
    max_d = 0
    d = 0
    for raw_line in text.splitlines():
        line = raw_line.split("//", 1)[0]
        i = 0
        while i < len(line):
            ch = line[i]
            if ch in ('"', "'"):
                quote = ch
                j = i + 1
                while j < len(line):
                    if line[j] == "\\":
                        j += 2
                        continue
                    if line[j] == quote:
                        i = j + 1
                        break
                    j += 1
                else:
                    i += 1
                continue
            if ch == "{":
                d += 1
                max_d = max(max_d, d)
            elif ch == "}":
                d = max(0, d - 1)
            i += 1
    return max_d


def avg_non_empty_line_length(code: str) -> float:
    lines = [ln for ln in code.splitlines() if ln.strip()]
    if not lines:
        return 0.0
    return round(sum(len(ln) for ln in lines) / len(lines), 3)


def collect_quality_metrics(code: str) -> dict[str, Any]:
    fpu_op_count = len(re.findall(r'\b(float|double)\b|\w+\s*\.\s*fpu', code, re.IGNORECASE))
    jump_table_count = len(re.findall(r'\bswitch\s*\(', code))

    signature_match = re.search(
        r"^\s*(?P<ret>[A-Za-z_][A-Za-z0-9_\s\*]*?)\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)\((?P<params>[^)]*)\)",
        code,
        flags=re.MULTILINE,
    )
    function_name = signature_match.group("name") if signature_match else ""
    return_type = signature_match.group("ret").strip() if signature_match else ""
    params_blob = signature_match.group("params") if signature_match else ""

    named_param_count = 0
    generic_param_name_count = 0
    surfaced_param_type_count = 0
    for param in [part.strip() for part in params_blob.split(",") if part.strip()]:
        parsed = _split_type_and_name(param)
        if parsed is None:
            continue
        type_name, name = parsed
        if _is_generic_param_name(name):
            generic_param_name_count += 1
        else:
            named_param_count += 1
        if not _is_raw_surface_type(type_name):
            surfaced_param_type_count += 1

    named_local_count = 0
    generic_local_name_count = 0
    surfaced_local_type_count = 0
    slot_alias_count = 0
    for line in code.splitlines():
        stripped = line.strip()
        if not stripped or "(" in stripped:
            continue
        if stripped.startswith(("if ", "while ", "for ", "switch ", "return ", "goto ", "case ", "default:", "break;", "continue;", "}")):
            continue
        if not stripped.endswith(";"):
            continue
        decl = stripped[:-1]
        decl = decl.split("=", 1)[0].rstrip()
        parsed = _split_type_and_name(decl)
        if parsed is None:
            continue
        type_name, name = parsed
        if _is_generic_local_name(name):
            generic_local_name_count += 1
        else:
            named_local_count += 1
        if name.startswith("slot_"):
            slot_alias_count += 1
        if not _is_raw_surface_type(type_name):
            surfaced_local_type_count += 1

    return {
        "label_count": count_regex(r"^\s*[A-Za-z_]\w*:\s*$", code),
        "named_param_count": named_param_count,
        "named_local_count": named_local_count,
        "surfaced_param_type_count": surfaced_param_type_count,
        "surfaced_local_type_count": surfaced_local_type_count,
        "surfaced_return_type_present": bool(return_type) and not _is_raw_surface_type(return_type),
        "slot_alias_count": slot_alias_count,
        "function_name_is_generic": _is_generic_function_name(function_name),
        "param_name_generic_count": generic_param_name_count,
        "local_name_generic_count": generic_local_name_count,
        "fpu_op_count": fpu_op_count,
        "jump_table_count": jump_table_count,
    }


def extract_quality_metrics(metrics: dict[str, Any]) -> dict[str, Any]:
    keys = (
        "goto_count",
        "label_count",
        "named_param_count",
        "named_local_count",
        "surfaced_param_type_count",
        "surfaced_local_type_count",
        "surfaced_return_type_present",
        "slot_alias_count",
        "function_name_is_generic",
        "param_name_generic_count",
        "local_name_generic_count",
        "avg_line_length_chars",
        "max_brace_nesting_depth",
        "local_mention_count",
        "local_identifier_share",
        "comment_char_ratio",
        "block_comment_char_ratio",
        "line_comment_char_ratio",
    )
    return {key: metrics.get(key) for key in keys}


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
    metrics["synthetic_helper_call_count"] = count_regex(r"\b__fission_[A-Za-z0-9_]*\s*\(", code)

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
    metrics.update(collect_quality_metrics(code))

    # ── Phase 4 pass quality indicators ──────────────────────────────────────
    # Detect whether the function's return type is still "undefined" (i.e.
    # return-type inference did not fire or produced no result).
    signature_line = code.split("{", 1)[0] if "{" in code else code.splitlines()[0]
    metrics["undefined_return_type"] = bool(
        re.match(r"^\s*undefined\b", signature_line.strip())
    )

    # Count pointer-arithmetic recovery nodes emitted by Fission:
    #   PtrOffset  →  base->field or base + offset rendered as *((T *)(base + k))
    #   Index      →  base[idx] patterns where the base is a typed pointer
    # We count "->field" accesses (field_access_count already tracked) and
    # additionally count subscript patterns with a non-constant index (Index node).
    metrics["ptr_offset_count"] = int(metrics.get("field_access_count", 0))
    metrics["index_expr_count"] = count_regex(
        r"\b\w[\w.\->]*\[\s*[A-Za-z_]\w*\s*\]",  # var[non_const_idx]
        code,
    )

    # Count local variable declarations whose type is still "undefined" or a
    # raw void-pointer — proxy for unresolved Unknown NirType bindings.
    metrics["unknown_type_var_count"] = count_regex(
        r"^\s*(?:undefined\d*|void\s*\*)\s+[A-Za-z_]\w*\s*[=;]",
        code,
        # re.MULTILINE already handled inside count_regex
    )

    # ── Heuristic readability / surface proxies (whole function text) ───────
    metrics["avg_line_length_chars"] = avg_non_empty_line_length(code)
    metrics["max_brace_nesting_depth"] = approx_max_brace_nesting(code)
    n_local = len(re.findall(r"\blocal_\d+\b", code))
    n_slot = len(re.findall(r"\bslot_\w+\b", code))
    metrics["local_mention_count"] = n_local
    denom = max(1, n_local + n_slot)
    metrics["local_identifier_share"] = round(n_local / denom, 4)
    block_chunks = re.findall(r"/\*.*?\*/", code, flags=re.DOTALL)
    block_chars = sum(len(c) for c in block_chunks)
    line_comment_chars = sum(len(m.group(0)) for m in re.finditer(r"//[^\n]*", code))
    total_chars = max(len(code), 1)
    metrics["block_comment_char_ratio"] = round(block_chars / total_chars, 6)
    metrics["line_comment_char_ratio"] = round(line_comment_chars / total_chars, 6)
    metrics["comment_char_ratio"] = round((block_chars + line_comment_chars) / total_chars, 6)

    metrics["anti_pattern_counts"] = {
        "nested_cast": len(
            re.findall(r"\(\s*\([^)]{4,}\)\s*\)\s*[\w*(]", code)
        ),
        "line_over_200_chars": sum(1 for ln in code.splitlines() if len(ln) > 200),
        "ternary_operator": len(re.findall(r"\?[^;\n]+:", code)),
        "address_of_chain": len(re.findall(r"&\s*&\s*\w", code)),
        "double_semicolon": len(re.findall(r";\s*;", code)),
    }

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
