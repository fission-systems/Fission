#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import time
from collections import Counter
from pathlib import Path
from typing import Any


ROOT_DIR = Path(__file__).resolve().parents[3]
DEFAULT_RESULTS_DIR = ROOT_DIR / "artifacts" / "grand_finale"
DEFAULT_GHIDRA_DIR = ROOT_DIR / "vendor" / "ghidra" / "ghidra_11.4.2_PUBLIC"
BASE_TYPES_JSON = ROOT_DIR / "crates" / "fission-signatures" / "data" / "win_types" / "base_types.json"
MANDATORY_SAMPLE_ADDRESSES: dict[str, list[str]] = {
    "cmkr": [
        "0x140001000",
        "0x140003270",
        "0x1400034a0",
        "0x1400036e0",
        "0x140003920",
        "0x140004010",
    ]
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Function-sampled decompilation quality benchmark between Fission and Ghidra."
    )
    parser.add_argument("binaries", nargs="+", help="Target binaries to benchmark")
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=DEFAULT_RESULTS_DIR,
        help="Directory to write benchmark artifacts into",
    )
    parser.add_argument(
        "--fission-bin",
        type=Path,
        default=ROOT_DIR / "target" / "release" / "fission_cli",
        help="Path to a prebuilt fission_cli binary with native_decomp enabled",
    )
    parser.add_argument(
        "--ghidra-dir",
        type=Path,
        default=DEFAULT_GHIDRA_DIR,
        help="Path to Ghidra installation directory",
    )
    parser.add_argument(
        "--limit",
        type=int,
        default=30,
        help="Number of functions to sample from each binary (0 means all)",
    )
    parser.add_argument(
        "--per-func-timeout",
        type=int,
        default=90,
        help="Per-function timeout in seconds for Fission and Ghidra decompilation",
    )
    parser.add_argument(
        "--skip-ghidra",
        action="store_true",
        help="Collect only Fission metrics",
    )
    return parser.parse_args()


def normalize_address(address: str) -> str:
    return address.lower().replace("0x", "").lstrip("0") or "0"


def run_command_json(
    cmd: list[str],
    cwd: Path,
    env: dict[str, str] | None = None,
    timeout: int = 90,
) -> tuple[dict[str, Any] | None, str | None, float]:
    start = time.perf_counter()
    try:
        res = subprocess.run(
            cmd,
            cwd=cwd,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            timeout=timeout,
            check=True,
            env=env,
        )
    except subprocess.TimeoutExpired:
        return None, "timeout", time.perf_counter() - start
    except subprocess.CalledProcessError as exc:
        error = exc.stderr.strip() or exc.stdout.strip() or "command_failed"
        return None, error, time.perf_counter() - start

    try:
        return json.loads(res.stdout), None, time.perf_counter() - start
    except json.JSONDecodeError:
        return None, "invalid_json", time.perf_counter() - start


def list_functions_with_fission(binary_path: Path, fission_bin: Path) -> list[tuple[str, str]]:
    cmd = [str(fission_bin), str(binary_path), "--list"]
    res = subprocess.run(
        cmd,
        cwd=ROOT_DIR,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        check=True,
    )
    functions: list[tuple[str, str]] = []
    for line in res.stdout.splitlines():
        parts = line.split()
        if len(parts) >= 3 and parts[0].startswith("0x"):
            functions.append((parts[0], parts[-1]))
    return functions


def sample_functions(
    binary_name: str,
    functions: list[tuple[str, str]],
    limit: int,
) -> list[tuple[str, str]]:
    if limit <= 0 or len(functions) <= limit:
        return functions
    selected: list[tuple[str, str]] = []
    seen: set[str] = set()
    mandatory = {normalize_address(addr) for addr in MANDATORY_SAMPLE_ADDRESSES.get(binary_name, [])}

    for address, name in functions:
        normalized = normalize_address(address)
        if normalized in mandatory and normalized not in seen:
            selected.append((address, name))
            seen.add(normalized)

    for address, name in functions:
        normalized = normalize_address(address)
        if normalized in seen:
            continue
        selected.append((address, name))
        seen.add(normalized)
        if len(selected) >= limit:
            break

    return selected[:limit]


