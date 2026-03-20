use super::provenance::{detect_pdb_source_present, to_inventory_row, update_inventory_summary};
use super::schema::{FunctionFactsInventorySummary, write_inventory_summary};
use crate::cli::args::OneShotArgs;
use crate::cli::oneshot::common::{
    apply_profile, init_decompiler, resolve_compiler_id, resolve_profile,
};
use crate::cli::oneshot::decompile::{
    PreviewCandidateScanSummary, ScopedQuietPanicHook, preview_candidate_entry_with_recovery,
    select_candidate_functions,
};
use crate::cli::output::OutputSilencer;
use fission_ffi::DecompilerNative;
use fission_loader::loader::LoadedBinary;
use fission_static::analysis::decomp::{
    FactStore, PrepareOptions, PrepareTimings, prepare_native_decompiler_for_binary,
    serialize_win_api_signatures_json,
};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};

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

pub(crate) fn emit_function_facts_inventory(
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
