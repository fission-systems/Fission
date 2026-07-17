//! Post-lift midend: builder, normalization, structuring, and orchestration
//! into dual-layer print ([`crate::render`]).
//!
//! Stage order: [`builder`] → [`normalize`] → [`structuring`] → [`crate::render`].
//! Directory guide: `crates/fission-pcode/src/midend/AGENTS.md`.
//!
//! Historical crate path `fission_pcode::nir` is a re-export of this module
//! during the ADR 0012 migration window.

// Bridge imports used by child owners via `super::…` (historical shared prelude).
use crate::pcode::{PcodeFunction, PcodeOp, PcodeOpcode, Varnode};
#[allow(unused_imports)]
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
#[allow(unused_imports)]
use std::time::Instant;

mod abi;
mod abstract_location;
mod action_pipeline;
mod builder;
mod cfg;
pub mod cspec;
mod labels;
/// Normalize owner (future extraction: `fission-midend-normalize`, ADR 0012).
pub mod normalize;
mod orchestrate;
pub(crate) mod pass;
mod piece;
mod stats;
/// Structuring owner (future extraction: `fission-midend-structuring`, ADR 0012).
pub mod structuring;
mod support;
mod telemetry;
#[cfg(test)]
mod tests;
/// Structured IR substrate (`Hir*`, options, `NirBuildStats`).
pub mod ir;
mod var_rename;
mod vsa;

pub(crate) use self::abi::{
    AbiKind, AbiState, CarrierAssignment, CarrierResource, GenericAbiProvider,
    WindowsX64AbiProvider,
};
pub use self::abstract_location::{AbstractStackSlot, ParamSlotIndex};
pub(crate) use self::action_pipeline::STRUCTURING_TIME_CEILING_SECS;
pub(crate) use self::labels::SWITCH_FALLTHROUGH_SENTINEL;

pub(super) use self::support::*;
pub use self::telemetry::{
    take_last_nir_build_stats, take_last_nir_hint_stats, take_last_preview_build_stats,
    take_last_preview_hint_stats,
};
pub use self::ir::*;
use self::{action_pipeline::*, builder::*, cfg::*, normalize::*, structuring::*};

// Presentation/print surface lives at crate root `render` (ADR 0011).
// `pub(crate)` so tests and owners can keep using `crate::midend::print_*` / `crate::midend::print_*`.
pub(crate) use crate::render::{
    print_expr, print_hir_function, print_hir_function_with_global_names,
    print_hir_function_with_profile, print_stmt, print_type, recover_global_symbol_accesses,
    render_hir_function_with_global_decls, render_layered_pseudocode,
};
pub use fission_core::CallingConvention;

pub use self::abi::infer_entry_register_param_arity;
pub use self::cfg::structuring_cfg_edges;
pub use self::cspec::RegisterNamer;
pub use self::normalize::{
    summarize_direct_tail_wrapper_from_ops, summarize_direct_tail_wrapper_from_pcode,
};
pub use crate::render::{
    LayeredPseudocode, PrintProfile, PseudocodeLayer, render_contracted_wrapper_summary,
};

// Top-level preview/NIR entrypoints (builder → normalize → structure → print).
pub use self::orchestrate::{
    render_mlil_preview, render_mlil_preview_with_binary_and_context,
    render_mlil_preview_with_context, render_nir, render_nir_with_binary_and_context,
    render_nir_with_context, take_last_layered_pseudocode, test_refine_partitions,
};
