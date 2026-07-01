//! Shared HIR/NIR surface types and canonical build telemetry ([`NirBuildStats`]).
//!
//! New counters or schema changes must stay aligned with automation/reporting lanes.
//! Overview: `crates/fission-pcode/src/nir/AGENTS.md` (`types.rs` row).

use fission_loader::loader::LoadedBinary;
use indexmap::IndexMap;
use std::collections::{HashMap, HashSet};
use thiserror::Error;

use crate::pcode::{PcodeFunction, PcodeOpcode};
use fission_core::CallingConvention;

mod build_stats;
mod decomp_facts;
mod hir;
pub(crate) mod inference;
mod options;
mod procedure;

pub use build_stats::*;
pub use decomp_facts::*;
pub use hir::*;
pub use options::*;
pub use procedure::*;
