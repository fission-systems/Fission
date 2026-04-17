pub(super) use super::support::*;
use super::*;

mod cfg_analysis;
mod collapse;
mod cleanup;
mod conditionals;
mod driver;
mod graph;
mod guarded_tail;
pub(super) mod irreducible;
mod linear;
mod loops;
mod recovery;
mod regions;
mod surfacing;
mod switch;

pub(crate) use cfg_analysis::{
    CfgAnalysis, CfgFactCache, DomTree, EdgeClass, PostDomTree, SccAnalysis,
};
pub(crate) use collapse::{CollapseCandidate, CollapseRule, ACTIVE_COLLAPSE_RULES};
pub(crate) use cleanup::{cleanup_redundant_labels, finalize_structured_body};
pub(crate) use driver::discover_guarded_tail_candidates_for_stats;
pub(crate) use driver::structuring_diag_enabled;
pub(crate) use graph::{
    StructureEdgeFlags, StructureGraph, StructureNode, StructureNodeKind,
};
pub(crate) use linear::LinearBodyCachedOutcome;
#[allow(unused_imports)]
pub(crate) use regions::{
    EmitReadyDecision, EmitReadyFailureFamily, RegionKind, RegionLegality, RegionProof,
    RegionRejectionReason,
};
pub(crate) use surfacing::surface_structure_graph;

#[cfg(test)]
pub(super) use driver::{
    discover_guarded_tail_candidates_for_test, promote_single_entry_guarded_tail_regions_for_test,
};
#[cfg(test)]
pub(super) use linear::{LinearBodyLoweringOutcome, LinearBodyRejectReason};
pub(crate) mod loop_analysis;
