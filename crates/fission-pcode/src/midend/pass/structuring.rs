use crate::midend::ir::MlilPreviewError;
use crate::midend::pass::{AnalysisStore, InvariantBasis, NirFunc, NirPass, PassResult};
use crate::midend::structuring::irreducible::{compute_fas_virtual_gotos, compute_node_splits};
use fission_midend_prehir::PreHirStmt;
// ADR 0012: admission / SESE / collapse free-fns owned by midend-structuring.
use fission_midend_structuring::{
    StructuringAdmissionInput, StructuringAdmissionReason, StructuringHost,
    apply_blockgraph_collapse_admission_gate, blockgraph_collapse_admission_enabled,
    build_linear_multiblock_body, build_sese_region_body, collapse_loop_admission_enabled,
    decide_structuring_admission, structure_cfg_via_sese, structuring_diag_enabled,
    try_repair_orphan_gotos,
};

pub(crate) struct EarlyReturnPass;

impl NirPass for EarlyReturnPass {
    fn name(&self) -> &str {
        "EarlyReturnPass"
    }

    /// Basis: [`InvariantBasis::EdgeClassification`]
    ///
    /// An intra-instruction conditional return exists when the CFG contains a
    /// single-block function whose only exit is a conditional fall-through to
    /// the return instruction. The structural criterion is purely edge-based:
    /// one conditional branch edge + one fall-through edge within a single block.
    fn invariant_basis(&self) -> InvariantBasis {
        InvariantBasis::EdgeClassification
    }

    fn run(
        &mut self,
        ir: &mut NirFunc<'_, '_>,
        _store: &mut AnalysisStore,
    ) -> Result<PassResult, String> {
        if ir.structured_body().is_some() {
            return Ok(PassResult::NoChange);
        }

        let body = ir
            .builder
            .try_lower_intra_instruction_conditional_return()
            .map_err(|e| e.to_string())?;
        if let Some(body) = body {
            ir.set_structured_body(body);
            return Ok(PassResult::Changed);
        }

        let body = ir
            .builder
            .try_lower_conditional_tailcall_after_return()
            .map_err(|e| e.to_string())?;
        if let Some(body) = body {
            ir.set_structured_body(body);
            return Ok(PassResult::Changed);
        }

        Ok(PassResult::NoChange)
    }
}

pub(crate) struct IrreducibleReductionPass;

impl NirPass for IrreducibleReductionPass {
    fn name(&self) -> &str {
        "IrreducibleReductionPass"
    }

    /// Basis: [`InvariantBasis::StronglyConnectedComponents`]
    ///
    /// A CFG is irreducible iff it contains an SCC with two or more distinct
    /// loop headers (no single dom-tree node dominates all back-edges in the
    /// SCC). This pass applies Tarjan SCC analysis, then eliminates
    /// irreducibility via node-splitting or FAS edge virtualization — both
    /// invariant-based CFG transforms that do not depend on binary content.
    fn invariant_basis(&self) -> InvariantBasis {
        InvariantBasis::StronglyConnectedComponents
    }

