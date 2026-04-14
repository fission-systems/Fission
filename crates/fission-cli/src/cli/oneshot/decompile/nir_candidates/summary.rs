use super::super::*;
use super::assessment::canonical_indirect_classification;
use super::schema::{PreviewCandidateEntry, PreviewCandidateScanSummary};
use fission_pcode::NirBuildStats;

fn pcode_total_ops(pcode: &PcodeFunction) -> usize {
    pcode.blocks.iter().map(|block| block.ops.len()).sum()
}

pub(crate) fn strict_explicit_candidate(entry: &PreviewCandidateEntry) -> bool {
    let indirect = canonical_indirect_classification(entry.nir_build_stats.as_ref());
    (entry.dwarf_param_count + entry.dwarf_local_count + usize::from(entry.has_dwarf_return_type))
        >= 2
        && entry.preview_direct_success
        && indirect.allows_strict_explicit_candidate(entry.pcode_op_count)
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

pub(super) fn canonicalize_nir_failure_kind(kind: Option<&str>) -> Option<String> {
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

pub(super) fn preview_block_signature(
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

pub(super) fn preview_block_detail(
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
    if let Some(output_class) = entry.nir_output_class.as_ref() {
        *summary
            .nir_output_class_counts
            .entry(output_class.clone())
            .or_insert(0) += 1;
    }
    if let Some(build_stats) = entry.nir_build_stats {
        summary.nir_build_stats_totals.merge_assign(&build_stats);
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

pub(super) fn pcode_metrics(pcode: &PcodeFunction) -> (usize, usize) {
    (pcode.blocks.len(), pcode_total_ops(pcode))
}

pub(super) fn fact_density(
    has_dwarf_function: bool,
    dwarf_param_count: usize,
    dwarf_local_count: usize,
    has_dwarf_return_type: bool,
    loader_type_count: usize,
) -> i32 {
    super::assessment::fact_density(
        has_dwarf_function,
        dwarf_param_count,
        dwarf_local_count,
        has_dwarf_return_type,
        loader_type_count,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_prefer_over_raw_flags_for_indirect_classification() {
        let stats = NirBuildStats {
            dispatcher_shape_recovered_count: 1,
            ..Default::default()
        };
        let classification = canonical_indirect_classification(Some(&stats));
        assert!(classification.has_indirect_control);
        assert!(classification.has_dispatcher_recovery);
        assert!(!classification.has_unresolved_unsupported_indirect);
    }

    #[test]
    fn missing_stats_fail_closed_for_indirect_classification() {
        let classification = canonical_indirect_classification(None);
        assert!(!classification.has_indirect_control);
        assert!(!classification.has_preserved_indirect_surface);
        assert!(!classification.has_unresolved_unsupported_indirect);
    }
}
