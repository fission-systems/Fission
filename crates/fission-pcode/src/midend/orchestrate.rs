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
    NirRenderOptions, NirTypeContext, PreviewBuildStats, PreviewBuilder, PreviewHintStats,
    PreviewTypeContext, apply_preview_type_hints_with_stack_bias,
    discover_guarded_tail_candidates_for_stats, record_ghidra_action_stage,
    record_ghidra_clean_room_pipeline_complete, recover_global_symbol_accesses,
    render_layered_pseudocode, structuring,
};
use crate::pcode::PcodeFunction;
use fission_loader::loader::LoadedBinary;
use fission_midend_structuring::StructuringHost;
// Owner crate (not pcode re-export path) — keeps orchestrate boundary explicit.
use fission_midend_normalize::{
    GlobalSymbolContext, NormalizeContext, apply_callsite_type_prop_pass,
    normalize_hir_function_with_context_and_facts, take_normalize_wave_stats,
};
use std::time::Instant;

pub fn test_refine_partitions(accesses: &[(i64, u32)]) -> Vec<(i64, u32)> {
    super::builder::test_refine_partitions(accesses)
}

/// Typed result for one successful NIR/MLIL render.
///
/// The legacy render functions still return only their primary code for API
/// compatibility. New orchestration code should use this result so snapshots
/// and telemetry travel with the render that produced them instead of being
/// consumed from separate observation calls.
#[derive(Debug, Clone, PartialEq)]
pub struct NirDecompileOutput {
    pub code: String,
    pub layered: Option<LayeredPseudocode>,
    pub raw_hir: Option<super::PreHirFunction>,
    pub prehir: Option<super::PreHirFunction>,
    pub hir_function: Option<super::HirFunction>,
    pub recovered_variables: Option<Vec<crate::render::RecoveredVariable>>,
    pub build_stats: Option<PreviewBuildStats>,
    pub hint_stats: Option<PreviewHintStats>,
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
    if options.dual_layer_structuring {
        return render_mlil_preview_dual_layer(pcode, name, address, options, None, type_context);
    }
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

/// Build each layer from its own structuring, then stitch them.
///
/// The two layers want different things from a structuring and the
/// difference is not cosmetic: `SelectionAxis::NodeEstimate` keeps a jump
/// when removing it would cost more CFG nodes than it saves, and
/// `fission_midend_core::ir::SelectionAxis::Jumps` never does. Sharing one tree meant one of them was
/// always getting the other's answer.
///
/// This runs the pipeline twice, so it is behind
/// `NirRenderOptions::dual_layer_structuring` and off by default. The layer
/// DecBench scores already receives the accuracy objective through
/// `selection_axis`; what the second run adds is the *readable* variant, for
/// a person rather than for the metric.
///
/// Returns the accuracy surface, which is what every existing caller of
/// `render_mlil_preview_with_binary_and_context` expects back.
pub fn render_mlil_preview_dual_layer(
    pcode: &PcodeFunction,
    name: &str,
    address: u64,
    options: &MlilPreviewOptions,
    binary: Option<&LoadedBinary>,
    type_context: Option<&PreviewTypeContext>,
) -> Result<String, MlilPreviewError> {
    render_mlil_preview_dual_layer_output(pcode, name, address, options, binary, type_context, None)
        .map(|output| output.code)
}

fn render_mlil_preview_dual_layer_output(
    pcode: &PcodeFunction,
    name: &str,
    address: u64,
    options: &MlilPreviewOptions,
    binary: Option<&LoadedBinary>,
    type_context: Option<&PreviewTypeContext>,
    mut decomp_facts: Option<&mut dyn DecompFacts>,
) -> Result<NirDecompileOutput, MlilPreviewError> {
    let mut scored_options = options.clone();
    scored_options.dual_layer_structuring = false;
    let scored = render_mlil_preview_with_binary_and_context_output(
        pcode,
        name,
        address,
        &scored_options,
        binary,
        type_context,
        reborrow_decomp_facts(&mut decomp_facts),
    )?;
    if !options.dual_layer_structuring
        || options.selection_axis == fission_midend_core::ir::SelectionAxis::Jumps
    {
        return Ok(scored);
    }
    let Some(scored_layers) = scored.layered.clone() else {
        return Ok(scored);
    };
    let mut readable_options = options.clone();
    readable_options.dual_layer_structuring = false;
    readable_options.selection_axis = fission_midend_core::ir::SelectionAxis::Jumps;
    // A failure here is not a failure of the decompilation -- the scored
    // surface is already built. Fall back to sharing its tree, which is what
    // every caller got before this existed.
    let readable = render_mlil_preview_with_binary_and_context_output(
        pcode,
        name,
        address,
        &readable_options,
        binary,
        type_context,
        None,
    );
    let (mut output, hir) = match readable {
        Ok(output) => {
            let hir = output
                .layered
                .as_ref()
                .map(|layers| layers.hir.clone())
                .unwrap_or_else(|| scored_layers.hir.clone());
            (output, hir)
        }
        Err(_) => (scored.clone(), scored_layers.hir.clone()),
    };
    // Both surfaces are presented. The two modes differ in *structuring* --
    // which is the difference that exists -- not in whether they went
    // through the presentation pass. NIR remains the scored tree's
    // semantic-faithful print, while HIR comes from the jump-minimizing tree
    // and is allowed to optimize for readability.
    let layered = stitch_dual_layers(&scored_layers, hir);
    output.code = scored.code;
    output.layered = Some(layered.clone());
    Ok(output)
}

fn stitch_dual_layers(
    scored_layers: &LayeredPseudocode,
    readable_hir: String,
) -> LayeredPseudocode {
    // Keep the scored tree's actual NIR print here. The HIR print is a
    // presentation surface and may elide casts or other syntax; placing it
    // in `nir` silently changes the semantic oracle exposed through `code_nir`
    // even though `scored.code` still came from NIR.
    LayeredPseudocode {
        nir: scored_layers.nir.clone(),
        hir: readable_hir,
    }
}

fn reborrow_decomp_facts<'borrow, 'facts>(
    decomp_facts: &'borrow mut Option<&'facts mut (dyn DecompFacts + 'facts)>,
) -> Option<&'borrow mut (dyn DecompFacts + 'borrow)>
where
    'facts: 'borrow,
{
    match decomp_facts {
        Some(facts) => Some(&mut **facts),
        None => None,
    }
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
    render_mlil_preview_with_binary_and_context_output(
        pcode,
        name,
        address,
        options,
        binary,
        type_context,
        decomp_facts,
    )
    .map(|output| output.code)
}

fn render_mlil_preview_with_binary_and_context_output(
    pcode: &PcodeFunction,
    name: &str,
    address: u64,
    options: &MlilPreviewOptions,
    binary: Option<&LoadedBinary>,
    type_context: Option<&PreviewTypeContext>,
    decomp_facts: Option<&mut dyn DecompFacts>,
) -> Result<NirDecompileOutput, MlilPreviewError> {
    // Two output modes, two structurings. Handled here rather than in a
    // wrapper because the pipeline calls this entry point directly. Legacy
    // string-returning wrappers install their observation compatibility state
    // before entering here; typed callers receive all successful observations
    // in the returned value and do not mutate that state.
    if options.dual_layer_structuring {
        return render_mlil_preview_dual_layer_output(
            pcode,
            name,
            address,
            options,
            binary,
            type_context,
            decomp_facts,
        );
    }
    let debug = RenderDebugFlags::from_env();
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
        return Err(MlilPreviewError::UnsupportedArchitectureDetailed);
    }

