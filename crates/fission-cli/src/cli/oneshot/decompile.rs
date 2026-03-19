use crate::cli::args::{OneShotArgs, parse_hex_address};
use crate::cli::oneshot::common::{
    EngineMode, apply_profile, fallback_reason_with_kind, init_decompiler, resolve_compiler_id,
    resolve_engine_mode, resolve_profile,
};
use crate::cli::oneshot::disasm::render_function_disassembly_text;
use crate::cli::output::OutputSilencer;
use fission_core::FissionError;
use fission_ffi::DecompilerNative;
use fission_loader::loader::{FunctionInfo, LoadedBinary};
use fission_pcode::{PcodeFunction, PcodeOpcode, PreviewBuildStats, PreviewHintStats};
use fission_static::analysis::decomp::postprocess::PostProcessor;
use fission_static::analysis::decomp::preview_engine::auto_mlil_eligible;
use fission_static::analysis::decomp::{
    FactStore, PrepareOptions, PrepareTimings, PreviewEngineMode, PreviewSurfaceKind,
    classify_native_failure_kind, log_type_diag, prepare_native_decompiler_for_binary,
    rescue_preview_output_with_facts, select_preview_output_with_facts,
    serialize_win_api_signatures_json,
};
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::collections::{BTreeMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::panic::{AssertUnwindSafe, catch_unwind, set_hook, take_hook};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tracing::warn;

#[cfg(feature = "native_decomp")]
use rayon::prelude::*;

fn prefer_function_name(candidate: &str, current: &str) -> bool {
    let candidate_is_sub = candidate.starts_with("sub_");
    let current_is_sub = current.starts_with("sub_");
    if candidate_is_sub != current_is_sub {
        return !candidate_is_sub;
    }
    candidate.len() > current.len()
}

/// Strip WARNING / NOTICE diagnostic lines from decompiler output.
/// Removes lines starting with `WARNING:`, `NOTICE:`, or `/* WARNING` comments.
fn strip_warnings(code: &str) -> String {
    code.lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.starts_with("WARNING:")
                && !trimmed.starts_with("NOTICE:")
                && !trimmed.starts_with("/* WARNING")
                && !trimmed.starts_with("// WARNING")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Strip inferred struct definitions (typedef struct ... } name;) blocks
/// from the top of decompiler output for cleaner Ghidra-compatible comparison.
fn strip_inferred_structs(code: &str) -> String {
    let mut result = String::new();
    let mut in_struct_block = false;
    for line in code.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("typedef struct") || trimmed.starts_with("// Inferred Structure") {
            in_struct_block = true;
            continue;
        }
        if in_struct_block {
            // End of struct block: closing `} name;`
            if trimmed.starts_with('}') && trimmed.ends_with(';') {
                in_struct_block = false;
                continue;
            }
            // Still inside struct definition
            continue;
        }
        result.push_str(line);
        result.push('\n');
    }
    result
}

fn should_use_assembly_fallback(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.contains("duplicate variablepiece")
        || lower.contains("control flow analysis error")
        || lower.contains("followflow")
        || lower.contains("preview_timeout")
        || lower.contains("could not find op at target address")
        || lower.contains("ghidra lowlevelerror")
}

fn make_assembly_fallback(
    binary: &LoadedBinary,
    binary_data: &[u8],
    func: &FunctionInfo,
    error: &str,
) -> Option<String> {
    if !should_use_assembly_fallback(error) {
        return None;
    }
    let error_class = classify_native_failure_kind(error);
    let asm = render_function_disassembly_text(binary, binary_data, func.address).ok()?;
    Some(format!(
        "// Assembly fallback: {}\n// Function: {} @ 0x{:x}\n// Error class: {}\n\n{}",
        error, func.name, func.address, error_class, asm
    ))
}

fn attach_native_timing(entry: &mut serde_json::Value, decomp: &DecompilerNative) {
    let Ok(raw_timing) = decomp.get_last_timing_json() else {
        return;
    };
    if raw_timing.trim().is_empty() || raw_timing.trim() == "{}" {
        return;
    }
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw_timing) {
        entry["native_timing"] = value;
    }
}

/// Result of decompiling one function (used for both sequential and parallel paths)
struct DecompEntry {
    address: u64,
    name: String,
    code: Result<RenderedCode, fission_core::FissionError>,
    decomp_sec: f64,
    postprocess_sec: f64,
    last_timing_json: Option<String>,
}

struct RenderedCode {
    code: String,
    postprocess_sec: f64,
    engine_used: &'static str,
    fell_back: bool,
    fallback_reason: Option<String>,
    preview_build_stats: Option<PreviewBuildStats>,
    preview_hint_stats: Option<PreviewHintStats>,
}