def load_struct_pointer_aliases() -> dict[str, str]:
    items = json.loads(BASE_TYPES_JSON.read_text())
    aliases: dict[str, str] = {}
    for item in items:
        name = item.get("name", "")
        if item.get("is_pointer") and name.startswith("LP") and len(name) > 2:
            aliases[name] = name[2:]
    return aliases


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
    metrics = {
        "goto_count": count_regex(r"\bgoto\s+[A-Za-z_]\w*\s*;", code),
        "switch_count": count_regex(r"\bswitch\s*\(", code),
        "for_count": count_regex(r"\bfor\s*\(", code),
        "do_while_count": count_regex(r"\bdo\s*\{", code),
        "while_count": count_regex(r"\bwhile\s*\(", code),
    }

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


def run_fission_function(
    binary_path: Path,
    address: str,
    fission_bin: Path,
    timeout_sec: int,
    struct_ptr_aliases: dict[str, str],
) -> dict[str, Any]:
    cmd = [
        str(fission_bin),
        str(binary_path),
        "--decomp",
        address,
        "--json",
        "--benchmark",
        "--ghidra-compat",
        "--no-header",
        "--no-warnings",
    ]
    payload, error, wall_sec = run_command_json(cmd, cwd=ROOT_DIR, timeout=timeout_sec)
    if payload is None:
        return {
            "success": False,
            "failure_kind": classify_failure_kind(error),
            "failure_detail": error,
            "wall_sec": round(wall_sec, 6),
        }

    func = payload.get("functions", [{}])[0]
    code = func.get("code", "")
    entry = {
        "success": True,
        "address": func.get("address", address),
        "name": func.get("name", ""),
        "decomp_sec": round(float(func.get("decomp_sec", 0.0)), 6),
        "postprocess_sec": round(float(func.get("postprocess_sec", 0.0)), 6),
        "wall_sec": round(wall_sec, 6),
        "code": code,
    }
    if failure := detect_embedded_failure(code):
        entry["success"] = False
        entry["failure_kind"] = failure[0]
        entry["failure_detail"] = failure[1]
        if code.lstrip().startswith("// Assembly fallback:"):
            entry["fallback_counts"] = {"assembly_fallback": 1}
        return entry
    entry["metrics"] = collect_code_metrics(code, struct_ptr_aliases)
    return entry


def run_ghidra_binary(
    binary_path: Path,
    functions: list[tuple[str, str]],
    ghidra_dir: Path,
    timeout_sec: int,
    struct_ptr_aliases: dict[str, str],
) -> tuple[float, dict[str, dict[str, Any]]]:
    os.environ["GHIDRA_INSTALL_DIR"] = str(ghidra_dir)
    import pyghidra

    pyghidra.start()
    from ghidra.app.decompiler import DecompInterface
    from ghidra.util.task import ConsoleTaskMonitor

    results: dict[str, dict[str, Any]] = {}
    load_start = time.perf_counter()
    with pyghidra.open_program(str(binary_path), analyze=True) as flat_api:
        program = flat_api.getCurrentProgram()
        monitor = ConsoleTaskMonitor()
        decomp = DecompInterface()
        decomp.openProgram(program)
        init_sec = time.perf_counter() - load_start

        function_manager = program.getFunctionManager()
        addr_factory = program.getAddressFactory()

        for addr_str, name in functions:
            start = time.perf_counter()
            clean_addr = normalize_address(addr_str)
            entry: dict[str, Any] = {
                "address": addr_str,
                "name": name,
                "success": False,
            }
            try:
                addr = addr_factory.getAddress(clean_addr)
                target = None
                if addr:
                    target = function_manager.getFunctionContaining(addr)
                    if not target:
                        target = function_manager.getFunctionAt(addr)
                if target is None:
                    for func in list(function_manager.getFunctions(True)):
                        if func.getName() == name or func.getName() == f"_{name}":
                            target = func
                            break
                if target is None:
                    entry["failure_kind"] = "missing_function"
                else:
                    result = decomp.decompileFunction(target, timeout_sec, monitor)
                    if result and result.decompileCompleted() and result.getDecompiledFunction():
                        code = result.getDecompiledFunction().getC()
                        entry["success"] = True
                        entry["code"] = code
                        entry["metrics"] = collect_code_metrics(code, struct_ptr_aliases)
                    else:
                        entry["failure_kind"] = "other"
                        entry["failure_detail"] = "decompile_incomplete"
            except Exception as exc:  # noqa: BLE001
                entry["failure_kind"] = classify_failure_kind(str(exc))
                entry["error"] = str(exc)
            entry["decomp_sec"] = round(time.perf_counter() - start, 6)
            results[normalize_address(addr_str)] = entry
    return init_sec, results


