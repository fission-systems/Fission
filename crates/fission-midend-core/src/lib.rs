//! Midend **shared substrate**.
//!
//! Owns structured IR types (`Hir*`, options, `NirBuildStats`), the
//! action-pipeline framework, quality-wave counters, and shared label
//! sentinels used by normalize/structuring/builder (ADR 0012 Phase D).
//!
//! Downstream owners should depend on this crate instead of deep
//! `fission-pcode` paths for shared IR and pipeline infrastructure.

#![allow(clippy::all)]
#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(unused_variables)]
#![allow(unused_assignments)]

pub mod action_pipeline;
pub mod ir;
pub mod labels;
pub mod wave_stats;

/// Shared structured-IR types and telemetry contract.
pub use ir::{
    HirExpr, HirFunction, HirStmt, MlilPreviewOptions, NirAdmissionFacts, NirBinding,
    NirBindingOrigin, NirBuildStats, NirFunctionHints, NirRenderOptions, NirType,
};

/// Switch fallthrough sentinel used by structuring and print.
pub use labels::SWITCH_FALLTHROUGH_SENTINEL;