    fn run(
        &mut self,
        ir: &mut NirFunc<'_, '_>,
        store: &mut AnalysisStore,
    ) -> Result<PassResult, String> {
        if ir.structured_body().is_some() {
            return Ok(PassResult::NoChange);
        }

        let diag = structuring_diag_enabled();
        let (
            scc_component_count,
            scc_irreducible_count,
            scc_irreducible_header_count,
            max_scc_component_size,
        ) = {
            let scc = store.cfg_facts(ir).scc();
            (
                scc.component_count(),
                scc.irreducible_count(),
                scc.irreducible_header_total_count(),
                scc.max_component_size(),
            )
        };

        ir.builder
            .telemetry
            .structuring
            .structuring_scc_component_count += scc_component_count;
        ir.builder.telemetry.core.max_structuring_scc_component_size = ir
            .builder
            .telemetry
            .core
            .max_structuring_scc_component_size
            .max(max_scc_component_size);
        ir.builder
            .telemetry
            .structuring
            .structuring_irreducible_scc_count += scc_irreducible_count;
        ir.builder
            .telemetry
            .structuring
            .structuring_irreducible_header_count += scc_irreducible_header_count;

        let original_admission = ir
            .builder
            .structuring_admission_reason(scc_irreducible_count, max_scc_component_size);
        let blockgraph_collapse_enabled = blockgraph_collapse_admission_enabled();
        if blockgraph_collapse_enabled {
            ir.builder
                .telemetry
                .structuring
                .blockgraph_collapse_admission_enabled_count += 1;
            match original_admission {
                StructuringAdmissionReason::IrreducibleBudget => {
                    ir.builder
                        .telemetry
                        .structuring
                        .blockgraph_collapse_irreducible_budget_bypass_count += 1;
                }
                StructuringAdmissionReason::ExtremeBudget => {
                    ir.builder
                        .telemetry
                        .structuring
                        .blockgraph_collapse_extreme_budget_blocked_count += 1;
                }
                _ => {}
            }
        }
        let admission = apply_blockgraph_collapse_admission_gate(
            original_admission,
            blockgraph_collapse_enabled,
        );
        let force_linear = !matches!(admission, StructuringAdmissionReason::GraphCollapse);

        let mut changed = false;
        if scc_irreducible_count > 0 && !force_linear {
            let block_stmt_counts: Vec<usize> = ir
                .builder
                .pcode
                .blocks
                .iter()
                .map(|b| b.ops.len())
                .collect();
            if let Some(split) =
                compute_node_splits(ir.successors(), ir.predecessors(), &block_stmt_counts)
            {
                if diag {
                    eprintln!(
                        "[DIAG] node-splitting: applied {} splits, virtual_blocks={}",
                        split.splits_applied,
                        split.virtual_to_original.len()
                    );
                }
                ir.apply_node_splits(split);
                changed = true;
            } else {
                let fas_edges = compute_fas_virtual_gotos(ir.successors(), ir.predecessors());
                if !fas_edges.is_empty() {
                    if diag {
                        eprintln!(
                            "[DIAG] FAS edge virtualization: {} edges virtualized as gotos: {:?}",
                            fas_edges.len(),
                            fas_edges
                        );
                    }
                    for (from, to) in fas_edges {
                        if ir.apply_virtual_goto_edge(from, to) {
                            changed = true;
                        }
                    }
                }
            }
        }

        if changed {
            store.invalidate();
            Ok(PassResult::Changed)
        } else {
            Ok(PassResult::NoChange)
        }
    }
}

/// Offer each opt-in driver a chance to beat `baseline`, keeping it unless one
/// strictly wins.
///
/// Drivers first run through [`StructuringHost::lower_observed`], so even a
/// successful candidate cannot touch the host before the comparison. Only the
/// stable winner is rerun through [`StructuringHost::lower_isolated`] and the
/// body from that committed rerun may be emitted.
#[derive(Clone, Copy)]
enum AlternativeAdmission {
    Established,
    LinearFallback,
}

