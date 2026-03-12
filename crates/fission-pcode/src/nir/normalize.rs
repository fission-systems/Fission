use super::*;

pub(super) fn normalize_stmt(stmt: &mut HirStmt) {
    match stmt {
        HirStmt::Assign { rhs, .. } => normalize_expr(rhs),
        HirStmt::Expr(expr) => normalize_expr(expr),
        HirStmt::Block(stmts) => {
            for stmt in stmts {
                normalize_stmt(stmt);
            }
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            normalize_expr(cond);
            for stmt in then_body {
                normalize_stmt(stmt);
            }
            for stmt in else_body {
                normalize_stmt(stmt);
            }
        }
        HirStmt::While { cond, body } => {
            normalize_expr(cond);
            for stmt in body {
                normalize_stmt(stmt);
            }
        }
        HirStmt::DoWhile { body, cond } => {
            for stmt in body {
                normalize_stmt(stmt);
            }
            normalize_expr(cond);
        }
        HirStmt::Label(_) | HirStmt::Goto(_) => {}
        HirStmt::Return(Some(expr)) => normalize_expr(expr),
        HirStmt::Return(None) | HirStmt::Break | HirStmt::Continue => {}
    }
}

fn normalize_expr(expr: &mut HirExpr) {
    match expr {
        HirExpr::Cast { expr: inner, .. } => normalize_expr(inner),
        HirExpr::Unary { expr: inner, .. } => normalize_expr(inner),
        HirExpr::Binary { lhs, rhs, .. } => {
            normalize_expr(lhs);
            normalize_expr(rhs);
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                normalize_expr(arg);
            }
        }
        HirExpr::Load { ptr, .. } | HirExpr::PtrOffset { base: ptr, .. } => normalize_expr(ptr),
        HirExpr::Index { base, .. } => normalize_expr(base),
        HirExpr::AggregateCopy { src, .. } => normalize_expr(src),
        HirExpr::Var(_) | HirExpr::Const(_, _) => {}
    }

    let mut current = expr.clone();
    loop {
        let next = canonicalize_cast_expr(&current)
            .or_else(|| normalize_signed_power_of_two_mod(&current))
            .or_else(|| normalize_unsigned_power_of_two_mod(&current))
            .or_else(|| normalize_boolean_logic(&current))
            .or_else(|| collapse_zero_offset_cast(&current));
        match next {
            Some(next_expr) if next_expr != current => current = next_expr,
            _ => break,
        }
    }
    *expr = current;
}

fn canonicalize_cast_expr(expr: &HirExpr) -> Option<HirExpr> {
    let HirExpr::Cast { ty, expr: inner } = expr else {
        return None;
    };

    if should_preserve_non_scalar_cast(ty) {
        if let HirExpr::Cast {
            ty: inner_ty,
            expr: inner_inner,
        } = inner.as_ref()
        {
            if inner_ty == ty {
                return Some(HirExpr::Cast {
                    ty: ty.clone(),
                    expr: inner_inner.clone(),
                });
            }
        }
        return None;
    }

    let inner_ty = expr_type(inner);
    if inner_ty == *ty {
        return Some((**inner).clone());
    }

    let HirExpr::Cast {
        ty: inner_cast_ty,
        expr: inner_inner,
    } = inner.as_ref()
    else {
        return None;
    };

    if inner_cast_ty == ty {
        return Some(HirExpr::Cast {
            ty: ty.clone(),
            expr: inner_inner.clone(),
        });
    }

    if should_drop_inner_scalar_cast(ty, inner_cast_ty, &expr_type(inner_inner)) {
        return Some(HirExpr::Cast {
            ty: ty.clone(),
            expr: inner_inner.clone(),
        });
    }

    None
}

fn should_preserve_non_scalar_cast(ty: &NirType) -> bool {
    matches!(ty, NirType::Ptr(_) | NirType::Aggregate { .. } | NirType::Float { .. })
}

fn scalar_cast_signature(ty: &NirType) -> Option<(u32, bool)> {
    match ty {
        NirType::Bool => Some((1, false)),
        NirType::Int { bits, signed } => Some((*bits, *signed)),
        _ => None,
    }
}

fn source_is_scalarish(ty: &NirType) -> bool {
    matches!(ty, NirType::Unknown | NirType::Bool | NirType::Int { .. })
}

fn should_drop_inner_scalar_cast(
    outer_ty: &NirType,
    inner_ty: &NirType,
    source_ty: &NirType,
) -> bool {
    if should_preserve_non_scalar_cast(outer_ty) || should_preserve_non_scalar_cast(inner_ty) {
        return false;
    }
    let Some((outer_bits, outer_signed)) = scalar_cast_signature(outer_ty) else {
        return false;
    };
    let Some((inner_bits, inner_signed)) = scalar_cast_signature(inner_ty) else {
        return false;
    };
    if !source_is_scalarish(source_ty) {
        return false;
    }

    if outer_bits < inner_bits {
        return true;
    }

    outer_bits == inner_bits && outer_signed == inner_signed
}

fn normalize_boolean_logic(expr: &HirExpr) -> Option<HirExpr> {
    match expr {
        HirExpr::Unary {
            op: HirUnaryOp::Not,
            expr,
            ..
        } => match expr.as_ref() {
            HirExpr::Unary {
                op: HirUnaryOp::Not,
                expr: inner,
                ..
            } => Some((**inner).clone()),
            HirExpr::Binary {
                op: HirBinaryOp::LogicalAnd,
                lhs,
                rhs,
                ..
            } => Some(HirExpr::Binary {
                op: HirBinaryOp::LogicalOr,
                lhs: Box::new(negate_expr((**lhs).clone())),
                rhs: Box::new(negate_expr((**rhs).clone())),
                ty: NirType::Bool,
            }),
            HirExpr::Binary {
                op: HirBinaryOp::LogicalOr,
                lhs,
                rhs,
                ..
            } => Some(HirExpr::Binary {
                op: HirBinaryOp::LogicalAnd,
                lhs: Box::new(negate_expr((**lhs).clone())),
                rhs: Box::new(negate_expr((**rhs).clone())),
                ty: NirType::Bool,
            }),
            _ => None,
        },
        _ => None,
    }
}

