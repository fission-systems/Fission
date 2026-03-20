use super::schema::{
    ExplicitFactBreakdown, FactSourcesPresent, FunctionFactsInventoryRow,
    FunctionFactsInventorySummary, ProvenanceFactBreakdown,
};
use crate::cli::oneshot::decompile::{
    PreviewCandidateEntry, PreviewCandidateScanSummary, update_scan_summary,
};
use fission_static::analysis::decomp::{FactStore, FunctionFacts};

pub(super) fn heuristic_surface_candidate(entry: &PreviewCandidateEntry) -> bool {
    let hint_stats = entry.preview_hint_stats;
    let heuristic_hits = hint_stats.is_some_and(|stats| {
        stats.heuristic_pointer_alias_hits > 0
            || stats.heuristic_local_surface_hits > 0
            || stats.derived_origin_type_hits > 0
    });
    let has_reason_tag = entry.reason_tags.iter().any(|tag| {
        matches!(
            tag.as_str(),
            "heuristic_pointer_alias" | "heuristic_local_surface" | "slot_alias_candidate"
        )
    });
    entry.preview_direct_success
        && !entry.has_indirect_control_flow
        && (heuristic_hits || has_reason_tag)
}

pub(super) fn detect_pdb_source_present(binary: &fission_loader::loader::LoadedBinary) -> bool {
    binary
        .inner()
        .pdb_debug_info
        .as_ref()
        .is_some_and(|info| info.has_codeview)
}

fn fact_sources_present(
    snapshot: &FunctionFacts,
    entry: &PreviewCandidateEntry,
    pdb_source_present: bool,
) -> FactSourcesPresent {
    FactSourcesPresent {
        dwarf: entry.has_dwarf_function,
        pdb: pdb_source_present,
        loader: snapshot.loader_type_fact_count() > 0 || entry.loader_type_count > 0,
        native_inferred: snapshot.native_type_fact_count() > 0,
    }
}

fn explicit_fact_breakdown(
    entry: &PreviewCandidateEntry,
    snapshot: &FunctionFacts,
) -> ExplicitFactBreakdown {
    ExplicitFactBreakdown {
        param_count: entry.dwarf_param_count,
        local_count: entry.dwarf_local_count,
        return_count: usize::from(entry.has_dwarf_return_type),
        pdb_type_count: snapshot.pdb_type_fact_count(),
        native_type_count: snapshot.native_type_fact_count(),
    }
}

fn explicit_fact_total(breakdown: &ExplicitFactBreakdown) -> usize {
    breakdown.param_count
        + breakdown.local_count
        + breakdown.return_count
        + breakdown.pdb_type_count
        + breakdown.native_type_count
}

fn provenance_fact_breakdown(snapshot: &FunctionFacts) -> ProvenanceFactBreakdown {
    ProvenanceFactBreakdown {
        dwarf_type_count: snapshot.dwarf_type_fact_count(),
        pdb_type_count: snapshot.pdb_type_fact_count(),
        native_type_count: snapshot.native_type_fact_count(),
        loader_type_count: snapshot.loader_type_fact_count(),
    }
}

fn inventory_surface_gap(sources: &FactSourcesPresent, explicit_fact_total: usize) -> bool {
    explicit_fact_total == 0 && (sources.dwarf || sources.pdb || sources.native_inferred)
}

fn strict_explicit_candidate_row(
    entry: &PreviewCandidateEntry,
    explicit_fact_total: usize,
) -> bool {
    explicit_fact_total >= 2
        && entry.preview_direct_success
        && !entry.has_indirect_control_flow
        && entry.pcode_op_count <= 800
}

fn admission_block_stage(entry: &PreviewCandidateEntry, inventory_surface_gap: bool) -> String {
    if entry.preview_direct_success {
        return "none".to_string();
    }
    if inventory_surface_gap {
        return "inventory_surface".to_string();
    }
    match entry
        .row_error_kind
        .as_deref()
        .or(entry.preview_fallback_kind_refined.as_deref())
    {
        Some(
            "preview_architecture_unsupported"
            | "preview_format_unsupported"
            | "preview_frontend_reject",
        ) => "admission".to_string(),
        Some(_) => "preview".to_string(),
        None => "none".to_string(),
    }
}