def summarize_binary(
    binary_name: str,
    functions: list[tuple[str, str]],
    fission_entries: dict[str, dict[str, Any]],
    ghidra_entries: dict[str, dict[str, Any]],
) -> dict[str, Any]:
    shared_success = [
        normalize_address(addr)
        for addr, _ in functions
        if normalize_address(addr) in fission_entries
        and normalize_address(addr) in ghidra_entries
        and fission_entries[normalize_address(addr)].get("success")
        and ghidra_entries[normalize_address(addr)].get("success")
    ]

    def sum_metric(entries: dict[str, dict[str, Any]], addrs: list[str], key: str) -> int:
        return sum(int(entries[addr]["metrics"].get(key, 0)) for addr in addrs)

    ghidra_gotos = sum_metric(ghidra_entries, shared_success, "goto_count")
    fission_gotos = sum_metric(fission_entries, shared_success, "goto_count")
    goto_reduction_pct = (
        round((ghidra_gotos - fission_gotos) * 100.0 / ghidra_gotos, 2) if ghidra_gotos else 0.0
    )

    def aggregate_type_hits(entries: dict[str, dict[str, Any]]) -> Counter[str]:
        total: Counter[str] = Counter()
        for entry in entries.values():
            if not entry.get("success"):
                continue
            total.update(entry["metrics"].get("type_hits", {}))
        return total

    def aggregate_type_preservation(entries: dict[str, dict[str, Any]]) -> Counter[str]:
        total: Counter[str] = Counter()
        for entry in entries.values():
            if not entry.get("success"):
                continue
            total.update(entry["metrics"].get("type_preservation_hits", {}))
        return total

    def aggregate_residues(entries: dict[str, dict[str, Any]], key: str) -> Counter[str]:
        total: Counter[str] = Counter()
        for entry in entries.values():
            if not entry.get("success"):
                continue
            total.update(entry["metrics"].get(key, {}))
        return total

    def aggregate_fallbacks(entries: dict[str, dict[str, Any]]) -> Counter[str]:
        total: Counter[str] = Counter()
        for entry in entries.values():
            if entry.get("success"):
                total.update(entry.get("metrics", {}).get("fallback_counts", {}))
            else:
                total.update(entry.get("fallback_counts", {}))
        return total

    fission_successes = sum(1 for entry in fission_entries.values() if entry.get("success"))
    ghidra_successes = sum(1 for entry in ghidra_entries.values() if entry.get("success"))
    top_residue_offenders = collect_top_residue_offenders(fission_entries)

    return {
        "binary": binary_name,
        "function_count": len(functions),
        "shared_success_count": len(shared_success),
        "fission_success_count": fission_successes,
        "ghidra_success_count": ghidra_successes,
        "fission_failure_count": len(functions) - fission_successes,
        "ghidra_failure_count": len(functions) - ghidra_successes,
        "control_flow": {
            "ghidra_gotos": ghidra_gotos,
            "fission_gotos": fission_gotos,
            "goto_reduction_pct": goto_reduction_pct,
            "ghidra_switches": sum_metric(ghidra_entries, shared_success, "switch_count"),
            "fission_switches": sum_metric(fission_entries, shared_success, "switch_count"),
            "ghidra_for_loops": sum_metric(ghidra_entries, shared_success, "for_count"),
            "fission_for_loops": sum_metric(fission_entries, shared_success, "for_count"),
            "ghidra_do_while": sum_metric(ghidra_entries, shared_success, "do_while_count"),
            "fission_do_while": sum_metric(fission_entries, shared_success, "do_while_count"),
        },
        "type_promotion": {
            "fission_hits": dict(aggregate_type_hits(fission_entries).most_common()),
            "ghidra_hits": dict(aggregate_type_hits(ghidra_entries).most_common()),
        },
        "type_preservation_counts": {
            "fission": dict(aggregate_type_preservation(fission_entries).most_common()),
            "ghidra": dict(aggregate_type_preservation(ghidra_entries).most_common()),
        },
        "fallback_counts": {
            "fission": dict(aggregate_fallbacks(fission_entries).most_common()),
            "ghidra": dict(aggregate_fallbacks(ghidra_entries).most_common()),
        },
        "cast_chain_counts": {
            "fission": sum(
                int(entry.get("metrics", {}).get("cast_chain_count", 0))
                for entry in fission_entries.values()
                if entry.get("success")
            ),
            "ghidra": sum(
                int(entry.get("metrics", {}).get("cast_chain_count", 0))
                for entry in ghidra_entries.values()
                if entry.get("success")
            ),
        },
        "residue_rankings": {
            "single_assign_temps": dict(
                aggregate_residues(fission_entries, "single_assign_temps").most_common(20)
            ),
            "residue_names": dict(
                aggregate_residues(fission_entries, "residue_names").most_common(20)
            ),
            "residue_families": dict(
                aggregate_residues(fission_entries, "residue_families").most_common()
            ),
        },
        "top_residue_offenders": top_residue_offenders,
        "failure_class_counts": {
            "fission": dict(
                Counter(
                    classify_failure_kind(entry.get("failure_kind") or entry.get("failure_detail"))
                    for entry in fission_entries.values()
                    if not entry.get("success")
                )
            ),
            "ghidra": dict(
                Counter(
                    classify_failure_kind(entry.get("failure_kind") or entry.get("error"))
                    for entry in ghidra_entries.values()
                    if not entry.get("success")
                )
            ),
        },
        "timings": {
            "fission_total_decomp_sec": round(
                sum(float(entry.get("decomp_sec", 0.0)) for entry in fission_entries.values()), 6
            ),
            "fission_total_postprocess_sec": round(
                sum(float(entry.get("postprocess_sec", 0.0)) for entry in fission_entries.values()),
                6,
            ),
            "ghidra_total_decomp_sec": round(
                sum(float(entry.get("decomp_sec", 0.0)) for entry in ghidra_entries.values()), 6
            ),
        },
    }


