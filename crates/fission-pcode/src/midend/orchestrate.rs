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
    apply_callsite_type_prop_pass, normalize_hir_function, pipeline as normalize_pipeline,
    take_normalize_wave_stats,
};
use std::time::Instant;

pub fn test_refine_partitions(accesses: &[(i64, u32)]) -> Vec<(i64, u32)> {
    super::builder::test_refine_partitions(accesses)
}

/// Decode `pcode` into a raw `PreHirFunction` only -- no normalize,
/// structuring, or render. A deliberately independent, lighter-weight path
/// from `render_mlil_preview_with_binary_and_context`'s own inline
/// `build_hir` call (not extracted from it): that call site's error path
/// also does telemetry/stats attribution and an unsupported-opcode
/// inventory event that a whole-program pre-pass calling this for every
/// function in a binary has no use for and shouldn't pay for, and keeping
/// the two independent means this can't change that path's own tested
/// behavior at all.
///
/// Used by `fission-decompiler`'s whole-program call-arity pre-pass: real
/// argument recovery (`call_recovery.rs`) with no normalize-stage pruning,
/// for every function in a binary, without the cost of a full render.
pub fn build_raw_hir(
    pcode: &PcodeFunction,
    name: &str,
    address: u64,
    options: &MlilPreviewOptions,
    binary: Option<&LoadedBinary>,
    type_context: Option<&PreviewTypeContext>,
) -> Result<super::PreHirFunction, MlilPreviewError> {
    super::builder::with_discarded_register_origins(|| {
        let mut builder = PreviewBuilder::new_with_binary(pcode, options, binary, type_context);
        builder.build_hir(name, address)
    })
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
    // Observation side channel, same rationale as `store_last_prehir_snapshot`
    // below, but captured *before* `normalize_hir_function` runs rather than
    // after: `apply_callsite_type_prop_pass` (an early pass inside
    // `normalize_hir_function`'s type-signature fixed point) truncates each
    // call's `args` down to the callee's own body-inferred arity via
    // `prune_known_api_call_args_stmts`, and only afterwards does
    // `apply_interproc_callsite_arity_pass` (a later cleanup-stage pass)
    // observe `args.len()` per call site into
    // `PreHirFunction::callee_observed_max_arity` -- so by construction that
    // field can never see more args than the callee's own preview-inferred
    // arity already implied, making it useless for the one thing it was
    // meant for (recovering a caller-observed arity *wider* than what the
    // callee's own body reveals). This snapshot captures `hir` right after
    // the builder's raw argument recovery (`call_recovery.rs`, which reads
    // real register writes at each call site with no arity cap at all) and
    // before any pruning touches it.
    store_last_raw_hir_snapshot(hir.clone());
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
    // Stage: midend-normalize (owner crate). `hir` is a real `PreHirFunction`
    // here (builder's native output) -- kept named `hir` through this
    // function for minimal diff, but its type is PreHIR until the explicit
    // conversion below.
    normalize_hir_function(&mut hir);
    // Observation side channel (mirrors `take_last_layered_pseudocode`
    // below): the real `PreHirFunction` structuring is about to consume,
    // captured before any structuring rewrite touches it. Zero effect on
    // `hir` itself -- purely a clone for whoever reads it back via
    // `take_last_prehir_snapshot`.
    store_last_prehir_snapshot(hir.clone());
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
    // Scratch values that cannot reach anything the function observes. This
    // runs here rather than inside normalize because a whole-function
    // definition count -- all normalize has before structuring -- cannot
    // retire a closed dataflow cycle such as `a = b; ... b = a + 1`, where
    // every name has a reader but the graph only feeds itself. Reachability
    // backwards from conditions, stores, calls, returns, and writes to named
    // bindings can. See docs/proposals/2026-08-17-ast-stage-copy-propagation.md.
    let _ = fission_midend_normalize::prune_unobservable_scratch(&mut hir);
    // Fold the copies that survive. A structured statement list is a
    // straight-line run, so a copy carried only inside one list -- dropped at
    // every nested construct, label, goto, and call -- needs no dataflow proof.
    // That is the argument the pre-structuring pass cannot make, which is why
    // it needs a whole-function definition count and a TempPreserved veto.
    // Fold, propagate, fold again. Constant folding turns `x = 200 + 100` into
    // `x = 300`, which only then is a pure copyable the run-scoped pass can
    // carry; and moving an expression to its consumer can in turn put two
    // constants next to each other that were separated when folding last ran
    // before structuring. Two folds bracket the propagation for that reason.
    let _ = fission_midend_normalize::constant_folding_pass(&mut hir.body);
    let mut structured_copies_changed =
        fission_midend_normalize::propagate_copies_in_runs(&mut hir);
    let _ = fission_midend_normalize::constant_folding_pass(&mut hir.body);
    // Folding can turn a computed definition into a constant one, which is only
    // then a pure copyable the run-scoped pass can carry. Propagate once more so
    // the group reaches a fixpoint instead of stopping one step short.
    structured_copies_changed |= fission_midend_normalize::propagate_copies_in_runs(&mut hir);
    let _ = fission_midend_normalize::prune_unobservable_scratch(&mut hir);
    // Run-scoped propagation can expose the stable source binding at an API
    // call only after CFG structuring, e.g. `stream_alias = param_1;
    // fputs(text, stream_alias)` becoming `fputs(text, param_1)`.  Re-run the
    // existing call-site contract on that simpler, equivalent body so the
    // exact API parameter type reaches the source binding. Deliberately do not
    // re-run the whole type fixed point here: doing so would reconsider
    // unrelated return, signedness, aggregate, and pointer-depth facts after
    // layout. This ordering avoids unsound backward typing through a reused
    // PreHIR name with multiple definitions while keeping the late scope local
    // to bindings newly exposed at calls.
    if structured_copies_changed {
        let _ = apply_callsite_type_prop_pass(&mut hir);
    }
    // The dead/identity cleanup above can expose an alias-only block that did
    // not exist at structuring time. Retarget its function-scoped predecessors
    // before crossing the canonical PreHIR -> HIR boundary.
    let protected = builder.lsda_landing_pad_labels();
    let body = std::mem::take(&mut hir.body);
    // Shared terminal tails (bare epilogues, abort handlers, cleanup-and-return
    // blocks) are emitted once with every other predecessor reaching them by
    // `goto`. Copying a *terminal* tail into its jump sites is behaviour-
    // preserving and removes the jump -- the AST-level analog of Ghidra's
    // `ActionReturnSplit` / angr SAILR's `ReturnDuplicatorHigh`. Runs after the
    // alias pass so aliased labels are already canonicalized to one target.
    // A forward `if (cond) { goto L; } SPAN; L:` says "run SPAN when cond is
    // false". Inverting the guard states that directly and drops the jump.
    // A join block can be lexically adjacent to only one predecessor, so
    // `sum(P - 1)` jumps are structural -- but a block every predecessor
    // *jumps* to is paying one more than that. Relocating it after one of them
    // claims that free adjacency, the same ordering-before-goto-marking
    // principle as Ghidra's `orderBlocks` in `ActionFinalStructure`.
    // The layout rewrites above run after normalize and can create new
    // structured fallthroughs. In particular, guard inversion may wrap a span
    // ending in `goto L` inside an `if` immediately followed by `L`. Re-run the
    // idempotent structuring finalizer so its parent-successor-aware goto rule
    // sees the final layout, then prune labels made unreferenced by any of the
    // post-layout rewrites.
    // Finalization can expose another bounded terminal tail: for example, the
    // first duplication may retire an inner shared return label, making its
    // predecessor a complete return tail only after the surrounding residual
    // labels and fallthroughs are finalized. Give the existing proof one more
    // chance, then remove labels made dead by that second rewrite. This stays
    // builder-free and uses the same terminality, loop-control, label, and
    // growth admission as the first invocation.
    let body = fission_midend_structuring::cleanup::finalize_post_layout_body(&protected, body);
    hir.body = body;
    // The real PreHirFunction -> HirFunction boundary: structuring's CFG-to-AST
    // rewrite is done, so `hir.body` (still `Vec<PreHirStmt>`) is converted to
    // the genuinely separate `HirStmt` grammar and `hir` is rebound to a
    // real `HirFunction` from here on -- not a type pun, an actual
    // structural conversion (`prehir_stmts_to_hir_stmts`).
    let hir_body = fission_midend_prehir::ir::prehir_stmts_to_hir_stmts(hir.body.clone());
    let mut hir = hir.into_hir_function(hir_body);
    // Observation side channel, same rationale as `store_last_prehir_snapshot`
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
    // mutate `hir`) defined for `PreHirStmt` input -- convert back via
    // `hir_stmts_to_prehir_stmts` rather than duplicating the pass for `HirStmt`.
    let normalized_discovery_stats = discover_guarded_tail_candidates_for_stats(
        &fission_midend_prehir::ir::hir_stmts_to_prehir_stmts(hir.body.clone()),
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
    static LAST_RAW_HIR_SNAPSHOT: std::cell::RefCell<Option<super::PreHirFunction>> =
        const { std::cell::RefCell::new(None) };
}

fn store_last_raw_hir_snapshot(func: super::PreHirFunction) {
    LAST_RAW_HIR_SNAPSHOT.with(|slot| {
        *slot.borrow_mut() = Some(func);
    });
}

/// Take the builder's raw [`super::PreHirFunction`] output from the most
/// recent `render_mlil_preview*`/`render_nir*` call on this thread, captured
/// before `normalize_hir_function` runs any pass on it -- see the comment at
/// this snapshot's capture site for why call-site argument counts here can
/// be wider (more accurate) than what [`take_last_prehir_snapshot`]'s
/// post-normalize `callee_observed_max_arity` field ever sees.
pub fn take_last_raw_hir_snapshot() -> Option<super::PreHirFunction> {
    LAST_RAW_HIR_SNAPSHOT.with(|slot| slot.borrow_mut().take())
}

thread_local! {
    static LAST_PREHIR_SNAPSHOT: std::cell::RefCell<Option<super::PreHirFunction>> =
        const { std::cell::RefCell::new(None) };
}

fn store_last_prehir_snapshot(func: super::PreHirFunction) {
    LAST_PREHIR_SNAPSHOT.with(|slot| {
        *slot.borrow_mut() = Some(func);
    });
}

/// Take the real [`super::PreHirFunction`] (builder's native output, the same
/// one normalize/structuring's own internal passes read and rewrite) that
/// structuring consumed as input on the most recent
/// `render_mlil_preview*`/`render_nir*` call on this thread -- captured
/// immediately before structuring's CFG-to-AST rewrite runs. `PreHirFunction`
/// is a genuinely independent type from [`super::HirFunction`] (see
/// `fission_midend_core::ir::hir`'s module doc), not the same type under a
/// different name, so callers can't accidentally swap this with the
/// structured HIR `take_last_hir_function_snapshot` returns. Pairing the
/// two lets an external verifier interpret both and diff results for the
/// same concrete inputs, without any change to what structuring itself
/// computes -- purely observational, same pattern as
/// `take_last_layered_pseudocode` above.
pub fn take_last_prehir_snapshot() -> Option<super::PreHirFunction> {
    LAST_PREHIR_SNAPSHOT.with(|slot| slot.borrow_mut().take())
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
/// on this thread -- the counterpart to [`take_last_prehir_snapshot`]: a
/// caller that wants to differentially verify structuring calls both after
/// one decompile call, wraps the returned `HirFunction::body` in
/// [`super::Hir`], and diffs it against the PreHIR snapshot using the same
/// `params`/`locals`. Same observational side-channel pattern as
/// `take_last_layered_pseudocode`/`take_last_prehir_snapshot` above.
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
