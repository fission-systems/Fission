pub(super) use super::support::*;
use super::*;

mod cfg_analysis;
mod cleanup;
mod conditionals;
mod driver;
mod guarded_tail;
pub(super) mod irreducible;
mod linear;
mod loops;
mod recovery;
mod surfacing;
mod switch;

pub(crate) use cfg_analysis::{
    CfgAnalysis, CfgFactCache, DomTree, EdgeClass, PostDomTree, SccAnalysis,
};
pub(crate) use cleanup::cleanup_redundant_labels;
pub(crate) use driver::discover_guarded_tail_candidates_for_stats;
pub(crate) use driver::structuring_diag_enabled;
pub(crate) use linear::LinearBodyCachedOutcome;

#[cfg(test)]
pub(super) use driver::{
    discover_guarded_tail_candidates_for_test, promote_single_entry_guarded_tail_regions_for_test,
};
#[cfg(test)]
pub(super) use linear::{LinearBodyLoweringOutcome, LinearBodyRejectReason};
pub(crate) mod loop_analysis;
