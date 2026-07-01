//! Shared HIR/NIR surface types and canonical build telemetry ([`NirBuildStats`]).
//!
//! New counters or schema changes must stay aligned with automation/reporting lanes.
//! Overview: `crates/fission-pcode/src/nir/AGENTS.md` (`types.rs` row).

use fission_loader::loader::LoadedBinary;
use indexmap::IndexMap;
use std::collections::{HashMap, HashSet};
use thiserror::Error;

use fission_core::CallingConvention;
use crate::pcode::{PcodeFunction, PcodeOpcode};

mod build_stats;
mod hir;
pub(crate) mod inference;
mod options;
mod procedure;

pub use build_stats::*;
pub use hir::*;
pub use options::*;
pub use procedure::*;
