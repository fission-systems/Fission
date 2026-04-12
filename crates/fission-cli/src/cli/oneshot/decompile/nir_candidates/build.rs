use super::super::*;
use super::schema::PreviewCandidateEntry;
use super::summary::{
    build_quality_tags_and_score, canonicalize_nir_failure_kind, classify_nir_output_class,
    explicit_hint_surface_count, fact_density, pcode_metrics, preview_block_detail,
    preview_block_signature, preview_goto_count, preview_surface_kind_str,
};
use fission_pcode::{IndirectControlClassification, NirBuildStats};

fn canonical_indirect_classification(
    build_stats: Option<&NirBuildStats>,
    _raw_has_indirect_control_flow: bool,
) -> IndirectControlClassification {
    IndirectControlClassification::from_stats_only(build_stats)
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
    let fact_density_score = fact_density(
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
    let mut nir_build_stats = None;
    let mut raw_has_indirect_control_flow = false;
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
                let metrics = pcode_metrics(&pcode);
                pcode_block_count = metrics.0;
                pcode_op_count = metrics.1;
                raw_has_indirect_control_flow = metrics.2;
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
                nir_build_stats = selection.build_stats;
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

            let indirect_classification = canonical_indirect_classification(
                nir_build_stats.as_ref(),
                raw_has_indirect_control_flow,
            );
            has_indirect = indirect_classification.has_indirect_control;
        }
        Err(err) => {
            preview_fallback_kind = Some("preview_unsupported".to_string());
            preview_fallback_kind_refined = Some("preview_frontend_reject".to_string());
            preview_fallback_reason = Some(format!(
                "mlil-preview frontend unavailable: failed to load pcode: {err}"
            ));
            has_indirect = raw_has_indirect_control_flow;
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
    let nir_goto_count = preview_code.as_deref().map(preview_goto_count);
    let nir_output_class = classify_nir_output_class(
        preview_direct_success,
        preview_surface_kind,
        nir_goto_count,
        nir_build_stats,
    );
    let indirect_classification =
        IndirectControlClassification::from_stats_only(nir_build_stats.as_ref());
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
        has_indirect_control_flow: indirect_classification.has_indirect_control,
        has_preserved_indirect_surface: indirect_classification.has_preserved_indirect_surface,
        has_unresolved_unsupported_indirect: indirect_classification
            .has_unresolved_unsupported_indirect,
        has_dispatcher_recovery: indirect_classification.has_dispatcher_recovery,
        auto_eligible,
        nir_goto_count,
        nir_output_class,
        nir_build_stats,
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
    let fact_density_score = fact_density(
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
        has_preserved_indirect_surface: false,
        has_unresolved_unsupported_indirect: false,
        has_dispatcher_recovery: false,
        auto_eligible: false,
        nir_goto_count: None,
        nir_output_class: None,
        nir_build_stats: None,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_stats_prefer_over_raw_flags_for_indirect_classification() {
        let classification = canonical_indirect_classification(
            Some(&NirBuildStats {
                unsupported_indirect_control_count: 1,
                ..Default::default()
            }),
            false,
        );

        assert!(classification.has_indirect_control);
        assert!(classification.has_unresolved_unsupported_indirect);
        assert!(!classification.has_preserved_indirect_surface);
    }

    #[test]
    fn legacy_flags_remain_fallback_when_stats_missing() {
        let classification = canonical_indirect_classification(None, true);

        assert!(classification.has_indirect_control);
        assert!(!classification.has_unresolved_unsupported_indirect);
    }
}
