//! Pseudocode print contracts.
//!
//! There is one output surface: the structured tree, selected for the CFG node
//! count Joern assigns (see `StructuringQuality::estimated_cfg_nodes_x100`),
//! then presented as HIR. `PrintProfile::Nir` survives only as the mechanical
//! print the unit tests compare that surface against.
//!
//! Semantics live in normalize/structuring; this module only selects presentation.

use serde::{Deserialize, Serialize};

/// Scoped instruction evidence on an actual output line.
/// Definition records apply only to `defined_variables`; call-only records
/// have an empty variable scope and an explicit `call_site`. Neither grants
/// provenance to unrelated RHS occurrences.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveredLineMapping {
    pub line_number: usize,
    pub addresses: Vec<u64>,
    pub defined_variables: Vec<String>,
    /// Call-only ownership. Empty variable scope never grants RHS variable PCs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call_site: Option<fission_midend_core::ir::CallOrigin>,
}

/// Printer presentation knobs (shared walk, different sugar).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PrintProfile {
    /// Keep casts, temps, and structure visible.
    #[default]
    Nir,
    /// Prefer compact C: omit unused noise locals, elide some casts.
    Hir,
}

/// Both print profiles of one tree. Test-only: the pipeline emits a single
/// surface, but the printer's profiles are still compared in unit tests.
#[cfg(test)]
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct LayeredPseudocode {
    /// Mechanical print of the structured tree.
    pub nir: String,
    /// Final surface.
    pub hir: String,
    pub nir_line_mappings: Vec<RecoveredLineMapping>,
    pub hir_line_mappings: Vec<RecoveredLineMapping>,
}