    if let Err(err) = pcode.validate() {
        if debug.diag || debug.preview_debug {
            eprintln!("[mlil-preview] invalid pcode shape fn=0x{address:x} err={err}");
        }
        return Err(MlilPreviewError::UnsupportedPattern("invalid pcode shape"));
    }

    let build_start = Instant::now();
    if debug.preview_debug {
        eprintln!("[mlil-preview] stage=build_hir start fn=0x{address:x}");
    }
    debug_log("build_hir_start");
    let mut builder = PreviewBuilder::new_with_binary(pcode, options, binary, type_context);
    let mut hir = builder.build_hir(name, address).map_err(|err| {
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
    // Returned raw observation, captured for the typed output for the same
    // reason the legacy compatibility snapshot exists below
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
    let raw_hir = hir.clone();
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
    let normalize_context = NormalizeContext::new(
        GlobalSymbolContext {
            names: options.global_names.clone(),
            sizes: options.global_sizes.clone(),
        },
        builder.lsda_landing_pad_labels(),
    );
    // Stage: midend-normalize (owner crate). `hir` is a real `PreHirFunction`
    // here (builder's native output) -- kept named `hir` through this
    // function for minimal diff, but its type is PreHIR until the explicit
    // conversion below.
    let mut decomp_facts = decomp_facts;
    normalize_hir_function_with_context_and_facts(
        &mut hir,
        &normalize_context,
        reborrow_decomp_facts(&mut decomp_facts),
    );
    // Returned observation (the typed output owns this snapshot)
    // below): the real `PreHirFunction` structuring is about to consume,
    // captured before any structuring rewrite touches it. Zero effect on
    // `hir` itself -- purely a clone for whoever reads it back via
    // `NirDecompileOutput::prehir`.
    let prehir = hir.clone();
    // Stage: post-structure cleanup pass shim (host residual still in pcode).
    // Provides PassTrace extension point for future per-CollapseRule migration.
    structuring::passes::pipeline::run_structuring_pipeline_with_facts(
        &mut hir,
        debug.diag,
        std::env::var_os("FISSION_PREVIEW_PERF").is_some(),
        reborrow_decomp_facts(&mut decomp_facts),
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
    // What the binary's own no-return fixpoint proved, so the `else` unwrap
    // can recognize a `sub_XXXX` wrapper around `exit` for what it is.
    let proven_no_return = |target: &str| {
        type_context.is_some_and(|ctx| {
            ctx.call_effect_summaries
                .get(target)
                .is_some_and(|summary| summary.may_exit == Some(true))
        })
    };
    let body = fission_midend_structuring::cleanup::finalize_post_layout_body_with(
        &protected,
        body,
        &proven_no_return,
    );
    hir.body = body;
    // The real PreHirFunction -> HirFunction boundary: structuring's CFG-to-AST
    // rewrite is done, so `hir.body` (still `Vec<PreHirStmt>`) is converted to
    // the genuinely separate `HirStmt` grammar and `hir` is rebound to a
    // real `HirFunction` from here on -- not a type pun, an actual
    // structural conversion (`prehir_stmts_to_hir_stmts`).
    let hir_body = fission_midend_prehir::ir::prehir_stmts_to_hir_stmts(hir.body.clone());
    let mut hir = hir.into_hir_function(hir_body);
    // Returned structured observation, captured for the typed output for the
    // same reason the legacy compatibility snapshot exists below
    // above: the fully-finalized `HirFunction` (structured body, plus the
    // `params`/`locals` an interpreter needs) as of the point a real caller
    // would consider structuring's semantic output done -- any remaining
    // steps below this point are printer-facing, not semantic (see
    // `midend/AGENTS.md`: "Do not fix structuring bugs only in printer.rs").
    let hir_function = hir.clone();
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
    let mut hint_stats = None;
    if let Some(context) = type_context {
        if debug.preview_debug {
            eprintln!("[mlil-preview] stage=type_hints start fn=0x{address:x}");
        }
        debug_log("type_hints_start");
        let type_hints_start = Instant::now();
        let stats = apply_preview_type_hints_with_stack_bias(
            &mut hir,
            context,
            &register_origins,
            builder.debug_cfa_stack_offset_bias(),
        );
        hint_stats = Some(stats);
        if debug.diag {
            eprintln!(
                "[DIAG] type_hints done: fn=0x{address:x} elapsed={:.3}s",
                type_hints_start.elapsed().as_secs_f64()
            );
        }
        debug_log("type_hints_done");
    }
    recover_global_symbol_accesses(&mut hir, options);
    // Collect the recovered variables here rather than from the snapshot
    // above: this is after the DWARF/signature overlay has put real names and
    // real type names on the bindings, which is what the printed declarations
    // will carry and what a consumer comparing against debug info needs.
    let recovered_variables = crate::render::recovered_variables(&hir);
    if debug.preview_debug {
        eprintln!("[mlil-preview] stage=print start fn=0x{address:x}");
    }
    debug_log("print_start");
    let print_start = Instant::now();
    // Always build dual NIR/HIR surfaces from one structured tree. Callers that
    // only need a single string use `LayeredPseudocode::primary` / legacy
    // `render_nir` which returns the NIR-faithful surface for oracle compat.
    let layered = render_layered_pseudocode(&hir, options);
    let rendered = layered.nir.clone();
    record_ghidra_action_stage(&mut build_stats, GhidraActionConcept::PrintC);
    record_ghidra_clean_room_pipeline_complete(&mut build_stats);
    build_stats.render_duration_ms = print_start.elapsed().as_millis() as usize;
    build_stats.rendered_code_len = rendered.len();
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
    Ok(NirDecompileOutput {
        code: rendered,
        layered: Some(layered),
        raw_hir: Some(raw_hir),
        prehir: Some(prehir),
        hir_function: Some(hir_function),
        recovered_variables: Some(recovered_variables),
        build_stats: Some(build_stats),
        hint_stats,
    })
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

/// Render NIR and return the primary code together with all observations from
/// that same render. This is the typed successor for orchestration callers;
/// the legacy `render_nir_*` functions remain string-returning wrappers.
pub fn render_nir_with_context_output(
    pcode: &PcodeFunction,
    name: &str,
    address: u64,
    options: &NirRenderOptions,
    type_context: Option<&NirTypeContext>,
    decomp_facts: Option<&mut dyn DecompFacts>,
) -> Result<NirDecompileOutput, MlilPreviewError> {
    render_nir_with_binary_and_context_output(
        pcode,
        name,
        address,
        options,
        None,
        type_context,
        decomp_facts,
    )
}

/// Binary-aware typed NIR render result. This calls the canonical pipeline
/// directly and returns every observation owned by that render; legacy string
/// wrappers only project the primary code from this value.
pub fn render_nir_with_binary_and_context_output(
    pcode: &PcodeFunction,
    name: &str,
    address: u64,
    options: &NirRenderOptions,
    binary: Option<&LoadedBinary>,
    type_context: Option<&NirTypeContext>,
    decomp_facts: Option<&mut dyn DecompFacts>,
) -> Result<NirDecompileOutput, MlilPreviewError> {
    render_mlil_preview_with_binary_and_context_output(
        pcode,
        name,
        address,
        options,
        binary,
        type_context,
        decomp_facts,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dual_layer_stitch_preserves_scored_nir_surface() {
        let scored = LayeredPseudocode {
            nir: "(unsigned long long)(uint)lane".into(),
            hir: "lane".into(),
        };

        let layered = stitch_dual_layers(&scored, "readable lane".into());

        assert_eq!(layered.nir, scored.nir);
        assert_eq!(layered.hir, "readable lane");
    }
}