pub(super) fn to_inventory_row(
    binary_path: &std::path::Path,
    pdb_source_present: bool,
    fact_store: &FactStore,
    entry: PreviewCandidateEntry,
) -> FunctionFactsInventoryRow {
    let address =
        u64::from_str_radix(entry.address.trim_start_matches("0x"), 16).unwrap_or_default();
    let snapshot = fact_store.function_facts_snapshot(address);
    let explicit_fact_breakdown = explicit_fact_breakdown(&entry, &snapshot);
    let explicit_fact_total = explicit_fact_total(&explicit_fact_breakdown);
    let strict_explicit = strict_explicit_candidate_row(&entry, explicit_fact_total);
    let heuristic_surface = heuristic_surface_candidate(&entry);
    let fact_sources_present = fact_sources_present(&snapshot, &entry, pdb_source_present);
    let provenance_fact_breakdown = provenance_fact_breakdown(&snapshot);
    let inventory_surface_gap = inventory_surface_gap(&fact_sources_present, explicit_fact_total);
    let admission_block_stage = admission_block_stage(&entry, inventory_surface_gap);
    let loader_type_count = snapshot.loader_type_fact_count();
    let resolved_name = snapshot
        .chosen_name
        .as_ref()
        .map(|fact| fact.name.clone())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| entry.name.clone());
    FunctionFactsInventoryRow {
        binary: entry.binary,
        binary_path: binary_path.display().to_string(),
        address: entry.address,
        name: resolved_name,
        has_dwarf_function: entry.has_dwarf_function,
        dwarf_param_count: entry.dwarf_param_count,
        dwarf_local_count: entry.dwarf_local_count,
        has_dwarf_return_type: entry.has_dwarf_return_type,
        loader_type_count,
        explicit_fact_total,
        fact_density_score: entry.fact_density_score,
        fact_sources_present,
        explicit_fact_breakdown,
        provenance_fact_breakdown,
        admission_block_stage,
        inventory_surface_gap,
        nir_direct_success: entry.nir_direct_success,
        nir_fallback_kind: entry.nir_fallback_kind.clone(),
        nir_fallback_kind_refined: entry.nir_fallback_kind_refined.clone(),
        nir_fallback_reason: entry.nir_fallback_reason.clone(),
        nir_block_signature: entry.nir_block_signature.clone(),
        nir_block_detail: entry.nir_block_detail.clone(),
        preview_direct_success: entry.preview_direct_success,
        preview_fallback_kind: entry.preview_fallback_kind,
        preview_fallback_kind_refined: entry.preview_fallback_kind_refined,
        preview_fallback_reason: entry.preview_fallback_reason,
        preview_block_signature: entry.preview_block_signature,
        preview_block_detail: entry.preview_block_detail,
        recovery_strategy_attempted: entry.recovery_strategy_attempted,
        recovery_strategy_applied: entry.recovery_strategy_applied,
        recovery_outcome: entry.recovery_outcome,
        recovery_source_signature: entry.recovery_source_signature,
        recovery_structuring_mode: entry.recovery_structuring_mode,
        recovery_goto_count_before: entry.recovery_goto_count_before,
        recovery_goto_count_after: entry.recovery_goto_count_after,
        recovery_hint_surface_before: entry.recovery_hint_surface_before,
        recovery_hint_surface_after: entry.recovery_hint_surface_after,
        recovery_quality_flags: entry.recovery_quality_flags,
        nir_surface_kind: entry.nir_surface_kind.clone(),
        preview_surface_kind: entry.preview_surface_kind,
        pcode_block_count: entry.pcode_block_count,
        pcode_op_count: entry.pcode_op_count,
        has_indirect_control_flow: entry.has_indirect_control_flow,
        auto_eligible: entry.auto_eligible,
        strict_explicit_candidate: strict_explicit,
        heuristic_surface_candidate: heuristic_surface,
        reason_tags: entry.reason_tags,
        row_status: entry.row_status,
        row_error_kind: entry.row_error_kind,
        row_error_message: entry.row_error_message,
    }
}

