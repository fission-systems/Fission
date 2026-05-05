use super::provenance::{
    InventoryCandidateEntry, detect_pdb_source_present, to_inventory_row, update_inventory_summary,
};
use super::schema::{FunctionFactsInventorySummary, write_inventory_summary};
use crate::cli::args::OneShotArgs;
#[cfg(not(feature = "native_decomp"))]
use crate::cli::oneshot::function_select::{
    BatchFunctionSelection, select_batch_functions, select_explicit_functions,
    select_function_by_address, select_functions_from_addresses_file,
};
use fission_decompiler::{NirEngineMode, NirSurfaceKind, auto_nir_eligible};
use fission_loader::loader::{FunctionInfo, LoadedBinary};
use fission_static::analysis::decomp::FactStore;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::cli::oneshot::assessment::canonical_indirect_classification;
#[cfg(feature = "native_decomp")]
use crate::cli::oneshot::common::{
    apply_profile, init_decompiler, resolve_compiler_id, resolve_profile,
};
#[cfg(feature = "native_decomp")]
use crate::cli::oneshot::decompile::{
    preview_candidate_entry_with_recovery, select_candidate_functions,
};
#[cfg(feature = "native_decomp")]
use crate::cli::output::OutputSilencer;
#[cfg(feature = "native_decomp")]
use fission_ffi::DecompilerNative;
#[cfg(feature = "native_decomp")]
use fission_static::analysis::decomp::{
    PrepareOptions, PrepareTimings, prepare_native_decompiler_for_binary,
    serialize_api_signatures_json,
};

#[cfg(not(feature = "native_decomp"))]
use fission_decompiler::{RustSleighDecompileConfig, select_nir_output_from_prebuilt_pcode};
#[cfg(not(feature = "native_decomp"))]
use fission_decompiler::{
    IndirectControlClassification, NirBuildStats, NirHintStats, NirRenderOptions, PcodeFunction,
};
#[cfg(not(feature = "native_decomp"))]
use fission_sleigh::runtime::{DecodeContract, RuntimeSleighFrontend};

#[cfg(feature = "native_decomp")]
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

#[cfg(feature = "native_decomp")]
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

#[cfg(feature = "native_decomp")]
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
    let functions = select_inventory_functions(cli, binary)?;
    let selection_accounting = functions.accounting;
    let functions = functions.functions;

    if let Some(parent) = output_jsonl.parent() {
        fs::create_dir_all(parent)?;
    }
    if let Some(parent) = summary_json.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(output_jsonl, b"")?;
    let mut writer = OpenOptions::new().append(true).open(output_jsonl)?;

    let mut summary = FunctionFactsInventorySummary {
        binary: binary_name.clone(),
        binary_path: cli.binary.display().to_string(),
        format: binary.format.clone(),
        arch_spec: binary.arch_spec.clone(),
        functions_total: functions.len(),
        functions_discovered_total: selection_accounting.functions_discovered_total,
        functions_selected_total: selection_accounting.functions_selected_total,
        functions_excluded_import_count: selection_accounting.functions_excluded_import_count,
        functions_excluded_runtime_wrapper_count: selection_accounting
            .functions_excluded_runtime_wrapper_count,
        include_nonuser_functions: selection_accounting.include_nonuser_functions,
        chunk_size,
        ..Default::default()
    };

    for chunk in functions.chunks(chunk_size) {
        for func in chunk {
            let candidate: InventoryCandidateEntry = preview_candidate_entry_with_recovery(
                &mut decomp,
                binary,
                &fact_store,
                &binary_name,
                func,
                cli.timeout_ms,
            )
            .into();
            try_ingest_native_inventory_facts(&mut decomp, &mut fact_store, func.address);
            let row = to_inventory_row(&cli.binary, pdb_source_present, &fact_store, candidate);
            serde_json::to_writer(&mut writer, &row)
                .map_err(|e| io::Error::other(format!("JSON serialization failed: {e}")))?;
            writer.write_all(b"\n")?;
            update_inventory_summary(&mut summary, &row);
            if summary.rows_emitted % 10 == 0 {
                writer.flush()?;
                write_inventory_summary(summary_json, &summary)?;
            }
        }
        writer.flush()?;
        summary.chunks_completed += 1;
        write_inventory_summary(summary_json, &summary)?;
    }

    write_inventory_summary(summary_json, &summary)?;
    Ok(())
}

