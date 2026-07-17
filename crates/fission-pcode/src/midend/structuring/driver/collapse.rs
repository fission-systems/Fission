use super::StructureNode;

// Canonical collapse-rule owner lives in fission-midend-structuring (ADR 0012).
pub(crate) use fission_midend_structuring::{
    ACTIVE_COLLAPSE_RULES, CollapseCandidate, CollapseRule,
};

// Back-compat alias if local StructureNode-based type was expected.
// CollapseCandidate is now the midend type (uses StructureNode from midend graph).