pub(super) fn update_inventory_summary(
    summary: &mut FunctionFactsInventorySummary,
    candidate_summary: &mut PreviewCandidateScanSummary,
    row: &FunctionFactsInventoryRow,
) {
    summary.rows_emitted += 1;
    if row.preview_direct_success {
        summary.direct_success_count += 1;
    }
    if row.explicit_fact_total > 0 {
        summary.explicit_fact_nonzero_count += 1;
    }
    if row.fact_sources_present.dwarf {
        summary.source_presence_counts.dwarf += 1;
    }
    if row.fact_sources_present.pdb {
        summary.source_presence_counts.pdb += 1;
    }
    if row.fact_sources_present.loader {
        summary.source_presence_counts.loader += 1;
    }
    if row.fact_sources_present.native_inferred {
        summary.source_presence_counts.native_inferred += 1;
    }
    summary.explicit_breakdown_totals.param_count += row.explicit_fact_breakdown.param_count;
    summary.explicit_breakdown_totals.local_count += row.explicit_fact_breakdown.local_count;
    summary.explicit_breakdown_totals.return_count += row.explicit_fact_breakdown.return_count;
    summary.explicit_breakdown_totals.pdb_type_count += row.explicit_fact_breakdown.pdb_type_count;
    summary.explicit_breakdown_totals.native_type_count +=
        row.explicit_fact_breakdown.native_type_count;
    if row.provenance_fact_breakdown.dwarf_type_count > 0 {
        summary.provenance_surface_totals.dwarf_nonzero_rows += 1;
    }
    if row.provenance_fact_breakdown.pdb_type_count > 0 {
        summary.provenance_surface_totals.pdb_nonzero_rows += 1;
    }
    if row.provenance_fact_breakdown.native_type_count > 0 {
        summary.provenance_surface_totals.native_nonzero_rows += 1;
    }
    if row.provenance_fact_breakdown.loader_type_count > 0 {
        summary.provenance_surface_totals.loader_nonzero_rows += 1;
    }
    if row.inventory_surface_gap {
        summary.inventory_surface_gap_count += 1;
    }
    if row.preview_direct_success && row.explicit_fact_total == 0 {
        summary.aligned_with_zero_explicit_count += 1;
    }
    if row.strict_explicit_candidate {
        summary.strict_explicit_candidate_count += 1;
    }
    if row.heuristic_surface_candidate {
        summary.heuristic_surface_candidate_count += 1;
    }
    if let Some(strategy) = row.recovery_strategy_attempted.as_ref() {
        *summary
            .recovery_strategy_attempted_counts
            .entry(strategy.clone())
            .or_insert(0) += 1;
    }
    if let Some(strategy) = row.recovery_strategy_applied.as_ref() {
        *summary
            .recovery_strategy_applied_counts
            .entry(strategy.clone())
            .or_insert(0) += 1;
    }
    if let Some(outcome) = row.recovery_outcome.as_ref() {
        *summary
            .recovery_outcome_counts
            .entry(outcome.clone())
            .or_insert(0) += 1;
    }
    if row.recovery_strategy_attempted.is_some()
        && let Some(mode) = row.recovery_structuring_mode.as_ref()
    {
        *summary
            .recovery_structuring_mode_counts
            .entry(mode.clone())
            .or_insert(0) += 1;
    }
    for flag in &row.recovery_quality_flags {
        *summary
            .recovery_quality_flag_counts
            .entry(flag.clone())
            .or_insert(0) += 1;
    }

    let candidate_entry = PreviewCandidateEntry {
        binary: row.binary.clone(),
        address: row.address.clone(),
        name: row.name.clone(),
        row_status: row.row_status.clone(),
        row_error_kind: row.row_error_kind.clone(),
        row_error_message: row.row_error_message.clone(),
        row_error_verbose: None,
        has_dwarf_function: row.has_dwarf_function,
        dwarf_param_count: row.dwarf_param_count,
        dwarf_local_count: row.dwarf_local_count,
        has_dwarf_return_type: row.has_dwarf_return_type,
        loader_type_count: row.loader_type_count,
        fact_density_score: row.fact_density_score,
        preview_direct_success: row.preview_direct_success,
        nir_direct_success: row.nir_direct_success,
        nir_fallback_kind: row.nir_fallback_kind.clone(),
        nir_fallback_kind_refined: row.nir_fallback_kind_refined.clone(),
        nir_fallback_reason: row.nir_fallback_reason.clone(),
        nir_block_signature: row.nir_block_signature.clone(),
        nir_block_detail: row.nir_block_detail.clone(),
        preview_fallback_kind: row.preview_fallback_kind.clone(),
        preview_fallback_kind_refined: row.preview_fallback_kind_refined.clone(),
        preview_fallback_reason: row.preview_fallback_reason.clone(),
        preview_block_signature: row.preview_block_signature.clone(),
        preview_block_detail: row.preview_block_detail.clone(),
        recovery_strategy_attempted: row.recovery_strategy_attempted.clone(),
        recovery_strategy_applied: row.recovery_strategy_applied.clone(),
        recovery_outcome: row.recovery_outcome.clone(),
        recovery_source_signature: row.recovery_source_signature.clone(),
        recovery_structuring_mode: row.recovery_structuring_mode.clone(),
        recovery_goto_count_before: row.recovery_goto_count_before,
        recovery_goto_count_after: row.recovery_goto_count_after,
        recovery_hint_surface_before: row.recovery_hint_surface_before,
        recovery_hint_surface_after: row.recovery_hint_surface_after,
        recovery_quality_flags: row.recovery_quality_flags.clone(),
        pcode_block_count: row.pcode_block_count,
        pcode_op_count: row.pcode_op_count,
        has_indirect_control_flow: row.has_indirect_control_flow,
        auto_eligible: row.auto_eligible,
        nir_surface_kind: row.nir_surface_kind.clone(),
        preview_surface_kind: row.preview_surface_kind.clone(),
        quality_potential_score: 0,
        reason_tags: row.reason_tags.clone(),
        preview_hint_stats: None,
    };
    update_scan_summary(candidate_summary, &candidate_entry);
    summary.nir_failure_count = candidate_summary.nir_failure_count;
    summary.preview_failure_count = candidate_summary.preview_failure_count;
    summary.panic_recovered_count = candidate_summary.panic_recovered_count;
    summary.internal_error_count = candidate_summary.internal_error_count;
    summary.failure_kind_counts = candidate_summary.failure_kind_counts.clone();
    summary.row_error_kind_counts = candidate_summary.row_error_kind_counts.clone();
}
