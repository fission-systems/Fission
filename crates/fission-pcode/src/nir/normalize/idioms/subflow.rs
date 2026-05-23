use super::super::*;
use std::collections::HashMap;

/// Optimizes redundant bit-widths, casts, and bitmasks in HIR expressions.
/// Drawn from Ghidra's `subflow.cc` active bit analysis and bitstream pruning.
pub(crate) fn apply_subflow_pruning(func: &mut HirFunction) -> bool {
    let mut type_map = HashMap::new();
    for binding in func.params.iter().chain(func.locals.iter()) {
        type_map.insert(binding.name.clone(), binding.ty.clone());
    }

    let mut changed = false;
    let mut round = 0;
    // Walk the HIR tree to a fixed point (typically 1 or 2 rounds).
    while round < 3 {
        let round_changed = optimize_stmts(&mut func.body, &type_map);
        if !round_changed {
            break;
        }
        changed = true;
        round += 1;
    }
    changed
}

/// Recursively evaluates the conservative mask of possible active bits for an expression.
/// If a bit is 0 in the returned mask, it is guaranteed to be 0 at runtime.
fn active_bits(expr: &HirExpr, type_map: &HashMap<String, NirType>) -> u64 {
    match expr {
        HirExpr::Const(val, _) => *val as u64,
        HirExpr::Var(name) => {
            if let Some(ty) = type_map.get(name) {
                type_mask(ty)
            } else {
                u64::MAX
            }
        }
        HirExpr::Cast { ty, expr } => {
            let outer_mask = type_mask(ty);
            let inner_ty = get_expr_type(expr, type_map);
            if let NirType::Int { bits: inner_bits, signed: true } = inner_ty {
                if let NirType::Int { bits: outer_bits, .. } = ty {
                    if *outer_bits > inner_bits {
                        // Sign-extension: upper bits might be set if value is negative.
                        return outer_mask;
                    }
                }
            }
            let inner_active = active_bits(expr, type_map);
            inner_active & outer_mask
        }
        HirExpr::Binary { op, lhs, rhs, ty } => {
            match op {
                HirBinaryOp::And => {
                    active_bits(lhs, type_map) & active_bits(rhs, type_map)
                }
                HirBinaryOp::Or | HirBinaryOp::Xor => {
                    active_bits(lhs, type_map) | active_bits(rhs, type_map)
                }
                HirBinaryOp::Shr | HirBinaryOp::Sar => {
                    if let HirExpr::Const(shift, _) = &**rhs {
                        if *shift < 64 {
                            active_bits(lhs, type_map) >> shift
                        } else {
                            type_mask(ty)
                        }
                    } else {
                        type_mask(ty)
                    }
                }
                HirBinaryOp::Shl => {
                    if let HirExpr::Const(shift, _) = &**rhs {
                        if *shift < 64 {
                            let inner = active_bits(lhs, type_map);
                            let mask = type_mask(ty);
                            (inner << shift) & mask
                        } else {
                            type_mask(ty)
                        }
                    } else {
                        type_mask(ty)
                    }
                }
                _ => type_mask(ty),
            }
        }
        _ => {
            let ty = expr_type(expr);
            type_mask(&ty)
        }
    }
}

/// Returns the maximum possible bitmask for a given NirType.
fn type_mask(ty: &NirType) -> u64 {
    match ty {
        NirType::Bool => 1,
        NirType::Int { bits, .. } => {
            if *bits >= 64 {
                u64::MAX
            } else {
                (1u64 << bits) - 1
            }
        }
        _ => u64::MAX,
    }
}

/// Helper to get the type of an expression, checking the local/parameter binding map for variables.
fn get_expr_type(expr: &HirExpr, type_map: &HashMap<String, NirType>) -> NirType {
    match expr {
        HirExpr::Var(name) => type_map.get(name).cloned().unwrap_or(NirType::Unknown),
        _ => expr_type(expr),
    }
}

/// Returns the scalar bit-width of integer and boolean types.
fn scalar_bit_width(ty: &NirType) -> Option<u32> {
    match ty {
        NirType::Bool => Some(1),
        NirType::Int { bits, .. } => Some(*bits),
        _ => None,
    }
}

/// Recursively optimizes expressions in HIR statements.
fn optimize_stmts(stmts: &mut [HirStmt], type_map: &HashMap<String, NirType>) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        changed |= optimize_stmt(stmt, type_map);
    }
    changed
}

fn optimize_stmt(stmt: &mut HirStmt, type_map: &HashMap<String, NirType>) -> bool {
    let mut changed = false;
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            changed |= optimize_lvalue(lhs, type_map);
            changed |= optimize_expr(rhs, type_map);
        }
        HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => {
            changed |= optimize_expr(expr, type_map);
        }
        HirStmt::VaStart { va_list, .. } => {
            changed |= optimize_expr(va_list, type_map);
        }
        HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            changed |= optimize_stmts(body, type_map);
        }
        HirStmt::For { init, cond, update, body } => {
            if let Some(i) = init {
                changed |= optimize_stmt(i.as_mut(), type_map);
            }
            if let Some(c) = cond {
                changed |= optimize_expr(c, type_map);
            }
            if let Some(u) = update {
                changed |= optimize_stmt(u.as_mut(), type_map);
            }
            changed |= optimize_stmts(body, type_map);
        }
        HirStmt::If { cond, then_body, else_body } => {
            changed |= optimize_expr(cond, type_map);
            changed |= optimize_stmts(then_body, type_map);
            changed |= optimize_stmts(else_body, type_map);
        }
        HirStmt::Switch { expr, cases, default } => {
            changed |= optimize_expr(expr, type_map);
            for case in cases {
                changed |= optimize_stmts(&mut case.body, type_map);
            }
            changed |= optimize_stmts(default, type_map);
        }
        HirStmt::Return(None) | HirStmt::Label(_) | HirStmt::Goto(_) | HirStmt::Break | HirStmt::Continue => {}
    }
    changed
}