#[cfg(not(feature = "native_decomp"))]
fn select_inventory_functions<'a>(
    cli: &OneShotArgs,
    binary: &'a LoadedBinary,
) -> io::Result<BatchFunctionSelection<'a>> {
    if let Some(address_file) = &cli.addresses_file {
        let functions = select_functions_from_addresses_file(binary, address_file)?;
        return Ok(select_explicit_functions(
            functions,
            cli.include_nonuser_functions,
        ));
    }

    if let Some(address) = cli.address {
        let functions = select_function_by_address(binary, address)
            .into_iter()
            .collect();
        return Ok(select_explicit_functions(
            functions,
            cli.include_nonuser_functions,
        ));
    }

    Ok(select_batch_functions(
        binary,
        cli.include_nonuser_functions,
        cli.functions_limit,
    ))
}

#[cfg(not(feature = "native_decomp"))]
fn fact_density(
    has_dwarf_function: bool,
    dwarf_param_count: usize,
    dwarf_local_count: usize,
    has_dwarf_return_type: bool,
    loader_type_count: usize,
) -> i32 {
    let mut score = 0;
    if has_dwarf_function {
        score += 3;
    }
    score += dwarf_param_count as i32;
    score += dwarf_local_count as i32;
    if has_dwarf_return_type {
        score += 2;
    }
    if loader_type_count > 0 {
        score += 1;
    }
    score
}

#[cfg(not(feature = "native_decomp"))]
fn preview_goto_count(code: &str) -> usize {
    code.matches("goto ").count()
}

#[cfg(not(feature = "native_decomp"))]
fn explicit_hint_surface_count(stats: Option<NirHintStats>) -> usize {
    stats.map_or(0, |stats| {
        stats.explicit_param_name_hits
            + stats.explicit_local_name_hits
            + stats.explicit_param_type_hits
            + stats.explicit_local_type_hits
            + stats.explicit_return_type_hit
    })
}

#[cfg(not(feature = "native_decomp"))]
fn preview_surface_kind_str(kind: Option<NirSurfaceKind>) -> Option<String> {
    match kind {
        Some(NirSurfaceKind::Structured) => Some("structured".to_string()),
        Some(NirSurfaceKind::Unstructured) => Some("unstructured".to_string()),
        None => None,
    }
}

#[cfg(not(feature = "native_decomp"))]
fn classify_nir_output_class(
    direct_success: bool,
    surface_kind: Option<NirSurfaceKind>,
    goto_count: Option<usize>,
    build_stats: Option<&NirBuildStats>,
) -> Option<String> {
    if !direct_success {
        return None;
    }
    let goto_count = goto_count.unwrap_or(0);
    let build_stats = build_stats.cloned().unwrap_or_default();
    if build_stats.forced_linear_structuring_count > 0 {
        return Some("linear_fallback".to_string());
    }
    if surface_kind == Some(NirSurfaceKind::Structured) && goto_count == 0 {
        return Some("structured".to_string());
    }
    Some("partially_structured".to_string())
}

