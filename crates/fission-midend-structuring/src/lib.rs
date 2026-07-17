//! Midend **structuring** owner facade.
//!
//! # Extraction status (ADR 0012)
//!
//! Shared substrate lives in [`fission_midend_core`]. Structuring **source**
//! still lives under [`fission_pcode::midend::structuring`] because several
//! regions are still implemented as methods on `PreviewBuilder` (builder
//! ownership remains in `fission-pcode`).
//!
//! Prefer this crate for structuring-facing entrypoints as they stabilize.

#![doc = "See docs/adr/0012-midend-rename-and-crate-extraction.md"]

/// Owner module re-export.
pub use fission_pcode::midend::structuring as owner;

/// Shared structured-IR types (via midend-core substrate).
pub use fission_midend_core::{HirFunction, HirStmt, NirBuildStats};

/// Switch fallthrough sentinel used by structuring and print.
pub use fission_midend_core::SWITCH_FALLTHROUGH_SENTINEL;
