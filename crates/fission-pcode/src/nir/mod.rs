//! NIR/HIR pipeline after P-code lifting: preview builder, normalization,
//! structuring, rendering, and telemetry wired through [`types::NirBuildStats`].
//!
//! Typical stage order: [`builder`] → [`normalize`] → [`structuring`] →
//! [`crate::render`]. Directory guide: `crates/fission-pcode/src/nir/AGENTS.md`.

use crate::pcode::{PcodeFunction, PcodeOp, PcodeOpcode, Varnode};
use fission_loader::loader::LoadedBinary;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::time::Instant;

mod abi;
mod abstract_location;
mod action_pipeline;
mod builder;
mod cfg;
pub mod cspec;
mod normalize;
pub(crate) mod pass;
mod piece;
mod stats;
mod structuring;
mod support;
mod telemetry;
#[cfg(test)]
mod tests;
mod types;
mod var_rename;
mod vsa;

pub(crate) use self::abi::{
    AbiKind, AbiState, CarrierAssignment, CarrierResource, GenericAbiProvider,
    WindowsX64AbiProvider,
};
pub use self::abstract_location::{AbstractStackSlot, ParamSlotIndex};
pub(crate) use self::action_pipeline::STRUCTURING_TIME_CEILING_SECS;

pub(super) use self::support::*;
pub use self::telemetry::{
    take_last_nir_build_stats, take_last_nir_hint_stats, take_last_preview_build_stats,
    take_last_preview_hint_stats,
};
pub use self::types::*;
use self::{action_pipeline::*, builder::*, cfg::*, normalize::*, structuring::*};
// Presentation/print surface lives at crate root `render` (ADR 0011 Phase 1).
// `pub(crate)` so tests and owners can keep using `crate::nir::print_*`.
pub(crate) use crate::render::{
    print_expr, print_hir_function, print_hir_function_with_global_names,
    print_hir_function_with_profile, print_stmt, print_type, recover_global_symbol_accesses,
    render_hir_function_with_global_decls, render_layered_pseudocode,
};
pub use fission_core::CallingConvention;

pub use self::abi::infer_entry_register_param_arity;
pub use self::cfg::structuring_cfg_edges;
pub use self::cspec::RegisterNamer;
pub use self::normalize::{
    summarize_direct_tail_wrapper_from_ops, summarize_direct_tail_wrapper_from_pcode,
};
pub use crate::render::{
    LayeredPseudocode, PrintProfile, PseudocodeLayer, render_contracted_wrapper_summary,
};
// Compat path: historical `crate::nir::render::…` imports.
pub use crate::render as render;
// take_last_layered_pseudocode defined below after render path

pub fn test_refine_partitions(accesses: &[(i64, u32)]) -> Vec<(i64, u32)> {
    self::builder::test_refine_partitions(accesses)
}

pub fn render_mlil_preview(
    pcode: &PcodeFunction,
    name: &str,
    address: u64,
    options: &MlilPreviewOptions,
) -> Result<String, MlilPreviewError> {
    render_mlil_preview_with_binary_and_context(pcode, name, address, options, None, None, None)
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
    render_mlil_preview_with_binary_and_context(
        pcode,
        name,
        address,
        options,
        None,
        type_context,
        None,
    )
}