#[cfg(not(feature = "native_decomp"))]
fn build_quality_tags_and_score(
    dwarf_param_count: usize,
    dwarf_local_count: usize,
    has_dwarf_return_type: bool,
    loader_type_count: usize,
    preview_direct_success: bool,
    preview_surface_kind: Option<NirSurfaceKind>,
    pcode_block_count: usize,
    pcode_op_count: usize,
    indirect_classification: &IndirectControlClassification,
    preview_code: Option<&str>,
    preview_hint_stats: Option<NirHintStats>,
) -> (i32, Vec<String>) {
    let mut score = 0;
    let mut tags = Vec::new();

    if dwarf_param_count > 0 {
        score += 2;
        tags.push("dwarf_params".to_string());
    }
    if dwarf_local_count > 0 {
        score += 2;
        tags.push("dwarf_locals".to_string());
    }
    if has_dwarf_return_type {
        score += 1;
        tags.push("return_type".to_string());
    }
    if loader_type_count > 0 {
        score += 1;
        tags.push("loader_types".to_string());
    }
    if preview_direct_success {
        score += 2;
        tags.push("preview_direct_success".to_string());
    }
    if !indirect_classification.has_indirect_control
        && pcode_block_count <= 12
        && pcode_op_count <= 600
    {
        tags.push("low_cfg_risk".to_string());
    }
    if preview_code.is_some_and(|code| code.contains("slot_")) {
        score += 2;
        tags.push("slot_alias_candidate".to_string());
    }
    if preview_surface_kind == Some(NirSurfaceKind::Unstructured) {
        score -= 1;
        tags.push("unstructured_heavy".to_string());
    }
    if pcode_op_count > 800 {
        score -= 2;
        tags.push("large_pcode".to_string());
    }
    if let Some(stats) = preview_hint_stats {
        if stats.explicit_param_name_hits > 0 || stats.explicit_local_name_hits > 0 {
            tags.push("explicit_name_hints".to_string());
        }
        if stats.explicit_param_type_hits > 0
            || stats.explicit_local_type_hits > 0
            || stats.explicit_return_type_hit > 0
        {
            tags.push("explicit_type_hints".to_string());
        }
        if stats.pointer_alias_hits > 0 {
            tags.push("pointer_alias".to_string());
        }
        if stats.local_surface_hits > 0 {
            tags.push("local_surface".to_string());
        }
        if stats.derived_origin_type_hits > 0 {
            tags.push("derived_origin_type".to_string());
        }
    }

    tags.sort();
    tags.dedup();
    (score, tags)
}

#[cfg(not(feature = "native_decomp"))]
fn preview_block_signature(
    row_error_kind: Option<&str>,
    row_error_message: Option<&str>,
    indirect_classification: &IndirectControlClassification,
    pcode_block_count: usize,
    pcode_op_count: usize,
) -> Option<String> {
    let kind = row_error_kind?;
    let message = row_error_message.unwrap_or_default().to_ascii_lowercase();
    let signature = match kind {
        "preview_frontend_reject" => {
            if message.contains("failed to load pcode")
                || message.contains("could not find op at target address")
            {
                "frontend_missing_pcode_op"
            } else {
                "frontend_reject"
            }
        }
        "preview_architecture_unsupported" => "unsupported_architecture",
        "preview_format_unsupported" => "unsupported_format",
        "preview_timeout" => "preview_timeout",
        "preview_worker_failure" => "worker_internal_error",
        "preview_structuring_failure" => {
            if message.contains("unsupported_cfg_region_shape")
                || message.contains("unsupported region shape")
            {
                "unsupported_cfg_region_shape"
            } else if message.contains("unsupported_cfg_phi_join")
                || message.contains("unsupported phi join")
            {
                "unsupported_cfg_phi_join"
            } else if message.contains("unsupported_cfg_indirect_call_region")
                || message.contains("unsupported indirect call region")
            {
                "unsupported_cfg_indirect_call_region"
            } else {
                "structuring_failure"
            }
        }
        "preview_parse_or_lowering_failure" => {
            if message.contains("unsupported op") {
                "lowering_unsupported_op"
            } else if message.contains("unsupported address materialization") {
                "lowering_address_materialization"
            } else {
                "lowering_failure"
            }
        }
        "preview_unsupported_cfg" => {
            if message.contains("unsupported branch target") {
                if indirect_classification.has_indirect_control {
                    "unsupported_indirect_branch_target"
                } else {
                    "unsupported_branch_target"
                }
            } else if message.contains("unsupported indirect call region") {
                "unsupported_indirect_call_region"
            } else if message.contains("unsupported phi join") {
                "unsupported_phi_join"
            } else if message.contains("unsupported region shape") {
                "unsupported_region_shape"
            } else if indirect_classification.has_indirect_control {
                "unsupported_indirect_control_flow"
            } else {
                "unsupported_cfg"
            }
        }
        "preview_non_success_unknown" => {
            if pcode_block_count == 0 && pcode_op_count == 0 {
                "preview_no_pcode"
            } else {
                "preview_no_result"
            }
        }
        _ => return Some(kind.to_string()),
    };
    Some(signature.to_string())
}

#[cfg(not(feature = "native_decomp"))]
fn preview_block_detail(
    row_error_message: Option<&str>,
    preview_fallback_reason: Option<&str>,
) -> Option<String> {
    row_error_message
        .or(preview_fallback_reason)
        .map(|detail| detail.trim().to_string())
        .filter(|detail| !detail.is_empty())
}

