use super::StructureNode;

// Canonical collapse-rule owner lives in fission-midend-structuring (ADR 0012).
pub(crate) use fission_midend_structuring::{ACTIVE_COLLAPSE_RULES, CollapseRule};

#[derive(Debug, Clone)]
pub(crate) struct CollapseCandidate {
    pub(crate) rule: CollapseRule,
    pub(crate) node: StructureNode,
}
