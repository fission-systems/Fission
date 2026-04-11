use super::super::*;
use fission_pcode::NirBuildStats;

#[derive(Debug, Serialize)]
pub(crate) struct NirCandidateInventory {
    pub(crate) binary: String,
    pub(crate) binary_path: String,
    pub(crate) format: String,
    pub(crate) arch_spec: String,
    pub(crate) candidate_count: usize,
    pub(crate) candidates: Vec<NirCandidateEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct NirCandidateEntry {
    pub(crate) binary: String,
    pub(crate) address: String,
    pub(crate) name: String,
    pub(crate) row_status: String,
    pub(crate) row_error_kind: Option<String>,
    pub(crate) row_error_message: Option<String>,
    pub(crate) row_error_verbose: Option<String>,
    pub(crate) has_dwarf_function: bool,
    pub(crate) dwarf_param_count: usize,
    pub(crate) dwarf_local_count: usize,
    pub(crate) has_dwarf_return_type: bool,
    pub(crate) loader_type_count: usize,
    pub(crate) fact_density_score: i32,
    pub(crate) nir_direct_success: bool,
    pub(crate) nir_fallback_kind: Option<String>,
    pub(crate) nir_fallback_kind_refined: Option<String>,
    pub(crate) nir_fallback_reason: Option<String>,
    pub(crate) nir_block_signature: Option<String>,
    pub(crate) nir_block_detail: Option<String>,
    pub(crate) preview_direct_success: bool,
    pub(crate) preview_fallback_kind: Option<String>,
    pub(crate) preview_fallback_kind_refined: Option<String>,
    pub(crate) preview_fallback_reason: Option<String>,
    pub(crate) preview_block_signature: Option<String>,
    pub(crate) preview_block_detail: Option<String>,
    pub(crate) recovery_strategy_attempted: Option<String>,
    pub(crate) recovery_strategy_applied: Option<String>,
    pub(crate) recovery_outcome: Option<String>,
    pub(crate) recovery_source_signature: Option<String>,
    pub(crate) recovery_structuring_mode: Option<String>,
    pub(crate) recovery_goto_count_before: Option<usize>,
    pub(crate) recovery_goto_count_after: Option<usize>,
    pub(crate) recovery_hint_surface_before: Option<usize>,
    pub(crate) recovery_hint_surface_after: Option<usize>,
    pub(crate) recovery_quality_flags: Vec<String>,
    pub(crate) pcode_block_count: usize,
    pub(crate) pcode_op_count: usize,
    pub(crate) has_indirect_control_flow: bool,
    pub(crate) has_preserved_indirect_surface: bool,
    pub(crate) has_unresolved_unsupported_indirect: bool,
    pub(crate) has_dispatcher_recovery: bool,
    pub(crate) auto_eligible: bool,
    pub(crate) nir_goto_count: Option<usize>,
    pub(crate) nir_output_class: Option<String>,
    pub(crate) nir_build_stats: Option<NirBuildStats>,
    pub(crate) nir_surface_kind: Option<String>,
    pub(crate) preview_surface_kind: Option<String>,
    pub(crate) quality_potential_score: i32,
    pub(crate) reason_tags: Vec<String>,
    pub(crate) preview_hint_stats: Option<NirHintStats>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct NirCandidateScanSummary {
    pub(crate) binary: String,
    pub(crate) binary_path: String,
    pub(crate) format: String,
    pub(crate) arch_spec: String,
    pub(crate) functions_total: usize,
    pub(crate) addresses_scanned: usize,
    pub(crate) chunks_completed: usize,
    pub(crate) chunk_size: usize,
    pub(crate) timeout_count: usize,
    pub(crate) nir_failure_count: usize,
    pub(crate) preview_failure_count: usize,
    pub(crate) panic_recovered_count: usize,
    pub(crate) internal_error_count: usize,
    pub(crate) nonzero_explicit_candidates: usize,
    pub(crate) strict_explicit_candidates: usize,
    pub(crate) failure_kind_counts: BTreeMap<String, usize>,
    pub(crate) row_error_kind_counts: BTreeMap<String, usize>,
    pub(crate) recovery_quality_flag_counts: BTreeMap<String, usize>,
    pub(crate) recovery_structuring_mode_counts: BTreeMap<String, usize>,
    pub(crate) nir_output_class_counts: BTreeMap<String, usize>,
    pub(crate) nir_build_stats_totals: NirBuildStats,
    pub(crate) suppressed_stderr_count: usize,
    pub(crate) resume_loaded_rows: usize,
}

pub(crate) type PreviewCandidateInventory = NirCandidateInventory;
pub(crate) type PreviewCandidateEntry = NirCandidateEntry;
pub(crate) type PreviewCandidateScanSummary = NirCandidateScanSummary;

pub(crate) struct ScopedQuietPanicHook {
    previous: Option<Box<dyn Fn(&std::panic::PanicHookInfo<'_>) + Sync + Send + 'static>>,
    suppressed: Arc<AtomicUsize>,
}

impl ScopedQuietPanicHook {
    pub(crate) fn install(enabled: bool) -> Option<Self> {
        if !enabled {
            return None;
        }
        let suppressed = Arc::new(AtomicUsize::new(0));
        let suppressed_for_hook = Arc::clone(&suppressed);
        let previous = take_hook();
        set_hook(Box::new(move |_| {
            suppressed_for_hook.fetch_add(1, Ordering::Relaxed);
        }));
        Some(Self {
            previous: Some(previous),
            suppressed,
        })
    }

    pub(crate) fn suppressed_count(&self) -> usize {
        self.suppressed.load(Ordering::Relaxed)
    }
}

impl Drop for ScopedQuietPanicHook {
    fn drop(&mut self) {
        if let Some(previous) = self.previous.take() {
            set_hook(previous);
        }
    }
}
