//! Guarded-tail free-function owner (ADR 0012).
//!
//! Types, pure HIR helpers, promote entry, and canonicalize/execution bodies.

pub mod pure_hir;
pub mod promote;
pub mod bodies;
pub mod types;

pub use types::*;
pub use pure_hir::*;
pub use promote::{
    discover_guarded_tail_candidates, promote_guarded_tail_regions_until_stable,
    promote_single_entry_guarded_tail_regions,
};
pub use bodies::{
    StructuringCounter, build_guarded_tail_execution_plan, canonicalize_guarded_tail_segment,
    canonicalize_interleaved_local_aliases, classify_must_emit_label_rejection,
    collect_guarded_tail_exported_bindings, discover_guarded_tail_candidates_in_body,
    execute_guarded_tail_plan, map_guarded_tail_canonicalization_rejection,
    try_build_guarded_tail_trial, try_build_guarded_tail_witness, verify_guarded_tail_trial,
};