def write_markdown_report(report: dict[str, Any], output_path: Path) -> None:
    lines = [
        "# GRAND-FINALE Report",
        "",
        f"- Generated: {time.strftime('%Y-%m-%d %H:%M:%S')}",
        f"- Binaries: {len(report['binaries'])}",
        "",
        "## Global Summary",
        "",
        f"- Shared successful functions: {report['global']['shared_success_count']}",
        f"- Fission success count: {report['global']['fission_success_count']}",
        f"- Ghidra success count: {report['global']['ghidra_success_count']}",
        f"- Goto reduction vs Ghidra: {report['global']['control_flow']['goto_reduction_pct']:.2f}%",
        f"- Fission switches / Ghidra switches: {report['global']['control_flow']['fission_switches']} / {report['global']['control_flow']['ghidra_switches']}",
        f"- Fission for-loops / Ghidra for-loops: {report['global']['control_flow']['fission_for_loops']} / {report['global']['control_flow']['ghidra_for_loops']}",
        f"- Fission do-while / Ghidra do-while: {report['global']['control_flow']['fission_do_while']} / {report['global']['control_flow']['ghidra_do_while']}",
        f"- Failure classes (Fission): {report['global']['failure_class_counts']['fission']}",
        f"- Failure classes (Ghidra): {report['global']['failure_class_counts']['ghidra']}",
        f"- Type preservation hits (Fission): {report['global']['type_preservation_counts']['fission']}",
        f"- Raw pointer / assembly fallbacks (Fission): {report['global']['fallback_counts']['fission']}",
        f"- Cast chains (Fission/Ghidra): {report['global']['cast_chain_counts']['fission']} / {report['global']['cast_chain_counts']['ghidra']}",
        "",
        "## Residue Intel",
        "",
    ]

    for title, payload in [
        ("Single-Assign Temps", report["global"]["residue_rankings"]["single_assign_temps"]),
        ("Residue Names", report["global"]["residue_rankings"]["residue_names"]),
        ("Residue Families", report["global"]["residue_rankings"]["residue_families"]),
    ]:
        lines.append(f"### {title}")
        if not payload:
            lines.append("- none")
        else:
            for name, count in payload.items():
                lines.append(f"- `{name}`: {count}")
        lines.append("")

    lines.append("### Top Offenders")
    if not report["global"]["top_residue_offenders"]:
        lines.append("- none")
    else:
        for offender in report["global"]["top_residue_offenders"]:
            lines.append(
                f"- `{offender['binary']}` `{offender['address']}` `{offender['name']}`: "
                f"score={offender['residue_score']}, raw_pointer_fallback={offender['raw_pointer_fallback']}, "
                f"single_assign_temps={offender['single_assign_temp_total']}, "
                f"top_names={offender['top_residue_names']}"
            )
    lines.append("")

    lines.append("## Per-Binary Summary")
    lines.append("")
    for binary in report["binaries"]:
        lines.append(f"### {binary['binary']}")
        lines.append(
            f"- Shared success: {binary['shared_success_count']} / {binary['function_count']} | "
            f"Goto reduction: {binary['control_flow']['goto_reduction_pct']:.2f}%"
        )
        lines.append(
            f"- Fission/Ghidra success: {binary['fission_success_count']} / {binary['ghidra_success_count']}"
        )
        lines.append(
            f"- Struct pointer hits: Fission {sum(binary['type_promotion']['fission_hits'].values())}, "
            f"Ghidra {sum(binary['type_promotion']['ghidra_hits'].values())}"
        )
        lines.append(
            f"- Type preservation: Fission {binary['type_preservation_counts']['fission']}, "
            f"Ghidra {binary['type_preservation_counts']['ghidra']}"
        )
        lines.append(
            f"- Failure classes: Fission {binary['failure_class_counts']['fission']} | "
            f"Ghidra {binary['failure_class_counts']['ghidra']}"
        )
        lines.append(
            f"- Cast chains: Fission {binary['cast_chain_counts']['fission']} | "
            f"Ghidra {binary['cast_chain_counts']['ghidra']}"
        )
        if binary["top_residue_offenders"]:
            lines.append("- Top residue offenders:")
            for offender in binary["top_residue_offenders"]:
                lines.append(
                    f"  - `{offender['address']}` `{offender['name']}`: "
                    f"score={offender['residue_score']}, raw_pointer_fallback={offender['raw_pointer_fallback']}, "
                    f"single_assign_temps={offender['single_assign_temp_total']}, "
                    f"top_names={offender['top_residue_names']}"
                )
        lines.append("")

    output_path.write_text("\n".join(lines))


