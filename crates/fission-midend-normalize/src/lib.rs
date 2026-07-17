//! Midend **normalize** owner facade.
//!
//! # Extraction status (ADR 0012)
//!
//! Implementation still lives in [`fission_pcode::midend::normalize`]. This
//! crate is the stable dependency name for normalize-facing callers while the
//! physical code move waits on midend-core (`ir` + `action_pipeline` +
//! `wave_stats`; ADR 0012 Phase D). Reverse `wave_stats` edges are already
//! off the normalize path (Phase D0).
//!
//! Prefer this crate over deep `fission-pcode` paths for normalize entrypoints.

#![doc = "See docs/adr/0012-midend-rename-and-crate-extraction.md"]

pub use fission_pcode::midend::normalize::{
    is_known_api_signature, normalize_hir_function, summarize_direct_tail_wrapper_from_ops,
    summarize_direct_tail_wrapper_from_pcode, take_normalize_wave_stats,
};

/// Owner module re-export (grows as normalize API is stabilized for extraction).
pub use fission_pcode::midend::normalize as owner;

/// Shared structured-IR types (via midend-core substrate facade).
pub use fission_midend_core::{HirFunction, HirStmt, NirBuildStats};
