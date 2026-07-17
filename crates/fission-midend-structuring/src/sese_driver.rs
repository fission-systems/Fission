//! SESE / multiblock driver free functions.
//!
//! Collapse-rule dispatch uses free `try_lower_*` entry points. Full SESE body
//! construction still uses host hooks for graph overlay + residual recovery.

use crate::host::StructuringHost;
use crate::conditionals::{
    try_lower_if, try_lower_if_else, try_lower_short_circuit_if, try_reduce_if_else_with_follow,
};
use crate::loops::{
    try_lower_dowhile, try_lower_for, try_lower_infloop, try_lower_infloop_with_break,
    try_lower_multiblock_dowhile, try_lower_multiblock_infloop, try_lower_while,
};
use crate::switch::try_lower_switch;
use crate::linear_types::structuring_diag_enabled;
use crate::guarded_tail::promote_guarded_tail_regions_until_stable;
use fission_midend_core::ir::{HirStmt, MlilPreviewError};

/// Collapse rule tags (Ghidra ActionStructureTransform analog).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollapseRule {
    Switch,
    ForLoop,
    DoWhile,
    WhileDo,
    InfLoopBreak,
    InfLoop,
    Conditional,
    Sequence,
    Unstructured,
}

impl CollapseRule {
    pub fn name(self) -> &'static str {
        match self {
            Self::Switch => "switch",
            Self::ForLoop => "for",
            Self::DoWhile => "do-while",
            Self::WhileDo => "while",
            Self::InfLoopBreak => "infloop-break",
            Self::InfLoop => "infloop",
            Self::Conditional => "conditional",
            Self::Sequence => "sequence",
            Self::Unstructured => "unstructured",
        }
    }
}

/// Active collapse rule order (matches pcode ACTIVE_COLLAPSE_RULES).
pub const ACTIVE_COLLAPSE_RULES: [CollapseRule; 9] = [
    CollapseRule::Switch,
    CollapseRule::ForLoop,
    CollapseRule::DoWhile,
    CollapseRule::WhileDo,
    CollapseRule::InfLoopBreak,
    CollapseRule::InfLoop,
    CollapseRule::Conditional,
    CollapseRule::Sequence,
    CollapseRule::Unstructured,
];

/// Ideal-rule subset for SESE tier-1 collapse.
pub const IDEAL_COLLAPSE_RULES: [CollapseRule; 7] = [
    CollapseRule::Switch,
    CollapseRule::ForLoop,
    CollapseRule::DoWhile,
    CollapseRule::WhileDo,
    CollapseRule::InfLoopBreak,
    CollapseRule::InfLoop,
    CollapseRule::Conditional,
];

/// Apply one collapse rule at `idx` via free-function `try_lower_*` owners.
pub fn apply_collapse_rule(
    host: &mut impl StructuringHost,
    rule: CollapseRule,
    idx: usize,
    follow: Option<usize>,
) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
    match rule {
        CollapseRule::Switch => try_lower_switch(host, idx),
        CollapseRule::ForLoop => try_lower_for(host, idx),
        CollapseRule::DoWhile => {
            let mut dw = try_lower_dowhile(host, idx)?;
            if dw.is_none() {
                dw = try_lower_multiblock_dowhile(host, idx)?;
            }
            Ok(dw)
        }
        CollapseRule::WhileDo => try_lower_while(host, idx),
        CollapseRule::InfLoopBreak => try_lower_infloop_with_break(host, idx),
        CollapseRule::InfLoop => {
            let mut inf = try_lower_infloop(host, idx);
            if inf.is_err() || matches!(inf, Ok(None)) {
                inf = try_lower_multiblock_infloop(host, idx);
            }
            inf
        }
        CollapseRule::Conditional => {
            let mut cond = try_lower_short_circuit_if(host, idx);
            if cond.is_err() || matches!(cond, Ok(None)) {
                cond = try_reduce_if_else_with_follow(host, idx, follow);
            }
            if cond.is_err() || matches!(cond, Ok(None)) {
                cond = try_lower_if_else(host, idx);
            }
            if cond.is_err() || matches!(cond, Ok(None)) {
                cond = try_lower_if(host, idx);
            }
            cond
        }
        CollapseRule::Sequence | CollapseRule::Unstructured => Ok(None),
    }
}

/// Promote guarded-tail regions to a fixed point (free entry).
pub fn promote_guarded_tails(host: &mut impl StructuringHost, body: &mut Vec<HirStmt>) {
    promote_guarded_tail_regions_until_stable(host, body);
    if structuring_diag_enabled() {
        // keep quiet unless already enabled elsewhere
    }
}
