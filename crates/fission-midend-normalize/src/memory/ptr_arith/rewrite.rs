use super::PointerLayout;
use super::additive_trees::try_recover_ptr_arith_tree;
use super::comparisons::{
    recover_negated_pointer_difference_condition, recover_pointer_difference_condition,
    recover_pointer_difference_zero_compare,
};
use super::field_access::{
    try_recover_aggregate_scalar_index_field_access,
    try_recover_direct_aggregate_scalar_field_access, try_recover_field_access,
};
use super::index_access::{try_recover_const_index_access, try_recover_index_access};
use super::pointer_offsets::{
    cast_pointer_operands_for_integer_arith, fold_ptr_offset_chains, try_recover_ptr_arith,
};
use crate::HashMap;
use crate::prelude::*;

/// Recursively rewrite all pointer-arithmetic sub-expressions in `expr`.
fn recover_in_expr(
    expr: &mut PreHirExpr,
    binding_types: &HashMap<String, NirType>,
    pointer_layout: PointerLayout,
) -> bool {
    // Try the top-level single-level pattern first.
    if let Some(new_expr) = try_recover_ptr_arith(expr, binding_types, pointer_layout) {
        *expr = new_expr;
        // After single-level recovery, try PtrOffset chain folding.
        fold_ptr_offset_chains(expr);
        return true;
    }
    // Try multi-level ADD tree (Ghidra AddTreeState::spanAddTree analog).
    if let Some(new_expr) = try_recover_ptr_arith_tree(expr, binding_types, pointer_layout) {
        *expr = new_expr;
        fold_ptr_offset_chains(expr);
        return true;
    }
    if let Some(new_expr) =
        cast_pointer_operands_for_integer_arith(expr, binding_types, pointer_layout)
    {
        *expr = new_expr;
        return true;
    }
    // Recurse into children.
    let mut changed = false;
    match expr {
        PreHirExpr::Binary { lhs, rhs, .. } => {
            changed |= recover_in_expr(lhs, binding_types, pointer_layout);
            changed |= recover_in_expr(rhs, binding_types, pointer_layout);
        }
        PreHirExpr::Unary { expr: inner, .. } => {
            changed |= recover_in_expr(inner, binding_types, pointer_layout);
        }
        PreHirExpr::Cast { expr: inner, .. } => {
            changed |= recover_in_expr(inner, binding_types, pointer_layout);
            // After recursing: if we now have Cast(Ptr(Int8), PtrOffset { base, .. })
            // and base is already pointer-typed, strip the cast.
            if let PreHirExpr::Cast {
                ty: NirType::Ptr(pointee),
                expr: inner2,
            } = expr
            {
                if matches!(pointee.as_ref(), NirType::Int { bits: 8, .. }) {
                    if matches!(
                        inner2.as_ref(),
                        PreHirExpr::PtrOffset { .. } | PreHirExpr::Index { .. }
                    ) {
                        let new_inner = *inner2.clone();
                        *expr = new_inner;
                        changed = true;
                    }
                }
            }
        }
        PreHirExpr::Load { ptr, ty } => {
            if let Some(field_expr) = try_recover_direct_aggregate_scalar_field_access(
                ptr,
                ty,
                binding_types,
                pointer_layout,
            ) {
                *expr = field_expr;
                return true;
            }
            if let Some(field_expr) =
                try_recover_field_access(ptr, ty, binding_types, pointer_layout)
            {
                *expr = field_expr;
                return true;
            }
            if let Some(index_expr) =
                try_recover_index_access(ptr, ty, binding_types, pointer_layout).or_else(|| {
                    try_recover_const_index_access(ptr, ty, binding_types, pointer_layout)
                })
            {
                *expr = index_expr;
                return true;
            }
            changed |= recover_in_expr(ptr, binding_types, pointer_layout);
        }
        PreHirExpr::Call { args, .. } => {
            for arg in args.iter_mut() {
                changed |= recover_in_expr(arg, binding_types, pointer_layout);
            }
        }
        PreHirExpr::PtrOffset { base, .. } => {
            // `PtrOffset.offset` is already a byte offset. Pointee refinement
            // can change the base type after this node was formed, but cannot
            // safely recover whether an earlier scalar was an element count.
            // Rescaling here therefore corrupts proven byte advances whenever
            // the new pointee is wider than the offset.
            changed |= recover_in_expr(base, binding_types, pointer_layout);
        }
        PreHirExpr::FieldAccess { base, .. } => {
            changed |= recover_in_expr(base, binding_types, pointer_layout);
        }
        PreHirExpr::Index { base, index, .. } => {
            changed |= recover_in_expr(base, binding_types, pointer_layout);
            changed |= recover_in_expr(index, binding_types, pointer_layout);
            if let PreHirExpr::Index {
                base,
                index,
                elem_ty,
            } = expr
                && let Some(field_expr) = try_recover_aggregate_scalar_index_field_access(
                    base,
                    index,
                    elem_ty,
                    binding_types,
                    pointer_layout,
                )
            {
                *expr = field_expr;
                return true;
            }
        }
        PreHirExpr::AggregateCopy { src, .. } => {
            changed |= recover_in_expr(src, binding_types, pointer_layout);
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            changed |= recover_in_expr(cond, binding_types, pointer_layout);
            changed |= recover_in_expr(then_expr, binding_types, pointer_layout);
            changed |= recover_in_expr(else_expr, binding_types, pointer_layout);
        }
        PreHirExpr::Var(_)
        | PreHirExpr::AddressOfGlobal(_)
        | PreHirExpr::AddressOfLocal(_)
        | PreHirExpr::Const(_, _) => {}
    }
    changed
}

