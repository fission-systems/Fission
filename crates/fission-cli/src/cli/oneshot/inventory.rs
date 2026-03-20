use crate::cli::args::OneShotArgs;
use crate::cli::oneshot::common::{
    apply_profile, init_decompiler, resolve_compiler_id, resolve_profile,
};
use crate::cli::oneshot::decompile::{
    PreviewCandidateEntry, PreviewCandidateScanSummary, ScopedQuietPanicHook,
    preview_candidate_entry_with_recovery, select_candidate_functions, update_scan_summary,
};
use crate::cli::output::OutputSilencer;
use fission_ffi::DecompilerNative;
use fission_loader::loader::LoadedBinary;
use fission_static::analysis::decomp::{
    FactStore, PrepareOptions, PrepareTimings, prepare_native_decompiler_for_binary,
    serialize_win_api_signatures_json,
};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};

#[derive(Debug, Serialize)]
struct FactSourcesPresent {
    dwarf: bool,
    pdb: bool,
    loader: bool,
    native_inferred: bool,
}

#[derive(Debug, Serialize)]
struct ExplicitFactBreakdown {
    param_count: usize,
    local_count: usize,
    return_count: usize,
    pdb_type_count: usize,
    native_type_count: usize,
}

#[derive(Debug, Serialize)]
struct ProvenanceFactBreakdown {
    dwarf_type_count: usize,
    pdb_type_count: usize,
    native_type_count: usize,
    loader_type_count: usize,
}

#[derive(Debug, Serialize)]
struct FunctionFactsInventoryRow {
    binary: String,
    binary_path: String,
    address: String,
    name: String,
    has_dwarf_function: bool,
    dwarf_param_count: usize,
    dwarf_local_count: usize,
    has_dwarf_return_type: bool,
    loader_type_count: usize,
    explicit_fact_total: usize,
    fact_density_score: i32,
    fact_sources_present: FactSourcesPresent,
    explicit_fact_breakdown: ExplicitFactBreakdown,
    provenance_fact_breakdown: ProvenanceFactBreakdown,
    admission_block_stage: String,
    inventory_surface_gap: bool,
    nir_direct_success: bool,
    nir_fallback_kind: Option<String>,
    nir_fallback_kind_refined: Option<String>,
    nir_fallback_reason: Option<String>,
    nir_block_signature: Option<String>,
    nir_block_detail: Option<String>,
    preview_direct_success: bool,
    preview_fallback_kind: Option<String>,
    preview_fallback_kind_refined: Option<String>,
    preview_fallback_reason: Option<String>,
    preview_block_signature: Option<String>,
    preview_block_detail: Option<String>,
    recovery_strategy_attempted: Option<String>,
    recovery_strategy_applied: Option<String>,
    recovery_outcome: Option<String>,
    recovery_source_signature: Option<String>,
    recovery_structuring_mode: Option<String>,
    recovery_goto_count_before: Option<usize>,
    recovery_goto_count_after: Option<usize>,
    recovery_hint_surface_before: Option<usize>,
    recovery_hint_surface_after: Option<usize>,
    recovery_quality_flags: Vec<String>,
    nir_surface_kind: Option<String>,
    preview_surface_kind: Option<String>,
    pcode_block_count: usize,
    pcode_op_count: usize,
    has_indirect_control_flow: bool,
    auto_eligible: bool,
    strict_explicit_candidate: bool,
    heuristic_surface_candidate: bool,
    reason_tags: Vec<String>,
    row_status: String,
    row_error_kind: Option<String>,
    row_error_message: Option<String>,
}

#[derive(Debug, Default, Serialize)]
struct SourcePresenceCounts {
    dwarf: usize,
    pdb: usize,
    loader: usize,
    native_inferred: usize,
}

#[derive(Debug, Default, Serialize)]
struct ExplicitBreakdownTotals {
    param_count: usize,
    local_count: usize,
    return_count: usize,
    pdb_type_count: usize,
    native_type_count: usize,
}