fn try_alternative_structurings(
    ir: &mut NirFunc<'_, '_>,
    baseline: Vec<PreHirStmt>,
    diag: bool,
    admission: AlternativeAdmission,
) -> Vec<PreHirStmt> {
    use fission_midend_structuring::structuring_quality::measure;

    // Measure through the cleanup that runs after structuring, not the body as
    // it stands here. Both `finalize_structured_body` and normalize's own
    // statement cleanup remove jumps, and not the same number from every
    // structuring -- so a candidate can win on this bench and lose in the
    // shipped output, which is where the regressions come from.
    let protected = ir.builder.lsda_landing_pad_labels();
    let shipped = |body: &Vec<PreHirStmt>| {
        let mut cleaned =
            crate::midend::structuring::finalize_structured_body(&protected, body.clone());
        fission_midend_normalize::normalize_function_body(&mut cleaned);
        measure(&cleaned)
    };

    let baseline_quality = measure(&baseline);
    let baseline_shipped = shipped(&baseline);
    // Nothing to win: the existing path already left no jumps.
    if baseline_quality.gotos == 0 {
        if diag {
            eprintln!("[DIAG] alternatives not asked: baseline already jump-free");
        }
        return baseline;
    }

    #[derive(Debug, Clone, Copy)]
    enum AlternativeDriver {
        MatchFold,
        Dream,
        DreamVirtualGotos,
    }

    let mut best_quality = baseline_quality;
    let mut best_shipped = baseline_shipped;
    let mut winner = None;

    type Q = fission_midend_structuring::structuring_quality::StructuringQuality;
    let consider = |driver: AlternativeDriver,
                    name: &str,
                    candidate: Result<Option<Vec<PreHirStmt>>, MlilPreviewError>,
                    best_quality: &mut Q,
                    best_shipped: &mut Q,
                    winner: &mut Option<AlternativeDriver>| {
        let candidate = match candidate {
            Ok(Some(body)) => body,
            Ok(None) => return,
            Err(err) => {
                if diag {
                    eprintln!("[DIAG] {name} failed ({err:?}), keeping the existing structuring");
                }
                return;
            }
        };
        // Two measures, because neither is enough on its own. The raw bodies
        // are what this stage can compare exactly, and the cleaned ones are a
        // closer guess at what ships -- closer, not right, since normalize does
        // more afterwards. Requiring a win on the first and no loss on the
        // second keeps the candidates that pay off and drops the ones that
        // reverse downstream: measured, using the cleaned figure alone removed
        // every regression and 88 of the gains with it.
        let quality = measure(&candidate);
        let candidate_shipped = shipped(&candidate);
        let max_guard_admitted = matches!(admission, AlternativeAdmission::Established)
            || quality.has_proportional_max_guard_growth(best_quality);
        if quality.improves_on(best_quality)
            && candidate_shipped.gotos <= best_shipped.gotos
            && max_guard_admitted
        {
            if diag {
                eprintln!(
                    "[DIAG] {name} admitted: gotos {} -> {}, guards {} -> {} max {} -> {} (cleaned {} -> {}, max {} -> {})",
                    best_quality.gotos,
                    quality.gotos,
                    best_quality.guard_formula_size,
                    quality.guard_formula_size,
                    best_quality.max_guard_formula_size,
                    quality.max_guard_formula_size,
                    best_shipped.guard_formula_size,
                    candidate_shipped.guard_formula_size,
                    best_shipped.max_guard_formula_size,
                    candidate_shipped.max_guard_formula_size,
                );
            }
            *best_quality = quality;
            *best_shipped = candidate_shipped;
            *winner = Some(driver);
        } else if diag {
            eprintln!(
                "[DIAG] {name} rejected: gotos {} -> {}, guards {} -> {} max {} -> {}, worse on {:?}",
                best_quality.gotos,
                quality.gotos,
                best_quality.guard_formula_size,
                quality.guard_formula_size,
                best_quality.max_guard_formula_size,
                quality.max_guard_formula_size,
                {
                    let mut regressions = quality.regressions_against(best_quality);
                    if !max_guard_admitted {
                        regressions.push("max_guard_formula_size");
                    }
                    regressions
                }
            );
        }
    };

    if fission_midend_structuring::collapse_driver::match_fold_driver_enabled() {
        let c = fission_midend_structuring::collapse_driver::preview_match_fold(ir.builder);
        consider(
            AlternativeDriver::MatchFold,
            "match-fold",
            c,
            &mut best_quality,
            &mut best_shipped,
            &mut winner,
        );
    }
    if fission_midend_structuring::reaching_driver::dream_driver_enabled() {
        let c =
            fission_midend_structuring::reaching_driver::preview_reaching_conditions(ir.builder);
        consider(
            AlternativeDriver::Dream,
            "DREAM",
            c,
            &mut best_quality,
            &mut best_shipped,
            &mut winner,
        );
        let c = fission_midend_structuring::reaching_driver::preview_reaching_conditions_with_virtual_gotos(
            ir.builder,
        );
        consider(
            AlternativeDriver::DreamVirtualGotos,
            "DREAM+virtual-gotos",
            c,
            &mut best_quality,
            &mut best_shipped,
            &mut winner,
        );
    }

    let (name, committed) = match winner {
        Some(AlternativeDriver::MatchFold) => (
            "match-fold",
            fission_midend_structuring::collapse_driver::structure_by_match_fold(ir.builder),
        ),
        Some(AlternativeDriver::Dream) => (
            "DREAM",
            fission_midend_structuring::reaching_driver::structure_by_reaching_conditions(
                ir.builder,
            ),
        ),
        Some(AlternativeDriver::DreamVirtualGotos) => (
            "DREAM+virtual-gotos",
            fission_midend_structuring::reaching_driver::structure_by_reaching_conditions_with_virtual_gotos(
                ir.builder,
            ),
        ),
        None => return baseline,
    };
    match committed {
        Ok(Some(body)) => {
            if diag {
                eprintln!("[DIAG] {name} committed after admission");
            }
            body
        }
        Ok(None) => {
            if diag {
                eprintln!("[DIAG] {name} commit rerun declined, keeping the existing structuring");
            }
            baseline
        }
        Err(err) => {
            if diag {
                eprintln!(
                    "[DIAG] {name} commit rerun failed ({err:?}), keeping the existing structuring"
                );
            }
            baseline
        }
    }
}

pub(crate) struct SeseStructuringPass;

impl NirPass for SeseStructuringPass {
    fn name(&self) -> &str {
        "SeseStructuringPass"
    }

