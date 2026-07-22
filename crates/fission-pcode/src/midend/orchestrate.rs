//! Preview/NIR orchestration: builder → normalize → structuring → render.
//!
//! Owns the top-level `render_mlil_preview*` / `render_nir*` entrypoints that
//! wire owner layers together. **Semantic ownership (ADR 0012):**
//! - builder / PreviewBuilder: `fission-pcode` (p-code → HIR materialize)
//! - normalize: `fission-midend-normalize` (called directly below)
//! - structuring: `fission-midend-structuring` free-fns + PreviewBuilder host
//! - print: [`crate::render`] (NIR/HIR dual layer)
//!
//! This module must not re-implement owner logic; it only sequences stages.

use super::{
    DecompFacts, GhidraActionConcept, LayeredPseudocode, MlilPreviewError, MlilPreviewOptions,
    NirRenderOptions, NirTypeContext, PreviewBuildStats, PreviewBuilder, PreviewTypeContext,
    apply_preview_type_hints, discover_guarded_tail_candidates_for_stats,
    record_ghidra_action_stage, record_ghidra_clean_room_pipeline_complete,
    recover_global_symbol_accesses, render_layered_pseudocode, structuring, telemetry,
};
use crate::pcode::PcodeFunction;
use fission_loader::loader::LoadedBinary;
use fission_midend_structuring::StructuringHost;
// Owner crate (not pcode re-export path) — keeps orchestrate boundary explicit.
use fission_midend_normalize::{
    normalize_hir_function, pipeline as normalize_pipeline, take_normalize_wave_stats,
};
use std::time::Instant;