#[derive(Debug, Default, Serialize)]
struct ProvenanceSurfaceTotals {
    dwarf_nonzero_rows: usize,
    pdb_nonzero_rows: usize,
    native_nonzero_rows: usize,
    loader_nonzero_rows: usize,
}

#[derive(Debug, Default, Serialize)]
struct FunctionFactsInventorySummary {
    binary: String,
    binary_path: String,
    format: String,
    arch_spec: String,
    functions_total: usize,
    rows_emitted: usize,
    chunks_completed: usize,
    chunk_size: usize,
    direct_success_count: usize,
    nir_failure_count: usize,
    preview_failure_count: usize,
    panic_recovered_count: usize,
    internal_error_count: usize,
    explicit_fact_nonzero_count: usize,
    source_presence_counts: SourcePresenceCounts,
    explicit_breakdown_totals: ExplicitBreakdownTotals,
    provenance_surface_totals: ProvenanceSurfaceTotals,
    inventory_surface_gap_count: usize,
    aligned_with_zero_explicit_count: usize,
    strict_explicit_candidate_count: usize,
    heuristic_surface_candidate_count: usize,
    failure_kind_counts: BTreeMap<String, usize>,
    row_error_kind_counts: BTreeMap<String, usize>,
    recovery_strategy_attempted_counts: BTreeMap<String, usize>,
    recovery_strategy_applied_counts: BTreeMap<String, usize>,
    recovery_outcome_counts: BTreeMap<String, usize>,
    recovery_quality_flag_counts: BTreeMap<String, usize>,
    recovery_structuring_mode_counts: BTreeMap<String, usize>,
    suppressed_stderr_count: usize,
}

fn prepare_inventory_decompiler(
    cli: &OneShotArgs,
    binary: &LoadedBinary,
    binary_data: &[u8],
) -> io::Result<DecompilerNative> {
    let mut decomp = init_decompiler(cli.verbose);
    let (selected_profile, _) = resolve_profile(cli.profile.as_deref());
    apply_profile(&mut decomp, selected_profile);
    let (compiler_id, _) = resolve_compiler_id(binary, cli.compiler_id.as_deref());
    let gdt_path_owned = fission_core::PATHS
        .get_gdt_path(binary.is_64bit)
        .and_then(|p| p.to_str().map(String::from));
    let signatures_json = serialize_win_api_signatures_json();
    let mut prepare_timings = PrepareTimings::default();
    let mut prepare_options = PrepareOptions {
        compiler_id: compiler_id.as_deref(),
        verbose: cli.verbose,
        timings: Some(&mut prepare_timings),
        gdt_path: gdt_path_owned.as_deref(),
        signatures_json: signatures_json.as_deref(),
        timeout_ms: cli.timeout_ms,
    };
    prepare_native_decompiler_for_binary(&mut decomp, binary, binary_data, &mut prepare_options)
        .map_err(|e| io::Error::other(format!("prepare decompiler failed: {e}")))?;
    Ok(decomp)
}

