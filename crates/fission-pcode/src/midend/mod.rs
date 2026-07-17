//! Post-lift midend: builder, normalization, structuring, and orchestration
//! into dual-layer print ([`crate::render`]).
//!
//! Stage order: [`builder`] → [`normalize`] → [`structuring`] → [`crate::render`].
//! Directory guide: `crates/fission-pcode/src/midend/AGENTS.md`.
//!
//! Prefer `fission_pcode::midend` as the public path (ADR 0012).
//! Shared substrate (`ir`, `action_pipeline`, `wave_stats`, labels) lives in
//! [`fission_midend_core`] and is re-exported here for stable paths.

// Bridge imports used by child owners via `super::…` (historical shared prelude).
use crate::pcode::{PcodeFunction, PcodeOp, PcodeOpcode, Varnode};
#[allow(unused_imports)]
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
#[allow(unused_imports)]
use std::time::Instant;

mod abi;
mod abstract_location;
mod builder;
mod cfg;
pub mod cspec;
/// Normalize owner (future extraction: `fission-midend-normalize`, ADR 0012).
pub mod normalize;
mod orchestrate;
pub(crate) mod pass;
mod piece;
/// Structuring owner (future extraction: `fission-midend-structuring`, ADR 0012).
pub mod structuring;
mod support;
mod telemetry;
#[cfg(test)]
mod tests;
mod var_rename;
mod vsa;

// ── Shared substrate re-exports (owned by fission-midend-core) ──────────────
/// Structured IR substrate (`Hir*`, options, `NirBuildStats`).
pub use fission_midend_core::ir;
/// Action-pipeline framework (Pass / ActionGroup / budget helpers).
pub use fission_midend_core::action_pipeline;
/// Shared quality-wave telemetry counters.
pub use fission_midend_core::wave_stats;
/// Shared label sentinels.
pub use fission_midend_core::labels;

pub(crate) use self::abi::{
    AbiKind, AbiState, CarrierAssignment, CarrierResource, GenericAbiProvider,
    WindowsX64AbiProvider,
};
pub use self::abstract_location::{AbstractStackSlot, ParamSlotIndex};
pub(crate) use action_pipeline::STRUCTURING_TIME_CEILING_SECS;
pub use labels::SWITCH_FALLTHROUGH_SENTINEL;

pub(super) use self::support::*;
pub use self::telemetry::{
    take_last_nir_build_stats, take_last_nir_hint_stats, take_last_preview_build_stats,
    take_last_preview_hint_stats,
};
pub use ir::*;
use self::{action_pipeline::*, builder::*, cfg::*, normalize::*, structuring::*};

// Presentation/print surface lives at crate root `render` (ADR 0011).
// `pub(crate)` so tests and owners can keep using `crate::midend::print_*`.
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
    is_known_api_signature, normalize_hir_function, summarize_direct_tail_wrapper_from_ops,
    summarize_direct_tail_wrapper_from_pcode, take_normalize_wave_stats,
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

/// Seed [`NirRenderOptions`] from a loaded binary and populate SLA register map.
///
/// Prefer this over bare [`NirRenderOptions::from_loaded_binary`] when register
/// naming / cspec data is needed (the core constructor stays resource-free).
pub fn seed_nir_render_options(binary: &fission_loader::loader::LoadedBinary) -> NirRenderOptions {
    let mut options = NirRenderOptions::from_loaded_binary(binary);
    cspec::apply::ensure_sla_register_map(&mut options);
    options
}

/// P-code adapter: admission facts from a lifted function shape.
pub fn nir_admission_facts_from_pcode(pcode: &crate::pcode::PcodeFunction) -> NirAdmissionFacts {
    use crate::pcode::PcodeOpcode;
    NirAdmissionFacts::from_counts(
        pcode.blocks.len(),
        pcode.blocks.iter().map(|block| block.ops.len()).sum(),
        pcode
            .blocks
            .iter()
            .flat_map(|block| block.ops.iter())
            .filter(|op| op.opcode == PcodeOpcode::MultiEqual)
            .map(|op| op.inputs.len())
            .max()
            .unwrap_or(0),
    )
}

/// P-code adapter: indirect-control classification from raw p-code observation.
pub fn indirect_control_classification_from_pcode(
    pcode: &crate::pcode::PcodeFunction,
) -> IndirectControlClassification {
    IndirectControlClassification::from_indirect_observation(crate::pcode_has_indirect_control_flow(
        pcode,
    ))
}
