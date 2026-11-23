use super::super::for_loops::stmt_list_contains_continue_pub;
use super::indexed_cursor_rewrite::{direct_cursor_var, rewrite_cursor_body_to_index};
use super::labels::{labels_after, single_goto_target};
use super::loop_expressions::{
    index_type_for_count, is_const_val, is_zero, strip_casts, substitute_var_in_expr,
    type_size_bytes, vars_equivalent_after_casts,
};
use super::recurrences::{find_iv_update, loop_variant_vars};
use super::variable_uses::{count_var_uses, count_var_uses_in_stmt, fresh_index_name};
use crate::HashSet;
use crate::prelude::*;
use fission_midend_prehir::util::expr_type;

fn find_pointer_end_assignment_before(
    stmts: &[PreHirStmt],
    loop_idx: usize,
    cursor: &str,
    end: &str,
) -> Option<(usize, PreHirExpr)> {
    let mut scan = loop_idx;
    while scan > 0 {
        scan -= 1;
        match &stmts[scan] {
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var(name),
                rhs,
            } if name == end => {
                let mut found_count_expr = None;
                if let PreHirExpr::Binary {
                    op: PreHirBinaryOp::Add,
                    lhs,
                    rhs: add_rhs,
                    ..
                } = strip_casts(rhs)
                {
                    if matches!(strip_casts(lhs), PreHirExpr::Var(name) if name == cursor) {
                        found_count_expr = Some(add_rhs.as_ref().clone());
                    } else if matches!(strip_casts(add_rhs), PreHirExpr::Var(name) if name == cursor)
                    {
                        found_count_expr = Some(lhs.as_ref().clone());
                    }
                }

                if let Some(mut expr) = found_count_expr {
                    let mut backtrack = scan;
                    while backtrack > 0 {
                        backtrack -= 1;
                        if let PreHirStmt::Assign {
                            lhs: PreHirLValue::Var(v),
                            rhs: val,
                        } = &stmts[backtrack]
                        {
                            if count_var_uses(&expr, v) > 0 {
                                substitute_var_in_expr(&mut expr, v, val);
                            }
                        } else if matches!(
                            &stmts[backtrack],
                            PreHirStmt::Label(_)
                                | PreHirStmt::Goto(_)
                                | PreHirStmt::While { .. }
                                | PreHirStmt::DoWhile { .. }
                                | PreHirStmt::For { .. }
                                | PreHirStmt::Switch { .. }
                                | PreHirStmt::Block(_)
                                | PreHirStmt::Return(_)
                                | PreHirStmt::Break
                                | PreHirStmt::Continue
                        ) {
                            break;
                        }
                    }
                    return Some((scan, expr));
                }
                return None;
            }
            PreHirStmt::Label(_)
            | PreHirStmt::Goto(_)
            | PreHirStmt::While { .. }
            | PreHirStmt::DoWhile { .. }
            | PreHirStmt::For { .. }
            | PreHirStmt::Switch { .. }
            | PreHirStmt::Expr(_)
            | PreHirStmt::Return(_)
            | PreHirStmt::Break
            | PreHirStmt::Continue => break,
            PreHirStmt::If { .. } | PreHirStmt::Assign { .. } | PreHirStmt::VaStart { .. } => {}
            PreHirStmt::Block(_) => break,
        }
    }
    None
}

fn positive_count_loop_cmp(cond: &PreHirExpr, count: &PreHirExpr) -> Option<PreHirBinaryOp> {
    let PreHirExpr::Binary { op, lhs, rhs, .. } = cond else {
        return None;
    };
    match op {
        PreHirBinaryOp::Le | PreHirBinaryOp::SLe => (vars_equivalent_after_casts(lhs, count)
            && is_zero(rhs))
        .then_some(if matches!(op, PreHirBinaryOp::SLe) {
            PreHirBinaryOp::SLt
        } else {
            PreHirBinaryOp::Lt
        }),
        PreHirBinaryOp::Ge | PreHirBinaryOp::SGe => (is_zero(lhs)
            && vars_equivalent_after_casts(rhs, count))
        .then_some(if matches!(op, PreHirBinaryOp::SGe) {
            PreHirBinaryOp::SLt
        } else {
            PreHirBinaryOp::Lt
        }),
        _ => None,
    }
}