    /// Basis: [`InvariantBasis::DominatorTree`]
    ///
    /// SESE (Single-Entry Single-Exit) region structuring decomposes the CFG
    /// into dom-tree intervals. A region is valid iff its entry node dominates
    /// all interior nodes and its exit node post-dominates all interior nodes.
    /// The collapse loop (Tier 1 + Tier 2) operates solely on these
    /// dominator/post-dominator invariants — no binary-specific knowledge.
    fn invariant_basis(&self) -> InvariantBasis {
        InvariantBasis::DominatorTree
    }

    fn run(
        &mut self,
        ir: &mut NirFunc<'_, '_>,
        store: &mut AnalysisStore,
    ) -> Result<PassResult, String> {
        if ir.structured_body().is_some() {
            return Ok(PassResult::NoChange);
        }

        let diag = structuring_diag_enabled();
        let scc = store.cfg_facts(ir).scc();
        let scc_irreducible_count = scc.irreducible_count();
        let max_scc_component_size = scc.max_component_size();

        let original_admission = ir
            .builder
            .structuring_admission_reason(scc_irreducible_count, max_scc_component_size);
        let blockgraph_collapse_enabled = blockgraph_collapse_admission_enabled();
        let admission = apply_blockgraph_collapse_admission_gate(
            original_admission,
            blockgraph_collapse_enabled,
        );
        let force_linear = !matches!(admission, StructuringAdmissionReason::GraphCollapse);

        if diag {
            eprintln!(
                "[DIAG] structuring start: blocks={} edges={} force_linear={}",
                ir.builder.pcode.blocks.len(),
                ir.successors().iter().map(Vec::len).sum::<usize>(),
                force_linear
            );
        }

        if force_linear {
            return Ok(PassResult::NoChange);
        }

        let total_blocks = ir.block_count();

        // Collapse-loop admission: whole-function SESE body at (0, N). SESE tree
        // path is the free-fn structure_cfg_via_sese (no pcode thin wrap).
        let sese_result = if collapse_loop_admission_enabled() {
            match build_sese_region_body(ir.builder, 0, total_blocks, Default::default()) {
                Ok((body, _achieved_exit, _extra_members)) => Ok(body),
                Err(err) => {
                    if diag {
                        eprintln!(
                            "[DIAG] collapse loop failed ({err:?}), falling back to SESE tree"
                        );
                    }
                    structure_cfg_via_sese(ir.builder, total_blocks)
                }
            }
        } else {
            structure_cfg_via_sese(ir.builder, total_blocks)
        };

        match sese_result {
            Ok(body) => {
                // Drivers run *after* the existing path, not instead of it,
                // and only displace it when they are strictly better on jumps
                // while giving up nothing else. Running first was tried for
                // both of them and lost `switch` recovery and short-circuit
                // `&&` folding -- losses the goto count scores as wins.
                let body =
                    try_alternative_structurings(ir, body, diag, AlternativeAdmission::Established);
                let elapsed = ir
                    .builder
                    .structuring_start
                    .map(|t| t.elapsed().as_secs_f64())
                    .unwrap_or(0.0);
                if diag {
                    eprintln!(
                        "[DIAG] structuring done (SESE): elapsed={:.3}s stmts={}",
                        elapsed,
                        body.len()
                    );
                }
                let protected = ir.builder.lsda_landing_pad_labels();
                let finalized =
                    crate::midend::structuring::finalize_structured_body(&protected, body);
                ir.set_structured_body(finalized);
                Ok(PassResult::Changed)
            }
            Err(err) => {
                if diag {
                    eprintln!(
                        "[DIAG] SESE structuring failed, falling back to linear: {:?}",
                        err
                    );
                }
                Ok(PassResult::NoChange)
            }
        }
    }
}

pub(crate) struct OrphanGotoRepairPass;

impl NirPass for OrphanGotoRepairPass {
    fn name(&self) -> &str {
        "OrphanGotoRepairPass"
    }

    /// Basis: [`InvariantBasis::PostStructuringCleanup`]
    ///
    /// After SESE structuring some goto labels may remain unreachable from
    /// the structured body (orphan gotos). This pass repairs them by
    /// localized re-linking — it operates only on the already-structured HIR
    /// statement list, not on raw CFG edges or binary-specific data.
    fn invariant_basis(&self) -> InvariantBasis {
        InvariantBasis::PostStructuringCleanup
    }

