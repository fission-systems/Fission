//! Midend **normalize** owner facade.
//!
//! # Extraction status (ADR 0012)
//!
//! Implementation still lives in [`fission_pcode::midend::normalize`]. This
//! crate is the stable dependency name for normalize-facing callers while the
//! physical code move waits on a shared midend-core split (action_pipeline /
//! vsa reverse edges).
//!
//! Prefer this crate over deep `fission-pcode` paths for normalize entrypoints.

#![doc = "See docs/adr/0012-midend-rename-and-crate-extraction.md"]

pub use fission_pcode::midend::normalize::{
    is_known_api_signature, normalize_hir_function, summarize_direct_tail_wrapper_from_ops,
    summarize_direct_tail_wrapper_from_pcode, take_normalize_wave_stats,
};

/// Owner module re-export (grows as normalize API is stabilized for extraction).
pub use fission_pcode::midend::normalize as owner;

/// Shared structured-IR types needed by normalize callers.
pub use fission_pcode::midend::{HirFunction, HirStmt, NirBuildStats};
