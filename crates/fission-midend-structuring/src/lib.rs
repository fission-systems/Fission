//! Midend **structuring** owner facade.
//!
//! # Extraction status (ADR 0012)
//!
//! Implementation still lives in [`fission_pcode::midend::structuring`]. This
//! crate establishes the future extraction dependency name. Expand re-exports
//! here as structuring surfaces are stabilized for a physical code move.

#![doc = "See docs/adr/0012-midend-rename-and-crate-extraction.md"]

/// Owner module re-export.
pub use fission_pcode::midend::structuring as owner;

/// Shared structured-IR types (via midend-core substrate facade).
pub use fission_midend_core::{HirFunction, HirStmt, NirBuildStats};

/// Switch fallthrough sentinel used by structuring and print.
pub use fission_midend_core::SWITCH_FALLTHROUGH_SENTINEL;
