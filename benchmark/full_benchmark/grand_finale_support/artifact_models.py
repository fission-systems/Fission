from __future__ import annotations

from typing import Any

from pydantic import BaseModel, ConfigDict, Field


class FlexibleModel(BaseModel):
    model_config = ConfigDict(extra="allow", populate_by_name=True)


class VerboseEngineSummary(FlexibleModel):
    function_count: int = 0
    goto_total: int = 0
    top_level_label_total: int = 0
    replacement_plan_rejected_alias_unsafe_count: int = 0
    replacement_plan_rejected_missing_merge_count: int = 0
    replacement_plan_rejected_representative_root_attribution_count: int = 0
    replacement_plan_rejected_temp_only_representative_lifecycle_count: int = 0
    replacement_plan_rejected_dead_temp_representative_count: int = 0
    representative_downgrade_count: int = 0
    representative_downgrade_no_aliassafe_source_count: int = 0
    representative_downgrade_join_conflict_count: int = 0
    materialization_stabilized_count: int = 0
    generic_local_name_sum: float = 0.0
    generic_param_name_sum: float = 0.0
    unknown_type_var_total: float = 0.0
    ptr_offset_total: float = 0.0
    index_expr_total: float = 0.0
    heuristic_avg_line_length_mean: float = 0.0
    heuristic_max_brace_nesting_mean: float = 0.0
    synthetic_helper_call_total: float = 0.0


class VerboseSingleSummary(FlexibleModel):
    binary: str | None = None
    generated_at: str | None = None
    public_summary_line: str | None = None
    quality: dict[str, Any] = Field(default_factory=dict)
    coverage: dict[str, Any] = Field(default_factory=dict)
    row_fidelity_targets: dict[str, Any] = Field(default_factory=dict)
    owner_metrics: dict[str, Any] = Field(default_factory=dict)
    shape_drift_metrics: dict[str, Any] = Field(default_factory=dict)
    normalize_pass_metrics: dict[str, Any] = Field(default_factory=dict)
    ghidra_action_metrics: dict[str, Any] = Field(default_factory=dict)
    blockgraph_region_metrics: dict[str, Any] = Field(default_factory=dict)
    target_structuring_rows: list[dict[str, Any]] = Field(default_factory=list)
    giant_function_speed_family_counts: dict[str, Any] = Field(default_factory=dict)
    max_pathological_examples: list[dict[str, Any]] = Field(default_factory=list)
    engines: dict[str, Any] = Field(default_factory=dict)
    samples: dict[str, Any] = Field(default_factory=dict)


class VerboseSingleBenchmarkArtifact(FlexibleModel):
    summary: VerboseSingleSummary
    baseline_regression_gate: dict[str, Any] | None = None


class VerboseCorpusBinaryRow(FlexibleModel):
    id: str | None = None
    arch: str | None = None
    role: str | None = None
    weight: int | float | None = None
    avg_normalized_similarity: float | None = None
    coverage_ratio_pct: float | None = None
    direct_success: str | None = None
    row_fidelity_gate_status: str | None = None
    watchlist_source: str | None = None
    watchlist_diagnostics: dict[str, Any] = Field(default_factory=dict)
    owner_metrics: dict[str, Any] = Field(default_factory=dict)
    shape_drift_metrics: dict[str, Any] = Field(default_factory=dict)
    normalize_pass_metrics: dict[str, Any] = Field(default_factory=dict)
    ghidra_action_metrics: dict[str, Any] = Field(default_factory=dict)
    blockgraph_region_metrics: dict[str, Any] = Field(default_factory=dict)
    target_structuring_rows: list[dict[str, Any]] = Field(default_factory=list)
    giant_function_speed_family_counts: dict[str, Any] = Field(default_factory=dict)
    max_pathological_examples: list[dict[str, Any]] = Field(default_factory=list)
    eligibility: dict[str, Any] = Field(default_factory=dict)


class VerboseCorpusSummary(FlexibleModel):
    binary_count: int = 0
    release_candidate_count: int = 0
    release_eligible_count: int = 0
    weighted_avg_normalized_similarity: float = 0.0
    coverage_non_worse_count: int = 0
    direct_success_non_worse_count: int = 0
    regressions: list[str] = Field(default_factory=list)
    row_regression_reasons: dict[str, Any] = Field(default_factory=dict)
    status: str | None = None


class VerboseCorpusBenchmarkArtifact(FlexibleModel):
    generated_at: str | None = None
    suite_tier: str = "release"
    gate_mode: str = "advisory"
    comparable_to_baseline: bool = False
    baseline_artifact: str | None = None
    release_promotion_allowed: bool = False
    promotion_blockers: list[str] = Field(default_factory=list)
    manifest: dict[str, Any] = Field(default_factory=dict)
    corpus_summary: VerboseCorpusSummary
    binaries: list[VerboseCorpusBinaryRow] = Field(default_factory=list)
    owner_metric_totals: dict[str, Any] = Field(default_factory=dict)
    owner_metric_totals_per_binary: dict[str, Any] = Field(default_factory=dict)
    shape_drift_totals: dict[str, Any] = Field(default_factory=dict)
    shape_drift_totals_per_binary: dict[str, Any] = Field(default_factory=dict)
    normalize_pass_metric_totals: dict[str, Any] = Field(default_factory=dict)
    normalize_pass_metrics_per_binary: dict[str, Any] = Field(default_factory=dict)
    ghidra_action_metric_totals: dict[str, Any] = Field(default_factory=dict)
    ghidra_action_metrics_per_binary: dict[str, Any] = Field(default_factory=dict)
    blockgraph_region_metric_totals: dict[str, Any] = Field(default_factory=dict)
    blockgraph_region_metrics_per_binary: dict[str, Any] = Field(default_factory=dict)
    blockgraph_region_rejection_totals: dict[str, Any] = Field(default_factory=dict)
    target_structuring_rows: list[dict[str, Any]] = Field(default_factory=list)
    giant_function_speed_family_totals: dict[str, Any] = Field(default_factory=dict)
    max_pathological_examples: list[dict[str, Any]] = Field(default_factory=list)
    arch_summary: dict[str, Any] = Field(default_factory=dict)
    watchlist_source_per_binary: dict[str, Any] = Field(default_factory=dict)
    watchlist_reason_counts: dict[str, Any] = Field(default_factory=dict)
    cross_binary_degraded_watchlist: list[dict[str, Any]] = Field(default_factory=list)