#[cfg(not(feature = "native_decomp"))]
fn pcode_metrics(pcode: &PcodeFunction) -> (usize, usize) {
    let total_ops = pcode.blocks.iter().map(|block| block.ops.len()).sum();
    (pcode.blocks.len(), total_ops)
}

#[cfg(not(feature = "native_decomp"))]
fn extract_safe_bytes_from_decode_error(err: &str, func_addr: u64) -> Option<usize> {
    let marker = "decode failed at 0x";
    let idx = err.find(marker)?;
    let hex_start = idx + marker.len();
    let hex_end = err[hex_start..]
        .find(|c: char| !c.is_ascii_hexdigit())
        .map(|i| hex_start + i)
        .unwrap_or(err.len());
    let fail_addr = u64::from_str_radix(&err[hex_start..hex_end], 16).ok()?;
    let safe = fail_addr.checked_sub(func_addr)? as usize;
    if safe == 0 { None } else { Some(safe) }
}

#[cfg(not(feature = "native_decomp"))]
fn decode_rust_sleigh_pcode(
    binary: &LoadedBinary,
    name: &str,
    entry_address: u64,
    max_bytes: usize,
    instruction_limit: usize,
    continue_past_indirect_branch: bool,
    retry_on_decode_error: bool,
) -> Result<PcodeFunction, String> {
    let bytes = binary.view_bytes(entry_address, max_bytes).ok_or_else(|| {
        format!("rust_sleigh: unable to read bytes at 0x{entry_address:x} for {name}")
    })?;

    let load_spec = binary.load_spec().ok_or_else(|| {
        format!(
            "rust_sleigh: missing Ghidra load spec for '{}'",
            binary.path
        )
    })?;

    let lifter = RuntimeSleighFrontend::new_for_load_spec(load_spec)
        .map_err(|e| format!("rust_sleigh: {e:#}"))?;
    let lift_contract = if continue_past_indirect_branch {
        DecodeContract::decomp_function(instruction_limit)
    } else {
        DecodeContract::strict_function(instruction_limit)
    };
    let result =
        lifter.lift_raw_pcode_function_with_decode_contract(&bytes, entry_address, lift_contract);
    match result {
        Ok(lifted) => Ok(lifted.function),
        Err(first_err) => {
            if retry_on_decode_error {
                let err_str = format!("{first_err:#}");
                if let Some(safe) = extract_safe_bytes_from_decode_error(&err_str, entry_address) {
                    if safe > 0 && safe < bytes.len() {
                        if let Ok(retry) = lifter.lift_raw_pcode_function_with_decode_contract(
                            &bytes[..safe],
                            entry_address,
                            lift_contract,
                        ) {
                            return Ok(retry.function);
                        }
                    }
                }
            }
            Err(format!(
                "rust_sleigh: function lift failed for {name} at 0x{entry_address:x}: {first_err:#}"
            ))
        }
    }
}

#[cfg(not(feature = "native_decomp"))]
fn decode_inventory_pcode(
    binary: &LoadedBinary,
    func: &FunctionInfo,
    config: &RustSleighDecompileConfig,
) -> Result<PcodeFunction, String> {
    let entry_address = func.address;
    let function_size = usize::try_from(func.size).unwrap_or(0);
    let max_bytes_limit = config
        .default_decode_bytes
        .max(1)
        .min(config.decode_max_bytes_cap.max(1));
    let fallback_default_bytes = config.default_decode_bytes.max(1).min(max_bytes_limit);
    let max_bytes = if function_size > 0 {
        function_size.min(max_bytes_limit)
    } else if config.use_next_function_distance_if_unknown {
        binary
            .function_after(entry_address)
            .and_then(|next| {
                let dist = next.address.saturating_sub(entry_address) as usize;
                if dist > 0 {
                    Some(dist.min(max_bytes_limit))
                } else {
                    None
                }
            })
            .unwrap_or(fallback_default_bytes)
    } else {
        fallback_default_bytes
    }
    .max(1);
    let default_instruction_limit = if config.continue_past_indirect_branch {
        config
            .instruction_budget_default
            .max(max_bytes.min(config.instruction_budget_cap.max(1)))
    } else {
        config.instruction_budget_default
    };
    let instruction_limit = default_instruction_limit
        .max(1)
        .min(config.instruction_budget_cap.max(1));

    decode_rust_sleigh_pcode(
        binary,
        &func.name,
        entry_address,
        max_bytes,
        instruction_limit,
        config.continue_past_indirect_branch,
        config.retry_on_decode_error,
    )
}