fn recover_in_lvalue(
    lhs: &mut PreHirLValue,
    binding_types: &HashMap<String, NirType>,
    pointer_layout: PointerLayout,
) -> bool {
    match lhs {
        PreHirLValue::Deref { ptr, ty } => {
            if let Some(PreHirExpr::FieldAccess {
                base,
                field_name,
                offset,
                ty,
            }) = try_recover_direct_aggregate_scalar_field_access(
                ptr,
                ty,
                binding_types,
                pointer_layout,
            ) {
                *lhs = PreHirLValue::FieldAccess {
                    base,
                    field_name,
                    offset,
                    ty,
                };
                return true;
            }
            if let Some(field_expr) =
                try_recover_field_access(ptr, ty, binding_types, pointer_layout)
            {
                let PreHirExpr::FieldAccess {
                    base,
                    field_name,
                    offset,
                    ty: f_ty,
                } = field_expr
                else {
                    unreachable!()
                };
                *lhs = PreHirLValue::FieldAccess {
                    base,
                    field_name,
                    offset,
                    ty: f_ty,
                };
                true
            } else if let Some(PreHirExpr::Index {
                base,
                index,
                elem_ty,
            }) = try_recover_index_access(ptr, ty, binding_types, pointer_layout)
                .or_else(|| try_recover_const_index_access(ptr, ty, binding_types, pointer_layout))
            {
                *lhs = PreHirLValue::Index {
                    base,
                    index,
                    elem_ty,
                };
                true
            } else {
                recover_in_expr(ptr, binding_types, pointer_layout)
            }
        }
        PreHirLValue::Index {
            base,
            index,
            elem_ty,
        } => {
            let changed = recover_in_expr(base, binding_types, pointer_layout)
                | recover_in_expr(index, binding_types, pointer_layout);
            if let Some(PreHirExpr::FieldAccess {
                base,
                field_name,
                offset,
                ty,
            }) = try_recover_aggregate_scalar_index_field_access(
                base,
                index,
                elem_ty,
                binding_types,
                pointer_layout,
            ) {
                *lhs = PreHirLValue::FieldAccess {
                    base,
                    field_name,
                    offset,
                    ty,
                };
                true
            } else {
                changed
            }
        }
        PreHirLValue::Var(_) => false,
        PreHirLValue::FieldAccess { base, .. } => {
            recover_in_expr(base, binding_types, pointer_layout)
        }
    }
}

pub(super) fn recover_in_stmts(
    stmts: &mut Vec<PreHirStmt>,
    binding_types: &HashMap<String, NirType>,
    pointer_layout: PointerLayout,
) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        changed |= recover_in_stmt(stmt, binding_types, pointer_layout);
    }
    changed
}

fn recover_in_stmt(
    stmt: &mut PreHirStmt,
    binding_types: &HashMap<String, NirType>,
    pointer_layout: PointerLayout,
) -> bool {
    let mut changed = false;
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            changed |= recover_in_lvalue(lhs, binding_types, pointer_layout);
            changed |= recover_in_expr(rhs, binding_types, pointer_layout);
        }
        PreHirStmt::Expr(expr) => {
            changed |= recover_in_expr(expr, binding_types, pointer_layout);
        }
        PreHirStmt::Block(body) => {
            changed |= recover_in_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                binding_types,
                pointer_layout,
            );
        }
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            changed |= recover_condition_expr(cond, binding_types, pointer_layout);
            changed |= recover_in_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                binding_types,
                pointer_layout,
            );
            changed |= recover_in_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                binding_types,
                pointer_layout,
            );
        }
        PreHirStmt::While { cond, body } => {
            changed |= recover_condition_expr(cond, binding_types, pointer_layout);
            changed |= recover_in_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                binding_types,
                pointer_layout,
            );
        }
        PreHirStmt::DoWhile { body, cond } => {
            changed |= recover_in_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                binding_types,
                pointer_layout,
            );
            changed |= recover_condition_expr(cond, binding_types, pointer_layout);
        }
        PreHirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(i) = init {
                changed |= recover_in_stmt(i, binding_types, pointer_layout);
            }
            if let Some(c) = cond {
                changed |= recover_condition_expr(c, binding_types, pointer_layout);
            }
            if let Some(u) = update {
                changed |= recover_in_stmt(u, binding_types, pointer_layout);
            }
            changed |= recover_in_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                binding_types,
                pointer_layout,
            );
        }
        PreHirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            changed |= recover_in_expr(expr, binding_types, pointer_layout);
            for case in cases.iter_mut() {
                changed |= recover_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                    binding_types,
                    pointer_layout,
                );
            }
            changed |= recover_in_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                binding_types,
                pointer_layout,
            );
        }
        PreHirStmt::Return(Some(expr)) => {
            changed |= recover_in_expr(expr, binding_types, pointer_layout);
        }
        _ => {}
    }
    changed
}

fn recover_condition_expr(
    expr: &mut PreHirExpr,
    binding_types: &HashMap<String, NirType>,
    pointer_layout: PointerLayout,
) -> bool {
    let mut changed = recover_in_expr(expr, binding_types, pointer_layout);
    if let Some(new_expr) = recover_pointer_difference_condition(expr, binding_types)
        .or_else(|| recover_pointer_difference_zero_compare(expr, binding_types))
        .or_else(|| recover_negated_pointer_difference_condition(expr, binding_types))
    {
        *expr = new_expr;
        changed = true;
    }
    changed
}
