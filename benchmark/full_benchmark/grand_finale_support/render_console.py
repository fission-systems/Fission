from __future__ import annotations

from pathlib import Path
from typing import Any

from rich.console import Console
from rich.panel import Panel
from rich.table import Table


def _default_console(console: Console | None = None) -> Console:
    return console if console is not None else Console()


def print_single_benchmark_console(
    summary: dict[str, Any],
    output_dir: Path,
    baseline_gate: dict[str, Any] | None = None,
    *,
    console: Console | None = None,
) -> None:
    console = _default_console(console)
    quality = (summary.get("quality") or {}).get("pyghidra_vs_fission", {}) or {}
    coverage = (summary.get("coverage") or {}).get("pyghidra_vs_fission", {}) or {}
    owner_metrics = ((summary.get("owner_metrics") or {}).get("fission")) or {}
    shape_metrics = ((summary.get("shape_drift_metrics") or {}).get("fission")) or {}
    normalize_pass_metrics = ((summary.get("normalize_pass_metrics") or {}).get("fission")) or {}
    ghidra_action_metrics = ((summary.get("ghidra_action_metrics") or {}).get("fission")) or {}
    blockgraph_region_metrics = ((summary.get("blockgraph_region_metrics") or {}).get("fission")) or {}
    alias_interleave_metrics = ((summary.get("alias_interleave_metrics") or {}).get("fission")) or {}
    giant_family_counts = summary.get("giant_function_speed_family_counts", {}) or {}

    console.print(
        Panel.fit(
            "\n".join(
                [
                    f"binary={summary.get('binary')}",
                    f"avg_norm_sim={float(quality.get('avg_normalized_similarity', 0.0)):.3f}%",
                    f"coverage={float(coverage.get('coverage_ratio_pct', 0.0)):.3f}%",
                    f"artifacts={output_dir}",
                ]
            ),
            title="Whole Decomp Benchmark",
        )
    )

    owner_table = Table(title="Owner Metrics", show_header=True)
    owner_table.add_column("Metric")
    owner_table.add_column("Value", justify="right")
    for key, value in sorted(owner_metrics.items()):
        owner_table.add_row(key, f"{float(value):.3f}")
    if owner_metrics:
        console.print(owner_table)

    shape_table = Table(title="Shape Drift", show_header=True)
    shape_table.add_column("Metric")
    shape_table.add_column("Value", justify="right")
    for key, value in sorted(shape_metrics.items()):
        shape_table.add_row(key, f"{float(value):.3f}")
    if shape_metrics:
        console.print(shape_table)

    normalize_table = Table(title="Normalize Pass Metrics", show_header=True)
    normalize_table.add_column("Metric")
    normalize_table.add_column("Value", justify="right")
    for key, value in sorted(normalize_pass_metrics.items()):
        normalize_table.add_row(key, f"{float(value):.3f}")
    if normalize_pass_metrics:
        console.print(normalize_table)

    action_table = Table(title="Ghidra Concept Stage Metrics", show_header=True)
    action_table.add_column("Metric")
    action_table.add_column("Value", justify="right")
    for key, value in sorted(ghidra_action_metrics.items()):
        action_table.add_row(key, f"{float(value):.3f}")
    if ghidra_action_metrics:
        console.print(action_table)

    blockgraph_table = Table(title="BlockGraph Region Proof Metrics", show_header=True)
    blockgraph_table.add_column("Metric")
    blockgraph_table.add_column("Value", justify="right")
    for key, value in sorted(blockgraph_region_metrics.items()):
        blockgraph_table.add_row(key, f"{float(value):.3f}")
    if blockgraph_region_metrics:
        console.print(blockgraph_table)

    alias_table = Table(title="Alias Interleave Metrics", show_header=True)
    alias_table.add_column("Metric")
    alias_table.add_column("Value", justify="right")
    for key, value in sorted(alias_interleave_metrics.items()):
        alias_table.add_row(key, f"{float(value):.3f}")
    if alias_interleave_metrics:
        console.print(alias_table)

    if giant_family_counts:
        giant_table = Table(title="Giant Function Families", show_header=True)
        giant_table.add_column("Family")
        giant_table.add_column("Count", justify="right")
        for key, value in sorted(giant_family_counts.items()):
            giant_table.add_row(str(key), str(int(value)))
        console.print(giant_table)

    if baseline_gate:
        row_gate = (baseline_gate.get("row_fidelity_gate") or {})
        console.print(
            Panel.fit(
                "\n".join(
                    [
                        f"status={baseline_gate.get('status', 'unknown')}",
                        f"row_fidelity={row_gate.get('status', 'unknown')}",
                        f"failed_targets={','.join(row_gate.get('failed_targets', [])) or 'none'}",
                    ]
                ),
                title="Baseline Gate",
            )
        )