def aggregate_global_report(binary_reports: list[dict[str, Any]]) -> dict[str, Any]:
    global_report = {
        "shared_success_count": 0,
        "fission_success_count": 0,
        "ghidra_success_count": 0,
        "control_flow": Counter(),
        "failure_class_counts": {
            "fission": Counter(),
            "ghidra": Counter(),
        },
        "type_preservation_counts": {
            "fission": Counter(),
            "ghidra": Counter(),
        },
        "fallback_counts": {
            "fission": Counter(),
            "ghidra": Counter(),
        },
        "cast_chain_counts": {
            "fission": 0,
            "ghidra": 0,
        },
        "residue_rankings": {
            "single_assign_temps": Counter(),
            "residue_names": Counter(),
            "residue_families": Counter(),
        },
        "top_residue_offenders": [],
    }

    for report in binary_reports:
        global_report["shared_success_count"] += report["shared_success_count"]
        global_report["fission_success_count"] += report["fission_success_count"]
        global_report["ghidra_success_count"] += report["ghidra_success_count"]
        global_report["control_flow"].update(report["control_flow"])
        global_report["failure_class_counts"]["fission"].update(report["failure_class_counts"]["fission"])
        global_report["failure_class_counts"]["ghidra"].update(report["failure_class_counts"]["ghidra"])
        global_report["type_preservation_counts"]["fission"].update(report["type_preservation_counts"]["fission"])
        global_report["type_preservation_counts"]["ghidra"].update(report["type_preservation_counts"]["ghidra"])
        global_report["fallback_counts"]["fission"].update(report["fallback_counts"]["fission"])
        global_report["fallback_counts"]["ghidra"].update(report["fallback_counts"]["ghidra"])
        global_report["cast_chain_counts"]["fission"] += report["cast_chain_counts"]["fission"]
        global_report["cast_chain_counts"]["ghidra"] += report["cast_chain_counts"]["ghidra"]
        for key in global_report["residue_rankings"]:
            global_report["residue_rankings"][key].update(report["residue_rankings"][key])
        for offender in report.get("top_residue_offenders", []):
            global_report["top_residue_offenders"].append(
                {
                    "binary": report["binary"],
                    **offender,
                }
            )

    ghidra_gotos = global_report["control_flow"]["ghidra_gotos"]
    fission_gotos = global_report["control_flow"]["fission_gotos"]
    global_report["control_flow"]["goto_reduction_pct"] = (
        round((ghidra_gotos - fission_gotos) * 100.0 / ghidra_gotos, 2) if ghidra_gotos else 0.0
    )
    for key in global_report["residue_rankings"]:
        global_report["residue_rankings"][key] = dict(
            global_report["residue_rankings"][key].most_common(20)
        )
    global_report["top_residue_offenders"] = sorted(
        global_report["top_residue_offenders"],
        key=lambda item: (
            -int(item["residue_score"]),
            -int(item["raw_pointer_fallback"]),
            -int(item["single_assign_temp_total"]),
            item["binary"],
            item["address"],
        ),
    )[:10]
    global_report["failure_class_counts"] = {
        side: dict(counter)
        for side, counter in global_report["failure_class_counts"].items()
    }
    global_report["type_preservation_counts"] = {
        side: dict(counter)
        for side, counter in global_report["type_preservation_counts"].items()
    }
    global_report["fallback_counts"] = {
        side: dict(counter)
        for side, counter in global_report["fallback_counts"].items()
    }
    global_report["control_flow"] = dict(global_report["control_flow"])
    return global_report