fn positive_count_entry_guard_cmp(
    stmts: &[PreHirStmt],
    loop_idx: usize,
    count: &PreHirExpr,
    after_labels: &HashSet<String>,
) -> Option<PreHirBinaryOp> {
    stmts[..loop_idx].iter().find_map(|stmt| {
        let PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } = stmt
        else {
            return None;
        };
        if !else_body.is_empty() {
            return None;
        }
        let exits_before_loop = single_goto_target(then_body)
            .is_some_and(|label| after_labels.contains(label))
            || matches!(then_body.as_slice(), [PreHirStmt::Return(Some(expr))] if is_zero(expr));
        if exits_before_loop {
            positive_count_loop_cmp(cond, count)
        } else {
            None
        }
    })
}

fn pointer_cursor_condition(cond: &PreHirExpr) -> Option<(&str, &str)> {
    let PreHirExpr::Binary {
        op: PreHirBinaryOp::Ne,
        lhs,
        rhs,
        ..
    } = cond
    else {
        return None;
    };
    match (lhs.as_ref(), rhs.as_ref()) {
        (PreHirExpr::Var(cursor), PreHirExpr::Var(end)) => Some((cursor.as_str(), end.as_str())),
        _ => None,
    }
}

fn cursor_used_after_loop(stmts: &[PreHirStmt], loop_idx: usize, cursor: &str) -> bool {
    stmts
        .iter()
        .skip(loop_idx + 1)
        .any(|stmt| count_var_uses_in_stmt(stmt, cursor) > 0)
}

fn is_cursor_increment_by_one(stmt: &PreHirStmt, cursor: &str) -> bool {
    let PreHirStmt::Assign {
        lhs: PreHirLValue::Var(lhs),
        rhs,
    } = stmt
    else {
        return false;
    };
    if lhs != cursor {
        return false;
    }
    let PreHirExpr::Binary {
        op: PreHirBinaryOp::Add,
        lhs,
        rhs,
        ty,
    } = strip_casts(rhs)
    else {
        return false;
    };
    let element_size = match ty {
        NirType::Ptr(pointee) => type_size_bytes(pointee) as i64,
        _ => 1,
    };
    (direct_cursor_var(lhs, cursor) && (is_const_val(rhs, element_size) || is_const_val(rhs, 1)))
        || (direct_cursor_var(rhs, cursor)
            && (is_const_val(lhs, element_size) || is_const_val(lhs, 1)))
}

fn extract_pointer_cursor_and_count(
    cond: &PreHirExpr,
    stmts: &[PreHirStmt],
    loop_idx: usize,
) -> Option<(String, PreHirExpr, Option<usize>)> {
    let PreHirExpr::Binary {
        op: PreHirBinaryOp::Ne,
        lhs,
        rhs,
        ..
    } = cond
    else {
        return None;
    };

    let match_addition = |expr: &PreHirExpr, cursor: &str| -> Option<PreHirExpr> {
        let stripped = strip_casts(expr);
        if let PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs: add_lhs,
            rhs: add_rhs,
            ..
        } = stripped
        {
            if matches!(strip_casts(add_lhs.as_ref()), PreHirExpr::Var(name) if name == cursor) {
                return Some(*add_rhs.clone());
            }
            if matches!(strip_casts(add_rhs.as_ref()), PreHirExpr::Var(name) if name == cursor) {
                return Some(*add_lhs.clone());
            }
        }
        None
    };

    if let PreHirExpr::Var(cursor) = strip_casts(lhs.as_ref()) {
        if let Some(count_expr) = match_addition(rhs.as_ref(), cursor) {
            return Some((cursor.clone(), count_expr, None));
        }
    }
    if let PreHirExpr::Var(cursor) = strip_casts(rhs.as_ref()) {
        if let Some(count_expr) = match_addition(lhs.as_ref(), cursor) {
            return Some((cursor.clone(), count_expr, None));
        }
    }

    match (strip_casts(lhs.as_ref()), strip_casts(rhs.as_ref())) {
        (PreHirExpr::Var(cursor), PreHirExpr::Var(end)) => {
            let (scan_idx, count_expr) =
                find_pointer_end_assignment_before(stmts, loop_idx, cursor, end)?;
            Some((cursor.clone(), count_expr, Some(scan_idx)))
        }
        _ => None,
    }
}