fn heuristic_surface_candidate(entry: &PreviewCandidateEntry) -> bool {
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

fn detect_pdb_source_present(binary: &LoadedBinary) -> bool {
    binary
        .inner()
        .pdb_debug_info
        .as_ref()
        .is_some_and(|info| info.has_codeview)
}

fn fact_sources_present(
    snapshot: &fission_static::analysis::decomp::FunctionFacts,
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
    snapshot: &fission_static::analysis::decomp::FunctionFacts,
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

fn provenance_fact_breakdown(
    snapshot: &fission_static::analysis::decomp::FunctionFacts,
) -> ProvenanceFactBreakdown {
    ProvenanceFactBreakdown {
        dwarf_type_count: snapshot.dwarf_type_fact_count(),
        pdb_type_count: snapshot.pdb_type_fact_count(),
        native_type_count: snapshot.native_type_fact_count(),
        loader_type_count: snapshot.loader_type_fact_count(),
    }
}

fn inventory_surface_gap(sources: &FactSourcesPresent, explicit_fact_total: usize) -> bool {
    // Global loader metadata can exist for the whole image without being function-scoped.
    // Only treat per-function/debug sources as an explicit surface gap signal here.
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

fn try_ingest_native_inventory_facts(
    decomp: &mut DecompilerNative,
    fact_store: &mut FactStore,
    address: u64,
) {
    let Ok(result) = decomp.decompile_with_metadata(address) else {
        return;
    };
    if result.inferred_types.is_empty() {
        return;
    }
    fact_store.ingest_native_function_types(address, result.inferred_types);
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

fn to_inventory_row(
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

fn update_inventory_summary(
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

fn write_inventory_summary(
    path: &std::path::Path,
    summary: &FunctionFactsInventorySummary,
) -> io::Result<()> {
    let body = serde_json::to_string_pretty(summary)
        .map_err(|e| io::Error::other(format!("JSON serialization failed: {e}")))?;
    fs::write(path, body)
}

pub(super) fn emit_function_facts_inventory(
    cli: &OneShotArgs,
    binary: &LoadedBinary,
    binary_data: &[u8],
) -> io::Result<()> {
    let output_jsonl = cli.output_jsonl.as_ref().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "--output-jsonl is required for --emit-function-facts-inventory",
        )
    })?;
    let summary_json = cli.summary_json.as_ref().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "--summary-json is required for --emit-function-facts-inventory",
        )
    })?;
    let chunk_size = cli.chunk_size.unwrap_or(100).max(1);
    let quiet_batch_errors = cli.quiet_batch_errors || !cli.verbose;
    let _silencer = OutputSilencer::new_if(quiet_batch_errors);

    let mut decomp = prepare_inventory_decompiler(cli, binary, binary_data)?;
    let mut fact_store = FactStore::from_binary(binary);
    let pdb_source_present = detect_pdb_source_present(binary);
    let binary_name = cli
        .binary
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();
    let functions = select_candidate_functions(cli, binary)?;
    let quiet_panic_hook = ScopedQuietPanicHook::install(quiet_batch_errors);

    if let Some(parent) = output_jsonl.parent() {
        fs::create_dir_all(parent)?;
    }
    if let Some(parent) = summary_json.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(output_jsonl, b"")?;
    let mut writer = OpenOptions::new().append(true).open(output_jsonl)?;

    let mut candidate_summary = PreviewCandidateScanSummary::default();
    let mut summary = FunctionFactsInventorySummary {
        binary: binary_name.clone(),
        binary_path: cli.binary.display().to_string(),
        format: binary.format.clone(),
        arch_spec: binary.arch_spec.clone(),
        functions_total: functions.len(),
        chunk_size,
        ..Default::default()
    };

    for chunk in functions.chunks(chunk_size) {
        for func in chunk {
            let candidate = preview_candidate_entry_with_recovery(
                &mut decomp,
                binary,
                &fact_store,
                &binary_name,
                func,
                cli.timeout_ms,
            );
            try_ingest_native_inventory_facts(&mut decomp, &mut fact_store, func.address);
            let row = to_inventory_row(&cli.binary, pdb_source_present, &fact_store, candidate);
            serde_json::to_writer(&mut writer, &row)
                .map_err(|e| io::Error::other(format!("JSON serialization failed: {e}")))?;
            writer.write_all(b"\n")?;
            update_inventory_summary(&mut summary, &mut candidate_summary, &row);
            if summary.rows_emitted % 10 == 0 {
                writer.flush()?;
                if let Some(hook) = quiet_panic_hook.as_ref() {
                    summary.suppressed_stderr_count = hook.suppressed_count();
                }
                write_inventory_summary(summary_json, &summary)?;
            }
        }
        writer.flush()?;
        summary.chunks_completed += 1;
        if let Some(hook) = quiet_panic_hook.as_ref() {
            summary.suppressed_stderr_count = hook.suppressed_count();
        }
        write_inventory_summary(summary_json, &summary)?;
    }

    if let Some(hook) = quiet_panic_hook.as_ref() {
        summary.suppressed_stderr_count = hook.suppressed_count();
    }
    write_inventory_summary(summary_json, &summary)?;
    Ok(())
}