class CompactArchSummary(FlexibleModel):
    binary_count: int = 0
    release_candidate_count: int = 0
    weighted_avg_normalized_similarity: float = 0.0
    coverage_non_worse_count: int = 0
    direct_success_non_worse_count: int = 0
    failed_binary_ids: list[str] = Field(default_factory=list)
    owner_metric_totals: dict[str, float] = Field(default_factory=dict)
    shape_drift_totals: dict[str, float] = Field(default_factory=dict)


class CompactBinaryRow(FlexibleModel):
    id: str
    arch: str = "unknown"
    role: str = "unknown"
    avg_normalized_similarity: float = 0.0
    coverage_ratio_pct: float = 0.0
    direct_success: str = "unknown"
    row_fidelity_gate_status: str = "unknown"
    watchlist_source: str = "unknown"
    selected_watchlist_reasons: list[str] = Field(default_factory=list)
    owner_metrics: dict[str, float] = Field(default_factory=dict)
    shape_drift_metrics: dict[str, float] = Field(default_factory=dict)
    normalize_pass_metrics: dict[str, float] = Field(default_factory=dict)
    ghidra_action_metrics: dict[str, float] = Field(default_factory=dict)
    blockgraph_region_metrics: dict[str, float] = Field(default_factory=dict)
    eligibility_reason: str = "unknown"


class CompactRowExample(FlexibleModel):
    address: str | None = None
    binary_id: str | None = None
    function_name: str | None = None
    normalized_similarity_delta: float | None = None
    current_normalized_similarity: float | None = None
    previous_normalized_similarity: float | None = None
    selected_because: str | None = None
    reason_tags: list[str] = Field(default_factory=list)


class CompactSingleBenchmarkSummary(FlexibleModel):
    summary_kind: str = "compact_single_benchmark"
    compact_version: int = 1
    binary_path: str | None = None
    generated_at: str | None = None
    comparable_to_baseline: bool = False
    baseline_artifact: str | None = None
    avg_normalized_similarity: float = 0.0
    aggregate_normalized_similarity: float = 0.0
    both_success_rate_pct: float = 0.0
    owner_metrics: dict[str, float] = Field(default_factory=dict)
    shape_drift_metrics: dict[str, float] = Field(default_factory=dict)
    normalize_pass_metrics: dict[str, float] = Field(default_factory=dict)
    ghidra_action_metrics: dict[str, float] = Field(default_factory=dict)
    blockgraph_region_metrics: dict[str, float] = Field(default_factory=dict)
    giant_function_speed_family_counts: dict[str, int] = Field(default_factory=dict)
    watchlist_diagnostics: dict[str, Any] = Field(default_factory=dict)
    baseline_blockers: list[str] = Field(default_factory=list)
    top_regressions: list[CompactRowExample] = Field(default_factory=list)
    top_row_examples: list[CompactRowExample] = Field(default_factory=list)
    max_pathological_examples: list[dict[str, Any]] = Field(default_factory=list)
    target_structuring_rows: list[dict[str, Any]] = Field(default_factory=list)


class CompactCorpusBenchmarkSummary(FlexibleModel):
    summary_kind: str = "compact_corpus_benchmark"
    compact_version: int = 1
    manifest_name: str | None = None
    generated_at: str | None = None
    suite_tier: str = "release"
    gate_mode: str = "advisory"
    comparable_to_baseline: bool = False
    baseline_artifact: str | None = None
    release_promotion_allowed: bool = False
    promotion_blockers: list[str] = Field(default_factory=list)
    weighted_avg_normalized_similarity: float = 0.0
    x86_summary: CompactArchSummary | None = None
    x64_summary: CompactArchSummary | None = None
    owner_metric_totals: dict[str, float] = Field(default_factory=dict)
    shape_drift_totals: dict[str, float] = Field(default_factory=dict)
    normalize_pass_metric_totals: dict[str, float] = Field(default_factory=dict)
    ghidra_action_metric_totals: dict[str, float] = Field(default_factory=dict)
    blockgraph_region_metric_totals: dict[str, float] = Field(default_factory=dict)
    giant_function_speed_family_totals: dict[str, int] = Field(default_factory=dict)
    watchlist_reason_counts: dict[str, int] = Field(default_factory=dict)
    top_degraded_rows: list[CompactRowExample] = Field(default_factory=list)
    per_binary_rows: list[CompactBinaryRow] = Field(default_factory=list)
    max_pathological_examples: list[dict[str, Any]] = Field(default_factory=list)
    target_structuring_rows: list[dict[str, Any]] = Field(default_factory=list)
