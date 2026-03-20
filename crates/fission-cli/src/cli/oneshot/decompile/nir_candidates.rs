use super::*;

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
    pub(crate) auto_eligible: bool,
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

fn preview_goto_count(code: &str) -> usize {
    code.matches("goto ").count()
}

fn explicit_hint_surface_count(stats: Option<NirHintStats>) -> usize {
    stats.map_or(0, |stats| {
        stats.explicit_param_name_hits
            + stats.explicit_local_name_hits
            + stats.explicit_param_type_hits
            + stats.explicit_local_type_hits
            + stats.explicit_return_type_hit
    })
}

fn preview_surface_kind_str(kind: Option<NirSurfaceKind>) -> Option<String> {
    match kind {
        Some(NirSurfaceKind::Structured) => Some("structured".to_string()),
        Some(NirSurfaceKind::Unstructured) => Some("unstructured".to_string()),
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
    preview_surface_kind: Option<NirSurfaceKind>,
    pcode_block_count: usize,
    pcode_op_count: usize,
    has_indirect_control_flow: bool,
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
    if !has_indirect_control_flow && pcode_block_count <= 12 && pcode_op_count <= 600 {
        tags.push("low_cfg_risk".to_string());
    }
    if preview_code.is_some_and(slot_alias_candidate) {
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

pub(crate) fn strict_explicit_candidate(entry: &PreviewCandidateEntry) -> bool {
    (entry.dwarf_param_count + entry.dwarf_local_count + usize::from(entry.has_dwarf_return_type))
        >= 2
        && entry.preview_direct_success
        && !entry.has_indirect_control_flow
        && entry.pcode_op_count <= 800
}

pub(crate) fn effective_failure_kind(entry: &PreviewCandidateEntry) -> &str {
    if entry.row_status == "ok" {
        return "direct_success";
    }
    if let Some(kind) = entry.row_error_kind.as_deref() {
        return kind;
    }
    entry
        .preview_fallback_kind_refined
        .as_deref()
        .or(entry.preview_fallback_kind.as_deref())
        .unwrap_or("preview_non_success_unknown")
}

fn canonicalize_nir_failure_kind(kind: Option<&str>) -> Option<String> {
    let kind = kind?;
    let canonical = match kind {
        "preview_timeout" => "nir_timeout",
        "preview_unsupported" => "nir_unsupported",
        "preview_frontend_reject" => "nir_frontend_reject",
        "preview_worker_failure" => "nir_worker_failure",
        "preview_structuring_failure" => "nir_structuring_failure",
        "preview_parse_or_lowering_failure" => "nir_parse_or_lowering_failure",
        "preview_unsupported_cfg" => "nir_unsupported_cfg",
        "preview_architecture_unsupported" => "nir_architecture_unsupported",
        "preview_format_unsupported" => "nir_format_unsupported",
        "preview_non_success_unknown" => "nir_non_success_unknown",
        other => other,
    };
    Some(canonical.to_string())
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

pub(crate) fn update_scan_summary(
    summary: &mut PreviewCandidateScanSummary,
    entry: &PreviewCandidateEntry,
) {
    summary.addresses_scanned += 1;
    if (entry.dwarf_param_count
        + entry.dwarf_local_count
        + usize::from(entry.has_dwarf_return_type))
        > 0
    {
        summary.nonzero_explicit_candidates += 1;
    }
    if strict_explicit_candidate(entry) {
        summary.strict_explicit_candidates += 1;
    }
    match entry.row_status.as_str() {
        "preview_failure" => {
            summary.nir_failure_count += 1;
            summary.preview_failure_count += 1;
        }
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
        *summary
            .row_error_kind_counts
            .entry(kind.to_string())
            .or_insert(0) += 1;
    }
    if entry.recovery_strategy_attempted.is_some()
        && let Some(mode) = entry.recovery_structuring_mode.as_deref()
    {
        *summary
            .recovery_structuring_mode_counts
            .entry(mode.to_string())
            .or_insert(0) += 1;
    }
    for flag in &entry.recovery_quality_flags {
        *summary
            .recovery_quality_flag_counts
            .entry(flag.clone())
            .or_insert(0) += 1;
    }
}

pub(crate) fn write_scan_summary(
    path: &std::path::Path,
    summary: &PreviewCandidateScanSummary,
) -> io::Result<()> {
    let body = serde_json::to_string_pretty(summary)
        .map_err(|e| io::Error::other(format!("JSON serialization failed: {e}")))?;
    fs::write(path, body)
}

pub(crate) fn load_resume_rows(
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
    let mut recovery_strategy_attempted = None;
    let mut recovery_strategy_applied = None;
    let mut recovery_outcome = None;
    let mut recovery_source_signature = None;
    let mut recovery_structuring_mode = Some("normal".to_string());
    let recovery_goto_count_before = None;
    let mut recovery_goto_count_after = None;
    let mut recovery_hint_surface_before = None;
    let mut recovery_hint_surface_after = None;

    match decomp.get_pcode(func.address) {
        Ok(pcode_json) => {
            if let Ok(pcode) = PcodeFunction::from_json(&pcode_json) {
                pcode_block_count = pcode.blocks.len();
                pcode_op_count = pcode_total_ops(&pcode);
                has_indirect = contains_indirect_control_flow(&pcode);
                auto_eligible = auto_nir_eligible(binary, &pcode);
            }

            if let Ok(selection) = select_nir_output_with_facts(
                decomp,
                binary,
                fact_store,
                func.address,
                &func.name,
                NirEngineMode::Nir,
                timeout_ms,
            ) {
                preview_direct_success = selection.nir_code.is_some()
                    && !selection.fell_back
                    && selection.engine_used == NirEngineMode::Nir;
                preview_fallback_kind = selection.fallback_kind.map(str::to_string);
                preview_fallback_kind_refined = selection.fallback_kind_refined.map(str::to_string);
                preview_fallback_reason = selection.fallback_reason.clone();
                preview_surface_kind = selection.nir_surface;
                preview_hint_stats = selection.hint_stats;
                preview_code = selection.nir_code;
                recovery_strategy_attempted =
                    selection.recovery_strategy_attempted.map(str::to_string);
                recovery_strategy_applied = selection.recovery_strategy_applied.map(str::to_string);
                recovery_outcome = selection.recovery_outcome.map(str::to_string);
                recovery_source_signature = selection.recovery_source_signature.clone();
                recovery_structuring_mode = selection
                    .recovery_structuring_mode
                    .map(str::to_string)
                    .or(recovery_structuring_mode);
                if selection.recovery_strategy_attempted.is_some() {
                    recovery_goto_count_after = preview_code.as_deref().map(preview_goto_count);
                    recovery_hint_surface_before = Some(0);
                    recovery_hint_surface_after =
                        Some(explicit_hint_surface_count(preview_hint_stats));
                }
            }
        }
        Err(err) => {
            preview_fallback_kind = Some("preview_unsupported".to_string());
            preview_fallback_kind_refined = Some("preview_frontend_reject".to_string());
            preview_fallback_reason = Some(format!(
                "mlil-preview frontend unavailable: failed to load pcode: {err}"
            ));
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
    let preview_block_detail = preview_block_detail(
        row_error_message.as_deref(),
        preview_fallback_reason.as_deref(),
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
        nir_direct_success: preview_direct_success,
        nir_fallback_kind: canonicalize_nir_failure_kind(preview_fallback_kind.as_deref()),
        nir_fallback_kind_refined: canonicalize_nir_failure_kind(
            preview_fallback_kind_refined.as_deref(),
        ),
        nir_fallback_reason: preview_fallback_reason.clone(),
        nir_block_signature: preview_block_signature.clone(),
        nir_block_detail: preview_block_detail.clone(),
        preview_direct_success,
        preview_fallback_kind,
        preview_fallback_kind_refined,
        preview_fallback_reason,
        preview_block_signature,
        preview_block_detail,
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
        pcode_block_count,
        pcode_op_count,
        has_indirect_control_flow: has_indirect,
        auto_eligible,
        nir_surface_kind: preview_surface_kind_str(preview_surface_kind),
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
        nir_direct_success: false,
        nir_fallback_kind: Some("internal_error".to_string()),
        nir_fallback_kind_refined: canonicalize_nir_failure_kind(Some(row_error_kind))
            .or_else(|| Some(row_error_kind.to_string())),
        nir_fallback_reason: Some(reason.clone()),
        nir_block_signature: preview_block_signature(
            Some(row_error_kind),
            Some(reason.as_str()),
            false,
            0,
            0,
        ),
        nir_block_detail: Some(reason.clone()),
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
        pcode_block_count: 0,
        pcode_op_count: 0,
        has_indirect_control_flow: false,
        auto_eligible: false,
        nir_surface_kind: None,
        preview_surface_kind: None,
        quality_potential_score,
        reason_tags,
        preview_hint_stats: None,
    }
}

pub(crate) fn preview_candidate_entry_with_recovery(
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
