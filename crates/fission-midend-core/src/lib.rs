//! Midend **shared substrate** facade.
//!
//! # Extraction status (ADR 0012 Phase D)
//!
//! Implementation still lives under [`fission_pcode::midend`] (`ir/`,
//! `action_pipeline/`, `wave_stats`, shared labels). This crate is the stable
//! dependency name for shared midend types so normalize/structuring facades
//! and future owners can depend on **core** without deep `fission-pcode`
//! paths.
//!
//! Target ownership after the physical move:
//! - structured IR (`Hir*`, options, `NirBuildStats`)
//! - action-pipeline framework (Pass / ActionGroup / budget helpers)
//! - quality-wave counters (`wave_stats`)
//! - shared label sentinels (e.g. switch fallthrough)
//!
//! Prefer this crate over `fission_pcode::midend::ir` for shared IR types in
//! new call sites.

#![doc = "See docs/adr/0012-midend-rename-and-crate-extraction.md"]

/// Shared structured-IR types and telemetry contract.
pub use fission_pcode::midend::{
    HirExpr, HirFunction, HirStmt, MlilPreviewOptions, NirBinding, NirBindingOrigin, NirBuildStats,
    NirType, SWITCH_FALLTHROUGH_SENTINEL,
};

/// IR substrate module re-export (grows as core surface stabilizes).
pub use fission_pcode::midend::ir as ir;
