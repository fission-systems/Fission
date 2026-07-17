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
pub mod host;
pub mod irreducible;
pub mod linear_types;
pub mod loop_analysis;
pub mod regions;

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
    IfLoweringBudget, LinearBodyCacheKey, LinearExit, LoweredTerminator, structuring_diag_enabled,
};
pub use conditionals::{
    is_trivial_structuring_stmt, try_lower_if, try_lower_if_else, try_lower_return_chain_arm,
    try_lower_short_circuit_and, try_lower_short_circuit_and_else, try_lower_short_circuit_if,
    try_lower_short_circuit_or, try_reduce_if_else_with_follow,
};

pub use fission_midend_core::{HirFunction, HirStmt, NirBuildStats, SWITCH_FALLTHROUGH_SENTINEL};
pub use regions::{
    BlockGraphLegalityReason, BlockGraphRegionKind, BlockGraphRegionProof, EmitReadyDecision,
    EmitReadyFailureFamily, RegionKind, RegionLegality, RegionProof, RegionRejectionReason,
};