    fn run(
        &mut self,
        ir: &mut NirFunc<'_, '_>,
        store: &mut AnalysisStore,
    ) -> Result<PassResult, String> {
        let diag = structuring_diag_enabled();
        let scc = store.cfg_facts(ir).scc();
        let scc_irreducible_count = scc.irreducible_count();
        let max_scc_component_size = scc.max_component_size();

        let original_admission = ir
            .builder
            .structuring_admission_reason(scc_irreducible_count, max_scc_component_size);
        let blockgraph_collapse_enabled = blockgraph_collapse_admission_enabled();
        let admission = apply_blockgraph_collapse_admission_gate(
            original_admission,
            blockgraph_collapse_enabled,
        );

        if let Some(body) = ir.structured_body().map(|b| b.to_vec()) {
            if crate::midend::structuring::has_orphan_goto_labels(&body) {
                if let Some(repaired) = try_repair_orphan_gotos(ir.builder, body.clone()) {
                    if diag {
                        eprintln!(
                            "[DIAG] SESE orphan goto labels localized without flat goto fallback"
                        );
                    }
                    ir.builder
                        .telemetry
                        .structuring
                        .structuring_orphan_goto_localized_count += 1;

                    let elapsed = ir
                        .builder
                        .structuring_start
                        .map(|t| t.elapsed().as_secs_f64())
                        .unwrap_or(0.0);
                    metrics::histogram!("fission.structuring.total_ms").record(elapsed * 1000.0);
                    metrics::counter!("fission.structuring.invocations_total").increment(1);

                    ir.set_structured_body(repaired);
                    return Ok(PassResult::Changed);
                }

                if diag {
                    eprintln!("[DIAG] SESE result has orphan goto labels, falling back to linear");
                }
                ir.builder
                    .telemetry
                    .structuring
                    .forced_linear_structuring_count += 1;
                ir.builder
                    .telemetry
                    .structuring
                    .structuring_sese_orphan_goto_fallback_count += 1;
                ir.builder
                    .telemetry
                    .structuring
                    .structuring_orphan_goto_unrepairable_count += 1;

                // proof_first=true → try_switch_recovery on linear multiblock free-fn.
                let fallback_result =
                    build_linear_multiblock_body(ir.builder, true).map_err(|e| e.to_string())?;

                let elapsed = ir
                    .builder
                    .structuring_start
                    .map(|t| t.elapsed().as_secs_f64())
                    .unwrap_or(0.0);
                if diag {
                    eprintln!(
                        "[DIAG] structuring linear done: elapsed={:.3}s success=true proof_first=true admission={:?}",
                        elapsed, admission,
                    );
                }

                ir.set_structured_body(fallback_result);
                return Ok(PassResult::Changed);
            } else {
                let elapsed = ir
                    .builder
                    .structuring_start
                    .map(|t| t.elapsed().as_secs_f64())
                    .unwrap_or(0.0);
                metrics::histogram!("fission.structuring.total_ms").record(elapsed * 1000.0);
                metrics::counter!("fission.structuring.invocations_total").increment(1);

                return Ok(PassResult::NoChange);
            }
        } else {
            ir.builder
                .telemetry
                .structuring
                .forced_linear_structuring_count += 1;

            let force_linear = !matches!(admission, StructuringAdmissionReason::GraphCollapse);
            if force_linear {
                match admission {
                    StructuringAdmissionReason::ExplicitForceLinear => {
                        ir.builder
                            .telemetry
                            .structuring
                            .structuring_force_linear_explicit_count += 1;
                    }
                    StructuringAdmissionReason::IrreducibleBudget => {
                        ir.builder
                            .telemetry
                            .structuring
                            .structuring_force_linear_irreducible_budget_count += 1;
                    }
                    StructuringAdmissionReason::ExtremeBudget => {
                        ir.builder
                            .telemetry
                            .structuring
                            .structuring_force_linear_extreme_budget_count += 1;
                    }
                    StructuringAdmissionReason::GraphCollapse => {}
                }
            }

            let fallback_result =
                build_linear_multiblock_body(ir.builder, true).map_err(|e| e.to_string())?;

            // A primary SESE failure means only that strategy could not build
            // a baseline; it does not invalidate the graph-collapse admission
            // or the independent alternatives. Price them against the exact
            // linear body that would otherwise ship, while preserving forced
            // linear admissions as hard budget decisions.
            let fallback_result = if matches!(admission, StructuringAdmissionReason::GraphCollapse)
            {
                try_alternative_structurings(
                    ir,
                    fallback_result,
                    diag,
                    AlternativeAdmission::LinearFallback,
                )
            } else {
                fallback_result
            };

            let elapsed = ir
                .builder
                .structuring_start
                .map(|t| t.elapsed().as_secs_f64())
                .unwrap_or(0.0);
            if diag {
                eprintln!(
                    "[DIAG] structuring linear done: elapsed={:.3}s success=true proof_first=true admission={:?}",
                    elapsed, admission,
                );
            }

            ir.set_structured_body(fallback_result);
            return Ok(PassResult::Changed);
        }
    }
}
