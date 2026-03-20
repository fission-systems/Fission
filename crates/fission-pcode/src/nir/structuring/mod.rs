use super::*;

mod cleanup;
mod conditionals;
mod driver;
mod guards;
mod linear;
mod loops;
mod recovery;
mod surfacing;
mod switch;

pub(crate) use cleanup::cleanup_redundant_labels;
pub(crate) use driver::discover_guarded_tail_candidates_for_stats;
pub(crate) use driver::structuring_diag_enabled;

#[cfg(test)]
pub(super) use driver::{
    discover_guarded_tail_candidates_for_test, promote_single_entry_guarded_tail_regions_for_test,
};