#[derive(Debug, Serialize)]
pub(super) struct PreviewCandidateInventory {
    pub(super) binary: String,
    pub(super) binary_path: String,
    pub(super) format: String,
    pub(super) arch_spec: String,
    pub(super) candidate_count: usize,
    pub(super) candidates: Vec<PreviewCandidateEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct PreviewCandidateEntry {
    pub(super) binary: String,
    pub(super) address: String,
    pub(super) name: String,
    pub(super) row_status: String,
    pub(super) row_error_kind: Option<String>,
    pub(super) row_error_message: Option<String>,
    pub(super) row_error_verbose: Option<String>,
    pub(super) has_dwarf_function: bool,
    pub(super) dwarf_param_count: usize,
    pub(super) dwarf_local_count: usize,
    pub(super) has_dwarf_return_type: bool,
    pub(super) loader_type_count: usize,
    pub(super) fact_density_score: i32,
    pub(super) preview_direct_success: bool,
    pub(super) preview_fallback_kind: Option<String>,
    pub(super) preview_fallback_kind_refined: Option<String>,
    pub(super) preview_fallback_reason: Option<String>,
    pub(super) preview_block_signature: Option<String>,
    pub(super) preview_block_detail: Option<String>,
    pub(super) pcode_block_count: usize,
    pub(super) pcode_op_count: usize,
    pub(super) has_indirect_control_flow: bool,
    pub(super) auto_eligible: bool,
    pub(super) preview_surface_kind: Option<String>,
    pub(super) quality_potential_score: i32,
    pub(super) reason_tags: Vec<String>,
    pub(super) preview_hint_stats: Option<PreviewHintStats>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub(super) struct PreviewCandidateScanSummary {
    pub(super) binary: String,
    pub(super) binary_path: String,
    pub(super) format: String,
    pub(super) arch_spec: String,
    pub(super) functions_total: usize,
    pub(super) addresses_scanned: usize,
    pub(super) chunks_completed: usize,
    pub(super) chunk_size: usize,
    pub(super) timeout_count: usize,
    pub(super) preview_failure_count: usize,
    pub(super) panic_recovered_count: usize,
    pub(super) internal_error_count: usize,
    pub(super) nonzero_explicit_candidates: usize,
    pub(super) strict_explicit_candidates: usize,
    pub(super) failure_kind_counts: BTreeMap<String, usize>,
    pub(super) row_error_kind_counts: BTreeMap<String, usize>,
    pub(super) suppressed_stderr_count: usize,
    pub(super) resume_loaded_rows: usize,
}

pub(super) struct ScopedQuietPanicHook {
    previous: Option<Box<dyn Fn(&std::panic::PanicHookInfo<'_>) + Sync + Send + 'static>>,
    suppressed: Arc<AtomicUsize>,
}

impl ScopedQuietPanicHook {
    pub(super) fn install(enabled: bool) -> Option<Self> {
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

    pub(super) fn suppressed_count(&self) -> usize {
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

fn pcode_total_ops(pcode: &PcodeFunction) -> usize {
    pcode.blocks.iter().map(|block| block.ops.len()).sum()
}

fn contains_indirect_control_flow(pcode: &PcodeFunction) -> bool {
    pcode
        .blocks
        .iter()
        .flat_map(|block| block.ops.iter())
        .any(|op| matches!(op.opcode, PcodeOpcode::CallInd | PcodeOpcode::BranchInd))
}

fn slot_alias_candidate(code: &str) -> bool {
    code.contains("slot_")
}

fn preview_surface_kind_str(kind: Option<PreviewSurfaceKind>) -> Option<String> {
    match kind {
        Some(PreviewSurfaceKind::Structured) => Some("structured".to_string()),
        Some(PreviewSurfaceKind::Unstructured) => Some("unstructured".to_string()),
        None => None,
    }
}

fn fact_density_score(
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

fn build_quality_tags_and_score(
    dwarf_param_count: usize,
    dwarf_local_count: usize,
    has_dwarf_return_type: bool,
    loader_type_count: usize,
    preview_direct_success: bool,
    preview_surface_kind: Option<PreviewSurfaceKind>,
    pcode_block_count: usize,
    pcode_op_count: usize,
    has_indirect_control_flow: bool,
    preview_code: Option<&str>,
    preview_hint_stats: Option<PreviewHintStats>,
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
    if !has_indirect_control_flow && pcode_block_count <= 12 && pcode_op_count <= 600 {
        tags.push("low_cfg_risk".to_string());
    }
    if preview_code.is_some_and(slot_alias_candidate) {
        score += 2;
        tags.push("slot_alias_candidate".to_string());
    }
    if preview_surface_kind == Some(PreviewSurfaceKind::Unstructured) {
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
        if stats.heuristic_pointer_alias_hits > 0 {
            tags.push("heuristic_pointer_alias".to_string());
        }
        if stats.heuristic_local_surface_hits > 0 {
            tags.push("heuristic_local_surface".to_string());
        }
        if stats.derived_origin_type_hits > 0 {
            tags.push("derived_origin_type".to_string());
        }
    }

    tags.sort();
    tags.dedup();
    (score, tags)
}

pub(super) fn strict_explicit_candidate(entry: &PreviewCandidateEntry) -> bool {
    (entry.dwarf_param_count + entry.dwarf_local_count + usize::from(entry.has_dwarf_return_type)) >= 2
        && entry.preview_direct_success
        && !entry.has_indirect_control_flow
        && entry.pcode_op_count <= 800
}

pub(super) fn effective_failure_kind(entry: &PreviewCandidateEntry) -> &str {
    if entry.row_status == "ok" {
        return "direct_success";
    }
    if let Some(kind) = entry.row_error_kind.as_deref() {
        return kind;
    }
    entry.preview_fallback_kind_refined
        .as_deref()
        .or(entry.preview_fallback_kind.as_deref())
        .unwrap_or("preview_non_success_unknown")
}

fn preview_block_signature(
    row_error_kind: Option<&str>,
    row_error_message: Option<&str>,
    has_indirect_control_flow: bool,
    pcode_block_count: usize,
    pcode_op_count: usize,
) -> Option<String> {
    let kind = row_error_kind?;
    let message = row_error_message.unwrap_or_default().to_ascii_lowercase();
    let signature = match kind {
        "preview_frontend_reject" => {
            if message.contains("failed to load pcode") || message.contains("could not find op at target address") {
                "frontend_missing_pcode_op"
            } else {
                "frontend_reject"
            }
        }
        "preview_architecture_unsupported" => "unsupported_architecture",
        "preview_format_unsupported" => "unsupported_format",
        "preview_timeout" => "preview_timeout",
        "preview_worker_failure" => "worker_internal_error",
        "preview_structuring_failure" => "structuring_failure",
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
                if has_indirect_control_flow {
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
            } else if has_indirect_control_flow {
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

fn preview_block_detail(
    row_error_message: Option<&str>,
    preview_fallback_reason: Option<&str>,
) -> Option<String> {
    row_error_message
        .or(preview_fallback_reason)
        .map(|detail| detail.trim().to_string())
        .filter(|detail| !detail.is_empty())
}

pub(super) fn update_scan_summary(
    summary: &mut PreviewCandidateScanSummary,
    entry: &PreviewCandidateEntry,
) {
    summary.addresses_scanned += 1;
    if (entry.dwarf_param_count + entry.dwarf_local_count + usize::from(entry.has_dwarf_return_type)) > 0 {
        summary.nonzero_explicit_candidates += 1;
    }
    if strict_explicit_candidate(entry) {
        summary.strict_explicit_candidates += 1;
    }
    match entry.row_status.as_str() {
        "preview_failure" => summary.preview_failure_count += 1,
        "panic_recovered" => summary.panic_recovered_count += 1,
        "internal_error" => summary.internal_error_count += 1,
        _ => {}
    }
    if entry.row_status != "ok" {
        let failure_kind = effective_failure_kind(entry).to_string();
        if failure_kind == "preview_timeout" {
            summary.timeout_count += 1;
        }
        *summary.failure_kind_counts.entry(failure_kind).or_insert(0) += 1;
    }
    if let Some(kind) = entry.row_error_kind.as_deref() {
        *summary.row_error_kind_counts.entry(kind.to_string()).or_insert(0) += 1;
    }
}

pub(super) fn write_scan_summary(
    path: &std::path::Path,
    summary: &PreviewCandidateScanSummary,
) -> io::Result<()> {
    let body = serde_json::to_string_pretty(summary)
        .map_err(|e| io::Error::other(format!("JSON serialization failed: {e}")))?;
    fs::write(path, body)
}

pub(super) fn load_resume_rows(
    path: &std::path::Path,
) -> io::Result<(HashSet<u64>, PreviewCandidateScanSummary)> {
    if !path.exists() {
        return Ok((HashSet::new(), PreviewCandidateScanSummary::default()));
    }

    let file = fs::File::open(path)?;
    let reader = BufReader::new(file);
    let mut seen = HashSet::new();
    let mut summary = PreviewCandidateScanSummary::default();

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let Ok(entry) = serde_json::from_str::<PreviewCandidateEntry>(&line) else {
            continue;
        };
        let Ok(address) = parse_hex_address(&entry.address) else {
            continue;
        };
        if !seen.insert(address) {
            continue;
        }
        summary.resume_loaded_rows += 1;
        update_scan_summary(&mut summary, &entry);
    }

    Ok((seen, summary))
}

pub(super) fn select_candidate_functions<'a>(
    cli: &OneShotArgs,
    binary: &'a LoadedBinary,
) -> io::Result<Vec<&'a FunctionInfo>> {
    let mut functions = binary.functions.iter().collect::<Vec<_>>();
    functions.sort_by_key(|func| func.address);

    if let Some(address_file) = &cli.addresses_file {
        let contents = fs::read_to_string(address_file)?;
        let mut selected = Vec::new();
        for line in contents.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let address = parse_hex_address(trimmed)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
            if let Some(func) = functions.iter().copied().find(|func| func.address == address) {
                selected.push(func);
            }
        }
        return Ok(selected);
    }

    if let Some(address) = cli.address {
        functions.retain(|func| func.address == address);
    } else if let Some(limit) = cli.functions_limit {
        functions.truncate(limit);
    }

    Ok(functions)
}

fn build_preview_candidate_entry(
    decomp: &mut DecompilerNative,
    binary: &LoadedBinary,
    fact_store: &FactStore,
    binary_name: &str,
    func: &FunctionInfo,
    timeout_ms: Option<u64>,
) -> PreviewCandidateEntry {
    let dwarf = fact_store.dwarf_function(func.address);
    let has_dwarf_function = dwarf.is_some();
    let dwarf_param_count = dwarf.map(|info| info.params.len()).unwrap_or(0);
    let dwarf_local_count = dwarf.map(|info| info.local_vars.len()).unwrap_or(0);
    let has_dwarf_return_type = dwarf
        .and_then(|info| info.return_type.as_deref())
        .is_some_and(|name| !name.trim().is_empty());
    let loader_type_count = fact_store.merged_inferred_types(func.address).len();
    let fact_density_score = fact_density_score(
        has_dwarf_function,
        dwarf_param_count,
        dwarf_local_count,
        has_dwarf_return_type,
        loader_type_count,
    );

    let mut pcode_block_count = 0usize;
    let mut pcode_op_count = 0usize;
    let mut has_indirect = false;
    let mut auto_eligible = false;
    let mut preview_direct_success = false;
    let mut preview_fallback_kind = None;
    let mut preview_fallback_kind_refined = None;
    let mut preview_fallback_reason = None;
    let mut preview_surface_kind = None;
    let mut preview_hint_stats = None;
    let mut preview_code = None;

    match decomp.get_pcode(func.address) {
        Ok(pcode_json) => {
            if let Ok(pcode) = PcodeFunction::from_json(&pcode_json) {
                pcode_block_count = pcode.blocks.len();
                pcode_op_count = pcode_total_ops(&pcode);
                has_indirect = contains_indirect_control_flow(&pcode);
                auto_eligible = auto_mlil_eligible(binary, &pcode);
            }

            if let Ok(selection) = select_preview_output_with_facts(
                decomp,
                binary,
                fact_store,
                func.address,
                &func.name,
                PreviewEngineMode::MlilPreview,
                timeout_ms,
            ) {
                preview_direct_success = selection.preview_code.is_some()
                    && !selection.fell_back
                    && selection.engine_used == PreviewEngineMode::MlilPreview;
                preview_fallback_kind = selection.fallback_kind.map(str::to_string);
                preview_fallback_kind_refined = selection.fallback_kind_refined.map(str::to_string);
                preview_fallback_reason = selection.fallback_reason.clone();
                preview_surface_kind = selection.preview_surface;
                preview_hint_stats = selection.hint_stats;
                preview_code = selection.preview_code;
            }
        }
        Err(err) => {
            preview_fallback_kind = Some("preview_unsupported".to_string());
            preview_fallback_kind_refined = Some("preview_frontend_reject".to_string());
            preview_fallback_reason =
                Some(format!("mlil-preview frontend unavailable: failed to load pcode: {err}"));
        }
    }

    let (quality_potential_score, reason_tags) = build_quality_tags_and_score(
        dwarf_param_count,
        dwarf_local_count,
        has_dwarf_return_type,
        loader_type_count,
        preview_direct_success,
        preview_surface_kind,
        pcode_block_count,
        pcode_op_count,
        has_indirect,
        preview_code.as_deref(),
        preview_hint_stats,
    );

    let (row_status, row_error_kind, row_error_message, row_error_verbose) =
        if preview_direct_success {
            ("ok".to_string(), None, None, None)
        } else {
            let error_kind = preview_fallback_kind_refined
                .clone()
                .or(preview_fallback_kind.clone())
                .or_else(|| Some("preview_non_success_unknown".to_string()));
            let error_message = preview_fallback_reason
                .clone()
                .or_else(|| Some("preview candidate did not produce direct preview".to_string()));
            (
                "preview_failure".to_string(),
                error_kind,
                error_message,
                preview_fallback_reason.clone(),
            )
        };

    let preview_block_signature = preview_block_signature(
        row_error_kind.as_deref(),
        row_error_message.as_deref(),
        has_indirect,
        pcode_block_count,
        pcode_op_count,
    );
    let preview_block_detail =
        preview_block_detail(row_error_message.as_deref(), preview_fallback_reason.as_deref());

    PreviewCandidateEntry {
        binary: binary_name.to_string(),
        address: format!("0x{:x}", func.address),
        name: func.name.clone(),
        row_status,
        row_error_kind,
        row_error_message,
        row_error_verbose,
        has_dwarf_function,
        dwarf_param_count,
        dwarf_local_count,
        has_dwarf_return_type,
        loader_type_count,
        fact_density_score,
        preview_direct_success,
        preview_fallback_kind,
        preview_fallback_kind_refined,
        preview_fallback_reason,
        preview_block_signature,
        preview_block_detail,
        pcode_block_count,
        pcode_op_count,
        has_indirect_control_flow: has_indirect,
        auto_eligible,
        preview_surface_kind: preview_surface_kind_str(preview_surface_kind),
        quality_potential_score,
        reason_tags,
        preview_hint_stats,
    }
}

fn build_preview_candidate_fallback_entry(
    fact_store: &FactStore,
    binary_name: &str,
    func: &FunctionInfo,
    row_status: &str,
    row_error_kind: &str,
    reason: String,
    verbose_reason: Option<String>,
) -> PreviewCandidateEntry {
    let dwarf = fact_store.dwarf_function(func.address);
    let has_dwarf_function = dwarf.is_some();
    let dwarf_param_count = dwarf.map(|info| info.params.len()).unwrap_or(0);
    let dwarf_local_count = dwarf.map(|info| info.local_vars.len()).unwrap_or(0);
    let has_dwarf_return_type = dwarf
        .and_then(|info| info.return_type.as_deref())
        .is_some_and(|name| !name.trim().is_empty());
    let loader_type_count = fact_store.merged_inferred_types(func.address).len();
    let fact_density_score = fact_density_score(
        has_dwarf_function,
        dwarf_param_count,
        dwarf_local_count,
        has_dwarf_return_type,
        loader_type_count,
    );
    let (quality_potential_score, reason_tags) = build_quality_tags_and_score(
        dwarf_param_count,
        dwarf_local_count,
        has_dwarf_return_type,
        loader_type_count,
        false,
        None,
        0,
        0,
        false,
        None,
        None,
    );

    PreviewCandidateEntry {
        binary: binary_name.to_string(),
        address: format!("0x{:x}", func.address),
        name: func.name.clone(),
        row_status: row_status.to_string(),
        row_error_kind: Some(row_error_kind.to_string()),
        row_error_message: Some(reason.clone()),
        row_error_verbose: verbose_reason,
        has_dwarf_function,
        dwarf_param_count,
        dwarf_local_count,
        has_dwarf_return_type,
        loader_type_count,
        fact_density_score,
        preview_direct_success: false,
        preview_fallback_kind: Some("internal_error".to_string()),
        preview_fallback_kind_refined: Some(row_error_kind.to_string()),
        preview_fallback_reason: Some(reason.clone()),
        preview_block_signature: preview_block_signature(
            Some(row_error_kind),
            Some(reason.as_str()),
            false,
            0,
            0,
        ),
        preview_block_detail: Some(reason.clone()),
        pcode_block_count: 0,
        pcode_op_count: 0,
        has_indirect_control_flow: false,
        auto_eligible: false,
        preview_surface_kind: None,
        quality_potential_score,
        reason_tags,
        preview_hint_stats: None,
    }
}

pub(super) fn preview_candidate_entry_with_recovery(
    decomp: &mut DecompilerNative,
    binary: &LoadedBinary,
    fact_store: &FactStore,
    binary_name: &str,
    func: &FunctionInfo,
    timeout_ms: Option<u64>,
) -> PreviewCandidateEntry {
    let result = catch_unwind(AssertUnwindSafe(|| {
        build_preview_candidate_entry(decomp, binary, fact_store, binary_name, func, timeout_ms)
    }));
    match result {
        Ok(entry) => entry,
        Err(payload) => {
            let verbose = panic_payload_to_string(payload.as_ref());
            let message = verbose
                .as_deref()
                .map(|msg| format!("preview candidate scan panicked: {msg}"))
                .unwrap_or_else(|| "preview candidate scan panicked".to_string());
            build_preview_candidate_fallback_entry(
            fact_store,
            binary_name,
            func,
            "panic_recovered",
            "panic",
            message,
            verbose,
        )
        }
    }
}

fn panic_payload_to_string(payload: &(dyn Any + Send)) -> Option<String> {
    if let Some(message) = payload.downcast_ref::<String>() {
        return Some(message.clone());
    }
    payload
        .downcast_ref::<&str>()
        .map(|message| (*message).to_string())
}

fn write_output_bytes(cli: &OneShotArgs, body: &str) -> io::Result<()> {
    if let Some(ref output_path) = cli.output {
        fs::write(output_path, body.as_bytes())?;
        if cli.verbose {
            eprintln!("[✓] Output written to: {}", output_path.display());
        }
    } else {
        let mut stdout = io::stdout().lock();
        stdout.write_all(body.as_bytes())?;
    }
    Ok(())
}

fn render_legacy_code(
    address: u64,
    binary: &LoadedBinary,
    fact_store: &mut FactStore,
    result: fission_ffi::DecompilationResult,
) -> (String, f64) {
    let function_types = result.inferred_types;
    fact_store.ingest_native_function_types(address, function_types.clone());
    let merged_types = fact_store.merged_inferred_types(address);
    log_type_diag(
        address,
        &function_types,
        fact_store.loader_type_facts(),
        &merged_types,
    );
    let postprocessor = PostProcessor::new()
        .with_inferred_types(merged_types)
        .with_dwarf_info(fact_store.dwarf_function(address).cloned())
        .with_string_map(Some(binary.inner().string_map.clone()));
    let postprocess_start = std::time::Instant::now();
    let code = postprocessor.process(&result.code);
    let postprocess_sec = postprocess_start.elapsed().as_secs_f64();
    (code, postprocess_sec)
}

fn legacy_rendered_code(
    address: u64,
    binary: &LoadedBinary,
    fact_store: &mut FactStore,
    result: fission_ffi::DecompilationResult,
) -> RenderedCode {
    let (code, postprocess_sec) = render_legacy_code(address, binary, fact_store, result);
    RenderedCode {
        code,
        postprocess_sec,
        engine_used: PreviewEngineMode::Legacy.as_str(),
        fell_back: false,
        fallback_reason: None,
        preview_build_stats: None,
        preview_hint_stats: None,
    }
}

fn decompile_code_with_profile(
    _profile: &str,
    engine_mode: EngineMode,
    decomp: &mut DecompilerNative,
    binary: &LoadedBinary,
    address: u64,
    name: &str,
    timeout_ms: Option<u64>,
    _verbose: bool,
) -> Result<RenderedCode, FissionError> {
    let mut fact_store = FactStore::from_binary(binary);
    let preview_mode = match engine_mode {
        EngineMode::Legacy => PreviewEngineMode::Legacy,
        EngineMode::MlilPreview => PreviewEngineMode::MlilPreview,
        EngineMode::Auto => PreviewEngineMode::Auto,
    };
    let preview = select_preview_output_with_facts(
        decomp,
        binary,
        &fact_store,
        address,
        name,
        preview_mode,
        timeout_ms,
    )
    .map_err(FissionError::decompiler)?;

    if let Some(code) = preview.preview_code {
        return Ok(RenderedCode {
            code,
            postprocess_sec: 0.0,
            engine_used: PreviewEngineMode::MlilPreview.as_str(),
            fell_back: false,
            fallback_reason: None,
            preview_build_stats: preview.build_stats,
            preview_hint_stats: preview.hint_stats,
        });
    }

    if preview.fell_back
        && preview
            .fallback_reason
            .as_deref()
            .is_some_and(|reason| reason.to_ascii_lowercase().contains("preview_timeout"))
    {
        return Err(FissionError::decompiler(
            preview.fallback_reason.unwrap_or_else(|| {
                fallback_reason_with_kind("preview_timeout", "preview timed out")
            }),
        ));
    }

    let result = match decomp.decompile_with_metadata(address) {
        Ok(result) => result,
        Err(e) => {
            let error_text = e.to_string();
            if !matches!(engine_mode, EngineMode::Legacy) {
                if let Some(selection) = rescue_preview_output_with_facts(
                    decomp,
                    binary,
                    &fact_store,
                    address,
                    name,
                    &error_text,
                    timeout_ms,
                )
                .map_err(FissionError::decompiler)?
                {
                    if let Some(code) = selection.preview_code {
                        return Ok(RenderedCode {
                            code,
                            postprocess_sec: 0.0,
                            engine_used: PreviewEngineMode::MlilPreview.as_str(),
                            fell_back: true,
                            fallback_reason: selection.fallback_reason,
                            preview_build_stats: selection.build_stats,
                            preview_hint_stats: selection.hint_stats,
                        });
                    }
                }
            }
            return Err(e);
        }
    };
    let mut rendered = legacy_rendered_code(address, binary, &mut fact_store, result);
    rendered.fell_back = preview.fell_back;
    rendered.fallback_reason = preview.fallback_reason;
    rendered.preview_hint_stats = preview.hint_stats;
    Ok(rendered)
}

pub(super) fn emit_preview_candidate_inventory(
    cli: &OneShotArgs,
    binary: &LoadedBinary,
    binary_data: &[u8],
) -> io::Result<()> {
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

    let fact_store = FactStore::from_binary(binary);
    let binary_name = cli
        .binary
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    let mut functions = binary.functions.clone();
    functions.sort_by_key(|func| func.address);
    if let Some(address) = cli.address {
        functions.retain(|func| func.address == address);
    } else if let Some(limit) = cli.preview_candidate_limit {
        functions.truncate(limit);
    }

    let mut candidates = Vec::with_capacity(functions.len());
    for func in &functions {
        candidates.push(preview_candidate_entry_with_recovery(
            &mut decomp,
            binary,
            &fact_store,
            &binary_name,
            func,
            cli.timeout_ms,
        ));
    }

    let report = PreviewCandidateInventory {
        binary: binary_name,
        binary_path: cli.binary.display().to_string(),
        format: binary.format.clone(),
        arch_spec: binary.arch_spec.clone(),
        candidate_count: candidates.len(),
        candidates,
    };
    let json = serde_json::to_string_pretty(&report)
        .map_err(|e| io::Error::other(format!("JSON serialization failed: {e}")))?;
    write_output_bytes(cli, &json)
}

pub(super) fn emit_preview_candidate_scan_batch(
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

    let fact_store = FactStore::from_binary(binary);
    let binary_name = cli
        .binary
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    let selected_functions = select_candidate_functions(cli, binary)?;
    let quiet_panic_hook = ScopedQuietPanicHook::install(quiet_batch_errors);
    let mut summary = PreviewCandidateScanSummary {
        binary: binary_name.clone(),
        binary_path: cli.binary.display().to_string(),
        format: binary.format.clone(),
        arch_spec: binary.arch_spec.clone(),
        functions_total: selected_functions.len(),
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
        for func in chunk {
            let entry = preview_candidate_entry_with_recovery(
                &mut decomp,
                binary,
                &fact_store,
                &binary_name,
                func,
                cli.timeout_ms,
            );
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

fn run_sequential_decompilation<'a>(
    cli: &OneShotArgs,
    decomp: &mut DecompilerNative,
    binary: &LoadedBinary,
    binary_data: &[u8],
    functions: &[&'a FunctionInfo],
    selected_profile: &str,
    engine_mode: EngineMode,
    effective_no_header: bool,
    effective_no_warnings: bool,
    effective_json: bool,
) -> (String, Vec<serde_json::Value>, f64, f64) {
    let mut all_output = String::new();
    let mut json_results = Vec::new();
    let mut total_decomp_secs = 0.0;
    let mut total_postprocess_secs = 0.0;
    for func in functions {
        if cli.verbose {
            eprintln!("[*] Decompiling {} (0x{:x})...", func.name, func.address);
        }

        let _silencer = OutputSilencer::new_if(!cli.verbose);
        let func_start = std::time::Instant::now();
        match decompile_code_with_profile(
            selected_profile,
            engine_mode,
            decomp,
            binary,
            func.address,
            &func.name,
            cli.timeout_ms,
            cli.verbose,
        ) {
            Ok(rendered) => {
                let postprocess_sec = rendered.postprocess_sec;
                let decomp_sec = func_start.elapsed().as_secs_f64();
                total_decomp_secs += decomp_sec;
                total_postprocess_secs += postprocess_sec;
                let mut filtered = rendered.code.clone();
                if effective_no_warnings {
                    filtered = strip_warnings(&filtered);
                }
                if cli.ghidra_compat {
                    filtered = strip_inferred_structs(&filtered);
                }

                if effective_json {
                    let mut entry = serde_json::json!({
                        "address": format!("0x{:x}", func.address),
                        "name": func.name,
                        "code": filtered,
                        "engine_used": rendered.engine_used,
                        "fell_back": rendered.fell_back,
                        "fallback_reason": rendered.fallback_reason,
                    });
                    if let Some(stats) = rendered.preview_build_stats {
                        entry["preview_build_stats"] = serde_json::json!(stats);
                    }
                    if let Some(stats) = rendered.preview_hint_stats {
                        entry["preview_hint_stats"] = serde_json::json!(stats);
                    }
                    if cli.benchmark {
                        entry["decomp_sec"] =
                            serde_json::json!((decomp_sec * 1_000_000.0).round() / 1_000_000.0);
                        entry["postprocess_sec"] = serde_json::json!(
                            (postprocess_sec * 1_000_000.0).round() / 1_000_000.0
                        );
                        attach_native_timing(&mut entry, decomp);
                    }
                    json_results.push(entry);
                } else {
                    if !effective_no_header {
                        all_output.push_str("// ============================================\n");
                        all_output.push_str(&format!(
                            "// Function: {} @ 0x{:x}\n",
                            func.name, func.address
                        ));
                        all_output.push_str("// ============================================\n\n");
                    }
                    all_output.push_str(&filtered);
                    all_output.push_str("\n\n");
                }
            }
            Err(e) => {
                let decomp_sec = func_start.elapsed().as_secs_f64();
                total_decomp_secs += decomp_sec;
                let error_text = e.to_string();
                if let Some(fallback) =
                    make_assembly_fallback(binary, binary_data, func, &error_text)
                {
                    if effective_json {
                        let fallback_class = classify_native_failure_kind(&error_text);
                        let mut entry = serde_json::json!({
                            "address": format!("0x{:x}", func.address),
                            "name": func.name,
                            "code": fallback,
                            "engine_used": PreviewEngineMode::Legacy.as_str(),
                            "fell_back": true,
                            "fallback": "assembly",
                            "fallback_reason": fallback_reason_with_kind("assembly_fallback", &error_text),
                            "fallback_class": fallback_class
                        });
                        if cli.benchmark {
                            entry["decomp_sec"] =
                                serde_json::json!((decomp_sec * 1_000_000.0).round() / 1_000_000.0);
                            attach_native_timing(&mut entry, decomp);
                        }
                        json_results.push(entry);
                    } else {
                        if !effective_no_header {
                            all_output
                                .push_str("// ============================================\n");
                            all_output.push_str(&format!(
                                "// Function: {} @ 0x{:x}\n",
                                func.name, func.address
                            ));
                            all_output
                                .push_str("// ============================================\n\n");
                        }
                        all_output.push_str(&fallback);
                        all_output.push_str("\n\n");
                    }
                    continue;
                }
                if effective_json {
                    let routing = fission_static::analysis::decomp::native_failure_routing_decision(
                        &error_text,
                    );
                    let mut entry = serde_json::json!({
                        "address": format!("0x{:x}", func.address),
                        "name": func.name,
                        "engine_used": match routing.engine_used {
                            PreviewEngineMode::Legacy => PreviewEngineMode::Legacy.as_str(),
                            PreviewEngineMode::MlilPreview => PreviewEngineMode::MlilPreview.as_str(),
                            PreviewEngineMode::Auto => PreviewEngineMode::Auto.as_str(),
                        },
                        "fell_back": routing.fell_back,
                        "fallback_reason": routing.fallback_reason,
                        "error": error_text
                    });
                    if cli.benchmark {
                        entry["decomp_sec"] =
                            serde_json::json!((decomp_sec * 1_000_000.0).round() / 1_000_000.0);
                        attach_native_timing(&mut entry, decomp);
                    }
                    json_results.push(entry);
                } else {
                    all_output.push_str(&format!(
                        "// Error decompiling {} (0x{:x}): {}\n\n",
                        func.name, func.address, error_text
                    ));
                }
            }
        }
    }

    (
        all_output,
        json_results,
        total_decomp_secs,
        total_postprocess_secs,
    )
}

#[cfg(feature = "native_decomp")]
fn run_parallel_decompilation<'a>(
    cli: &OneShotArgs,
    main_decomp: &mut DecompilerNative,
    binary: &LoadedBinary,
    binary_data: &[u8],
    functions: &[&'a FunctionInfo],
    _prepare_timings: &PrepareTimings,
    selected_profile: &str,
    engine_mode: EngineMode,
    _init_elapsed_sec: f64,
    _init_start: std::time::Instant,
    effective_no_header: bool,
    effective_no_warnings: bool,
    effective_json: bool,
) -> (String, Vec<serde_json::Value>, f64, f64) {
    let (compiler_id, _) = resolve_compiler_id(binary, cli.compiler_id.as_deref());
    let config = fission_core::config::Config::default();
    let gdt_path_owned = fission_core::PATHS
        .get_gdt_path(binary.is_64bit)
        .and_then(|p| p.to_str().map(String::from));
    // Dynamic worker scaling: avoid negative scaling when function count is low.
    // Each worker incurs ~3–4s init (FID/GDT/.sla). With 20 functions, 8 workers → 62s vs 1 → 26s.
    // Heuristic: aim for ≥50 functions per worker so init cost is amortized (Amdahl's Law).
    let num_workers = 8;

    // Round-robin distribution: spread heavy functions (often at low addresses) across workers
    // instead of clustering them in the first chunk (address-ordered chunks).
    let mut buckets: Vec<Vec<&'a FunctionInfo>> = (0..num_workers).map(|_| Vec::new()).collect();
    for (i, func) in functions.iter().enumerate() {
        buckets[i % num_workers].push(*func);
    }

    // Bucket 0: use the already-prepared main_decomp on the main thread
    let first_bucket_entries = if !buckets[0].is_empty() {
        let mut entries = Vec::with_capacity(buckets[0].len());
        for func in &buckets[0] {
            let start = std::time::Instant::now();
            let code_result = decompile_code_with_profile(
                selected_profile,
                engine_mode,
                main_decomp,
                binary,
                func.address,
                &func.name,
                cli.timeout_ms,
                false,
            );
            let decomp_sec = start.elapsed().as_secs_f64();
            let (code_result, postprocess_sec) = match code_result {
                Ok(rendered) => {
                    let postprocess_sec = rendered.postprocess_sec;
                    (Ok(rendered), postprocess_sec)
                }
                Err(e) => {
                    let error_text = e.to_string();
                    if let Some(fallback) =
                        make_assembly_fallback(binary, binary_data, func, &error_text)
                    {
                        (
                            Ok(RenderedCode {
                                code: fallback,
                                postprocess_sec: 0.0,
                                engine_used: PreviewEngineMode::Legacy.as_str(),
                                fell_back: true,
                                fallback_reason: Some(fallback_reason_with_kind(
                                    "assembly_fallback",
                                    &error_text,
                                )),
                                preview_build_stats: None,
                                preview_hint_stats: None,
                            }),
                            0.0,
                        )
                    } else {
                        (Err(e), 0.0)
                    }
                }
            };
            let timing = main_decomp.get_last_timing_json().ok();
            entries.push(DecompEntry {
                address: func.address,
                name: func.name.clone(),
                code: code_result,
                decomp_sec,
                postprocess_sec,
                last_timing_json: timing,
            });
        }
        entries
    } else {
        Vec::new()
    };

    // Pre-serialize Win API signatures once (avoid per-worker JSON serialization).
    let signatures_json = serialize_win_api_signatures_json();

    // Each worker creates its own decompiler (init per bucket). num_workers is capped above
    // so that small batches (e.g. limit 20) use 1 worker → 26s; large batches use all cores.
    let rest_buckets: Vec<_> = buckets.into_iter().skip(1).collect();
    let rest_results: Vec<Vec<DecompEntry>> = rest_buckets
        .par_iter()
        .map(|bucket| {
            let mut decomp = init_decompiler(false);
            apply_profile(&mut decomp, selected_profile);
            let mut opts = PrepareOptions {
                verbose: false,
                compiler_id: compiler_id.as_deref(),
                gdt_path: gdt_path_owned.as_deref(),
                timeout_ms: Some(cli.timeout_ms.unwrap_or(config.decompiler.timeout_ms)),
                timings: None,
                signatures_json: signatures_json.as_deref(),
            };
            if prepare_native_decompiler_for_binary(&mut decomp, binary, binary_data, &mut opts)
                .is_err()
            {
                return bucket
                    .iter()
                    .map(|f| DecompEntry {
                        address: f.address,
                        name: f.name.clone(),
                        code: Err(fission_core::FissionError::decompiler("Prepare failed")),
                        decomp_sec: 0.0,
                        postprocess_sec: 0.0,
                        last_timing_json: None,
                    })
                    .collect();
            }

            let mut entries = Vec::with_capacity(bucket.len());
            for func in bucket.iter().copied() {
                let start = std::time::Instant::now();
                let code_result = decompile_code_with_profile(
                    selected_profile,
                    engine_mode,
                    &mut decomp,
                    binary,
                    func.address,
                    &func.name,
                    cli.timeout_ms,
                    false,
                );
                let decomp_sec = start.elapsed().as_secs_f64();
                let (code_result, postprocess_sec) = match code_result {
                    Ok(rendered) => {
                        let postprocess_sec = rendered.postprocess_sec;
                        (Ok(rendered), postprocess_sec)
                    }
                    Err(e) => {
                        let error_text = e.to_string();
                        if let Some(fallback) =
                            make_assembly_fallback(binary, binary_data, func, &error_text)
                        {
                            (
                                Ok(RenderedCode {
                                    code: fallback,
                                    postprocess_sec: 0.0,
                                    engine_used: PreviewEngineMode::Legacy.as_str(),
                                    fell_back: true,
                                    fallback_reason: Some(fallback_reason_with_kind(
                                        "assembly_fallback",
                                        &error_text,
                                    )),
                                    preview_build_stats: None,
                                    preview_hint_stats: None,
                                }),
                                0.0,
                            )
                        } else {
                            (Err(e), 0.0)
                        }
                    }
                };
                let timing = decomp.get_last_timing_json().ok();
                entries.push(DecompEntry {
                    address: func.address,
                    name: func.name.clone(),
                    code: code_result,
                    decomp_sec,
                    postprocess_sec,
                    last_timing_json: timing,
                });
            }
            entries
        })
        .collect();

    let all_entries: Vec<DecompEntry> = {
        let mut entries: Vec<DecompEntry> = first_bucket_entries
            .into_iter()
            .chain(rest_results.into_iter().flatten())
            .collect();
        entries.sort_by_key(|e| e.address);
        entries
    };

    let mut all_output = String::new();
    let mut json_results = Vec::new();
    let mut total_decomp_secs = 0.0;
    let mut total_postprocess_secs = 0.0;

    for entry in all_entries {
        total_decomp_secs += entry.decomp_sec;
        total_postprocess_secs += entry.postprocess_sec;

        match &entry.code {
            Ok(rendered) => {
                let mut filtered = rendered.code.clone();
                if effective_no_warnings {
                    filtered = strip_warnings(&filtered);
                }
                if cli.ghidra_compat {
                    filtered = strip_inferred_structs(&filtered);
                }

                if effective_json {
                    let mut json_entry = serde_json::json!({
                        "address": format!("0x{:x}", entry.address),
                        "name": entry.name,
                        "code": filtered,
                        "engine_used": rendered.engine_used,
                        "fell_back": rendered.fell_back,
                        "fallback_reason": rendered.fallback_reason,
                    });
                    if let Some(stats) = rendered.preview_build_stats {
                        json_entry["preview_build_stats"] = serde_json::json!(stats);
                    }
                    if let Some(stats) = rendered.preview_hint_stats {
                        json_entry["preview_hint_stats"] = serde_json::json!(stats);
                    }
                    if cli.benchmark {
                        json_entry["decomp_sec"] = serde_json::json!(
                            (entry.decomp_sec * 1_000_000.0).round() / 1_000_000.0
                        );
                        json_entry["postprocess_sec"] = serde_json::json!(
                            (entry.postprocess_sec * 1_000_000.0).round() / 1_000_000.0
                        );
                        if let Some(ref timing) = entry.last_timing_json {
                            if !timing.is_empty() && timing != "{}" {
                                if let Ok(v) = serde_json::from_str::<serde_json::Value>(timing) {
                                    json_entry["native_timing"] = v;
                                }
                            }
                        }
                    }
                    json_results.push(json_entry);
                } else {
                    if !effective_no_header {
                        all_output.push_str("// ============================================\n");
                        all_output.push_str(&format!(
                            "// Function: {} @ 0x{:x}\n",
                            entry.name, entry.address
                        ));
                        all_output.push_str("// ============================================\n\n");
                    }
                    all_output.push_str(&filtered);
                    all_output.push_str("\n\n");
                }
            }
            Err(e) => {
                if effective_json {
                    let mut json_entry = serde_json::json!({
                        "address": format!("0x{:x}", entry.address),
                        "name": entry.name,
                        "engine_used": PreviewEngineMode::Legacy.as_str(),
                        "fell_back": true,
                        "fallback_reason": fallback_reason_with_kind(classify_native_failure_kind(&e.to_string()), e.to_string()),
                        "error": e.to_string()
                    });
                    if cli.benchmark {
                        json_entry["decomp_sec"] = serde_json::json!(
                            (entry.decomp_sec * 1_000_000.0).round() / 1_000_000.0
                        );
                        if let Some(ref timing) = entry.last_timing_json {
                            if !timing.is_empty() && timing != "{}" {
                                if let Ok(v) = serde_json::from_str::<serde_json::Value>(timing) {
                                    json_entry["native_timing"] = v;
                                }
                            }
                        }
                    }
                    json_results.push(json_entry);
                } else {
                    all_output.push_str(&format!(
                        "// Error decompiling {} (0x{:x}): {}\n\n",
                        entry.name, entry.address, e
                    ));
                }
            }
        }
    }

    (
        all_output,
        json_results,
        total_decomp_secs,
        total_postprocess_secs,
    )
}

fn collect_target_functions<'a>(
    binary: &'a LoadedBinary,
    address: Option<u64>,
    decomp_all: bool,
    decomp_limit: Option<usize>,
) -> Vec<&'a FunctionInfo> {
    if decomp_all {
        let collected: Vec<_> = binary.functions.iter().collect();
        if let Some(n) = decomp_limit {
            return collected.into_iter().take(n).collect();
        }
        return collected;
    }

    if let Some(addr) = address {
        let mut best: Option<&FunctionInfo> = None;
        for func in &binary.functions {
            if func.address != addr {
                continue;
            }
            match best {
                None => best = Some(func),
                Some(current) => {
                    if prefer_function_name(&func.name, &current.name) {
                        best = Some(func);
                    }
                }
            }
        }
        return best.into_iter().collect();
    }

    vec![]
}

pub(super) fn run_decompilation(
    cli: &OneShotArgs,
    binary: &LoadedBinary,
    binary_data: &[u8],
) -> io::Result<()> {
    let init_start = std::time::Instant::now();
    let mut decomp = init_decompiler(cli.verbose);

    // Apply one-shot profile before binary load/decompilation.
    let (selected_profile, unknown_profile) = resolve_profile(cli.profile.as_deref());
    let (engine_mode, unknown_engine, deprecated_preview_alias) =
        resolve_engine_mode(cli.engine.as_deref(), cli.profile.as_deref());
    if let Some(other) = unknown_profile {
        eprintln!(
            "[!] Unknown --profile '{}', using balanced (quality|speed|balanced|mlil-preview)",
            other
        );
        warn!(
            profile = other,
            "unknown decompilation profile, using balanced"
        );
    }
    if let Some(other) = unknown_engine {
        eprintln!(
            "[!] Unknown --engine '{}', using auto (mlil-preview|auto)",
            other
        );
        warn!(engine = other, "unknown decompilation engine, using auto");
    }
    if matches!(engine_mode, EngineMode::Legacy) && cli.verbose {
        eprintln!(
            "[*] '--engine legacy' is a hidden compatibility mode; preview-first remains the product default"
        );
    }
    if deprecated_preview_alias && cli.verbose {
        eprintln!(
            "[*] '--profile mlil-preview' is deprecated; use '--engine mlil-preview --profile quality'"
        );
    }
    apply_profile(&mut decomp, selected_profile);

    if cli.verbose {
        eprintln!("[*] Decompilation profile = {}", selected_profile);
        eprintln!("[*] Decompilation engine = {:?}", engine_mode);
    }

    let mut prepare_timings = PrepareTimings::default();
    {
        let (compiler_id, unknown_compiler) =
            resolve_compiler_id(binary, cli.compiler_id.as_deref());
        if let Some(user_compiler) = unknown_compiler {
            eprintln!(
                "[!] Unknown --compiler-id '{}', falling back to auto detection",
                user_compiler
            );
            warn!(
                compiler_id = user_compiler,
                "unknown compiler-id, falling back to auto detection"
            );
        }
        if cli.verbose {
            eprintln!(
                "[*] Decompiler compiler_id = {}",
                compiler_id.as_deref().unwrap_or("default")
            );
        }
        let config = fission_core::config::Config::default();
        let gdt_path_owned = fission_core::PATHS
            .get_gdt_path(binary.is_64bit)
            .and_then(|p| p.to_str().map(String::from));
        let signatures_json = serialize_win_api_signatures_json();
        let mut options = PrepareOptions {
            verbose: cli.verbose,
            compiler_id: compiler_id.as_deref(),
            gdt_path: gdt_path_owned.as_deref(),
            timeout_ms: Some(cli.timeout_ms.unwrap_or(config.decompiler.timeout_ms)),
            timings: if cli.benchmark {
                Some(&mut prepare_timings)
            } else {
                None
            },
            signatures_json: signatures_json.as_deref(),
        };
        if let Err(e) =
            prepare_native_decompiler_for_binary(&mut decomp, binary, binary_data, &mut options)
        {
            eprintln!("Error: Failed to prepare decompiler: {}", e);
            std::process::exit(1);
        }
    }

    let init_elapsed = init_start.elapsed();
    if cli.verbose {
        eprintln!(
            "[✓] Decompiler ready (init: {:.3}s)",
            init_elapsed.as_secs_f64()
        );
    }

    // Collect functions to decompile and deduplicate by address.
    // Some loaders may expose multiple aliases for a single address
    // (e.g., sub_xxx + exported symbol), which can trigger duplicate
    // decompile attempts and noisy recursive-guard errors.
    let functions = collect_target_functions(binary, cli.address, cli.decomp_all, cli.decomp_limit);

    if functions.is_empty() && cli.address.is_some() {
        // Use if-let for safer unwrapping
        if let Some(addr) = cli.address {
            eprintln!("Warning: No function found at address 0x{:x}", addr);
            // Try to decompile anyway
            decompile_and_output(
                cli,
                &mut decomp,
                binary,
                binary_data,
                selected_profile,
                engine_mode,
                addr,
                &format!("sub_{:x}", addr),
            )?;
        }
        return Ok(());
    }

    // Derive effective flags: --ghidra-compat implies --no-header + --no-warnings
    // --benchmark implies --json
    let effective_no_header = cli.no_header || cli.ghidra_compat;
    let effective_no_warnings = cli.no_warnings || cli.ghidra_compat;
    let effective_json = cli.json || cli.benchmark;

    let use_parallel = (cli.decomp_all || cli.decomp_limit.is_some()) && functions.len() > 1;

    let (all_output, json_results, total_decomp_secs, total_postprocess_secs) = if use_parallel {
        run_parallel_decompilation(
            cli,
            &mut decomp,
            binary,
            binary_data,
            &functions,
            &prepare_timings,
            selected_profile,
            engine_mode,
            init_elapsed.as_secs_f64(),
            init_start,
            effective_no_header,
            effective_no_warnings,
            effective_json,
        )
    } else {
        run_sequential_decompilation(
            cli,
            &mut decomp,
            binary,
            binary_data,
            &functions,
            selected_profile,
            engine_mode,
            effective_no_header,
            effective_no_warnings,
            effective_json,
        )
    };

    // In benchmark mode, wrap results with metadata envelope
    let final_output = if cli.benchmark {
        let envelope = serde_json::json!({
            "_meta": {
                "tool": "fission",
                "version": env!("CARGO_PKG_VERSION"),
                "profile": cli.profile.as_deref().unwrap_or("balanced"),
                "engine": cli.engine.as_deref().unwrap_or("auto"),
                "function_count": functions.len(),
                "init_sec": (init_elapsed.as_secs_f64() * 1_000_000.0).round() / 1_000_000.0,
                "prepare_timings": &prepare_timings,
                "total_decomp_sec": (total_decomp_secs * 1_000_000.0).round() / 1_000_000.0,
                "total_postprocess_sec": (total_postprocess_secs * 1_000_000.0).round() / 1_000_000.0,
                "wall_clock_sec": (init_start.elapsed().as_secs_f64() * 1_000_000.0).round() / 1_000_000.0,
            },
            "functions": json_results
        });
        serde_json::to_string_pretty(&envelope).map_err(|e| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("JSON serialization failed: {}", e),
            )
        })?
    } else if effective_json {
        serde_json::to_string_pretty(&json_results).map_err(|e| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("JSON serialization failed: {}", e),
            )
        })?
    } else {
        all_output
    };

    if let Some(ref output_path) = cli.output {
        let mut file = fs::File::create(output_path).map_err(|e| {
            io::Error::new(
                io::ErrorKind::Other,
                format!(
                    "Failed to create output file '{}': {}",
                    output_path.display(),
                    e
                ),
            )
        })?;
        file.write_all(final_output.as_bytes())?;
        if cli.verbose {
            eprintln!("[✓] Output written to: {}", output_path.display());
        }
    } else {
        let mut stdout = io::stdout().lock();
        stdout.write_all(final_output.as_bytes())?;
    }
    Ok(())
}

pub(super) fn decompile_and_output(
    cli: &OneShotArgs,
    decomp: &mut DecompilerNative,
    binary: &LoadedBinary,
    binary_data: &[u8],
    selected_profile: &str,
    engine_mode: EngineMode,
    addr: u64,
    name: &str,
) -> io::Result<()> {
    let effective_no_header = cli.no_header || cli.ghidra_compat;
    let effective_no_warnings = cli.no_warnings || cli.ghidra_compat;

    let _silencer = OutputSilencer::new_if(!cli.verbose);
    match decompile_code_with_profile(
        selected_profile,
        engine_mode,
        decomp,
        binary,
        addr,
        name,
        cli.timeout_ms,
        cli.verbose,
    ) {
        Ok(rendered) => {
            // Apply output filters
            let mut filtered = rendered.code.clone();
            if effective_no_warnings {
                filtered = strip_warnings(&filtered);
            }
            if cli.ghidra_compat {
                filtered = strip_inferred_structs(&filtered);
            }
            // Prepare final output string (respect --output when provided)
            if cli.json {
                let json_output = serde_json::to_string_pretty(&serde_json::json!({
                    "address": format!("0x{:x}", addr),
                    "name": name,
                    "code": filtered,
                    "engine_used": rendered.engine_used,
                    "fell_back": rendered.fell_back,
                    "fallback_reason": rendered.fallback_reason,
                    "preview_build_stats": rendered.preview_build_stats,
                    "preview_hint_stats": rendered.preview_hint_stats,
                }))
                .map_err(|e| {
                    io::Error::new(
                        io::ErrorKind::Other,
                        format!("JSON serialization failed: {}", e),
                    )
                })?;
                if let Some(ref output_path) = cli.output {
                    fs::write(output_path, json_output.as_bytes())?;
                    if cli.verbose {
                        eprintln!("[✓] Output written to: {}", output_path.display());
                    }
                } else {
                    let mut stdout = io::stdout().lock();
                    writeln!(stdout, "{}", json_output)?;
                }
            } else {
                let mut out_buf = String::new();
                if !effective_no_header {
                    out_buf.push_str(&format!("// Function: {} @ 0x{:x}\n\n", name, addr));
                }
                out_buf.push_str(&filtered);
                out_buf.push_str("\n");

                if let Some(ref output_path) = cli.output {
                    fs::write(output_path, out_buf.as_bytes())?;
                    if cli.verbose {
                        eprintln!("[✓] Output written to: {}", output_path.display());
                    }
                } else {
                    let mut stdout = io::stdout().lock();
                    writeln!(stdout, "{}", out_buf)?;
                }
            }
        }
        Err(e) => {
            let error_text = e.to_string();
            if let Some(func) = binary.function_at_exact(addr)
                && let Some(fallback) =
                    make_assembly_fallback(binary, binary_data, func, &error_text)
            {
                let mut stdout = io::stdout().lock();
                writeln!(stdout, "{}", fallback)?;
                return Ok(());
            }
            eprintln!("Error: {}", error_text);
            std::process::exit(1);
        }
    }
    Ok(())
}
