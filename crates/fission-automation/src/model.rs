use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FactSourcesPresent {
    #[serde(default)]
    pub dwarf: bool,
    #[serde(default)]
    pub pdb: bool,
    #[serde(default)]
    pub loader: bool,
    #[serde(default)]
    pub native_inferred: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExplicitFactBreakdown {
    #[serde(default)]
    pub param_count: usize,
    #[serde(default)]
    pub local_count: usize,
    #[serde(default)]
    pub return_count: usize,
    #[serde(default)]
    pub pdb_type_count: usize,
    #[serde(default)]
    pub native_type_count: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProvenanceFactBreakdown {
    #[serde(default)]
    pub dwarf_type_count: usize,
    #[serde(default)]
    pub pdb_type_count: usize,
    #[serde(default)]
    pub native_type_count: usize,
    #[serde(default)]
    pub loader_type_count: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SourcePresenceCounts {
    #[serde(default)]
    pub dwarf: usize,
    #[serde(default)]
    pub pdb: usize,
    #[serde(default)]
    pub loader: usize,
    #[serde(default)]
    pub native_inferred: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExplicitBreakdownTotals {
    #[serde(default)]
    pub param_count: usize,
    #[serde(default)]
    pub local_count: usize,
    #[serde(default)]
    pub return_count: usize,
    #[serde(default)]
    pub pdb_type_count: usize,
    #[serde(default)]
    pub native_type_count: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProvenanceSurfaceTotals {
    #[serde(default)]
    pub dwarf_nonzero_rows: usize,
    #[serde(default)]
    pub pdb_nonzero_rows: usize,
    #[serde(default)]
    pub native_nonzero_rows: usize,
    #[serde(default)]
    pub loader_nonzero_rows: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InventoryRow {
    #[serde(default)]
    pub binary: String,
    #[serde(default)]
    pub binary_path: String,
    #[serde(default)]
    pub address: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub row_status: String,
    #[serde(default)]
    pub row_error_kind: Option<String>,
    #[serde(default)]
    pub row_error_message: Option<String>,
    #[serde(default)]
    pub has_dwarf_function: bool,
    #[serde(default)]
    pub dwarf_param_count: usize,
    #[serde(default)]
    pub dwarf_local_count: usize,
    #[serde(default)]
    pub has_dwarf_return_type: bool,
    #[serde(default)]
    pub loader_type_count: usize,
    #[serde(default)]
    pub explicit_fact_total: usize,
    #[serde(default)]
    pub fact_density_score: i32,
    #[serde(default)]
    pub fact_sources_present: FactSourcesPresent,
    #[serde(default)]
    pub explicit_fact_breakdown: ExplicitFactBreakdown,
    #[serde(default)]
    pub provenance_fact_breakdown: ProvenanceFactBreakdown,
    #[serde(default)]
    pub admission_block_stage: String,
    #[serde(default)]
    pub inventory_surface_gap: bool,
    #[serde(default)]
    pub preview_direct_success: bool,
    #[serde(default)]
    pub preview_fallback_kind: Option<String>,
    #[serde(default)]
    pub preview_fallback_kind_refined: Option<String>,
    #[serde(default)]
    pub preview_fallback_reason: Option<String>,
    #[serde(default)]
    pub preview_block_signature: Option<String>,
    #[serde(default)]
    pub preview_block_detail: Option<String>,
    #[serde(default)]
    pub recovery_strategy_attempted: Option<String>,
    #[serde(default)]
    pub recovery_strategy_applied: Option<String>,
    #[serde(default)]
    pub recovery_outcome: Option<String>,
    #[serde(default)]
    pub recovery_source_signature: Option<String>,
    #[serde(default)]
    pub recovery_structuring_mode: Option<String>,
    #[serde(default)]
    pub recovery_goto_count_before: Option<usize>,
    #[serde(default)]
    pub recovery_goto_count_after: Option<usize>,
    #[serde(default)]
    pub recovery_hint_surface_before: Option<usize>,
    #[serde(default)]
    pub recovery_hint_surface_after: Option<usize>,
    #[serde(default)]
    pub recovery_quality_flags: Vec<String>,
    #[serde(default)]
    pub preview_surface_kind: Option<String>,
    #[serde(default)]
    pub pcode_block_count: usize,
    #[serde(default)]
    pub pcode_op_count: usize,
    #[serde(default)]
    pub has_indirect_control_flow: bool,
    #[serde(default)]
    pub auto_eligible: bool,
    #[serde(default)]
    pub strict_explicit_candidate: bool,
    #[serde(default)]
    pub heuristic_surface_candidate: bool,
    #[serde(default)]
    pub reason_tags: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InventorySummary {
    #[serde(default)]
    pub binary: String,
    #[serde(default)]
    pub binary_path: String,
    #[serde(default)]
    pub format: String,
    #[serde(default)]
    pub arch_spec: String,
    #[serde(default)]
    pub functions_total: usize,
    #[serde(default)]
    pub rows_emitted: usize,
    #[serde(default)]
    pub chunks_completed: usize,
    #[serde(default)]
    pub chunk_size: usize,
    #[serde(default)]
    pub direct_success_count: usize,
    #[serde(default)]
    pub preview_failure_count: usize,
    #[serde(default)]
    pub panic_recovered_count: usize,
    #[serde(default)]
    pub internal_error_count: usize,
    #[serde(default)]
    pub explicit_fact_nonzero_count: usize,
    #[serde(default)]
    pub source_presence_counts: SourcePresenceCounts,
    #[serde(default)]
    pub explicit_breakdown_totals: ExplicitBreakdownTotals,
    #[serde(default)]
    pub provenance_surface_totals: ProvenanceSurfaceTotals,
    #[serde(default)]
    pub inventory_surface_gap_count: usize,
    #[serde(default)]
    pub aligned_with_zero_explicit_count: usize,
    #[serde(default)]
    pub strict_explicit_candidate_count: usize,
    #[serde(default)]
    pub heuristic_surface_candidate_count: usize,
    #[serde(default)]
    pub failure_kind_counts: BTreeMap<String, usize>,
    #[serde(default)]
    pub row_error_kind_counts: BTreeMap<String, usize>,
    #[serde(default)]
    pub recovery_strategy_attempted_counts: BTreeMap<String, usize>,
    #[serde(default)]
    pub recovery_strategy_applied_counts: BTreeMap<String, usize>,
    #[serde(default)]
    pub recovery_outcome_counts: BTreeMap<String, usize>,
    #[serde(default)]
    pub recovery_quality_flag_counts: BTreeMap<String, usize>,
    #[serde(default)]
    pub recovery_structuring_mode_counts: BTreeMap<String, usize>,
    #[serde(default)]
    pub suppressed_stderr_count: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SourceMeta {
    #[serde(default)]
    pub binary: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub expected_preview_supported: Option<bool>,
    #[serde(default)]
    pub observed_preview_supported: Option<bool>,
    #[serde(default)]
    pub observed_preview_failure_kind: Option<String>,
    #[serde(default)]
    pub observed_preview_failure_reason: Option<String>,
    #[serde(default)]
    pub admission_alignment: Option<String>,
    #[serde(default)]
    pub rescan_priority: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaneTarget {
    pub binary: String,
    pub path: PathBuf,
    pub role: String,
    pub default_functions_limit: Option<usize>,
    pub default_timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlignedExplicitCandidate {
    pub binary: String,
    pub path: String,
    pub address: String,
    pub name: String,
    pub explicit_fact_total: usize,
    pub fact_density_score: i32,
    pub preview_direct_success: bool,
    pub preview_fallback_kind_refined: Option<String>,
    pub pcode_op_count: usize,
    pub preview_surface_kind: Option<String>,
    pub fact_sources_present: FactSourcesPresent,
    pub explicit_fact_breakdown: ExplicitFactBreakdown,
    pub provenance_fact_breakdown: ProvenanceFactBreakdown,
    pub admission_block_stage: String,
    pub inventory_surface_gap: bool,
    pub reason_tags: Vec<String>,
    pub source_binary: String,
    pub source_admission_alignment: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockedExplicitCandidate {
    pub binary: String,
    pub path: String,
    pub address: String,
    pub name: String,
    pub explicit_fact_total: usize,
    pub fact_density_score: i32,
    pub preview_direct_success: bool,
    pub block_reason: String,
    pub pcode_op_count: usize,
    pub has_indirect_control_flow: bool,
    pub fact_sources_present: FactSourcesPresent,
    pub explicit_fact_breakdown: ExplicitFactBreakdown,
    pub provenance_fact_breakdown: ProvenanceFactBreakdown,
    pub admission_block_stage: String,
    pub inventory_surface_gap: bool,
    pub reason_tags: Vec<String>,
    pub source_admission_alignment: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CuratedQualityEntry {
    pub binary: String,
    pub address: String,
    pub name: String,
    pub fact_density_score: i32,
    pub quality_potential_score: i32,
    pub reason_tags: Vec<String>,
}
