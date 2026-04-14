use super::super::*;
use crate::cli::oneshot::assessment::canonical_indirect_classification;
use fission_pcode::NirBuildStats;

fn slot_alias_candidate(code: &str) -> bool {
    code.contains("slot_")
}

pub(crate) fn preview_goto_count(code: &str) -> usize {
    code.matches("goto ").count()
}

pub(crate) fn classify_nir_output_class(
    direct_success: bool,
    surface_kind: Option<NirSurfaceKind>,
    goto_count: Option<usize>,
    build_stats: Option<NirBuildStats>,
) -> Option<String> {
    if !direct_success {
        return None;
    }
    let goto_count = goto_count.unwrap_or(0);
    let build_stats = build_stats.unwrap_or_default();
    if build_stats.forced_linear_structuring_count > 0 {
        return Some("linear_fallback".to_string());
    }
    if surface_kind == Some(NirSurfaceKind::Structured) && goto_count == 0 {
        return Some("structured".to_string());
    }
    Some("partially_structured".to_string())
}

pub(crate) fn explicit_hint_surface_count(stats: Option<NirHintStats>) -> usize {
    stats.map_or(0, |stats| {
        stats.explicit_param_name_hits
            + stats.explicit_local_name_hits
            + stats.explicit_param_type_hits
            + stats.explicit_local_type_hits
            + stats.explicit_return_type_hit
    })
}

pub(crate) fn preview_surface_kind_str(kind: Option<NirSurfaceKind>) -> Option<String> {
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

pub(crate) fn build_quality_tags_and_score(
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

pub(crate) fn fact_density(
    has_dwarf_function: bool,
    dwarf_param_count: usize,
    dwarf_local_count: usize,
    has_dwarf_return_type: bool,
    loader_type_count: usize,
) -> i32 {
    fact_density_score(
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
    fn stats_only_indirect_classification_fail_closed_without_stats() {
        let classification = canonical_indirect_classification(None);
        assert!(!classification.has_indirect_control);
        assert!(!classification.has_dispatcher_recovery);
    }

    #[test]
    fn stats_only_indirect_classification_prefers_canonical_payload() {
        let classification = canonical_indirect_classification(Some(&NirBuildStats {
            dispatcher_shape_recovered_count: 1,
            ..Default::default()
        }));
        assert!(classification.has_indirect_control);
        assert!(classification.has_dispatcher_recovery);
    }
}