pub fn render_mlil_preview_with_binary_and_context(
    pcode: &PcodeFunction,
    name: &str,
    address: u64,
    options: &MlilPreviewOptions,
    binary: Option<&LoadedBinary>,
    type_context: Option<&PreviewTypeContext>,
    decomp_facts: Option<&mut dyn DecompFacts>,
) -> Result<String, MlilPreviewError> {
    let debug = RenderDebugFlags::from_env();
    telemetry::reset_preview_telemetry();
    let debug_log = |stage: &str| {
        if debug.preview_debug {
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
    if debug.preview_debug {
        let _ = std::fs::remove_file(format!("/tmp/fission_preview_{address:x}_unsupported.json"));
    }
    let target_profile = options.target_profile();
    if !target_profile.preview_eligible {
        let mut stats = PreviewBuildStats::default();
        stats.pe_admission_profile_mismatch_count = 1;
        telemetry::store_preview_build_stats(stats);
        return Err(MlilPreviewError::UnsupportedArchitectureDetailed);
    }

    if let Err(err) = pcode.validate() {
        if debug.diag || debug.preview_debug {
            eprintln!("[mlil-preview] invalid pcode shape fn=0x{address:x} err={err}");
        }
        let stats = PreviewBuildStats {
            invalid_pcode_shape_count: 1,
            ..PreviewBuildStats::default()
        };
        telemetry::store_preview_build_stats(stats);
        return Err(MlilPreviewError::UnsupportedPattern("invalid pcode shape"));
    }

    let build_start = Instant::now();
    if debug.preview_debug {
        eprintln!("[mlil-preview] stage=build_hir start fn=0x{address:x}");
    }
    debug_log("build_hir_start");
    let mut builder = PreviewBuilder::new_with_binary(pcode, options, binary, type_context);
    let mut hir = builder.build_hir(name, address).map_err(|err| {
        let mut stats = builder.preview_build_stats();
        stats.build_duration_ms = build_start.elapsed().as_millis() as usize;
        telemetry::store_preview_build_stats(stats);
        if debug.preview_debug {
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
    record_ghidra_action_stage(&mut build_stats, GhidraActionConcept::FuncdataBuild);
    record_ghidra_action_stage(&mut build_stats, GhidraActionConcept::HeritageValueRecovery);
    if pcode.blocks.len() > 1 || build_stats.structuring_duration_ms > 0 {
        record_ghidra_action_stage(&mut build_stats, GhidraActionConcept::BlockGraphStructuring);
    }
    if debug.diag {
        eprintln!(
            "[DIAG] build_hir done: fn=0x{address:x} elapsed={:.3}s body_stmts={} locals={}",
            build_start.elapsed().as_secs_f64(),
            hir.body.len(),
            hir.locals.len()
        );
    }
    if debug.preview_debug {
        eprintln!("[mlil-preview] stage=normalize start fn=0x{address:x}");
    }
    debug_log("normalize_start");
    let normalize_start = Instant::now();
    let context = normalize::pipeline::GlobalSymbolContext {
        names: options.global_names.clone(),
        sizes: options.global_sizes.clone(),
    };
    normalize::pipeline::GLOBAL_SYMBOL_CONTEXT.with(|ctx| {
        *ctx.borrow_mut() = Some(context);
    });
    normalize_hir_function(&mut hir);
    // Run the explicit structuring Pass pipeline.  Today this is a thin shim
    // (PostStructuringCleanupPass) that records the stage in PassTrace and
    // provides the extension point for future per-CollapseRule Pass migration.
    structuring::passes::pipeline::run_structuring_pipeline(
        &mut hir,
        debug.diag,
        std::env::var_os("FISSION_PREVIEW_PERF").is_some(),
    );
    normalize::pipeline::GLOBAL_SYMBOL_CONTEXT.with(|ctx| {
        *ctx.borrow_mut() = None;
    });
    record_ghidra_action_stage(&mut build_stats, GhidraActionConcept::Normalize);
    record_ghidra_action_stage(&mut build_stats, GhidraActionConcept::PrototypeTypes);
    build_stats.merge_assign(&normalize::take_normalize_wave_stats());
    let normalized_discovery_stats = discover_guarded_tail_candidates_for_stats(&hir.body);
    build_stats.merge_guarded_tail_discovery_assign(&normalized_discovery_stats);
    build_stats.refresh_structuring_reason_families();
    build_stats.build_duration_ms = build_start.elapsed().as_millis() as usize;
    build_stats.normalize_duration_ms = normalize_start.elapsed().as_millis() as usize;
    if debug.diag {
        eprintln!(
            "[DIAG] normalize stage done: fn=0x{address:x} elapsed={:.3}s body_stmts={} locals={}",
            normalize_start.elapsed().as_secs_f64(),
            hir.body.len(),
            hir.locals.len()
        );
    }
    debug_log("normalize_done");
    if let Some(context) = type_context {
        if debug.preview_debug {
            eprintln!("[mlil-preview] stage=type_hints start fn=0x{address:x}");
        }
        debug_log("type_hints_start");
        let type_hints_start = Instant::now();
        let hint_stats = apply_preview_type_hints(&mut hir, context);
        telemetry::store_preview_hint_stats(hint_stats);
        if debug.diag {
            eprintln!(
                "[DIAG] type_hints done: fn=0x{address:x} elapsed={:.3}s",
                type_hints_start.elapsed().as_secs_f64()
            );
        }
        debug_log("type_hints_done");
    }
    recover_global_symbol_accesses(&mut hir, options);
    if debug.preview_debug {
        eprintln!("[mlil-preview] stage=print start fn=0x{address:x}");
    }
    debug_log("print_start");
    let print_start = Instant::now();
    // Always build dual NIR/HIR surfaces from one structured tree. Callers that
    // only need a single string use `LayeredPseudocode::primary` / legacy
    // `render_nir` which returns the NIR-faithful surface for oracle compat.
    let layered = render_layered_pseudocode(&hir, options);
    store_last_layered_pseudocode(layered.clone());
    let rendered = layered.nir;
    record_ghidra_action_stage(&mut build_stats, GhidraActionConcept::PrintC);
    record_ghidra_clean_room_pipeline_complete(&mut build_stats);
    build_stats.render_duration_ms = print_start.elapsed().as_millis() as usize;
    build_stats.rendered_code_len = rendered.len();
    telemetry::store_preview_build_stats(build_stats);
    if debug.diag {
        eprintln!(
            "[DIAG] print done: fn=0x{address:x} elapsed={:.3}s",
            print_start.elapsed().as_secs_f64()
        );
    }
    if debug.preview_debug {
        eprintln!("[mlil-preview] stage=print done fn=0x{address:x}");
    }
    debug_log("print_done");
    Ok(rendered)
}

thread_local! {
    static LAST_LAYERED_PSEUDOCODE: std::cell::RefCell<Option<LayeredPseudocode>> =
        const { std::cell::RefCell::new(None) };
}

fn store_last_layered_pseudocode(layered: LayeredPseudocode) {
    LAST_LAYERED_PSEUDOCODE.with(|slot| {
        *slot.borrow_mut() = Some(layered);
    });
}

/// Take the dual NIR/HIR strings produced by the most recent `render_nir*` call
/// on this thread (observation / CLI layer selection).
pub fn take_last_layered_pseudocode() -> Option<LayeredPseudocode> {
    LAST_LAYERED_PSEUDOCODE.with(|slot| slot.borrow_mut().take())
}

#[derive(Debug, Clone, Copy)]
struct RenderDebugFlags {
    diag: bool,
    preview_debug: bool,
}

impl RenderDebugFlags {
    fn from_env() -> Self {
        Self {
            diag: std::env::var_os("FISSION_PREVIEW_DIAG").is_some(),
            preview_debug: std::env::var_os("FISSION_PREVIEW_DEBUG").is_some(),
        }
    }
}

pub fn render_nir_with_context(
    pcode: &PcodeFunction,
    name: &str,
    address: u64,
    options: &NirRenderOptions,
    type_context: Option<&NirTypeContext>,
    decomp_facts: Option<&mut dyn DecompFacts>,
) -> Result<String, MlilPreviewError> {
    render_mlil_preview_with_binary_and_context(
        pcode,
        name,
        address,
        options,
        None,
        type_context,
        decomp_facts,
    )
}

pub fn render_nir_with_binary_and_context(
    pcode: &PcodeFunction,
    name: &str,
    address: u64,
    options: &NirRenderOptions,
    binary: Option<&LoadedBinary>,
    type_context: Option<&NirTypeContext>,
    decomp_facts: Option<&mut dyn DecompFacts>,
) -> Result<String, MlilPreviewError> {
    render_mlil_preview_with_binary_and_context(
        pcode,
        name,
        address,
        options,
        binary,
        type_context,
        decomp_facts,
    )
}
