from __future__ import annotations

from pathlib import Path
from typing import Any

from jinja2 import Environment, FileSystemLoader

from .artifact_models import (
    VerboseCorpusBenchmarkArtifact,
    VerboseSingleBenchmarkArtifact,
)

TEMPLATE_DIR = Path(__file__).resolve().parent / "templates"


def _env() -> Environment:
    environment = Environment(
        loader=FileSystemLoader(str(TEMPLATE_DIR)),
        autoescape=False,
        trim_blocks=True,
        lstrip_blocks=True,
    )
    environment.filters["boolword"] = lambda value: "yes" if value else "no"
    return environment


def render_single_benchmark_markdown(benchmark: dict[str, Any]) -> str:
    artifact = VerboseSingleBenchmarkArtifact.model_validate(benchmark)
    template = _env().get_template("single_benchmark.md.j2")
    summary = artifact.summary
    quality = (summary.quality or {}).get("pyghidra_vs_fission", {}) or {}
    coverage = (summary.coverage or {}).get("pyghidra_vs_fission", {}) or {}
    watchlist = ((summary.row_fidelity_targets or {}).get("pyghidra_vs_fission") or {})
    return template.render(
        benchmark=benchmark,
        summary=summary.model_dump(mode="json", exclude_none=True),
        quality=quality,
        coverage=coverage,
        owner_metrics=(summary.owner_metrics or {}).get("fission", {}),
        shape_drift_metrics=(summary.shape_drift_metrics or {}).get("fission", {}),
        ghidra_action_metrics=(summary.ghidra_action_metrics or {}).get("fission", {}),
        blockgraph_region_metrics=(summary.blockgraph_region_metrics or {}).get("fission", {}),
        watchlist=watchlist,
        baseline_gate=benchmark.get("baseline_regression_gate") or {},
    ).rstrip() + "\n"


def render_corpus_benchmark_markdown(corpus_summary: dict[str, Any]) -> str:
    artifact = VerboseCorpusBenchmarkArtifact.model_validate(corpus_summary)
    template = _env().get_template("corpus_benchmark.md.j2")
    return template.render(
        payload=artifact.model_dump(mode="json", exclude_none=True),
        corpus=artifact.corpus_summary.model_dump(mode="json", exclude_none=True),
        binaries=[row.model_dump(mode="json", exclude_none=True) for row in artifact.binaries],
    ).rstrip() + "\n"


def render_previous_comparison_markdown(comparison: dict[str, Any]) -> str:
    template = _env().get_template("previous_comparison.md.j2")
    return template.render(payload=comparison).rstrip() + "\n"


def render_baseline_regression_markdown(report: dict[str, Any]) -> str:
    template = _env().get_template("baseline_regression.md.j2")
    return template.render(payload=report).rstrip() + "\n"
