use fission_pcode::NirBuildStats;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::io;

#[derive(Debug, Serialize)]
pub(super) struct FactSourcesPresent {
    pub(super) dwarf: bool,
    pub(super) pdb: bool,
    pub(super) loader: bool,
    pub(super) native_inferred: bool,
}

#[derive(Debug, Serialize)]
pub(super) struct ExplicitFactBreakdown {
    pub(super) param_count: usize,
    pub(super) local_count: usize,
    pub(super) return_count: usize,
    pub(super) pdb_type_count: usize,
    pub(super) native_type_count: usize,
}

#[derive(Debug, Serialize)]
pub(super) struct ProvenanceFactBreakdown {
    pub(super) dwarf_type_count: usize,
    pub(super) pdb_type_count: usize,
    pub(super) native_type_count: usize,
    pub(super) loader_type_count: usize,
}

#[derive(Debug, Serialize)]
pub(super) struct FunctionFactsInventoryRow {
    pub(super) binary: String,
    pub(super) binary_path: String,
    pub(super) address: String,
    pub(super) name: String,
    pub(super) has_dwarf_function: bool,
    pub(super) dwarf_param_count: usize,
    pub(super) dwarf_local_count: usize,
    pub(super) has_dwarf_return_type: bool,
    pub(super) loader_type_count: usize,
    pub(super) explicit_fact_total: usize,
    pub(super) fact_density_score: i32,
    pub(super) fact_sources_present: FactSourcesPresent,
    pub(super) explicit_fact_breakdown: ExplicitFactBreakdown,
    pub(super) provenance_fact_breakdown: ProvenanceFactBreakdown,
    pub(super) admission_block_stage: String,
    pub(super) inventory_surface_gap: bool,
    pub(super) nir_direct_success: bool,
    pub(super) nir_fallback_kind: Option<String>,
    pub(super) nir_fallback_kind_refined: Option<String>,
    pub(super) nir_fallback_reason: Option<String>,
    pub(super) nir_block_signature: Option<String>,
    pub(super) nir_block_detail: Option<String>,
    pub(super) preview_direct_success: bool,
    pub(super) preview_fallback_kind: Option<String>,
    pub(super) preview_fallback_kind_refined: Option<String>,
    pub(super) preview_fallback_reason: Option<String>,
    pub(super) preview_block_signature: Option<String>,
    pub(super) preview_block_detail: Option<String>,
    pub(super) recovery_strategy_attempted: Option<String>,
    pub(super) recovery_strategy_applied: Option<String>,
    pub(super) recovery_outcome: Option<String>,
    pub(super) recovery_source_signature: Option<String>,
    pub(super) recovery_structuring_mode: Option<String>,
    pub(super) recovery_goto_count_before: Option<usize>,
    pub(super) recovery_goto_count_after: Option<usize>,
    pub(super) recovery_hint_surface_before: Option<usize>,
    pub(super) recovery_hint_surface_after: Option<usize>,
    pub(super) recovery_quality_flags: Vec<String>,
    pub(super) nir_surface_kind: Option<String>,
    pub(super) preview_surface_kind: Option<String>,
    pub(super) pcode_block_count: usize,
    pub(super) pcode_op_count: usize,
    pub(super) has_indirect_control_flow: bool,
    pub(super) has_preserved_indirect_surface: bool,
    pub(super) has_unresolved_unsupported_indirect: bool,
    pub(super) has_dispatcher_recovery: bool,
    pub(super) auto_eligible: bool,
    pub(super) nir_goto_count: Option<usize>,
    pub(super) nir_output_class: Option<String>,
    pub(super) nir_build_stats: Option<NirBuildStats>,
    pub(super) strict_explicit_candidate: bool,
    pub(super) heuristic_surface_candidate: bool,
    pub(super) reason_tags: Vec<String>,
    pub(super) row_status: String,
    pub(super) row_error_kind: Option<String>,
    pub(super) row_error_message: Option<String>,
}

#[derive(Debug, Default, Serialize)]
pub(super) struct SourcePresenceCounts {
    pub(super) dwarf: usize,
    pub(super) pdb: usize,
    pub(super) loader: usize,
    pub(super) native_inferred: usize,
}

#[derive(Debug, Default, Serialize)]
pub(super) struct ExplicitBreakdownTotals {
    pub(super) param_count: usize,
    pub(super) local_count: usize,
    pub(super) return_count: usize,
    pub(super) pdb_type_count: usize,
    pub(super) native_type_count: usize,
}

#[derive(Debug, Default, Serialize)]
pub(super) struct ProvenanceSurfaceTotals {
    pub(super) dwarf_nonzero_rows: usize,
    pub(super) pdb_nonzero_rows: usize,
    pub(super) native_nonzero_rows: usize,
    pub(super) loader_nonzero_rows: usize,
}

#[derive(Debug, Default, Serialize)]
pub(super) struct FunctionFactsInventorySummary {
    pub(super) binary: String,
    pub(super) binary_path: String,
    pub(super) format: String,
    pub(super) arch_spec: String,
    pub(super) functions_total: usize,
    pub(super) functions_discovered_total: usize,
    pub(super) functions_selected_total: usize,
    pub(super) functions_excluded_import_count: usize,
    pub(super) functions_excluded_runtime_wrapper_count: usize,
    pub(super) include_nonuser_functions: bool,
    pub(super) rows_emitted: usize,
    pub(super) chunks_completed: usize,
    pub(super) chunk_size: usize,
    pub(super) direct_success_count: usize,
    pub(super) nir_failure_count: usize,
    pub(super) preview_failure_count: usize,
    pub(super) panic_recovered_count: usize,
    pub(super) internal_error_count: usize,
    pub(super) explicit_fact_nonzero_count: usize,
    pub(super) source_presence_counts: SourcePresenceCounts,
    pub(super) explicit_breakdown_totals: ExplicitBreakdownTotals,
    pub(super) provenance_surface_totals: ProvenanceSurfaceTotals,
    pub(super) inventory_surface_gap_count: usize,
    pub(super) aligned_with_zero_explicit_count: usize,
    pub(super) strict_explicit_candidate_count: usize,
    pub(super) heuristic_surface_candidate_count: usize,
    pub(super) failure_kind_counts: BTreeMap<String, usize>,
    pub(super) row_error_kind_counts: BTreeMap<String, usize>,
    pub(super) recovery_strategy_attempted_counts: BTreeMap<String, usize>,
    pub(super) recovery_strategy_applied_counts: BTreeMap<String, usize>,
    pub(super) recovery_outcome_counts: BTreeMap<String, usize>,
    pub(super) recovery_quality_flag_counts: BTreeMap<String, usize>,
    pub(super) recovery_structuring_mode_counts: BTreeMap<String, usize>,
    pub(super) nir_output_class_counts: BTreeMap<String, usize>,
    pub(super) nir_build_stats_totals: NirBuildStats,
    pub(super) suppressed_stderr_count: usize,
}

pub(super) fn write_inventory_summary(
    path: &std::path::Path,
    summary: &FunctionFactsInventorySummary,
) -> io::Result<()> {
    let body = serde_json::to_string_pretty(summary)
        .map_err(|e| io::Error::other(format!("JSON serialization failed: {e}")))?;
    fs::write(path, body)
}
