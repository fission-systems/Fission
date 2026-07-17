//! Midend **structuring** owner (ADR 0012).
//!
//! Free-function CFG analysis, cleanup, regions, irreducible helpers, structure
//! graphs, admission, and the [`host::StructuringHost`] trait.
//!
//! Production host: `PreviewBuilder` in `fission-pcode` implements
//! [`StructuringHost`] (`structuring/host_impl.rs`). Residual collapse /
//! guarded-tail / linear / switch / loop lowering still lives in pcode as
//! `PreviewBuilder` methods; new free-function work should take
//! `&mut impl StructuringHost` instead of extending those methods.

#![allow(clippy::all)]
#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(unused_variables)]
#![allow(unused_assignments)]

pub mod cfg_analysis;
pub mod cleanup;
pub mod host;
pub mod graph;
pub mod admission;
pub mod irreducible;
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

pub use fission_midend_core::{HirFunction, HirStmt, NirBuildStats, SWITCH_FALLTHROUGH_SENTINEL};
pub use regions::{
    BlockGraphLegalityReason, BlockGraphRegionKind, BlockGraphRegionProof, EmitReadyDecision,
    EmitReadyFailureFamily, RegionKind, RegionLegality, RegionProof, RegionRejectionReason,
};
