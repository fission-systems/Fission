//! CFG-driven structuring from flattened HIR/NIR into loops, conditionals, guarded tails,
//! and related shapes. Prefer dom/postdom/SCC facts over lexical hacks.
//!
//! Guide: `crates/fission-pcode/src/nir/structuring/AGENTS.md`.

pub(super) use super::support::*;
use super::*;

pub(crate) mod cfg_analysis;
mod cleanup;
pub(crate) mod collapse_driver;
pub(crate) mod collapse_loop;
mod conditionals;
pub(crate) mod driver;
mod graph;
mod guarded_tail;
pub(super) mod irreducible;
mod linear;
mod loops;
pub(crate) mod passes;
mod regions;
pub(crate) mod sese;
mod switch;

// --- re-exports consumed by nir::builder and other nir subsystems ---
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
pub(crate) use graph::{
    StructureEdgeFlags, StructureGraph, StructureNode, StructureNodeKind, surface_structure_graph,
};
pub(crate) use linear::LinearBodyCachedOutcome;
pub(crate) use regions::{
    BlockGraphLegalityReason, BlockGraphRegionKind, BlockGraphRegionProof, EmitReadyDecision,
    EmitReadyFailureFamily, RegionKind, RegionLegality, RegionProof, RegionRejectionReason,
};
pub(crate) use switch::SWITCH_FALLTHROUGH_SENTINEL;

#[cfg(test)]
pub(super) use driver::{
    discover_guarded_tail_candidates_for_test, promote_single_entry_guarded_tail_regions_for_test,
};
#[cfg(test)]
pub(super) use linear::{LinearBodyLoweringOutcome, LinearBodyRejectReason};

pub(crate) mod loop_analysis;