#[cfg(not(feature = "native_decomp"))]
fn classify_decode_error(err: &str) -> (&'static str, &'static str) {
    let lower = err.to_ascii_lowercase();
    if lower.contains("unsupported arch_spec") || lower.contains("missing ghidra load spec") {
        return ("preview_unsupported", "preview_architecture_unsupported");
    }
    if lower.contains("unsupported format") {
        return ("preview_unsupported", "preview_format_unsupported");
    }
    ("preview_unsupported", "preview_frontend_reject")
}

#[cfg(not(feature = "native_decomp"))]
fn build_inventory_fallback_entry(
    fact_store: &FactStore,
    binary_name: &str,
    func: &FunctionInfo,
    row_status: &str,
    row_error_kind: &str,
    reason: String,
) -> InventoryCandidateEntry {
    let dwarf = fact_store.dwarf_function(func.address);
    let has_dwarf_function = dwarf.is_some();
    let dwarf_param_count = dwarf.map(|info| info.params.len()).unwrap_or(0);
    let dwarf_local_count = dwarf.map(|info| info.local_vars.len()).unwrap_or(0);
    let has_dwarf_return_type = dwarf
        .and_then(|info| info.return_type.as_deref())
        .is_some_and(|name| !name.trim().is_empty());
    let loader_type_count = fact_store.merged_inferred_types(func.address).len();
    let fact_density_score = fact_density(
        has_dwarf_function,
        dwarf_param_count,
        dwarf_local_count,
        has_dwarf_return_type,
        loader_type_count,
    );
    let indirect_classification = canonical_indirect_classification(None);
    let (_, reason_tags) = build_quality_tags_and_score(
        dwarf_param_count,
        dwarf_local_count,
        has_dwarf_return_type,
        loader_type_count,
        false,
        None,
        0,
        0,
        &indirect_classification,
        None,
        None,
    );

    InventoryCandidateEntry {
        binary: binary_name.to_string(),
        address: format!("0x{:x}", func.address),
        name: func.name.clone(),
        row_status: row_status.to_string(),
        row_error_kind: Some(row_error_kind.to_string()),
        row_error_message: Some(reason.clone()),
        has_dwarf_function,
        dwarf_param_count,
        dwarf_local_count,
        has_dwarf_return_type,
        loader_type_count,
        fact_density_score,
        preview_direct_success: false,
        nir_direct_success: false,
        nir_fallback_kind: Some("internal_error".to_string()),
        nir_fallback_kind_refined: Some(row_error_kind.to_string()),
        nir_fallback_reason: Some(reason.clone()),
        nir_block_signature: preview_block_signature(
            Some(row_error_kind),
            Some(reason.as_str()),
            &indirect_classification,
            0,
            0,
        ),
        nir_block_detail: Some(reason.clone()),
        preview_fallback_kind: Some("internal_error".to_string()),
        preview_fallback_kind_refined: Some(row_error_kind.to_string()),
        preview_fallback_reason: Some(reason.clone()),
        preview_block_signature: preview_block_signature(
            Some(row_error_kind),
            Some(reason.as_str()),
            &indirect_classification,
            0,
            0,
        ),
        preview_block_detail: Some(reason),
        recovery_strategy_attempted: None,
        recovery_strategy_applied: None,
        recovery_outcome: None,
        recovery_source_signature: None,
        recovery_structuring_mode: None,
        recovery_goto_count_before: None,
        recovery_goto_count_after: None,
        recovery_hint_surface_before: None,
        recovery_hint_surface_after: None,
        recovery_quality_flags: Vec::new(),
        nir_surface_kind: None,
        preview_surface_kind: None,
        pcode_block_count: 0,
        pcode_op_count: 0,
        has_indirect_control_flow: false,
        has_preserved_indirect_surface: false,
        has_unresolved_unsupported_indirect: false,
        has_dispatcher_recovery: false,
        auto_eligible: false,
        nir_goto_count: None,
        nir_output_class: None,
        nir_build_stats: None,
        reason_tags,
        preview_hint_stats: None,
    }
}

