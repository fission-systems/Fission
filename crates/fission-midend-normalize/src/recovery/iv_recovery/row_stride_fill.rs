use super::super::for_loops::stmt_list_contains_continue_pub;
use super::loop_expressions::{
    index_type_for_count, strip_casts, stripped_var_name, vars_equivalent_after_casts,
};
use super::loop_latch::{parse_break_eq_var, parse_self_add_assign, tail_meaningful_stmts};
use super::recurrences::{is_loop_invariant, loop_variant_vars};
use super::variable_uses::fresh_index_name;
use crate::HashSet;
use crate::prelude::*;

/// MSVC `/O` row-fill inner loops sometimes peel vectorized prolog/epilog into
/// label/goto pointer walks with `(end - start) & align` guards.  When the outer
/// latch is already an infinite `for` with a break tail, recover:
///   `for (j = 0; j < cols; j++) base[row_offset + j] = value`.
pub(super) fn try_recover_row_stride_fill_inner_loop(
    stmts: &mut Vec<PreHirStmt>,
    loop_idx: usize,
    locals: &mut Vec<PreHirBinding>,
) -> bool {
    let body = match &stmts[loop_idx] {
        PreHirStmt::For {
            init: None,
            cond: None,
            update: None,
            body,
        } => body.clone(),
        _ => return false,
    };
    if stmt_list_contains_continue_pub(&body) {
        return false;
    }
    let tail_start = match find_outer_infinite_for_tail_start(&body) {
        Some(i) => i,
        None => return false,
    };
    let loop_variant = loop_variant_vars(&body);
    let matched = match try_parse_row_stride_fill(&body, tail_start, &loop_variant) {
        Some(m) => m,
        None => return false,
    };

    let j_name = fresh_index_name(locals, stmts);
    let j_ty = index_type_for_count(&matched.stride);
    locals.push(PreHirBinding {
        name: j_name.clone(),
        ty: j_ty.clone(),
        surface_type_name: None,
        origin: Some(NirBindingOrigin::Temp),
        initializer: None,
    });

    let index_expr = PreHirExpr::Binary {
        op: PreHirBinaryOp::Add,
        lhs: Box::new(matched.row_offset),
        rhs: Box::new(PreHirExpr::Var(j_name.clone())),
        ty: j_ty.clone(),
    };
    let store_stmt = PreHirStmt::Assign {
        lhs: PreHirLValue::Index {
            base: Box::new(matched.base),
            index: Box::new(index_expr),
            elem_ty: matched.elem_ty.clone(),
        },
        rhs: matched.fill_value,
    };
    let inner_for = PreHirStmt::For {
        init: Some(Box::new(PreHirStmt::Assign {
            lhs: PreHirLValue::Var(j_name.clone()),
            rhs: PreHirExpr::Const(0, j_ty.clone()),
        })),
        cond: Some(PreHirExpr::Binary {
            op: PreHirBinaryOp::SLt,
            lhs: Box::new(PreHirExpr::Var(j_name.clone())),
            rhs: Box::new(matched.stride),
            ty: NirType::Bool,
        }),
        update: Some(Box::new(PreHirStmt::Assign {
            lhs: PreHirLValue::Var(j_name.clone()),
            rhs: PreHirExpr::Binary {
                op: PreHirBinaryOp::Add,
                lhs: Box::new(PreHirExpr::Var(j_name.clone())),
                rhs: Box::new(PreHirExpr::Const(1, j_ty.clone())),
                ty: j_ty,
            },
        })),
        body: vec![store_stmt].into(),
    };

    let mut new_body = vec![inner_for];
    new_body.extend_from_slice(&body[matched.tail_start..]);
    stmts[loop_idx] = PreHirStmt::For {
        init: None,
        cond: None,
        update: None,
        body: new_body.into(),
    };
    true
}

struct RowStrideFillMatch {
    base: PreHirExpr,
    row_offset: PreHirExpr,
    stride: PreHirExpr,
    fill_value: PreHirExpr,
    elem_ty: NirType,
    tail_start: usize,
}

fn find_outer_infinite_for_tail_start(body: &[PreHirStmt]) -> Option<usize> {
    let tail = tail_meaningful_stmts(body, 3)?;
    parse_break_eq_var(&tail[2].1)?;
    Some(tail[0].0)
}

fn parse_ptr_base_offset(expr: &PreHirExpr) -> Option<(PreHirExpr, PreHirExpr)> {
    match strip_casts(expr) {
        PreHirExpr::PtrOffset { base, offset } => Some((
            (*base).as_ref().clone(),
            PreHirExpr::Const(
                *offset,
                NirType::Int {
                    bits: 64,
                    signed: false,
                },
            ),
        )),
        PreHirExpr::Index { base, index, .. } => {
            Some(((*base).as_ref().clone(), (*index).as_ref().clone()))
        }
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs,
            rhs,
            ..
        } => Some(((*lhs).as_ref().clone(), (*rhs).as_ref().clone())),
        _ => None,
    }
}

fn assign_rhs_ptr_base_offset(stmt: &PreHirStmt) -> Option<(String, PreHirExpr, PreHirExpr)> {
    let PreHirStmt::Assign {
        lhs: PreHirLValue::Var(name),
        rhs,
    } = stmt
    else {
        return None;
    };
    let (base, offset) = parse_ptr_base_offset(rhs)?;
    Some((name.clone(), base, offset))
}

fn is_peel_corruption_pair(sub_stmt: &PreHirStmt, mask_stmt: &PreHirStmt) -> bool {
    let Some((_, _sub_rhs)) = parse_self_sub_assign(sub_stmt) else {
        return false;
    };
    let PreHirStmt::Assign {
        lhs: PreHirLValue::Var(_),
        rhs,
    } = mask_stmt
    else {
        return false;
    };
    matches!(
        strip_casts(rhs),
        PreHirExpr::Binary {
            op: PreHirBinaryOp::And,
            rhs: mask,
            ..
        } if matches!(strip_casts(mask.as_ref()), PreHirExpr::Const(v, _) if (1..=16).contains(v))
    )
}

