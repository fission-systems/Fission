//! Structuring admission gate and budget decisions.

use crate::HashMap;
use crate::HashSet;
use fission_midend_core::ir::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructuringAdmissionReason {
    GraphCollapse,
    ExplicitForceLinear,
    IrreducibleBudget,
    ExtremeBudget,
}

#[derive(Debug, Clone, Copy)]
pub struct StructuringAdmissionInput {
    pub block_count: usize,
    pub total_ops: usize,
    pub edge_count: usize,
    pub multi_pred_blocks: usize,
    pub max_predecessors: usize,
    pub scc_irreducible_count: usize,
    pub max_scc_component_size: usize,
    pub explicit_force_linear: bool,
}

pub fn decide_structuring_admission(
    input: StructuringAdmissionInput,
) -> StructuringAdmissionReason {
    if input.explicit_force_linear {
        return StructuringAdmissionReason::ExplicitForceLinear;
    }

    // Raised from 192/3_000 (measured on the DecBench sample-set corpus: 4
    // functions with 122-207 *reducible* blocks -- scc_irreducible_count==0,
    // so the ReplacementPlanExplosion risk documented for IrreducibleBudget
    // broadening does not apply here -- were being force-linearized purely
    // on size, well short of any actual structuring cost problem (full SESE
    // structuring completed in 2-3.6s each, vs. Ghidra emitting 3-4x fewer
    // gotos for the same functions). New thresholds keep ~3x headroom over
    // the largest confirmed-safe case (207 blocks) in that corpus rather
    // than removing the guard outright.
    //
    // Operation count is not structural complexity by itself. A dense
    // switch can have many independent case bodies while remaining reducible
    // and cheap for the CFG algorithms; a large loop SCC or an edge-dense
    // graph is the shape that makes the collapse work grow unpredictably.
    // Keep the hard block-count cap, and couple the operation cap to those
    // structural signals instead of linearizing every large switch arm.
    //
    // A large, shallow CFG is a separate risk: it can make region discovery
    // enumerate hundreds of candidates without changing the final emitted
    // CFG. Keep that shape on the bounded linear path until it has a
    // substantial SCC or another structural signal to justify the work.
    let high_operation_count_on_large_shallow_cfg =
        input.total_ops > 10_000 && input.block_count > 256 && input.max_scc_component_size < 32;
    let extreme_budget = input.block_count > 600
        || high_operation_count_on_large_shallow_cfg
        || (input.total_ops > 10_000
            && (input.edge_count > input.block_count.saturating_mul(4)
                || input.max_scc_component_size > 64))
        || (input.edge_count > input.block_count.saturating_mul(4)
            && input.max_predecessors >= 6
            && input.max_scc_component_size > 64);
    if extreme_budget {
        return StructuringAdmissionReason::ExtremeBudget;
    }

    let irreducible_budget = input.scc_irreducible_count > 0
        && (input.block_count > 64
            || input.total_ops > 900
            || input.edge_count > input.block_count.saturating_mul(3)
            || input.multi_pred_blocks > 16
            || input.max_predecessors >= 5
            || input.max_scc_component_size > 24);
    if irreducible_budget {
        return StructuringAdmissionReason::IrreducibleBudget;
    }

    StructuringAdmissionReason::GraphCollapse
}

pub fn blockgraph_collapse_admission_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var_os("FISSION_ENABLE_BLOCKGRAPH_COLLAPSE").is_some()
            || std::env::var_os("FISSION_ENABLE_MIR_BLOCKGRAPH").is_some()
    })
}
