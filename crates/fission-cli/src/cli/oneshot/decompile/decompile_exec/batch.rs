use super::super::decompile_render::write_output_bytes;
use super::super::nir_candidates::{
    PreviewCandidateInventory, PreviewCandidateScanSummary, ScopedQuietPanicHook, load_resume_rows,
    preview_candidate_entry_with_recovery, update_scan_summary, write_scan_summary,
};
use super::super::*;

fn prepare_batch_decompiler(
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
    let signatures_json = serialize_api_signatures_json();
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

pub(crate) fn emit_preview_candidate_inventory(
    cli: &OneShotArgs,
    binary: &LoadedBinary,
    binary_data: &[u8],
) -> io::Result<()> {
    let fact_store = FactStore::from_binary(binary);
    let binary_name = cli
        .binary
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    let selected_functions = select_candidate_functions(cli, binary)?;
    let selection_accounting = selected_functions.accounting;
    let functions = if cli.address.is_some() || cli.addresses_file.is_some() {
        selected_functions.functions
    } else {
        let limit = cli.preview_candidate_limit.or(cli.functions_limit);
        let mut functions = selected_functions.functions;
        if let Some(limit) = limit {
            functions.truncate(limit);
        }
        functions
    };

    use rayon::prelude::*;
    let candidates: Vec<PreviewCandidateEntry> = functions
        .par_iter()
        .map(|func| {
            let mut thread_decomp = prepare_batch_decompiler(cli, binary, binary_data)
                .expect("failed to prepare thread-local decompiler");
            preview_candidate_entry_with_recovery(
                &mut thread_decomp,
                binary,
                &fact_store,
                &binary_name,
                func,
                cli.timeout_ms,
            )
        })
        .collect();

    let report = PreviewCandidateInventory {
        binary: binary_name,
        binary_path: cli.binary.display().to_string(),
        format: binary.format.clone(),
        arch_spec: binary.arch_spec.clone(),
        candidate_count: candidates.len(),
        functions_discovered_total: selection_accounting.functions_discovered_total,
        functions_selected_total: candidates.len(),
        functions_excluded_import_count: selection_accounting.functions_excluded_import_count,
        functions_excluded_runtime_wrapper_count: selection_accounting
            .functions_excluded_runtime_wrapper_count,
        functions_excluded_provenance_count: selection_accounting
            .functions_excluded_provenance_count,
        include_nonuser_functions: selection_accounting.include_nonuser_functions,
        candidates,
    };
    let json = serde_json::to_string_pretty(&report)
        .map_err(|e| io::Error::other(format!("JSON serialization failed: {e}")))?;
    write_output_bytes(cli, &json)
}

pub(crate) fn emit_preview_candidate_scan_batch(
    cli: &OneShotArgs,
    binary: &LoadedBinary,
    binary_data: &[u8],
) -> io::Result<()> {
    let output_jsonl = cli.output_jsonl.as_ref().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "--output-jsonl is required for --preview-candidate-scan-batch",
        )
    })?;
    let summary_json = cli.summary_json.as_ref().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "--summary-json is required for --preview-candidate-scan-batch",
        )
    })?;
    let chunk_size = cli.chunk_size.unwrap_or(50).max(1);
    let quiet_batch_errors = cli.quiet_batch_errors || !cli.verbose;
    let _silencer = OutputSilencer::new_if(quiet_batch_errors);

    let fact_store = FactStore::from_binary(binary);
    let binary_name = cli
        .binary
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    let selected_functions = select_candidate_functions(cli, binary)?;
    let selection_accounting = selected_functions.accounting;
    let selected_functions = selected_functions.functions;
    let quiet_panic_hook = ScopedQuietPanicHook::install(quiet_batch_errors);
    let mut summary = PreviewCandidateScanSummary {
        binary: binary_name.clone(),
        binary_path: cli.binary.display().to_string(),
        format: binary.format.clone(),
        arch_spec: binary.arch_spec.clone(),
        functions_total: selected_functions.len(),
        functions_discovered_total: selection_accounting.functions_discovered_total,
        functions_selected_total: selection_accounting.functions_selected_total,
        functions_excluded_import_count: selection_accounting.functions_excluded_import_count,
        functions_excluded_runtime_wrapper_count: selection_accounting
            .functions_excluded_runtime_wrapper_count,
        functions_excluded_provenance_count: selection_accounting
            .functions_excluded_provenance_count,
        include_nonuser_functions: selection_accounting.include_nonuser_functions,
        chunk_size,
        ..Default::default()
    };

    let resume_path = cli.resume_from.as_ref().unwrap_or(output_jsonl);
    let (processed_addresses, resume_summary) = load_resume_rows(resume_path)?;
    summary.addresses_scanned = resume_summary.addresses_scanned;
    summary.timeout_count = resume_summary.timeout_count;
    summary.nonzero_explicit_candidates = resume_summary.nonzero_explicit_candidates;
    summary.strict_explicit_candidates = resume_summary.strict_explicit_candidates;
    summary.failure_kind_counts = resume_summary.failure_kind_counts;
    summary.resume_loaded_rows = resume_summary.resume_loaded_rows;

    if let Some(parent) = output_jsonl.parent() {
        fs::create_dir_all(parent)?;
    }
    if let Some(parent) = summary_json.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut writer = OpenOptions::new()
        .create(true)
        .append(true)
        .open(output_jsonl)?;

    let pending_functions = selected_functions
        .into_iter()
        .filter(|func| !processed_addresses.contains(&func.address))
        .collect::<Vec<_>>();

    for chunk in pending_functions.chunks(chunk_size) {
        use rayon::prelude::*;
        let entries: Vec<PreviewCandidateEntry> = chunk
            .par_iter()
            .map(|func| {
                let mut thread_decomp = prepare_batch_decompiler(cli, binary, binary_data)
                    .expect("failed to prepare thread-local decompiler");
                preview_candidate_entry_with_recovery(
                    &mut thread_decomp,
                    binary,
                    &fact_store,
                    &binary_name,
                    func,
                    cli.timeout_ms,
                )
            })
            .collect();

        for entry in entries {
            serde_json::to_writer(&mut writer, &entry)
                .map_err(|e| io::Error::other(format!("JSON serialization failed: {e}")))?;
            writer.write_all(b"\n")?;
            update_scan_summary(&mut summary, &entry);
        }
        writer.flush()?;
        summary.chunks_completed += 1;
        if let Some(hook) = quiet_panic_hook.as_ref() {
            summary.suppressed_stderr_count = hook.suppressed_count();
        }
        write_scan_summary(summary_json, &summary)?;
    }

    if let Some(hook) = quiet_panic_hook.as_ref() {
        summary.suppressed_stderr_count = hook.suppressed_count();
    }
    write_scan_summary(summary_json, &summary)?;
    Ok(())
}