#[cfg(not(feature = "native_decomp"))]
fn build_inventory_candidate_entry_rust(
    cli: &OneShotArgs,
    binary: &LoadedBinary,
    fact_store: &FactStore,
    binary_name: &str,
    func: &FunctionInfo,
) -> InventoryCandidateEntry {
    let dwarf = fact_store.dwarf_function(func.address);
    let has_dwarf_function = dwarf.is_some();
    let dwarf_param_count = dwarf.map(|info| info.params.len()).unwrap_or(0);
    let dwarf_local_count = dwarf.map(|info| info.local_vars.len()).unwrap_or(0);
    let has_dwarf_return_type = dwarf
        .and_then(|info| info.return_type.as_deref())
        .is_some_and(|name| !name.trim().is_empty());
    let loader_type_count = fact_store.merged_inferred_types(func.address).len();
    let fact_density_score = fact_density(
        has_dwarf_function,
        dwarf_param_count,
        dwarf_local_count,
        has_dwarf_return_type,
        loader_type_count,
    );

    let config = RustSleighDecompileConfig {
        nir_timeout_ms: cli.timeout_ms,
        ..RustSleighDecompileConfig::cli_defaults()
    };

    let mut pcode_block_count = 0usize;
    let mut pcode_op_count = 0usize;
    let mut auto_eligible = false;
    let mut preview_direct_success = false;
    let mut preview_fallback_kind = None;
    let mut preview_fallback_kind_refined = None;
    let mut preview_fallback_reason = None;
    let mut preview_surface_kind = None;
    let mut preview_hint_stats = None;
    let mut nir_build_stats = None;
    let mut preview_code = None;
    let mut recovery_strategy_attempted = None;
    let mut recovery_strategy_applied = None;
    let mut recovery_outcome = None;
    let mut recovery_source_signature = None;
    let mut recovery_structuring_mode = Some("normal".to_string());
    let recovery_goto_count_before = None;
    let mut recovery_goto_count_after = None;
    let mut recovery_hint_surface_before = None;
    let mut recovery_hint_surface_after = None;

    match decode_inventory_pcode(binary, func, &config) {
        Ok(pcode) => {
            let metrics = pcode_metrics(&pcode);
            pcode_block_count = metrics.0;
            pcode_op_count = metrics.1;
            auto_eligible = auto_nir_eligible(binary, &pcode);

            let mut options = NirRenderOptions::from_loaded_binary(binary);
            options.pe_x64_only = config.pe_x64_only;
            options.conservative_irreducible_fallback = config.conservative_irreducible_fallback;

            match select_nir_output_from_prebuilt_pcode(
                &pcode,
                binary,
                func.address,
                &func.name,
                NirEngineMode::Nir,
                cli.timeout_ms,
                options,
            ) {
                Ok(selection) => {
                    preview_direct_success = selection.nir_code.is_some()
                        && !selection.fell_back
                        && selection.engine_used == NirEngineMode::Nir;
                    preview_fallback_kind = selection.fallback_kind.map(str::to_string);
                    preview_fallback_kind_refined =
                        selection.fallback_kind_refined.map(str::to_string);
                    preview_fallback_reason = selection.fallback_reason.clone();
                    preview_surface_kind = selection.nir_surface;
                    preview_hint_stats = selection.hint_stats;
                    nir_build_stats = selection.build_stats;
                    preview_code = selection.nir_code;
                    recovery_strategy_attempted =
                        selection.recovery_strategy_attempted.map(str::to_string);
                    recovery_strategy_applied =
                        selection.recovery_strategy_applied.map(str::to_string);
                    recovery_outcome = selection.recovery_outcome.map(str::to_string);
                    recovery_source_signature = selection.recovery_source_signature;
                    recovery_structuring_mode = selection
                        .recovery_structuring_mode
                        .map(str::to_string)
                        .or(recovery_structuring_mode);
                    if recovery_strategy_attempted.is_some() {
                        recovery_goto_count_after = preview_code.as_deref().map(preview_goto_count);
                        recovery_hint_surface_before = Some(0);
                        recovery_hint_surface_after =
                            Some(explicit_hint_surface_count(preview_hint_stats));
                    }
                }
                Err(err) => {
                    preview_fallback_kind = Some("preview_unsupported".to_string());
                    preview_fallback_kind_refined = Some("preview_frontend_reject".to_string());
                    preview_fallback_reason = Some(err);
                }
            }
        }
        Err(err) => {
            let (kind, refined) = classify_decode_error(&err);
            preview_fallback_kind = Some(kind.to_string());
            preview_fallback_kind_refined = Some(refined.to_string());
            preview_fallback_reason = Some(err);
        }
    }

    let indirect_classification = canonical_indirect_classification(nir_build_stats.as_ref());

    let (_, reason_tags) = build_quality_tags_and_score(
        dwarf_param_count,
        dwarf_local_count,
        has_dwarf_return_type,
        loader_type_count,
        preview_direct_success,
        preview_surface_kind,
        pcode_block_count,
        pcode_op_count,
        &indirect_classification,
        preview_code.as_deref(),
        preview_hint_stats,
    );

    let (row_status, row_error_kind, row_error_message) = if preview_direct_success {
        ("ok".to_string(), None, None)
    } else {
        let error_kind = preview_fallback_kind_refined
            .clone()
            .or(preview_fallback_kind.clone())
            .or_else(|| Some("preview_non_success_unknown".to_string()));
        let error_message = preview_fallback_reason
            .clone()
            .or_else(|| Some("preview candidate did not produce direct preview".to_string()));
        ("preview_failure".to_string(), error_kind, error_message)
    };

    let nir_block_signature = preview_block_signature(
        row_error_kind.as_deref(),
        row_error_message.as_deref(),
        &indirect_classification,
        pcode_block_count,
        pcode_op_count,
    );
    let nir_block_detail = preview_block_detail(
        row_error_message.as_deref(),
        preview_fallback_reason.as_deref(),
    );
    let nir_goto_count = preview_code.as_deref().map(preview_goto_count);
    let nir_output_class = classify_nir_output_class(
        preview_direct_success,
        preview_surface_kind,
        nir_goto_count,
        nir_build_stats.as_ref(),
    );
    let mut recovery_quality_flags = Vec::new();
    if recovery_strategy_attempted.is_some() {
        if let Some(after) = recovery_goto_count_after {
            if recovery_goto_count_before.is_some_and(|before| after > before) {
                recovery_quality_flags.push("goto_increased".to_string());
            }
            if after > 0 && after.saturating_mul(2) >= pcode_block_count.max(1) {
                recovery_quality_flags.push("high_goto_density".to_string());
            }
        }
        if recovery_structuring_mode.as_deref() == Some("forced_linear")
            && preview_surface_kind == Some(NirSurfaceKind::Unstructured)
        {
            recovery_quality_flags.push("shape_linearized".to_string());
        }
        if recovery_structuring_mode.as_deref() == Some("region_linearized") {
            recovery_quality_flags.push("localized_linearization".to_string());
            if preview_surface_kind == Some(NirSurfaceKind::Unstructured) {
                recovery_quality_flags.push("shape_partially_linearized".to_string());
            }
        }
        if recovery_hint_surface_before
            .zip(recovery_hint_surface_after)
            .is_some_and(|(before, after)| after < before)
        {
            recovery_quality_flags.push("surface_regressed".to_string());
        }
        if recovery_hint_surface_after.unwrap_or(0) > 0 {
            recovery_quality_flags.push("explicit_hints_preserved".to_string());
        }
    }

    InventoryCandidateEntry {
        binary: binary_name.to_string(),
        address: format!("0x{:x}", func.address),
        name: func.name.clone(),
        row_status,
        row_error_kind,
        row_error_message,
        has_dwarf_function,
        dwarf_param_count,
        dwarf_local_count,
        has_dwarf_return_type,
        loader_type_count,
        fact_density_score,
        preview_direct_success,
        nir_direct_success: preview_direct_success,
        nir_fallback_kind: preview_fallback_kind.clone(),
        nir_fallback_kind_refined: preview_fallback_kind_refined.clone(),
        nir_fallback_reason: preview_fallback_reason.clone(),
        nir_block_signature: nir_block_signature.clone(),
        nir_block_detail: nir_block_detail.clone(),
        preview_fallback_kind,
        preview_fallback_kind_refined,
        preview_fallback_reason,
        preview_block_signature: nir_block_signature,
        preview_block_detail: nir_block_detail,
        recovery_strategy_attempted,
        recovery_strategy_applied,
        recovery_outcome,
        recovery_source_signature,
        recovery_structuring_mode,
        recovery_goto_count_before,
        recovery_goto_count_after,
        recovery_hint_surface_before,
        recovery_hint_surface_after,
        recovery_quality_flags,
        nir_surface_kind: preview_surface_kind_str(preview_surface_kind),
        preview_surface_kind: preview_surface_kind_str(preview_surface_kind),
        pcode_block_count,
        pcode_op_count,
        has_indirect_control_flow: indirect_classification.has_indirect_control,
        has_preserved_indirect_surface: indirect_classification.has_preserved_indirect_surface,
        has_unresolved_unsupported_indirect: indirect_classification
            .has_unresolved_unsupported_indirect,
        has_dispatcher_recovery: indirect_classification.has_dispatcher_recovery,
        auto_eligible,
        nir_goto_count,
        nir_output_class,
        nir_build_stats,
        reason_tags,
        preview_hint_stats,
    }
}

