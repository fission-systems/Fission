use crate::pcode::{PcodeFunction, PcodeOp, PcodeOpcode, Varnode};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::Instant;

mod builder;
mod cfg;
mod normalize;
mod piece;
mod printer;
mod structuring;
mod support;
mod telemetry;
mod var_rename;
#[cfg(test)]
mod tests;
mod types;
mod vsa;

pub(super) use self::support::*;
pub use self::support::CallingConvention;
pub use self::telemetry::{
    take_last_nir_build_stats, take_last_nir_hint_stats, take_last_preview_build_stats,
    take_last_preview_hint_stats,
};
pub use self::types::*;
use self::{builder::*, cfg::*, normalize::*, printer::*, structuring::*};

pub fn render_mlil_preview(
    pcode: &PcodeFunction,
    name: &str,
    address: u64,
    options: &MlilPreviewOptions,
) -> Result<String, MlilPreviewError> {
    render_mlil_preview_with_context(pcode, name, address, options, None)
}

pub fn render_nir(
    pcode: &PcodeFunction,
    name: &str,
    address: u64,
    options: &NirRenderOptions,
) -> Result<String, MlilPreviewError> {
    render_mlil_preview(pcode, name, address, options)
}

pub fn render_mlil_preview_with_context(
    pcode: &PcodeFunction,
    name: &str,
    address: u64,
    options: &MlilPreviewOptions,
    type_context: Option<&PreviewTypeContext>,
) -> Result<String, MlilPreviewError> {
    telemetry::reset_preview_telemetry();
    let diag = std::env::var_os("FISSION_PREVIEW_DIAG").is_some();
    let debug_log = |stage: &str| {
        if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
            let _ = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(format!("/tmp/fission_preview_{address:x}.log"))
                .and_then(|mut f| {
                    std::io::Write::write_all(
                        &mut f,
                        format!("[mlil-preview] stage={stage}\n").as_bytes(),
                    )
                });
        }
    };
    if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
        let _ = std::fs::remove_file(format!("/tmp/fission_preview_{address:x}_unsupported.json"));
    }
    if options.pe_x64_only && !options.is_supported_pe() {
        return Err(MlilPreviewError::UnsupportedArchitectureDetailed);
    }

    let build_start = Instant::now();
    if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
        eprintln!("[mlil-preview] stage=build_hir start fn=0x{address:x}");
    }
    debug_log("build_hir_start");
    let mut builder = PreviewBuilder::new(pcode, options, type_context);
    let mut hir = builder.build_hir(name, address).map_err(|err| {
        let mut stats = builder.preview_build_stats();
        stats.build_duration_ms = build_start.elapsed().as_millis() as usize;
        telemetry::store_preview_build_stats(stats);
        if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
            eprintln!("[mlil-preview] stage=build_hir error fn=0x{address:x} err={err}");
        }
        if matches!(err, MlilPreviewError::UnsupportedPattern("opcode")) {
            builder.record_unsupported_inventory_event(
                "build_hir_error",
                None,
                None,
                None,
                Some(address),
                None,
                true,
                "render_mlil_preview_with_context",
            );
        }
        debug_log("build_hir_error");
        err
    })?;
    let mut build_stats = builder.preview_build_stats();
    if diag {
        eprintln!(
            "[DIAG] build_hir done: fn=0x{address:x} elapsed={:.3}s body_stmts={} locals={}",
            build_start.elapsed().as_secs_f64(),
            hir.body.len(),
            hir.locals.len()
        );
    }
    if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
        eprintln!("[mlil-preview] stage=normalize start fn=0x{address:x}");
    }
    debug_log("normalize_start");
    let normalize_start = Instant::now();
    normalize_hir_function(&mut hir);
    build_stats.merge_assign(&normalize::take_normalize_wave_stats());
    let normalized_discovery_stats = discover_guarded_tail_candidates_for_stats(&hir.body);
    build_stats.promotion_candidate_count += normalized_discovery_stats.promotion_candidate_count;
    build_stats.promoted_region_count += normalized_discovery_stats.promoted_region_count;
    build_stats.promotion_rejected_by_shape_count +=
        normalized_discovery_stats.promotion_rejected_by_shape_count;
    build_stats.promotion_rejected_by_gate_count +=
        normalized_discovery_stats.promotion_rejected_by_gate_count;
    build_stats.discovery_seen_guarded_tail_like_shape_count +=
        normalized_discovery_stats.discovery_seen_guarded_tail_like_shape_count;
    build_stats.discovery_rejected_noncanonical_layout_count +=
        normalized_discovery_stats.discovery_rejected_noncanonical_layout_count;
    build_stats.canonicalized_guarded_tail_shape_count +=
        normalized_discovery_stats.canonicalized_guarded_tail_shape_count;
    build_stats.canonicalization_failed_multiple_payload_entries +=
        normalized_discovery_stats.canonicalization_failed_multiple_payload_entries;
    build_stats.canonicalization_failed_interleaved_join_uses +=
        normalized_discovery_stats.canonicalization_failed_interleaved_join_uses;
    build_stats.canonicalization_failed_interleaved_join_uses_no_next_label_count +=
        normalized_discovery_stats
            .canonicalization_failed_interleaved_join_uses_no_next_label_count;
    build_stats.canonicalization_failed_interleaved_join_uses_nontrivial_segment_count +=
        normalized_discovery_stats
            .canonicalization_failed_interleaved_join_uses_nontrivial_segment_count;
    build_stats.canonicalization_failed_nonterminal_join_label +=
        normalized_discovery_stats.canonicalization_failed_nonterminal_join_label;
    build_stats.canonicalization_failed_nested_tail_escape +=
        normalized_discovery_stats.canonicalization_failed_nested_tail_escape;
    build_stats.canonicalized_interleaved_join_use_count +=
        normalized_discovery_stats.canonicalized_interleaved_join_use_count;
    build_stats.canonicalized_local_nonfallthrough_alias_count +=
        normalized_discovery_stats.canonicalized_local_nonfallthrough_alias_count;
    build_stats.canonicalization_failed_alias_not_fallthrough_count +=
        normalized_discovery_stats.canonicalization_failed_alias_not_fallthrough_count;
    build_stats.canonicalization_failed_alias_not_fallthrough_top_level_after_label_count +=
        normalized_discovery_stats
            .canonicalization_failed_alias_not_fallthrough_top_level_after_label_count;
    build_stats.canonicalization_failed_alias_not_fallthrough_nested_after_label_count +=
        normalized_discovery_stats
            .canonicalization_failed_alias_not_fallthrough_nested_after_label_count;
    build_stats.canonicalization_failed_alias_has_multiple_internal_predecessors_count +=
        normalized_discovery_stats
            .canonicalization_failed_alias_has_multiple_internal_predecessors_count;
    build_stats.canonicalization_failed_alias_has_nonlocal_ref_count +=
        normalized_discovery_stats.canonicalization_failed_alias_has_nonlocal_ref_count;
    build_stats.canonicalization_failed_alias_body_not_trivial_count +=
        normalized_discovery_stats.canonicalization_failed_alias_body_not_trivial_count;
    build_stats.canonicalization_failed_join_has_external_ref_count +=
        normalized_discovery_stats.canonicalization_failed_join_has_external_ref_count;
    build_stats.canonicalization_failed_payload_crosses_join_count +=
        normalized_discovery_stats.canonicalization_failed_payload_crosses_join_count;
    build_stats.rejected_must_emit_label += normalized_discovery_stats.rejected_must_emit_label;
    build_stats.rejected_not_single_pred_succ +=
        normalized_discovery_stats.rejected_not_single_pred_succ;
    build_stats.rejected_external_entry += normalized_discovery_stats.rejected_external_entry;
    build_stats.rejected_loop_or_switch_target +=
        normalized_discovery_stats.rejected_loop_or_switch_target;
    build_stats.build_duration_ms = build_start.elapsed().as_millis() as usize;
    build_stats.normalize_duration_ms = normalize_start.elapsed().as_millis() as usize;
    telemetry::store_preview_build_stats(build_stats);
    if diag {
        eprintln!(
            "[DIAG] normalize stage done: fn=0x{address:x} elapsed={:.3}s body_stmts={} locals={}",
            normalize_start.elapsed().as_secs_f64(),
            hir.body.len(),
            hir.locals.len()
        );
    }
    debug_log("normalize_done");
    if let Some(context) = type_context {
        if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
            eprintln!("[mlil-preview] stage=type_hints start fn=0x{address:x}");
        }
        debug_log("type_hints_start");
        let type_hints_start = Instant::now();
        let hint_stats = apply_preview_type_hints(&mut hir, context);
        telemetry::store_preview_hint_stats(hint_stats);
        if diag {
            eprintln!(
                "[DIAG] type_hints done: fn=0x{address:x} elapsed={:.3}s",
                type_hints_start.elapsed().as_secs_f64()
            );
        }
        debug_log("type_hints_done");
    }
    if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
        eprintln!("[mlil-preview] stage=print start fn=0x{address:x}");
    }
    debug_log("print_start");
    let print_start = Instant::now();
    let rendered = print_hir_function(&hir);
    if diag {
        eprintln!(
            "[DIAG] print done: fn=0x{address:x} elapsed={:.3}s",
            print_start.elapsed().as_secs_f64()
        );
    }
    if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
        eprintln!("[mlil-preview] stage=print done fn=0x{address:x}");
    }
    debug_log("print_done");
    Ok(rendered)
}

pub fn render_nir_with_context(
    pcode: &PcodeFunction,
    name: &str,
    address: u64,
    options: &NirRenderOptions,
    type_context: Option<&NirTypeContext>,
) -> Result<String, MlilPreviewError> {
    render_mlil_preview_with_context(pcode, name, address, options, type_context)
}
