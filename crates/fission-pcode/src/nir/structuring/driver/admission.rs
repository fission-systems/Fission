//! Structuring admission gate and budget decisions.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StructuringAdmissionReason {
    GraphCollapse,
    ExplicitForceLinear,
    IrreducibleBudget,
    ExtremeBudget,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct StructuringAdmissionInput {
    pub block_count: usize,
    pub total_ops: usize,
    pub edge_count: usize,
    pub multi_pred_blocks: usize,
    pub max_predecessors: usize,
    pub scc_irreducible_count: usize,
    pub max_scc_component_size: usize,
    pub explicit_force_linear: bool,
}

pub(crate) fn decide_structuring_admission(input: StructuringAdmissionInput) -> StructuringAdmissionReason {
    if input.explicit_force_linear {
        return StructuringAdmissionReason::ExplicitForceLinear;
    }

    let extreme_budget = input.block_count > 192
        || input.total_ops > 3_000
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

pub(crate) fn mir_blockgraph_admission_enabled() -> bool {
    std::env::var_os("FISSION_ENABLE_MIR_BLOCKGRAPH").is_some()
}