fn optimize_lvalue(lhs: &mut HirLValue, type_map: &HashMap<String, NirType>) -> bool {
    let mut changed = false;
    match lhs {
        HirLValue::Var(_) => {}
        HirLValue::Deref { ptr, .. } => {
            changed |= optimize_expr(ptr, type_map);
        }
        HirLValue::Index { base, index, .. } => {
            changed |= optimize_expr(base, type_map);
            changed |= optimize_expr(index, type_map);
        }
    }
    changed
}

fn optimize_expr(expr: &mut HirExpr, type_map: &HashMap<String, NirType>) -> bool {
    let mut changed = false;

    // 1. Optimize children first (bottom-up)
    match expr {
        HirExpr::Cast { expr: inner, .. }
        | HirExpr::Unary { expr: inner, .. }
        | HirExpr::Load { ptr: inner, .. }
        | HirExpr::PtrOffset { base: inner, .. }
        | HirExpr::AggregateCopy { src: inner, .. } => {
            changed |= optimize_expr(inner, type_map);
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            changed |= optimize_expr(lhs, type_map);
            changed |= optimize_expr(rhs, type_map);
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                changed |= optimize_expr(arg, type_map);
            }
        }
        HirExpr::Index { base, index, .. } => {
            changed |= optimize_expr(base, type_map);
            changed |= optimize_expr(index, type_map);
        }
        HirExpr::Select { cond, then_expr, else_expr, .. } => {
            changed |= optimize_expr(cond, type_map);
            changed |= optimize_expr(then_expr, type_map);
            changed |= optimize_expr(else_expr, type_map);
        }
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => {}
    }

    // 2. Apply current-level optimizations
    // (A) Constant folding of Cast: (T)0x1234 -> 0x1234 (with T)
    if let HirExpr::Cast { ty, expr: inner } = expr {
        if let HirExpr::Const(val, _) = &**inner {
            let mask = type_mask(ty);
            let folded_val = *val & (mask as i64);
            *expr = HirExpr::Const(folded_val, ty.clone());
            return true;
        }
    }

    // (B) Redundant double casts: (T_outer)(T_inner)val -> (T_outer)val
    if let HirExpr::Cast { ty: outer_ty, expr: inner_cast } = expr {
        if let HirExpr::Cast { ty: inner_ty, expr: val } = &mut **inner_cast {
            if let (Some(outer_bits), Some(inner_bits)) = (scalar_bit_width(outer_ty), scalar_bit_width(inner_ty)) {
                let val_ty = get_expr_type(val, type_map);
                let val_bits = scalar_bit_width(&val_ty).unwrap_or(64);
                if outer_bits <= inner_bits || val_bits <= inner_bits {
                    *expr = HirExpr::Cast {
                        ty: outer_ty.clone(),
                        expr: Box::new((**val).clone()),
                    };
                    return true;
                }
            }
        }
    }

    // (C) Redundant Cast to same/wider type: (T)val -> val
    if let HirExpr::Cast { ty, expr: inner } = expr {
        let inner_ty = get_expr_type(inner, type_map);
        if *ty == inner_ty {
            *expr = (**inner).clone();
            return true;
        }
    }

    // (D) Redundant bitmask: lhs & Const(mask) -> lhs
    if let HirExpr::Binary { op: HirBinaryOp::And, lhs, rhs, .. } = expr {
        if let HirExpr::Const(mask, _) = &**rhs {
            let active = active_bits(lhs, type_map);
            if (active & !(*mask as u64)) == 0 {
                *expr = (**lhs).clone();
                return true;
            }
        } else if let HirExpr::Const(mask, _) = &**lhs {
            let active = active_bits(rhs, type_map);
            if (active & !(*mask as u64)) == 0 {
                *expr = (**rhs).clone();
                return true;
            }
        }
    }

    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u8_ty() -> NirType {
        NirType::Int { bits: 8, signed: false }
    }

    fn u32_ty() -> NirType {
        NirType::Int { bits: 32, signed: false }
    }

    fn u64_ty() -> NirType {
        NirType::Int { bits: 64, signed: false }
    }

    #[test]
    fn test_redundant_bitmask_pruning() {
        let mut type_map = HashMap::new();
        type_map.insert("x".to_string(), u8_ty());

        // x & 0xff where x is u8
        let mut expr = HirExpr::Binary {
            op: HirBinaryOp::And,
            lhs: Box::new(HirExpr::Var("x".to_string())),
            rhs: Box::new(HirExpr::Const(0xff, u8_ty())),
            ty: u8_ty(),
        };

        assert!(optimize_expr(&mut expr, &type_map));
        assert_eq!(expr, HirExpr::Var("x".to_string()));
    }

    #[test]
    fn test_redundant_double_cast() {
        let mut type_map = HashMap::new();
        type_map.insert("x".to_string(), u8_ty());

        // (u64)(u32)x where x is u8
        let mut expr = HirExpr::Cast {
            ty: u64_ty(),
            expr: Box::new(HirExpr::Cast {
                ty: u32_ty(),
                expr: Box::new(HirExpr::Var("x".to_string())),
            }),
        };

        assert!(optimize_expr(&mut expr, &type_map));
        assert_eq!(
            expr,
            HirExpr::Cast {
                ty: u64_ty(),
                expr: Box::new(HirExpr::Var("x".to_string())),
            }
        );
    }
}
