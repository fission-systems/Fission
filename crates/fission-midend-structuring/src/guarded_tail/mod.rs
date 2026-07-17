//! Guarded-tail free-function owner (ADR 0012).
//!
//! Types and promotion entry points live here. Deep canonicalize/execution
//! residual is still hosted by `PreviewBuilder` via [`StructuringHost`] hooks.

pub mod promote;
pub mod types;

pub use types::*;
pub use promote::{
    discover_guarded_tail_candidates, promote_guarded_tail_regions_until_stable,
    promote_single_entry_guarded_tail_regions,
};

// Convenience re-exports used by host_impl paths.
pub use types::{
    GuardedTailCanonicalizationFailure, GuardedTailExecutionPlan, GuardedTailExecutionRejection,
    GuardedTailTrial, GuardedTailVerification, GuardedTailWitnessRejection,
    PromotionGateRejection, PromotionShapeRejection,
};