def main() -> int:
    args = parse_args()
    output_dir = args.output_dir
    output_dir.mkdir(parents=True, exist_ok=True)

    if not args.fission_bin.exists():
        raise SystemExit(f"Fission binary not found: {args.fission_bin}")
    if not args.skip_ghidra and not args.ghidra_dir.exists():
        raise SystemExit(f"Ghidra dir not found: {args.ghidra_dir}")

    struct_ptr_aliases = load_struct_pointer_aliases()
    binary_reports: list[dict[str, Any]] = []

    for binary_str in args.binaries:
        binary_path = Path(binary_str).resolve()
        binary_name = binary_path.stem
        print(f"[*] Benchmarking {binary_name} ...", flush=True)

        functions = sample_functions(
            binary_name,
            list_functions_with_fission(binary_path, args.fission_bin),
            args.limit,
        )
        fission_entries: dict[str, dict[str, Any]] = {}
        for address, name in functions:
            print(f"    [Fission] {address} {name}", flush=True)
            entry = run_fission_function(
                binary_path,
                address,
                args.fission_bin,
                args.per_func_timeout,
                struct_ptr_aliases,
            )
            entry.setdefault("address", address)
            entry.setdefault("name", name)
            fission_entries[normalize_address(address)] = entry

        ghidra_entries: dict[str, dict[str, Any]] = {}
        ghidra_init_sec = 0.0
        if not args.skip_ghidra:
            ghidra_init_sec, ghidra_entries = run_ghidra_binary(
                binary_path,
                functions,
                args.ghidra_dir,
                args.per_func_timeout,
                struct_ptr_aliases,
            )
        else:
            for address, name in functions:
                ghidra_entries[normalize_address(address)] = {
                    "address": address,
                    "name": name,
                    "success": False,
                    "failure_kind": "skipped",
                    "decomp_sec": 0.0,
                }

        report = summarize_binary(binary_name, functions, fission_entries, ghidra_entries)
        report["ghidra_init_sec"] = round(ghidra_init_sec, 6)
        binary_reports.append(report)

        artifact = {
            "binary": str(binary_path),
            "summary": report,
            "functions": {
                "fission": fission_entries,
                "ghidra": ghidra_entries,
            },
        }
        (output_dir / f"{binary_name}_grand_finale.json").write_text(json.dumps(artifact, indent=2))

    final_report = {
        "binaries": binary_reports,
        "global": aggregate_global_report(binary_reports),
    }
    (output_dir / "grand_finale_summary.json").write_text(json.dumps(final_report, indent=2))
    write_markdown_report(final_report, output_dir / "grand_finale_summary.md")
    print(f"[+] Wrote report to {output_dir}", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
