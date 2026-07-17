//! Structured IR substrate: `Hir*` surface types and canonical telemetry
//! ([`NirBuildStats`]).
//!
//! This is the shared midend tree language (not the HIR *print layer* —
//! that lives in [`crate::render`]). New counters or schema changes must
//! stay aligned with automation/reporting lanes.
//! Overview: `crates/fission-pcode/src/nir/AGENTS.md`.

use fission_loader::loader::LoadedBinary;
use indexmap::IndexMap;
use std::collections::{HashMap, HashSet};
use thiserror::Error;

use fission_core::CallingConvention;

mod build_stats;
mod decomp_facts;
mod hir;
pub mod inference;
mod options;
mod procedure;
mod stats_merge;

pub use build_stats::*;
pub use decomp_facts::*;
pub use hir::*;
pub use options::*;
pub use procedure::*;
