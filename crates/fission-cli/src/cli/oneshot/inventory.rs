use crate::cli::args::OneShotArgs;
use crate::cli::oneshot::common::{apply_profile, init_decompiler, resolve_compiler_id, resolve_profile};
use crate::cli::oneshot::decompile::{
    PreviewCandidateEntry, PreviewCandidateScanSummary, ScopedQuietPanicHook,
    preview_candidate_entry_with_recovery, select_candidate_functions, strict_explicit_candidate,
    update_scan_summary,
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
    preview_direct_success: bool,
    preview_fallback_kind: Option<String>,
    preview_fallback_kind_refined: Option<String>,
    preview_fallback_reason: Option<String>,
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
    preview_failure_count: usize,
    panic_recovered_count: usize,
    internal_error_count: usize,
    explicit_fact_nonzero_count: usize,
    strict_explicit_candidate_count: usize,
    heuristic_surface_candidate_count: usize,
    failure_kind_counts: BTreeMap<String, usize>,
    row_error_kind_counts: BTreeMap<String, usize>,
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

fn explicit_fact_total(entry: &PreviewCandidateEntry) -> usize {
    entry.dwarf_param_count + entry.dwarf_local_count + usize::from(entry.has_dwarf_return_type)
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

fn to_inventory_row(
    binary_path: &std::path::Path,
    entry: PreviewCandidateEntry,
) -> FunctionFactsInventoryRow {
    let explicit_fact_total = explicit_fact_total(&entry);
    let strict_explicit = strict_explicit_candidate(&entry);
    let heuristic_surface = heuristic_surface_candidate(&entry);
    FunctionFactsInventoryRow {
        binary: entry.binary,
        binary_path: binary_path.display().to_string(),
        address: entry.address,
        name: entry.name,
        has_dwarf_function: entry.has_dwarf_function,
        dwarf_param_count: entry.dwarf_param_count,
        dwarf_local_count: entry.dwarf_local_count,
        has_dwarf_return_type: entry.has_dwarf_return_type,
        loader_type_count: entry.loader_type_count,
        explicit_fact_total,
        fact_density_score: entry.fact_density_score,
        preview_direct_success: entry.preview_direct_success,
        preview_fallback_kind: entry.preview_fallback_kind,
        preview_fallback_kind_refined: entry.preview_fallback_kind_refined,
        preview_fallback_reason: entry.preview_fallback_reason,
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
    if row.strict_explicit_candidate {
        summary.strict_explicit_candidate_count += 1;
    }
    if row.heuristic_surface_candidate {
        summary.heuristic_surface_candidate_count += 1;
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
        preview_fallback_kind: row.preview_fallback_kind.clone(),
        preview_fallback_kind_refined: row.preview_fallback_kind_refined.clone(),
        preview_fallback_reason: row.preview_fallback_reason.clone(),
        pcode_block_count: row.pcode_block_count,
        pcode_op_count: row.pcode_op_count,
        has_indirect_control_flow: row.has_indirect_control_flow,
        auto_eligible: row.auto_eligible,
        preview_surface_kind: row.preview_surface_kind.clone(),
        quality_potential_score: 0,
        reason_tags: row.reason_tags.clone(),
        preview_hint_stats: None,
    };
    update_scan_summary(candidate_summary, &candidate_entry);
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
    let fact_store = FactStore::from_binary(binary);
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
            let row = to_inventory_row(&cli.binary, candidate);
            serde_json::to_writer(&mut writer, &row)
                .map_err(|e| io::Error::other(format!("JSON serialization failed: {e}")))?;
            writer.write_all(b"\n")?;
            update_inventory_summary(&mut summary, &mut candidate_summary, &row);
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
