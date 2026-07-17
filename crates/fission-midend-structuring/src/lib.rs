//! Midend **structuring** free-function owners (ADR 0012).
//!
//! Pure CFG/dom/SCC analysis, region proofs, irreducible helpers, loop analysis,
//! and HIR cleanup live here. PreviewBuilder-bound collapse/guarded-tail/linear
//! recovery remains in `fission-pcode` until those methods are fully lifted to
//! free functions over a host trait.

#![allow(clippy::all)]
#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(unused_variables)]
#![allow(unused_assignments)]

pub mod cfg_analysis;
pub mod cleanup;
pub mod irreducible;
pub mod loop_analysis;
pub mod regions;

pub use cleanup::{
    cleanup_redundant_labels, finalize_structured_body, has_orphan_goto_labels,
};
pub use fission_midend_core::{HirFunction, HirStmt, NirBuildStats, SWITCH_FALLTHROUGH_SENTINEL};
pub use regions::{
    BlockGraphLegalityReason, BlockGraphRegionKind, BlockGraphRegionProof, EmitReadyDecision,
    EmitReadyFailureFamily, RegionKind, RegionLegality, RegionProof, RegionRejectionReason,
};