pub(super) fn try_guarded_dowhile_pointer_iv_upgrade(
    stmts: &mut [PreHirStmt],
    locals: &mut Vec<PreHirBinding>,
    loop_idx: usize,
    active_guards: &[PreHirExpr],
) -> bool {
    let (cond, body) = {
        let PreHirStmt::DoWhile { cond, body } = &stmts[loop_idx] else {
            return false;
        };
        (cond.clone(), body.clone())
    };

    if stmt_list_contains_continue_pub(&body) {
        return false;
    }

    let Some((cursor_str, count_expr, end_ptr_idx_opt)) =
        extract_pointer_cursor_and_count(&cond, stmts, loop_idx)
    else {
        return false;
    };
    let cursor = &cursor_str;
    let loop_variant = loop_variant_vars(&body);
    let Some((update_idx, true)) = find_iv_update(&body, cursor, &loop_variant) else {
        return false;
    };
    let after_labels = labels_after(stmts, loop_idx);
    let count_cmp = positive_count_entry_guard_cmp(stmts, loop_idx, &count_expr, &after_labels)
        .or_else(|| {
            if !active_guards.is_empty() {
                Some(PreHirBinaryOp::SLt)
            } else {
                None
            }
        });
    let Some(count_cmp) = count_cmp else {
        return false;
    };
    if cursor_used_after_loop(stmts, loop_idx, cursor) {
        return false;
    }

    let mut new_body = body;
    let update_stmt = std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut new_body).remove(update_idx);
    let cursor_body_uses = new_body
        .iter()
        .map(|stmt| count_var_uses_in_stmt(stmt, cursor))
        .sum::<usize>();
    let index_name = fresh_index_name(locals, stmts);
    let mut indexed_body = new_body.clone();
    if cursor_body_uses > 0
        && is_cursor_increment_by_one(&update_stmt, cursor)
        && rewrite_cursor_body_to_index(
            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut indexed_body),
            cursor,
            &index_name,
        )
    {
        let index_ty = index_type_for_count(&count_expr);
        locals.push(PreHirBinding {
            name: index_name.clone(),
            ty: index_ty.clone(),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        });
        let init_stmt = PreHirStmt::Assign {
            lhs: PreHirLValue::Var(index_name.clone()),
            rhs: PreHirExpr::Const(0, index_ty.clone()),
        };
        let cond = PreHirExpr::Binary {
            op: count_cmp,
            lhs: Box::new(PreHirExpr::Var(index_name.clone())),
            rhs: Box::new(count_expr),
            ty: NirType::Bool,
        };
        let update_stmt = PreHirStmt::Assign {
            lhs: PreHirLValue::Var(index_name.clone()),
            rhs: PreHirExpr::Binary {
                op: PreHirBinaryOp::Add,
                lhs: Box::new(PreHirExpr::Var(index_name.clone())),
                rhs: Box::new(PreHirExpr::Const(1, index_ty.clone())),
                ty: index_ty,
            },
        };
        stmts[loop_idx] = PreHirStmt::For {
            init: Some(Box::new(init_stmt)),
            cond: Some(cond),
            update: Some(Box::new(update_stmt)),
            body: indexed_body,
        };
        if let Some(end_ptr_idx) = end_ptr_idx_opt {
            stmts[end_ptr_idx] = PreHirStmt::Block(Vec::new().into());
        }
        return true;
    }

    stmts[loop_idx] = PreHirStmt::For {
        init: None,
        cond: Some(cond),
        update: Some(Box::new(update_stmt)),
        body: new_body,
    };
    if let Some(end_ptr_idx) = end_ptr_idx_opt {
        stmts[end_ptr_idx] = PreHirStmt::Block(Vec::new().into());
    }
    true
}
