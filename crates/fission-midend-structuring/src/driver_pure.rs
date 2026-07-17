//! Pure driver helpers for admission and region proof scaffolding.

use crate::admission::StructuringAdmissionReason;
use crate::regions::RegionKind;
use fission_midend_core::format_expr_key;
use fission_midend_core::ir::HirStmt;

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

pub fn is_switch_scaffold_stmt(stmt: &HirStmt) -> bool {
        match stmt {
            HirStmt::Goto(_) => true,
            HirStmt::Block(body) => body.iter().all(is_switch_scaffold_stmt),
            HirStmt::Label(_)
            | HirStmt::Assign { .. }
            | HirStmt::Expr(_)
            | HirStmt::VaStart { .. }
            | HirStmt::If { .. }
            | HirStmt::Switch { .. }
            | HirStmt::While { .. }
            | HirStmt::DoWhile { .. }
            | HirStmt::For { .. }
            | HirStmt::Return(_)
            | HirStmt::Break
            | HirStmt::Continue => false,
        }
    }

pub fn switch_stmt_has_scaffold_only_arms(stmt: &HirStmt) -> bool {
        let HirStmt::Switch { cases, default, .. } = stmt else {
            return false;
        };
        !cases.is_empty()
            && cases
                .iter()
                .all(|case| case.body.iter().all(is_switch_scaffold_stmt))
            && default.iter().all(is_switch_scaffold_stmt)
    }

pub fn region_kind_for_stmt(stmt: &HirStmt) -> Option<RegionKind> {
        match stmt {
            HirStmt::Switch { .. } => Some(RegionKind::Switch),
            HirStmt::If { .. } => Some(RegionKind::Conditional),
            HirStmt::While { .. } | HirStmt::DoWhile { .. } | HirStmt::For { .. } => {
                Some(RegionKind::Loop)
            }
            HirStmt::Block(_) => Some(RegionKind::Sequence),
            HirStmt::Assign { .. }
            | HirStmt::Expr(_)
            | HirStmt::VaStart { .. }
            | HirStmt::Label(_)
            | HirStmt::Goto(_)
            | HirStmt::Return(_)
            | HirStmt::Break
            | HirStmt::Continue => None,
        }
    }

pub fn region_selector_or_condition(stmt: &HirStmt) -> Option<String> {
        match stmt {
            HirStmt::Switch { expr, .. } => Some(format_expr_key(expr)),
            HirStmt::If { cond, .. }
            | HirStmt::While { cond, .. }
            | HirStmt::DoWhile { cond, .. } => Some(format_expr_key(cond)),
            HirStmt::For { cond, .. } => cond.as_ref().map(format_expr_key),
            HirStmt::Block(_)
            | HirStmt::Assign { .. }
            | HirStmt::Expr(_)
            | HirStmt::VaStart { .. }
            | HirStmt::Label(_)
            | HirStmt::Goto(_)
            | HirStmt::Return(_)
            | HirStmt::Break
            | HirStmt::Continue => None,
        }
    }