fn parse_self_sub_assign(stmt: &PreHirStmt) -> Option<(String, PreHirExpr)> {
    let PreHirStmt::Assign {
        lhs: PreHirLValue::Var(name),
        rhs,
    } = stmt
    else {
        return None;
    };
    let PreHirExpr::Binary {
        op: PreHirBinaryOp::Sub,
        lhs,
        rhs: subtrahend,
        ..
    } = strip_casts(rhs)
    else {
        return None;
    };
    if matches!(strip_casts(lhs.as_ref()), PreHirExpr::Var(var) if var == name) {
        Some((name.clone(), (*subtrahend).as_ref().clone()))
    } else {
        None
    }
}

fn deref_targets_var(ptr: &PreHirExpr, cursor: &str) -> bool {
    matches!(strip_casts(ptr), PreHirExpr::Var(name) if name == cursor)
}

fn find_fill_value_in_region(stmts: &[PreHirStmt], cursor: &str) -> Option<PreHirExpr> {
    for stmt in stmts {
        match stmt {
            PreHirStmt::Assign {
                lhs: PreHirLValue::Deref { ptr, .. },
                rhs,
            } if deref_targets_var(ptr, cursor) => return Some(rhs.clone()),
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                if let Some(v) = find_fill_value_in_region(then_body, cursor) {
                    return Some(v);
                }
                if let Some(v) = find_fill_value_in_region(else_body, cursor) {
                    return Some(v);
                }
            }
            PreHirStmt::Block(body) => {
                if let Some(v) = find_fill_value_in_region(body, cursor) {
                    return Some(v);
                }
            }
            _ => {}
        }
    }
    None
}

fn inner_region_has_cursor_stores(stmts: &[PreHirStmt], cursor: &str) -> bool {
    for stmt in stmts {
        match stmt {
            PreHirStmt::Assign {
                lhs: PreHirLValue::Deref { ptr, .. },
                ..
            } if deref_targets_var(ptr, cursor) => return true,
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                if inner_region_has_cursor_stores(then_body, cursor)
                    || inner_region_has_cursor_stores(else_body, cursor)
                {
                    return true;
                }
            }
            PreHirStmt::Block(body) => {
                if inner_region_has_cursor_stores(body, cursor) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

fn try_parse_row_stride_fill(
    body: &[PreHirStmt],
    tail_start: usize,
    loop_variant: &HashSet<String>,
) -> Option<RowStrideFillMatch> {
    let mut meaningful = Vec::new();
    for (idx, stmt) in body.iter().enumerate().take(tail_start) {
        if !matches!(stmt, PreHirStmt::Label(_)) {
            meaningful.push((idx, stmt));
        }
    }
    if meaningful.len() < 4 {
        return None;
    }

    let mut idx = 0;
    let mut row_offset: Option<PreHirExpr> = None;
    let mut base: Option<PreHirExpr> = None;
    let mut cursor: Option<String> = None;
    let mut stride: Option<PreHirExpr> = None;
    let mut prefix_end = 0;

    if let PreHirStmt::Assign {
        lhs: PreHirLValue::Var(_),
        rhs,
    } = meaningful[idx].1
    {
        if let Some(var) = stripped_var_name(strip_casts(rhs)) {
            row_offset = Some(PreHirExpr::Var(var.to_string()));
            prefix_end = meaningful[idx].0 + 1;
            idx += 1;
        } else if let PreHirExpr::Cast { expr, .. } = strip_casts(rhs) {
            if let Some(var) = stripped_var_name(expr) {
                row_offset = Some(PreHirExpr::Var(var.to_string()));
                prefix_end = meaningful[idx].0 + 1;
                idx += 1;
            }
        }
    }

    if idx >= meaningful.len() {
        return None;
    }
    let (cursor_name, base_expr, off_expr) = assign_rhs_ptr_base_offset(meaningful[idx].1)?;
    base = Some(base_expr);
    cursor = Some(cursor_name);
    if row_offset.is_none() {
        row_offset = Some(off_expr);
    }
    prefix_end = meaningful[idx].0 + 1;
    idx += 1;

    if idx >= meaningful.len() {
        return None;
    }
    let (_, addend) = parse_self_add_assign(meaningful[idx].1)?;
    if !is_loop_invariant(&addend, loop_variant) {
        return None;
    }
    stride = Some(addend);
    prefix_end = meaningful[idx].0 + 1;
    idx += 1;

    if idx >= meaningful.len() {
        return None;
    }
    let (_, base2, _) = assign_rhs_ptr_base_offset(meaningful[idx].1)?;
    if !vars_equivalent_after_casts(base.as_ref()?, &base2) {
        return None;
    }
    prefix_end = meaningful[idx].0 + 1;
    idx += 1;

    if idx + 1 < meaningful.len()
        && is_peel_corruption_pair(meaningful[idx].1, meaningful[idx + 1].1)
    {
        prefix_end = meaningful[idx + 1].0 + 1;
    }

    let inner_region = &body[prefix_end..tail_start];
    let fill_value = find_fill_value_in_region(inner_region, cursor.as_ref()?)?;
    if !inner_region_has_cursor_stores(inner_region, cursor.as_ref()?) {
        return None;
    }

    Some(RowStrideFillMatch {
        base: base?,
        row_offset: row_offset?,
        stride: stride?,
        fill_value,
        elem_ty: NirType::Int {
            bits: 32,
            signed: false,
        },
        tail_start,
    })
}