pub fn test_refine_partitions(accesses: &[(i64, u32)]) -> Vec<(i64, u32)> {
    super::builder::test_refine_partitions(accesses)
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
    let _ = decomp_facts;
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
    let context = normalize_pipeline::GlobalSymbolContext {
        names: options.global_names.clone(),
        sizes: options.global_sizes.clone(),
    };
    normalize_pipeline::GLOBAL_SYMBOL_CONTEXT.with(|ctx| {
        *ctx.borrow_mut() = Some(context);
    });
    normalize_pipeline::PROTECTED_LSDA_LABELS.with(|protected| {
        *protected.borrow_mut() = builder.lsda_landing_pad_labels().into_iter().collect();
    });
    // Stage: midend-normalize (owner crate). `hir` is a real `DirFunction`
    // here (builder's native output) -- kept named `hir` through this
    // function for minimal diff, but its type is DIR until the explicit
    // conversion below.
    normalize_hir_function(&mut hir);
    // Observation side channel (mirrors `take_last_layered_pseudocode`
    // below): the real `DirFunction` structuring is about to consume,
    // captured before any structuring rewrite touches it. Zero effect on
    // `hir` itself -- purely a clone for whoever reads it back via
    // `take_last_dir_snapshot`.
    store_last_dir_snapshot(hir.clone());
    // Stage: post-structure cleanup pass shim (host residual still in pcode).
    // Provides PassTrace extension point for future per-CollapseRule migration.
    structuring::passes::pipeline::run_structuring_pipeline(
        &mut hir,
        debug.diag,
        std::env::var_os("FISSION_PREVIEW_PERF").is_some(),
    );
    // Structuring may wrap/rearrange after normalize; drop pure identity
    // assigns that only become adjacent post-layout.
    let _ = fission_midend_normalize::eliminate_redundant_var_assigns(&mut hir.body);
    // The real DirFunction -> HirFunction boundary: structuring's CFG-to-AST
    // rewrite is done, so `hir.body` (still `Vec<DirStmt>`) is converted to
    // the genuinely separate `HirStmt` grammar and `hir` is rebound to a
    // real `HirFunction` from here on -- not a type pun, an actual
    // structural conversion (`dir_stmts_to_hir_stmts`).
    let hir_body = fission_midend_core::ir::dir_stmts_to_hir_stmts(hir.body.clone());
    let mut hir = hir.into_hir_function(hir_body);
    // Observation side channel, same rationale as `store_last_dir_snapshot`
    // above: the fully-finalized `HirFunction` (structured body, plus the
    // `params`/`locals` an interpreter needs) as of the point a real caller
    // would consider structuring's semantic output done -- any remaining
    // steps below this point are printer-facing, not semantic (see
    // `midend/AGENTS.md`: "Do not fix structuring bugs only in printer.rs").
    store_last_hir_function_snapshot(hir.clone());
    normalize_pipeline::GLOBAL_SYMBOL_CONTEXT.with(|ctx| {
        *ctx.borrow_mut() = None;
    });
    normalize_pipeline::PROTECTED_LSDA_LABELS.with(|protected| {
        protected.borrow_mut().clear();
    });
    record_ghidra_action_stage(&mut build_stats, GhidraActionConcept::Normalize);
    record_ghidra_action_stage(&mut build_stats, GhidraActionConcept::PrototypeTypes);
    build_stats.merge_assign(&take_normalize_wave_stats());
    // `discover_guarded_tail_candidates_for_stats` is a structuring-side stats
    // pass (re-runs guarded-tail promotion discovery for telemetry, doesn't
    // mutate `hir`) defined for `DirStmt` input -- convert back via
    // `hir_stmts_to_dir_stmts` rather than duplicating the pass for `HirStmt`.
    let normalized_discovery_stats = discover_guarded_tail_candidates_for_stats(
        &fission_midend_core::ir::hir_stmts_to_dir_stmts(hir.body.clone()),
    );
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
    // Always drain the register-origin side channel, even when `type_context`
    // is `None` below -- otherwise a leftover entry from this function could
    // wrongly satisfy a name lookup for the next function built on this
    // thread (register-derived binding names like a generic `uVar0` are
    // reused across unrelated functions' compilations).
    let register_origins = super::builder::take_register_origins();
    if let Some(context) = type_context {
        if debug.preview_debug {
            eprintln!("[mlil-preview] stage=type_hints start fn=0x{address:x}");
        }
        debug_log("type_hints_start");
        let type_hints_start = Instant::now();
        let hint_stats = apply_preview_type_hints(&mut hir, context, &register_origins);
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

thread_local! {
    static LAST_DIR_SNAPSHOT: std::cell::RefCell<Option<super::DirFunction>> =
        const { std::cell::RefCell::new(None) };
}

fn store_last_dir_snapshot(func: super::DirFunction) {
    LAST_DIR_SNAPSHOT.with(|slot| {
        *slot.borrow_mut() = Some(func);
    });
}

/// Take the real [`super::DirFunction`] (builder's native output, the same
/// one normalize/structuring's own internal passes read and rewrite) that
/// structuring consumed as input on the most recent
/// `render_mlil_preview*`/`render_nir*` call on this thread -- captured
/// immediately before structuring's CFG-to-AST rewrite runs. `DirFunction`
/// is a genuinely independent type from [`super::HirFunction`] (see
/// `fission_midend_core::ir::hir`'s module doc), not the same type under a
/// different name, so callers can't accidentally swap this with the
/// structured HIR `take_last_hir_function_snapshot` returns. Pairing the
/// two lets an external verifier (e.g. `fission-dir`) interpret both and
/// diff results for the same concrete inputs, without any change to what
/// structuring itself computes -- purely observational, same pattern as
/// `take_last_layered_pseudocode` above.
pub fn take_last_dir_snapshot() -> Option<super::DirFunction> {
    LAST_DIR_SNAPSHOT.with(|slot| slot.borrow_mut().take())
}

thread_local! {
    static LAST_HIR_FUNCTION_SNAPSHOT: std::cell::RefCell<Option<super::HirFunction>> =
        const { std::cell::RefCell::new(None) };
}

fn store_last_hir_function_snapshot(func: super::HirFunction) {
    LAST_HIR_FUNCTION_SNAPSHOT.with(|slot| {
        *slot.borrow_mut() = Some(func);
    });
}

/// Take the fully-finalized `HirFunction` (structured body, `params`,
/// `locals`) from the most recent `render_mlil_preview*`/`render_nir*` call
/// on this thread -- the counterpart to [`take_last_dir_snapshot`]: a
/// caller that wants to differentially verify structuring calls both after
/// one decompile call, wraps the returned `HirFunction::body` in
/// [`super::Hir`], and diffs it against the `Dir` using the same
/// `params`/`locals`. Same observational side-channel pattern as
/// `take_last_layered_pseudocode`/`take_last_dir_snapshot` above.
pub fn take_last_hir_function_snapshot() -> Option<super::HirFunction> {
    LAST_HIR_FUNCTION_SNAPSHOT.with(|slot| slot.borrow_mut().take())
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