def print_corpus_benchmark_console(
    corpus_summary: dict[str, Any],
    output_dir: Path,
    *,
    console: Console | None = None,
) -> None:
    console = _default_console(console)
    corpus = corpus_summary.get("corpus_summary", {}) or {}
    console.print(
        Panel.fit(
            "\n".join(
                [
                    f"suite_tier={corpus_summary.get('suite_tier', 'release')}",
                    f"gate_mode={corpus_summary.get('gate_mode', 'advisory')}",
                    f"weighted_avg_norm_sim={float(corpus.get('weighted_avg_normalized_similarity', 0.0)):.3f}%",
                    f"release_promotion_allowed={bool(corpus_summary.get('release_promotion_allowed', False))}",
                    f"artifacts={output_dir}",
                ]
            ),
            title="Corpus Benchmark",
        )
    )

    arch_table = Table(title="x86 / x64 Split", show_header=True)
    arch_table.add_column("Arch")
    arch_table.add_column("Binaries", justify="right")
    arch_table.add_column("Weighted Avg", justify="right")
    arch_table.add_column("Failed", justify="left")
    arch_summary = corpus_summary.get("arch_summary", {}) or {}
    for arch in ("x86", "x64"):
        payload = arch_summary.get(arch, {}) or {}
        arch_table.add_row(
            arch,
            str(int(payload.get("binary_count", 0) or 0)),
            f"{float(payload.get('weighted_avg_normalized_similarity', 0.0)):.3f}%",
            ",".join(payload.get("failed_binary_ids", [])) or "none",
        )
    console.print(arch_table)

    normalize_pass_totals = corpus_summary.get("normalize_pass_metric_totals", {}) or {}
    if normalize_pass_totals:
        normalize_table = Table(title="Normalize Pass Totals", show_header=True)
        normalize_table.add_column("Metric")
        normalize_table.add_column("Value", justify="right")
        for key, value in sorted(normalize_pass_totals.items()):
            normalize_table.add_row(key, f"{float(value):.3f}")
        console.print(normalize_table)

    ghidra_action_totals = corpus_summary.get("ghidra_action_metric_totals", {}) or {}
    if ghidra_action_totals:
        action_table = Table(title="Ghidra Concept Stage Totals", show_header=True)
        action_table.add_column("Metric")
        action_table.add_column("Value", justify="right")
        for key, value in sorted(ghidra_action_totals.items()):
            action_table.add_row(str(key), f"{float(value):.3f}")
        console.print(action_table)

    blockgraph_totals = corpus_summary.get("blockgraph_region_metric_totals", {}) or {}
    if blockgraph_totals:
        blockgraph_table = Table(title="BlockGraph Region Proof Totals", show_header=True)
        blockgraph_table.add_column("Metric")
        blockgraph_table.add_column("Value", justify="right")
        for key, value in sorted(blockgraph_totals.items()):
            blockgraph_table.add_row(str(key), f"{float(value):.3f}")
        console.print(blockgraph_table)

    alias_totals = corpus_summary.get("alias_interleave_metric_totals", {}) or {}
    if alias_totals:
        alias_table = Table(title="Alias Interleave Totals", show_header=True)
        alias_table.add_column("Metric")
        alias_table.add_column("Value", justify="right")
        for key, value in sorted(alias_totals.items()):
            alias_table.add_row(str(key), f"{float(value):.3f}")
        console.print(alias_table)

    giant_family_totals = corpus_summary.get("giant_function_speed_family_totals", {}) or {}
    if giant_family_totals:
        giant_table = Table(title="Giant Function Families", show_header=True)
        giant_table.add_column("Family")
        giant_table.add_column("Count", justify="right")
        for key, value in sorted(giant_family_totals.items()):
            giant_table.add_row(str(key), str(int(value)))
        console.print(giant_table)

    binary_table = Table(title="Per-Binary Summary", show_header=True)
    binary_table.add_column("ID")
    binary_table.add_column("Arch")
    binary_table.add_column("Role")
    binary_table.add_column("Avg Sim", justify="right")
    binary_table.add_column("Coverage", justify="right")
    binary_table.add_column("Row Gate")
    binary_table.add_column("Watchlist")
    for item in corpus_summary.get("binaries", []):
        binary_table.add_row(
            str(item.get("id", "")),
            str(item.get("arch", "unknown")),
            str(item.get("role", "")),
            f"{float(item.get('avg_normalized_similarity', 0.0)):.3f}%",
            f"{float(item.get('coverage_ratio_pct', 0.0)):.3f}%",
            str(item.get("row_fidelity_gate_status", "unknown")),
            str(item.get("watchlist_source", "unknown")),
        )
    console.print(binary_table)