fn normalize_unsigned_power_of_two_mod(expr: &HirExpr) -> Option<HirExpr> {
    let HirExpr::Binary {
        op: HirBinaryOp::And,
        lhs,
        rhs,
        ..
    } = expr
    else {
        return None;
    };
    let HirExpr::Const(
        mask,
        NirType::Int {
            bits,
            signed: false,
        },
    ) = rhs.as_ref()
    else {
        return None;
    };
    let divisor = (*mask as i128) + 1;
    if divisor <= 1 || (divisor & (divisor - 1)) != 0 {
        return None;
    }
    Some(HirExpr::Binary {
        op: HirBinaryOp::Mod,
        lhs: lhs.clone(),
        rhs: Box::new(HirExpr::Const(
            divisor as i64,
            NirType::Int {
                bits: *bits,
                signed: false,
            },
        )),
        ty: NirType::Int {
            bits: *bits,
            signed: false,
        },
    })
}

fn normalize_signed_power_of_two_mod(expr: &HirExpr) -> Option<HirExpr> {
    let HirExpr::Binary {
        op: HirBinaryOp::Sub,
        lhs,
        rhs,
        ty,
    } = expr
    else {
        return None;
    };
    let HirExpr::Binary {
        op: HirBinaryOp::Shl,
        lhs: shl_inner,
        rhs: shl_rhs,
        ..
    } = rhs.as_ref()
    else {
        return None;
    };
    let HirExpr::Const(shift_amount, _) = shl_rhs.as_ref() else {
        return None;
    };
    let HirExpr::Binary {
        op: HirBinaryOp::Sar,
        lhs: sar_inner,
        rhs: sar_rhs,
        ..
    } = shl_inner.as_ref()
    else {
        return None;
    };
    let HirExpr::Const(sar_shift, _) = sar_rhs.as_ref() else {
        return None;
    };
    if sar_shift != shift_amount {
        return None;
    }
    let HirExpr::Binary {
        op: HirBinaryOp::Add,
        lhs: add_lhs,
        rhs: add_rhs,
        ..
    } = sar_inner.as_ref()
    else {
        return None;
    };
    if add_lhs.as_ref() != lhs.as_ref() {
        return None;
    }
    let (sign_source, sign_shift, mask) = match add_rhs.as_ref() {
        HirExpr::Binary {
            op: HirBinaryOp::And,
            lhs: and_lhs,
            rhs: and_rhs,
            ..
        } => {
            let HirExpr::Binary {
                op: HirBinaryOp::Shr,
                lhs: shr_lhs,
                rhs: shr_rhs,
                ..
            } = and_lhs.as_ref()
            else {
                return None;
            };
            let HirExpr::Const(sign_shift, _) = shr_rhs.as_ref() else {
                return None;
            };
            let HirExpr::Const(mask, _) = and_rhs.as_ref() else {
                return None;
            };
            (shr_lhs.as_ref(), *sign_shift, *mask)
        }
        HirExpr::Binary {
            op: HirBinaryOp::Mod,
            lhs: mod_lhs,
            rhs: mod_rhs,
            ..
        } => {
            let HirExpr::Binary {
                op: HirBinaryOp::Shr,
                lhs: shr_lhs,
                rhs: shr_rhs,
                ..
            } = mod_lhs.as_ref()
            else {
                return None;
            };
            let HirExpr::Const(sign_shift, _) = shr_rhs.as_ref() else {
                return None;
            };
            let HirExpr::Const(divisor, _) = mod_rhs.as_ref() else {
                return None;
            };
            (shr_lhs.as_ref(), *sign_shift, *divisor - 1)
        }
        _ => return None,
    };
    if sign_source != lhs.as_ref() {
        return None;
    }

    let width = match ty {
        NirType::Int { bits, signed: true } => *bits,
        _ => 64,
    };
    let log2 = *shift_amount;
    let divisor = 1_i64 << log2;
    if sign_shift != i64::from(width.saturating_sub(1)) || mask != divisor - 1 {
        return None;
    }

    Some(HirExpr::Binary {
        op: HirBinaryOp::Mod,
        lhs: lhs.clone(),
        rhs: Box::new(HirExpr::Const(
            divisor,
            NirType::Int {
                bits: width,
                signed: true,
            },
        )),
        ty: NirType::Int {
            bits: width,
            signed: true,
        },
    })
}

fn collapse_zero_offset_cast(expr: &HirExpr) -> Option<HirExpr> {
    match expr {
        HirExpr::Load { ptr, ty } => {
            let HirExpr::PtrOffset { base, offset } = ptr.as_ref() else {
                return None;
            };
            if *offset != 0 {
                return None;
            }
            Some(HirExpr::Load {
                ptr: base.clone(),
                ty: ty.clone(),
            })
        }
        HirExpr::PtrOffset { base, offset } if *offset == 0 => Some((**base).clone()),
        HirExpr::Index {
            base,
            index,
            elem_ty,
        } if *index == 0 => Some(HirExpr::Load {
            ptr: base.clone(),
            ty: elem_ty.clone(),
        }),
        _ => None,
    }
}
