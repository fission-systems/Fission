//! Midend **normalize** owner facade.
//!
//! # Extraction status (ADR 0012)
//!
//! Shared substrate (`ir`, `action_pipeline`, `wave_stats`) now lives in
//! [`fission_midend_core`]. Normalize **source** still lives under
//! [`fission_pcode::midend::normalize`] until pure helpers (`support`/`vsa`/
//! `var_rename`) finish decoupling from builder/p-code.
//!
//! Prefer this crate over deep `fission-pcode` paths for normalize entrypoints.

#![doc = "See docs/adr/0012-midend-rename-and-crate-extraction.md"]

pub use fission_pcode::midend::normalize::{
    is_known_api_signature, normalize_hir_function, summarize_direct_tail_wrapper_from_ops,
    summarize_direct_tail_wrapper_from_pcode, take_normalize_wave_stats,
};

/// Owner module re-export (grows as normalize API is stabilized for extraction).
pub use fission_pcode::midend::normalize as owner;

/// Shared structured-IR types (via midend-core substrate).
pub use fission_midend_core::{HirFunction, HirStmt, NirBuildStats};
