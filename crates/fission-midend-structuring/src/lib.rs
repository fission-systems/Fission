//! Midend **structuring** owner (ADR 0012).
//!
//! Free-function CFG analysis, cleanup, regions, irreducible helpers, structure
//! graphs, admission, conditionals, collapse-loop helpers, and the
//! [`host::StructuringHost`] trait.
//!
//! Production host: `PreviewBuilder` in `fission-pcode` implements
//! [`StructuringHost`] (`structuring/host_impl.rs`). Residual guarded-tail /
//! linear / switch / loop lowering still lives partly in pcode; new free-function
//! work should take `&mut impl StructuringHost` instead of extending methods.

#![allow(clippy::all)]
#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(unused_variables)]
#![allow(unused_assignments)]

pub mod admission;
pub mod cfg_analysis;
pub mod cleanup;
pub mod collapse_loop;
pub mod conditionals;
pub mod graph;
pub mod guarded_tail_pure;
pub mod driver_pure;
pub mod helpers;
pub mod host;
pub mod irreducible;
pub mod linear_body;
pub mod linear_recovery;
pub mod linear_types;
pub mod loop_analysis;
pub mod loops;
pub mod regions;
pub mod switch;

pub use cfg_analysis::{
    CfgAnalysis, CfgFactCache, DomTree, EdgeClass, PostDomTree, SccAnalysis,
    compute_follow_blocks,
};
pub use cleanup::{
    cleanup_redundant_labels, finalize_structured_body, has_orphan_goto_labels,
};
pub use host::StructuringHost;
pub use graph::{
    StructureEdge, StructureEdgeFlags, StructureGraph, StructureNode, StructureNodeId,
    StructureNodeKind, capture_structuring_failure, surface_structure_graph,
};
pub use admission::{
    StructuringAdmissionInput, StructuringAdmissionReason, blockgraph_collapse_admission_enabled,
    decide_structuring_admission,
};
pub use linear_types::{
    CONDITION_RECOVERY_BUDGET_MS, CONDITION_RECOVERY_SUBCALL_LIMIT, ConditionalTailKey,
    ConditionalTailLoweringResult, ConditionalTailMismatchSubtype, IfLoweringBudget,
    LinearBodyCacheKey, LinearBodyCachedOutcome, LinearBodyLoweringOutcome, LinearBodyRejectReason,
    LinearExit, LoweredTerminator, MAX_LINEAR_STRUCTURING_DEPTH, NormalizedConditionalTailArm,
    structuring_diag_enabled,
};
pub use linear_recovery::{
    SESE_REGION_PROOF_BUDGET_MS, build_linear_sese_child_fallback,
    region_linearized_exit_candidates_algorithmic, try_recover_region_linearized_body,
};
pub use guarded_tail_pure::{
    count_var_defs_stmt, count_var_reads_stmt, expr_contains_var, replace_var_in_expr,
    replace_var_in_stmt,
};
pub use driver_pure::{
    apply_blockgraph_collapse_admission_gate, is_switch_scaffold_stmt, region_kind_for_stmt,
    region_selector_or_condition, switch_stmt_has_scaffold_only_arms,
};
pub use linear_body::{
    can_inline_linear_successor, can_inline_linear_successor_for_region, has_linear_body_cache,
    linear_exit, linear_exit_with_budget, lower_conditional_tail, lower_linear_body,
    lower_linear_body_for_region_recovery_detailed, lower_linear_body_with_budget,
    shared_exit_for_indices, shared_linear_exit,
};
pub use conditionals::{
    is_trivial_structuring_stmt, try_lower_if, try_lower_if_else, try_lower_return_chain_arm,
    try_lower_short_circuit_and, try_lower_short_circuit_and_else, try_lower_short_circuit_if,
    try_lower_short_circuit_or, try_reduce_if_else_with_follow,
};
pub use helpers::{block_label, merge_equivalent_switch_cases, recovered_switch_case_values};
pub use loops::{
    lower_loop_body_subgraph, try_lower_dowhile, try_lower_for, try_lower_infloop,
    try_lower_infloop_with_break, try_lower_multiblock_dowhile, try_lower_multiblock_infloop,
    try_lower_while,
};
pub use switch::{
    SWITCH_CHAIN_PARSE_BUDGET_MAX, canonicalize_switch_target, try_lower_switch,
};

pub use fission_midend_core::{HirFunction, HirStmt, NirBuildStats, SWITCH_FALLTHROUGH_SENTINEL};
pub use regions::{
    BlockGraphLegalityReason, BlockGraphRegionKind, BlockGraphRegionProof, EmitReadyDecision,
    EmitReadyFailureFamily, RegionKind, RegionLegality, RegionProof, RegionRejectionReason,
};