#[cfg(not(feature = "native_decomp"))]
fn rust_candidate_entry_with_recovery(
    cli: &OneShotArgs,
    binary: &LoadedBinary,
    fact_store: &FactStore,
    binary_name: &str,
    func: &FunctionInfo,
) -> InventoryCandidateEntry {
    match catch_unwind(AssertUnwindSafe(|| {
        build_inventory_candidate_entry_rust(cli, binary, fact_store, binary_name, func)
    })) {
        Ok(entry) => entry,
        Err(_) => build_inventory_fallback_entry(
            fact_store,
            binary_name,
            func,
            "panic_recovered",
            "panic",
            "rust-only inventory scan panicked".to_string(),
        ),
    }
}

#[cfg(not(feature = "native_decomp"))]
pub(crate) fn emit_function_facts_inventory(
    cli: &OneShotArgs,
    binary: &LoadedBinary,
    _binary_data: &[u8],
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
    let fact_store = FactStore::from_binary(binary);
    let pdb_source_present = detect_pdb_source_present(binary);
    let binary_name = cli
        .binary
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();
    let functions = select_inventory_functions(cli, binary)?;
    let selection_accounting = functions.accounting;
    let functions = functions.functions;

    if let Some(parent) = output_jsonl.parent() {
        fs::create_dir_all(parent)?;
    }
    if let Some(parent) = summary_json.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(output_jsonl, b"")?;
    let mut writer = OpenOptions::new().append(true).open(output_jsonl)?;

    let mut summary = FunctionFactsInventorySummary {
        binary: binary_name.clone(),
        binary_path: cli.binary.display().to_string(),
        format: binary.format.clone(),
        arch_spec: binary.arch_spec.clone(),
        functions_total: functions.len(),
        functions_discovered_total: selection_accounting.functions_discovered_total,
        functions_selected_total: selection_accounting.functions_selected_total,
        functions_excluded_import_count: selection_accounting.functions_excluded_import_count,
        functions_excluded_runtime_wrapper_count: selection_accounting
            .functions_excluded_runtime_wrapper_count,
        include_nonuser_functions: selection_accounting.include_nonuser_functions,
        chunk_size,
        ..Default::default()
    };

    for chunk in functions.chunks(chunk_size) {
        for func in chunk {
            let candidate =
                rust_candidate_entry_with_recovery(cli, binary, &fact_store, &binary_name, func);
            let row = to_inventory_row(&cli.binary, pdb_source_present, &fact_store, candidate);
            serde_json::to_writer(&mut writer, &row)
                .map_err(|e| io::Error::other(format!("JSON serialization failed: {e}")))?;
            writer.write_all(b"\n")?;
            update_inventory_summary(&mut summary, &row);
            if summary.rows_emitted % 10 == 0 {
                writer.flush()?;
                write_inventory_summary(summary_json, &summary)?;
            }
        }
        writer.flush()?;
        summary.chunks_completed += 1;
        write_inventory_summary(summary_json, &summary)?;
    }

    write_inventory_summary(summary_json, &summary)?;
    Ok(())
}
