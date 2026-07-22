//! Pure driver helpers for admission and region proof scaffolding.

use crate::admission::StructuringAdmissionReason;
use crate::regions::RegionKind;
use fission_midend_core::util_dir::format_expr_key;
use fission_midend_core::ir::DirStmt;

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

pub fn is_switch_scaffold_stmt(stmt: &DirStmt) -> bool {
        match stmt {
            DirStmt::Goto(_) => true,
            DirStmt::Block(body) => body.iter().all(is_switch_scaffold_stmt),
            DirStmt::Label(_)
            | DirStmt::Assign { .. }
            | DirStmt::Expr(_)
            | DirStmt::VaStart { .. }
            | DirStmt::If { .. }
            | DirStmt::Switch { .. }
            | DirStmt::While { .. }
            | DirStmt::DoWhile { .. }
            | DirStmt::For { .. }
            | DirStmt::Return(_)
            | DirStmt::Break
            | DirStmt::Continue => false,
        }
    }

pub fn switch_stmt_has_scaffold_only_arms(stmt: &DirStmt) -> bool {
        let DirStmt::Switch { cases, default, .. } = stmt else {
            return false;
        };
        !cases.is_empty()
            && cases
                .iter()
                .all(|case| case.body.iter().all(is_switch_scaffold_stmt))
            && default.iter().all(is_switch_scaffold_stmt)
    }

pub fn region_kind_for_stmt(stmt: &DirStmt) -> Option<RegionKind> {
        match stmt {
            DirStmt::Switch { .. } => Some(RegionKind::Switch),
            DirStmt::If { .. } => Some(RegionKind::Conditional),
            DirStmt::While { .. } | DirStmt::DoWhile { .. } | DirStmt::For { .. } => {
                Some(RegionKind::Loop)
            }
            DirStmt::Block(_) => Some(RegionKind::Sequence),
            DirStmt::Assign { .. }
            | DirStmt::Expr(_)
            | DirStmt::VaStart { .. }
            | DirStmt::Label(_)
            | DirStmt::Goto(_)
            | DirStmt::Return(_)
            | DirStmt::Break
            | DirStmt::Continue => None,
        }
    }

pub fn region_selector_or_condition(stmt: &DirStmt) -> Option<String> {
        match stmt {
            DirStmt::Switch { expr, .. } => Some(format_expr_key(expr)),
            DirStmt::If { cond, .. }
            | DirStmt::While { cond, .. }
            | DirStmt::DoWhile { cond, .. } => Some(format_expr_key(cond)),
            DirStmt::For { cond, .. } => cond.as_ref().map(format_expr_key),
            DirStmt::Block(_)
            | DirStmt::Assign { .. }
            | DirStmt::Expr(_)
            | DirStmt::VaStart { .. }
            | DirStmt::Label(_)
            | DirStmt::Goto(_)
            | DirStmt::Return(_)
            | DirStmt::Break
            | DirStmt::Continue => None,
        }
    }
