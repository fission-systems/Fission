//! Pure driver helpers for admission and region proof scaffolding.

use crate::admission::StructuringAdmissionReason;
use crate::regions::RegionKind;
use fission_midend_prehir::PreHirStmt;
use fission_midend_prehir::util::format_expr_key;

pub fn apply_blockgraph_collapse_admission_gate(
    admission: StructuringAdmissionReason,
    enabled: bool,
) -> StructuringAdmissionReason {
    if enabled && matches!(admission, StructuringAdmissionReason::IrreducibleBudget) {
        StructuringAdmissionReason::GraphCollapse
    } else {
        admission
    }
}

pub fn is_switch_scaffold_stmt(stmt: &PreHirStmt) -> bool {
    match stmt {
        PreHirStmt::Goto(_) => true,
        PreHirStmt::Block(body) => body.iter().all(is_switch_scaffold_stmt),
        PreHirStmt::Label(_)
        | PreHirStmt::Assign { .. }
        | PreHirStmt::Expr(_)
        | PreHirStmt::VaStart { .. }
        | PreHirStmt::If { .. }
        | PreHirStmt::Switch { .. }
        | PreHirStmt::While { .. }
        | PreHirStmt::DoWhile { .. }
        | PreHirStmt::For { .. }
        | PreHirStmt::Return(_)
        | PreHirStmt::Break
        | PreHirStmt::Continue => false,
    }
}

pub fn switch_stmt_has_scaffold_only_arms(stmt: &PreHirStmt) -> bool {
    let PreHirStmt::Switch { cases, default, .. } = stmt else {
        return false;
    };
    !cases.is_empty()
        && cases
            .iter()
            .all(|case| case.body.iter().all(is_switch_scaffold_stmt))
        && default.iter().all(is_switch_scaffold_stmt)
}

pub fn region_kind_for_stmt(stmt: &PreHirStmt) -> Option<RegionKind> {
    match stmt {
        PreHirStmt::Switch { .. } => Some(RegionKind::Switch),
        PreHirStmt::If { .. } => Some(RegionKind::Conditional),
        PreHirStmt::While { .. } | PreHirStmt::DoWhile { .. } | PreHirStmt::For { .. } => {
            Some(RegionKind::Loop)
        }
        PreHirStmt::Block(_) => Some(RegionKind::Sequence),
        PreHirStmt::Assign { .. }
        | PreHirStmt::Expr(_)
        | PreHirStmt::VaStart { .. }
        | PreHirStmt::Label(_)
        | PreHirStmt::Goto(_)
        | PreHirStmt::Return(_)
        | PreHirStmt::Break
        | PreHirStmt::Continue => None,
    }
}

pub fn region_selector_or_condition(stmt: &PreHirStmt) -> Option<String> {
    match stmt {
        PreHirStmt::Switch { expr, .. } => Some(format_expr_key(expr)),
        PreHirStmt::If { cond, .. }
        | PreHirStmt::While { cond, .. }
        | PreHirStmt::DoWhile { cond, .. } => Some(format_expr_key(cond)),
        PreHirStmt::For { cond, .. } => cond.as_ref().map(format_expr_key),
        PreHirStmt::Block(_)
        | PreHirStmt::Assign { .. }
        | PreHirStmt::Expr(_)
        | PreHirStmt::VaStart { .. }
        | PreHirStmt::Label(_)
        | PreHirStmt::Goto(_)
        | PreHirStmt::Return(_)
        | PreHirStmt::Break
        | PreHirStmt::Continue => None,
    }
}
