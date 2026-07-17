use crate::midend::pass::{AnalysisStore, InvariantBasis, NirFunc, NirPass, PassResult};
use crate::midend::structuring::driver::{
    StructuringAdmissionInput, StructuringAdmissionReason, blockgraph_collapse_admission_enabled,
    decide_structuring_admission,
};
use crate::midend::structuring::irreducible::{compute_fas_virtual_gotos, compute_node_splits};
use crate::midend::structuring::structuring_diag_enabled;
use crate::midend::ir::{HirStmt, MlilPreviewError};

fn apply_blockgraph_collapse_admission_gate(
    admission: StructuringAdmissionReason,
    enabled: bool,
) -> StructuringAdmissionReason {
    if enabled && matches!(admission, StructuringAdmissionReason::IrreducibleBudget) {
        StructuringAdmissionReason::GraphCollapse
    } else {
        admission
    }
}

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
        let sese_result =
            if crate::midend::structuring::collapse_loop::collapse_loop_admission_enabled() {
                match crate::midend::structuring::collapse_loop::structure_cfg_via_collapse_loop(
                    ir.builder,
                    total_blocks,
                ) {
                    Ok(body) => Ok(body),
                    Err(err) => {
                        if diag {
                            eprintln!(
                                "[DIAG] collapse loop failed ({err:?}), falling back to SESE tree"
                            );
                        }
                        crate::midend::structuring::sese::structure_cfg_via_sese(
                            ir.builder,
                            total_blocks,
                        )
                    }
                }
            } else {
                crate::midend::structuring::sese::structure_cfg_via_sese(ir.builder, total_blocks)
            };

        match sese_result {
            Ok(body) => {
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
                let finalized = crate::midend::structuring::finalize_structured_body(body);
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
                if let Some(repaired) = ir.builder.try_repair_orphan_gotos(body.clone()) {
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

                let fallback_result = ir
                    .builder
                    .build_proof_first_linear_multiblock_body()
                    .map_err(|e| e.to_string())?;

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

            let fallback_result = ir
                .builder
                .build_proof_first_linear_multiblock_body()
                .map_err(|e| e.to_string())?;

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
