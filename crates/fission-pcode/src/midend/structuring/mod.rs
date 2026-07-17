//! CFG-driven structuring from flattened HIR/NIR into loops, conditionals, guarded tails,
//! and related shapes. Prefer dom/postdom/SCC facts over lexical hacks.
//!
//! Guide: `crates/fission-pcode/src/nir/structuring/AGENTS.md`.

pub(super) use super::support::*;
use super::*;

pub(crate) mod cfg_analysis;
mod host_impl;

// Pure free-function owners: fission-midend-structuring
pub use fission_midend_structuring::cleanup;
pub use fission_midend_structuring::irreducible;
pub use fission_midend_structuring::loop_analysis;
pub use fission_midend_structuring::regions;
pub use fission_midend_structuring::StructuringHost;
// Graph types: no local thin module — re-export owner crate directly (ADR 0012).
pub use fission_midend_structuring::graph::{
    StructureEdge, StructureEdgeFlags, StructureGraph, StructureNode, StructureNodeId,
    StructureNodeKind, capture_structuring_failure, surface_structure_graph,
};
pub(crate) mod collapse_driver;
pub(crate) mod collapse_loop;
mod conditionals;
pub(crate) mod driver;
mod guarded_tail;
pub(super) mod linear;
mod loops;
pub(crate) mod passes;
pub(crate) mod sese;
mod switch;

// --- re-exports consumed by builder and other midend subsystems ---
pub(crate) use cfg_analysis::{
    CfgAnalysis, CfgFactCache, DomTree, EdgeClass, PostDomTree, SccAnalysis,
};
pub(crate) use cleanup::{
    cleanup_redundant_labels, finalize_structured_body, has_orphan_goto_labels,
};
pub(crate) use collapse_driver::CollapseDriver;
pub(crate) use driver::{
    collapse::{ACTIVE_COLLAPSE_RULES, CollapseCandidate, CollapseRule},
    discover_guarded_tail_candidates_for_stats, structuring_diag_enabled,
};
pub(crate) use linear::{
    LinearBodyCachedOutcome, LinearBodyLoweringOutcome, LinearBodyRejectReason,
};
pub(crate) use regions::{
    BlockGraphLegalityReason, BlockGraphRegionKind, BlockGraphRegionProof, EmitReadyDecision,
    EmitReadyFailureFamily, RegionKind, RegionLegality, RegionProof, RegionRejectionReason,
};
pub(crate) use crate::midend::SWITCH_FALLTHROUGH_SENTINEL;

#[cfg(test)]
pub(super) use driver::{
    discover_guarded_tail_candidates_for_test, promote_single_entry_guarded_tail_regions_for_test,
};
// LinearBodyLoweringOutcome / LinearBodyRejectReason re-exported above for all builds.
