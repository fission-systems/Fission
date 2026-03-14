use super::*;
use std::collections::{HashMap, HashSet};

pub(super) fn normalize_function_body(body: &mut Vec<HirStmt>) {
    for stmt in body.iter_mut() {
        normalize_stmt(stmt);
    }
    loop {
        let mut changed = false;
        changed |= collapse_trivial_assign_returns(body);
        changed |= inline_single_use_temps(body);
        if !changed {
            break;
        }
        for stmt in body.iter_mut() {
            normalize_stmt(stmt);
        }
    }
}

pub(super) fn normalize_hir_function(func: &mut HirFunction) {
    normalize_binding_initializers(&mut func.locals);
    normalize_function_body(&mut func.body);
    let allow_expensive_passes = !is_large_hir_function(func);
    let mut changed = false;
    if allow_expensive_passes {
        changed |= apply_memory_slot_surfacing(func);
        normalize_binding_initializers(&mut func.locals);
        normalize_function_body(&mut func.body);
        changed |= apply_bitstream_idioms(func);
        if changed {
            normalize_binding_initializers(&mut func.locals);
            normalize_function_body(&mut func.body);
        }
    }
}

fn is_large_hir_function(func: &HirFunction) -> bool {
    count_hir_stmts(&func.body) > 220 || func.locals.len() > 160
}

fn count_hir_stmts(stmts: &[HirStmt]) -> usize {
    fn count_stmt(stmt: &HirStmt) -> usize {
        match stmt {
            HirStmt::Block(stmts)
            | HirStmt::While { body: stmts, .. }
            | HirStmt::DoWhile { body: stmts, .. } => 1 + count_hir_stmts(stmts),
            HirStmt::Switch { cases, default, .. } => {
                1 + cases.iter().map(|case| count_hir_stmts(&case.body)).sum::<usize>()
                    + count_hir_stmts(default)
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => 1 + count_hir_stmts(then_body) + count_hir_stmts(else_body),
            _ => 1,
        }
    }

    stmts.iter().map(count_stmt).sum()
}

pub(super) fn normalize_stmt(stmt: &mut HirStmt) {
    match stmt {
        HirStmt::Assign { rhs, .. } => normalize_expr(rhs),
        HirStmt::Expr(expr) => normalize_expr(expr),
        HirStmt::Block(stmts) => {
            for stmt in stmts {
                normalize_stmt(stmt);
            }
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            normalize_expr(expr);
            for case in cases {
                for stmt in &mut case.body {
                    normalize_stmt(stmt);
                }
            }
            for stmt in default {
                normalize_stmt(stmt);
            }
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            normalize_condition_expr(cond);
            for stmt in then_body {
                normalize_stmt(stmt);
            }
            for stmt in else_body {
                normalize_stmt(stmt);
            }
        }
        HirStmt::While { cond, body } => {
            normalize_condition_expr(cond);
            for stmt in body {
                normalize_stmt(stmt);
            }
        }
        HirStmt::DoWhile { body, cond } => {
            for stmt in body {
                normalize_stmt(stmt);
            }
            normalize_condition_expr(cond);
        }
        HirStmt::Label(_) | HirStmt::Goto(_) => {}
        HirStmt::Return(Some(expr)) => normalize_expr(expr),
        HirStmt::Return(None) | HirStmt::Break | HirStmt::Continue => {}
    }
}

fn normalize_condition_expr(expr: &mut HirExpr) {
    normalize_expr(expr);
    let mut current = expr.clone();
    loop {
        let next = canonicalize_condition_expr(&current);
        match next {
            Some(next_expr) if next_expr != current => {
                current = next_expr;
                normalize_expr(&mut current);
            }
            _ => break,
        }
    }
    *expr = current;
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
        HirExpr::Index { base, index, .. } => {
            normalize_expr(base);
            normalize_expr(index);
        }
        HirExpr::AggregateCopy { src, .. } => normalize_expr(src),
        HirExpr::Var(_) | HirExpr::Const(_, _) => {}
    }

    let mut current = expr.clone();
    loop {
        let next = canonicalize_integer_expr(&current)
            .or_else(|| recognize_mod_div_power_of_two(&current))
            .or_else(|| recognize_hi_lo_extract(&current))
            .or_else(|| recognize_wide_integer_recombine(&current))
            .or_else(|| normalize_boolean_logic(&current))
            .or_else(|| cleanup_arithmetic_wrappers(&current))
            .or_else(|| collapse_zero_offset_cast(&current));
        match next {
            Some(next_expr) if next_expr != current => current = next_expr,
            _ => break,
        }
    }
    *expr = current;
}

fn canonicalize_integer_expr(expr: &HirExpr) -> Option<HirExpr> {
    canonicalize_cast_expr(expr)
}

fn recognize_mod_div_power_of_two(expr: &HirExpr) -> Option<HirExpr> {
    normalize_signed_power_of_two_mod(expr)
        .or_else(|| normalize_unsigned_power_of_two_mod(expr))
        .or_else(|| normalize_signed_power_of_two_div(expr))
        .or_else(|| normalize_unsigned_power_of_two_div(expr))
}

fn recognize_hi_lo_extract(expr: &HirExpr) -> Option<HirExpr> {
    match expr {
        HirExpr::Cast { ty, expr: inner } if is_integer_type(ty) => match inner.as_ref() {
            HirExpr::Binary {
                op: HirBinaryOp::And,
                lhs,
                rhs,
                ..
            } => {
                let HirExpr::Const(mask, _) = rhs.as_ref() else {
                    return None;
                };
                let mask_limit = full_mask_for_type(ty)?;
                if *mask == mask_limit {
                    return Some(HirExpr::Cast {
                        ty: ty.clone(),
                        expr: lhs.clone(),
                    });
                }
                None
            }
            HirExpr::Binary {
                op: HirBinaryOp::Shr | HirBinaryOp::Sar,
                lhs,
                rhs,
                ..
            } => {
                let HirExpr::Const(shift, _) = rhs.as_ref() else {
                    return None;
                };
                let inner_ty = expr_type(lhs);
                let Some(target_bits) = int_type_bits(ty) else {
                    return None;
                };
                let Some(source_bits) = int_type_bits(&inner_ty) else {
                    return None;
                };
                if *shift == i64::from(source_bits.saturating_sub(target_bits)) {
                    Some(HirExpr::Cast {
                        ty: ty.clone(),
                        expr: Box::new(HirExpr::Binary {
                            op: HirBinaryOp::Shr,
                            lhs: lhs.clone(),
                            rhs: rhs.clone(),
                            ty: inner_ty,
                        }),
                    })
                } else {
                    None
                }
            }
            _ => None,
        },
        HirExpr::Binary {
            op: HirBinaryOp::And,
            lhs,
            rhs,
            ty,
        } if is_integer_type(ty) => {
            let HirExpr::Const(mask, _) = rhs.as_ref() else {
                return None;
            };
            let mask_limit = full_mask_for_type(ty)?;
            if *mask != mask_limit {
                return None;
            }
            Some(HirExpr::Cast {
                ty: ty.clone(),
                expr: lhs.clone(),
            })
        }
        _ => None,
    }
}

fn recognize_wide_integer_recombine(expr: &HirExpr) -> Option<HirExpr> {
    let HirExpr::Binary {
        op: HirBinaryOp::Or,
        lhs,
        rhs,
        ty,
    } = expr
    else {
        return None;
    };
    let HirExpr::Binary {
        op: HirBinaryOp::Shl,
        lhs: hi_expr,
        rhs: hi_shift,
        ..
    } = lhs.as_ref()
    else {
        return None;
    };
    let HirExpr::Const(shift_amount, _) = hi_shift.as_ref() else {
        return None;
    };
    let Some(total_bits) = int_type_bits(ty) else {
        return None;
    };
    let high = extract_high_part(hi_expr, *shift_amount, total_bits)?;
    let low = extract_low_part(rhs, *shift_amount)?;
    if high.source != low.source || high.width_bits != low.width_bits || high.shift_bits != low.shift_bits {
        return None;
    }
    let source_ty = expr_type(&high.source);
    if source_ty == *ty {
        Some(high.source)
    } else if matches!(source_ty, NirType::Unknown) {
        Some(HirExpr::Cast {
            ty: ty.clone(),
            expr: Box::new(high.source),
        })
    } else {
        None
    }
}

#[derive(Clone)]
struct WidePart {
    source: HirExpr,
    width_bits: u32,
    shift_bits: i64,
}

fn extract_high_part(expr: &HirExpr, shift_amount: i64, total_bits: u32) -> Option<WidePart> {
    let HirExpr::Cast { ty, expr: inner } = expr else {
        return None;
    };
    let HirExpr::Binary {
        op: HirBinaryOp::Shr | HirBinaryOp::Sar,
        lhs,
        rhs,
        ..
    } = inner.as_ref()
    else {
        return None;
    };
    let HirExpr::Const(inner_shift, _) = rhs.as_ref() else {
        return None;
    };
    if *inner_shift != shift_amount {
        return None;
    }
    let width_bits = int_type_bits(ty)?;
    if shift_amount != i64::from(total_bits.saturating_sub(width_bits)) {
        return None;
    }
    Some(WidePart {
        source: (**lhs).clone(),
        width_bits,
        shift_bits: shift_amount,
    })
}

fn extract_low_part(expr: &HirExpr, shift_amount: i64) -> Option<WidePart> {
    match expr {
        HirExpr::Cast { ty, expr: inner } => {
            let width_bits = int_type_bits(ty)?;
            Some(WidePart {
                source: (**inner).clone(),
                width_bits,
                shift_bits: shift_amount,
            })
        }
        HirExpr::Binary {
            op: HirBinaryOp::And,
            lhs,
            rhs,
            ..
        } => {
            let HirExpr::Const(mask, _) = rhs.as_ref() else {
                return None;
            };
            let width_bits = shift_amount as u32;
            let expected_mask = full_mask_for_bits(width_bits)?;
            if *mask != expected_mask {
                return None;
            }
            Some(WidePart {
                source: (**lhs).clone(),
                width_bits,
                shift_bits: shift_amount,
            })
        }
        HirExpr::Binary {
            op: HirBinaryOp::Mod,
            lhs,
            rhs,
            ..
        } => {
            let HirExpr::Const(modulus, _) = rhs.as_ref() else {
                return None;
            };
            let width_bits = shift_amount as u32;
            let expected_modulus = 1i64.checked_shl(width_bits)?;
            if *modulus != expected_modulus {
                return None;
            }
            Some(WidePart {
                source: (**lhs).clone(),
                width_bits,
                shift_bits: shift_amount,
            })
        }
        _ => None,
    }
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
        HirExpr::Binary {
            op: HirBinaryOp::Ne,
            lhs,
            rhs,
            ..
        } if is_zero_const(rhs.as_ref()) && matches!(expr_type(lhs), NirType::Bool) => {
            Some((**lhs).clone())
        }
        HirExpr::Binary {
            op: HirBinaryOp::Eq,
            lhs,
            rhs,
            ..
        } if is_zero_const(rhs.as_ref()) && matches!(expr_type(lhs), NirType::Bool) => {
            Some(negate_expr((**lhs).clone()))
        }
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

fn canonicalize_condition_expr(expr: &HirExpr) -> Option<HirExpr> {
    match expr {
        HirExpr::Binary {
            op: HirBinaryOp::Ne,
            lhs,
            rhs,
            ..
        } if is_zero_const(rhs.as_ref()) && is_truthy_condition_type(&expr_type(lhs)) => {
            Some((**lhs).clone())
        }
        HirExpr::Binary {
            op: HirBinaryOp::Eq,
            lhs,
            rhs,
            ..
        } if is_zero_const(rhs.as_ref()) && is_truthy_condition_type(&expr_type(lhs)) => {
            Some(negate_expr((**lhs).clone()))
        }
        _ => None,
    }
}

fn is_truthy_condition_type(ty: &NirType) -> bool {
    matches!(ty, NirType::Unknown | NirType::Bool | NirType::Int { .. } | NirType::Ptr(_))
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
    if is_full_mask_const(rhs.as_ref(), &expr_type(lhs)) {
        return None;
    }
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

fn normalize_unsigned_power_of_two_div(expr: &HirExpr) -> Option<HirExpr> {
    let HirExpr::Binary {
        op: HirBinaryOp::Shr,
        lhs,
        rhs,
        ty,
    } = expr
    else {
        return None;
    };
    let HirExpr::Const(shift_amount, _) = rhs.as_ref() else {
        return None;
    };
    let width = match ty {
        NirType::Int {
            bits,
            signed: false,
        } => *bits,
        _ => return None,
    };
    match expr_type(lhs) {
        NirType::Int {
            bits,
            signed: false,
        } if bits == width => {}
        NirType::Unknown => {}
        _ => return None,
    }
    if *shift_amount < 0 || *shift_amount >= i64::from(width) {
        return None;
    }
    if *shift_amount == i64::from(width.saturating_sub(1)) {
        return None;
    }
    if (*shift_amount as u32) * 2 >= width && *shift_amount % 8 == 0 {
        return None;
    }
    let divisor = 1_i64.checked_shl(*shift_amount as u32)?;
    Some(HirExpr::Binary {
        op: HirBinaryOp::Div,
        lhs: lhs.clone(),
        rhs: Box::new(HirExpr::Const(
            divisor,
            NirType::Int {
                bits: width,
                signed: false,
            },
        )),
        ty: NirType::Int {
            bits: width,
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
    if let HirExpr::Binary {
        op: HirBinaryOp::Shl,
        lhs: shl_inner,
        rhs: shl_rhs,
        ..
    } = rhs.as_ref()
    {
        let HirExpr::Const(shift_amount, _) = shl_rhs.as_ref() else {
            return None;
        };
        let HirExpr::Binary {
            op: HirBinaryOp::Div,
            lhs: div_lhs,
            rhs: div_rhs,
            ..
        } = shl_inner.as_ref()
        else {
            return None;
        };
        let HirExpr::Const(divisor, _) = div_rhs.as_ref() else {
            return None;
        };
        if div_lhs.as_ref() == lhs.as_ref()
            && *divisor > 1
            && (*divisor & (*divisor - 1)) == 0
            && *divisor == (1_i64.checked_shl(*shift_amount as u32)?)
        {
            let width = match ty {
                NirType::Int { bits, signed: true } => *bits,
                _ => 64,
            };
            return Some(HirExpr::Binary {
                op: HirBinaryOp::Mod,
                lhs: lhs.clone(),
                rhs: Box::new(HirExpr::Const(
                    *divisor,
                    NirType::Int {
                        bits: width,
                        signed: true,
                    },
                )),
                ty: NirType::Int {
                    bits: width,
                    signed: true,
                },
            });
        }
    }
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
    let divisor = 1_i64.checked_shl(*shift_amount as u32)?;
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

fn normalize_signed_power_of_two_div(expr: &HirExpr) -> Option<HirExpr> {
    let HirExpr::Binary {
        op: HirBinaryOp::Sar,
        lhs,
        rhs,
        ty,
    } = expr
    else {
        return None;
    };
    let HirExpr::Const(shift_amount, _) = rhs.as_ref() else {
        return None;
    };
    let HirExpr::Binary {
        op: HirBinaryOp::Add,
        lhs: add_lhs,
        rhs: add_rhs,
        ..
    } = lhs.as_ref()
    else {
        return None;
    };
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
    if sign_source != add_lhs.as_ref() {
        return None;
    }

    let width = match ty {
        NirType::Int { bits, signed: true } => *bits,
        _ => return None,
    };
    if *shift_amount < 0 || *shift_amount >= i64::from(width) {
        return None;
    }
    let divisor = 1_i64.checked_shl(*shift_amount as u32)?;
    if sign_shift != i64::from(width.saturating_sub(1)) || mask != divisor - 1 {
        return None;
    }

    Some(HirExpr::Binary {
        op: HirBinaryOp::Div,
        lhs: add_lhs.clone(),
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
        } if matches!(index.as_ref(), HirExpr::Const(0, _))
            && !matches!(base.as_ref(), HirExpr::Var(_)) => Some(HirExpr::Load {
            ptr: base.clone(),
            ty: elem_ty.clone(),
        }),
        _ => None,
    }
}

fn cleanup_arithmetic_wrappers(expr: &HirExpr) -> Option<HirExpr> {
    match expr {
        HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs,
            rhs,
            ..
        } if is_zero_const(rhs.as_ref()) => Some((**lhs).clone()),
        HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs,
            rhs,
            ..
        } if is_zero_const(lhs.as_ref()) => Some((**rhs).clone()),
        HirExpr::Binary {
            op: HirBinaryOp::Sub,
            lhs,
            rhs,
            ..
        } if is_zero_const(rhs.as_ref()) => Some((**lhs).clone()),
        HirExpr::Binary {
            op: HirBinaryOp::Mul,
            lhs,
            rhs,
            ..
        } if is_one_const(rhs.as_ref()) => Some((**lhs).clone()),
        HirExpr::Binary {
            op: HirBinaryOp::Mul,
            lhs,
            rhs,
            ..
        } if is_one_const(lhs.as_ref()) => Some((**rhs).clone()),
        HirExpr::Binary {
            op: HirBinaryOp::Shl,
            lhs,
            rhs,
            ..
        } if is_zero_const(rhs.as_ref()) => Some((**lhs).clone()),
        HirExpr::Binary {
            op: HirBinaryOp::Shr,
            lhs,
            rhs,
            ..
        } if is_zero_const(rhs.as_ref()) => Some((**lhs).clone()),
        HirExpr::Binary {
            op: HirBinaryOp::Sar,
            lhs,
            rhs,
            ..
        } if is_zero_const(rhs.as_ref()) => Some((**lhs).clone()),
        HirExpr::Binary {
            op: HirBinaryOp::Or,
            lhs,
            rhs,
            ..
        } if is_zero_const(rhs.as_ref()) => Some((**lhs).clone()),
        HirExpr::Binary {
            op: HirBinaryOp::Or,
            lhs,
            rhs,
            ..
        } if is_zero_const(lhs.as_ref()) => Some((**rhs).clone()),
        HirExpr::Binary {
            op: HirBinaryOp::Xor,
            lhs,
            rhs,
            ..
        } if is_zero_const(rhs.as_ref()) => Some((**lhs).clone()),
        HirExpr::Binary {
            op: HirBinaryOp::Xor,
            lhs,
            rhs,
            ..
        } if is_zero_const(lhs.as_ref()) => Some((**rhs).clone()),
        HirExpr::Binary {
            op: HirBinaryOp::And,
            lhs,
            rhs,
            ..
        } if is_full_mask_const(rhs.as_ref(), &expr_type(lhs)) => Some((**lhs).clone()),
        HirExpr::Binary {
            op: HirBinaryOp::And,
            lhs,
            rhs,
            ..
        } if is_full_mask_const(lhs.as_ref(), &expr_type(rhs)) => Some((**rhs).clone()),
        HirExpr::Binary {
            op: HirBinaryOp::Ne,
            lhs,
            rhs,
            ..
        } if is_zero_const(rhs.as_ref()) => match lhs.as_ref() {
            HirExpr::Binary {
                op: HirBinaryOp::And,
                lhs: and_lhs,
                rhs: and_rhs,
                ty: _,
            } if is_one_const(and_rhs.as_ref()) && matches!(expr_type(and_lhs), NirType::Bool) => {
                Some((**and_lhs).clone())
            }
            _ => None,
        },
        _ => None,
    }
}

fn is_zero_const(expr: &HirExpr) -> bool {
    matches!(expr, HirExpr::Const(0, _))
}

fn is_one_const(expr: &HirExpr) -> bool {
    matches!(expr, HirExpr::Const(1, _))
}

fn is_integer_type(ty: &NirType) -> bool {
    matches!(ty, NirType::Bool | NirType::Int { .. })
}

fn int_type_bits(ty: &NirType) -> Option<u32> {
    match ty {
        NirType::Bool => Some(1),
        NirType::Int { bits, .. } => Some(*bits),
        _ => None,
    }
}

fn full_mask_for_bits(bits: u32) -> Option<i64> {
    match bits {
        0 => None,
        1..=62 => Some((1_i64 << bits) - 1),
        63 => Some(i64::MAX),
        _ => None,
    }
}

fn full_mask_for_type(ty: &NirType) -> Option<i64> {
    int_type_bits(ty).and_then(full_mask_for_bits)
}

fn is_full_mask_const(expr: &HirExpr, ty: &NirType) -> bool {
    let HirExpr::Const(value, _) = expr else {
        return false;
    };
    full_mask_for_type(ty).is_some_and(|mask| mask == *value)
}

fn collapse_trivial_assign_returns(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;
    let mut to_remove = vec![false; stmts.len()];
    let mut idx = 0usize;
    while idx + 1 < stmts.len() {
        let replacement = match (&stmts[idx], &stmts[idx + 1]) {
            (
                HirStmt::Assign {
                    lhs: HirLValue::Var(name),
                    rhs,
                },
                HirStmt::Return(Some(HirExpr::Var(ret_name))),
            ) if name == ret_name && is_trivial_temp_name(name) => Some(rhs.clone()),
            _ => None,
        };
        if let Some(expr) = replacement {
            stmts[idx + 1] = HirStmt::Return(Some(expr));
            to_remove[idx] = true;
            changed = true;
        }
        idx += 1;
    }
    if changed {
        retain_unmarked_stmts(stmts, &to_remove);
    }
    changed
}

fn inline_single_use_temps(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;
    let mut to_remove = vec![false; stmts.len()];
    let mut idx = 0usize;
    while idx + 1 < stmts.len() {
        let (name, rhs) = match &stmts[idx] {
            HirStmt::Assign {
                lhs: HirLValue::Var(name),
                rhs,
            } if is_trivial_temp_name(name) => (name.clone(), rhs.clone()),
            _ => {
                idx += 1;
                continue;
            }
        };

        let Some(target_idx) = find_single_use_forward_target(stmts, idx, &name) else {
            idx += 1;
            continue;
        };
        replace_var_in_stmt(&mut stmts[target_idx], &name, &rhs);
        to_remove[idx] = true;
        changed = true;
        idx += 1;
    }
    if changed {
        retain_unmarked_stmts(stmts, &to_remove);
    }
    changed
}

fn retain_unmarked_stmts(stmts: &mut Vec<HirStmt>, to_remove: &[bool]) {
    let mut idx = 0usize;
    stmts.retain(|_| {
        let keep = !to_remove.get(idx).copied().unwrap_or(false);
        idx += 1;
        keep
    });
}

fn find_single_use_forward_target(stmts: &[HirStmt], def_idx: usize, name: &str) -> Option<usize> {
    let mut scan_idx = def_idx + 1;
    while scan_idx < stmts.len() {
        let stmt = &stmts[scan_idx];
        let uses = count_var_uses_in_stmt(stmt, name);
        let redefines = stmt_redefines_temp(stmt, name);
        if redefines {
            return None;
        }
        if uses == 1 && stmt_allows_inline_target(stmt) {
            return Some(scan_idx);
        }
        if !stmt_allows_forward_scan(stmt) {
            return None;
        }
        if uses == 0 {
            scan_idx += 1;
            continue;
        }
        return None;
    }
    None
}

fn stmt_allows_forward_scan(stmt: &HirStmt) -> bool {
    match stmt {
        HirStmt::Assign {
            lhs: HirLValue::Var(_),
            rhs,
        } => !expr_has_side_effects(rhs),
        HirStmt::Return(Some(expr)) => !expr_has_side_effects(expr),
        HirStmt::If { cond, .. } => !expr_has_side_effects(cond),
        HirStmt::Expr(expr) => !expr_has_side_effects(expr),
        _ => false,
    }
}

fn stmt_allows_inline_target(stmt: &HirStmt) -> bool {
    matches!(
        stmt,
        HirStmt::Assign { .. }
            | HirStmt::Expr(_)
            | HirStmt::Return(_)
            | HirStmt::If { .. }
    )
}

fn stmt_redefines_temp(stmt: &HirStmt, name: &str) -> bool {
    matches!(
        stmt,
        HirStmt::Assign {
            lhs: HirLValue::Var(lhs_name),
            ..
        } if lhs_name == name
    )
}

fn is_trivial_temp_name(name: &str) -> bool {
    name == "result"
        || name == "retval"
        || name.starts_with("uVar")
        || name.starts_with("iVar")
        || name.starts_with("xVar")
        || name.starts_with("bVar")
}

fn count_var_uses_in_stmt(stmt: &HirStmt, name: &str) -> usize {
    match stmt {
        HirStmt::Assign { lhs, rhs } => count_var_uses_in_lvalue(lhs, name) + count_var_uses(rhs, name),
        HirStmt::Expr(expr) => count_var_uses(expr, name),
        HirStmt::Block(stmts) => stmts.iter().map(|stmt| count_var_uses_in_stmt(stmt, name)).sum(),
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            count_var_uses(expr, name)
                + cases
                    .iter()
                    .map(|case| case.body.iter().map(|stmt| count_var_uses_in_stmt(stmt, name)).sum::<usize>())
                    .sum::<usize>()
                + default.iter().map(|stmt| count_var_uses_in_stmt(stmt, name)).sum::<usize>()
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            count_var_uses(cond, name)
                + then_body.iter().map(|stmt| count_var_uses_in_stmt(stmt, name)).sum::<usize>()
                + else_body.iter().map(|stmt| count_var_uses_in_stmt(stmt, name)).sum::<usize>()
        }
        HirStmt::While { cond, body } => {
            count_var_uses(cond, name)
                + body.iter().map(|stmt| count_var_uses_in_stmt(stmt, name)).sum::<usize>()
        }
        HirStmt::DoWhile { body, cond } => {
            body.iter().map(|stmt| count_var_uses_in_stmt(stmt, name)).sum::<usize>()
                + count_var_uses(cond, name)
        }
        HirStmt::Return(Some(expr)) => count_var_uses(expr, name),
        HirStmt::Label(_) | HirStmt::Goto(_) | HirStmt::Return(None) | HirStmt::Break | HirStmt::Continue => 0,
    }
}

fn count_var_uses_in_lvalue(lhs: &HirLValue, name: &str) -> usize {
    match lhs {
        HirLValue::Var(var) => usize::from(var == name),
        HirLValue::Deref { ptr, .. } => count_var_uses(ptr, name),
        HirLValue::Index { base, index, .. } => {
            count_var_uses(base, name) + count_var_uses(index, name)
        }
    }
}

fn count_var_uses(expr: &HirExpr, name: &str) -> usize {
    match expr {
        HirExpr::Var(var) => usize::from(var == name),
        HirExpr::Const(_, _) => 0,
        HirExpr::Cast { expr, .. } => count_var_uses(expr, name),
        HirExpr::Unary { expr, .. } => count_var_uses(expr, name),
        HirExpr::Binary { lhs, rhs, .. } => count_var_uses(lhs, name) + count_var_uses(rhs, name),
        HirExpr::Call { args, .. } => args.iter().map(|arg| count_var_uses(arg, name)).sum(),
        HirExpr::Load { ptr, .. } => count_var_uses(ptr, name),
        HirExpr::PtrOffset { base, .. } => count_var_uses(base, name),
        HirExpr::Index { base, index, .. } => {
            count_var_uses(base, name) + count_var_uses(index, name)
        }
        HirExpr::AggregateCopy { src, .. } => count_var_uses(src, name),
    }
}

fn expr_has_side_effects(expr: &HirExpr) -> bool {
    match expr {
        HirExpr::Var(_) | HirExpr::Const(_, _) => false,
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::Load { ptr: expr, .. }
        | HirExpr::PtrOffset { base: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. } => expr_has_side_effects(expr),
        HirExpr::Binary { lhs, rhs, .. } => {
            expr_has_side_effects(lhs) || expr_has_side_effects(rhs)
        }
        HirExpr::Index { base, index, .. } => {
            expr_has_side_effects(base) || expr_has_side_effects(index)
        }
        HirExpr::Call { .. } => true,
    }
}

fn replace_var_in_stmt(stmt: &mut HirStmt, name: &str, replacement: &HirExpr) {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            replace_var_in_lvalue(lhs, name, replacement);
            replace_var_in_expr(rhs, name, replacement);
        }
        HirStmt::Expr(expr) => replace_var_in_expr(expr, name, replacement),
        HirStmt::Block(stmts) => {
            for stmt in stmts {
                replace_var_in_stmt(stmt, name, replacement);
            }
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            replace_var_in_expr(expr, name, replacement);
            for case in cases {
                for stmt in &mut case.body {
                    replace_var_in_stmt(stmt, name, replacement);
                }
            }
            for stmt in default {
                replace_var_in_stmt(stmt, name, replacement);
            }
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            replace_var_in_expr(cond, name, replacement);
            for stmt in then_body {
                replace_var_in_stmt(stmt, name, replacement);
            }
            for stmt in else_body {
                replace_var_in_stmt(stmt, name, replacement);
            }
        }
        HirStmt::While { cond, body } => {
            replace_var_in_expr(cond, name, replacement);
            for stmt in body {
                replace_var_in_stmt(stmt, name, replacement);
            }
        }
        HirStmt::DoWhile { body, cond } => {
            for stmt in body {
                replace_var_in_stmt(stmt, name, replacement);
            }
            replace_var_in_expr(cond, name, replacement);
        }
        HirStmt::Return(Some(expr)) => replace_var_in_expr(expr, name, replacement),
        HirStmt::Label(_) | HirStmt::Goto(_) | HirStmt::Return(None) | HirStmt::Break | HirStmt::Continue => {}
    }
}

fn replace_var_in_lvalue(lhs: &mut HirLValue, name: &str, replacement: &HirExpr) {
    match lhs {
        HirLValue::Var(_) => {}
        HirLValue::Deref { ptr, .. } => replace_var_in_expr(ptr, name, replacement),
        HirLValue::Index { base, index, .. } => {
            replace_var_in_expr(base, name, replacement);
            replace_var_in_expr(index, name, replacement);
        }
    }
}

fn replace_var_in_expr(expr: &mut HirExpr, name: &str, replacement: &HirExpr) {
    match expr {
        HirExpr::Var(var) if var == name => *expr = replacement.clone(),
        HirExpr::Var(_) | HirExpr::Const(_, _) => {}
        HirExpr::Cast { expr, .. } => replace_var_in_expr(expr, name, replacement),
        HirExpr::Unary { expr, .. } => replace_var_in_expr(expr, name, replacement),
        HirExpr::Binary { lhs, rhs, .. } => {
            replace_var_in_expr(lhs, name, replacement);
            replace_var_in_expr(rhs, name, replacement);
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                replace_var_in_expr(arg, name, replacement);
            }
        }
        HirExpr::Load { ptr, .. } => replace_var_in_expr(ptr, name, replacement),
        HirExpr::PtrOffset { base, .. } => replace_var_in_expr(base, name, replacement),
        HirExpr::Index { base, index, .. } => {
            replace_var_in_expr(base, name, replacement);
            replace_var_in_expr(index, name, replacement);
        }
        HirExpr::AggregateCopy { src, .. } => replace_var_in_expr(src, name, replacement),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct MemorySlotKey {
    base_repr: String,
    offset: i64,
    access_size: u32,
    stride: Option<i64>,
}

#[derive(Debug, Clone)]
struct MemorySlotCandidate {
    key: MemorySlotKey,
    base: HirExpr,
    offset: i64,
    elem_ty: NirType,
    access_size: u32,
    count: usize,
}

#[derive(Debug, Clone)]
struct MemorySlotPattern {
    key: MemorySlotKey,
    base: HirExpr,
    elem_ty: NirType,
    index: Option<HirExpr>,
}

#[derive(Debug, Default, Clone)]
struct AddressParts {
    base: Option<HirExpr>,
    const_offset: i64,
    scaled_index: Option<(HirExpr, i64)>,
}

#[derive(Debug, Clone)]
struct MemorySlotAlias {
    alias: String,
    elem_ty: NirType,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct MemorySlotFamilyKey {
    base_repr: String,
    family_offset: i64,
    access_size: u32,
    stride: i64,
}

fn normalize_binding_initializers(bindings: &mut [NirBinding]) {
    for binding in bindings {
        if let Some(initializer) = &mut binding.initializer {
            normalize_expr(initializer);
        }
    }
}

fn apply_memory_slot_surfacing(func: &mut HirFunction) -> bool {
    let mut candidates = HashMap::<MemorySlotKey, MemorySlotCandidate>::new();
    collect_memory_slot_candidates_from_stmts(&func.body, &mut candidates);
    let mut family_counts = HashMap::<MemorySlotFamilyKey, usize>::new();
    let mut family_lanes = HashMap::<MemorySlotFamilyKey, HashSet<i64>>::new();
    let mut family_base_offsets = HashMap::<MemorySlotFamilyKey, i64>::new();
    for candidate in candidates.values() {
        let family_key = memory_slot_family_key(&candidate.key);
        *family_counts.entry(family_key.clone()).or_insert(0) += candidate.count;
        family_lanes
            .entry(family_key.clone())
            .or_default()
            .insert(candidate.key.offset);
        family_base_offsets
            .entry(family_key)
            .and_modify(|offset| *offset = (*offset).min(candidate.key.offset))
            .or_insert(candidate.key.offset);
    }
    let mut aliases = HashMap::<MemorySlotKey, MemorySlotAlias>::new();
    let mut used_names = func
        .params
        .iter()
        .chain(func.locals.iter())
        .map(|binding| binding.name.clone())
        .collect::<HashSet<_>>();

    for candidate in candidates.values().filter(|candidate| {
        let family_key = memory_slot_family_key(&candidate.key);
        let family_total = family_counts.get(&family_key).copied().unwrap_or(0);
        let family_lane_count = family_lanes
            .get(&family_key)
            .map(HashSet::len)
            .unwrap_or(0);
        let exact_indexable = candidate.key.stride.is_none()
            || candidate.key.stride == Some(i64::from(candidate.key.access_size));
        (exact_indexable && candidate.count >= 2)
            || (family_total >= 2 && family_lane_count >= 2)
    }) {
        let family_base = family_base_offsets
            .get(&memory_slot_family_key(&candidate.key))
            .copied();
        let alias = next_slot_alias_name(&candidate.key, family_base, &mut used_names);
        aliases.insert(
            candidate.key.clone(),
            MemorySlotAlias {
                alias: alias.clone(),
                elem_ty: candidate.elem_ty.clone(),
            },
        );
        func.locals.push(NirBinding {
            name: alias,
            ty: NirType::Ptr(Box::new(candidate.elem_ty.clone())),
            surface_type_name: None,
            initializer: Some(HirExpr::Cast {
                ty: NirType::Ptr(Box::new(candidate.elem_ty.clone())),
                expr: Box::new(HirExpr::PtrOffset {
                    base: Box::new(candidate.base.clone()),
                    offset: candidate.offset,
                }),
            }),
        });
    }

    rewrite_memory_slot_stmts(&mut func.body, &aliases)
}

fn memory_slot_family_key(key: &MemorySlotKey) -> MemorySlotFamilyKey {
    let (family_offset, _) = slot_family_layout(key);
    MemorySlotFamilyKey {
        base_repr: key.base_repr.clone(),
        family_offset,
        access_size: key.access_size,
        stride: key.stride.unwrap_or(i64::from(key.access_size)),
    }
}

fn collect_memory_slot_candidates_from_stmts(
    stmts: &[HirStmt],
    candidates: &mut HashMap<MemorySlotKey, MemorySlotCandidate>,
) {
    for stmt in stmts {
        match stmt {
            HirStmt::Assign { lhs, rhs } => {
                if let HirLValue::Deref { ptr, ty } = lhs {
                    collect_memory_slot_candidate_from_ptr(ptr, ty, candidates);
                }
                collect_memory_slot_candidates_from_expr(rhs, candidates);
            }
            HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => {
                collect_memory_slot_candidates_from_expr(expr, candidates);
            }
            HirStmt::Block(stmts)
            | HirStmt::While { body: stmts, .. }
            | HirStmt::DoWhile { body: stmts, .. } => {
                collect_memory_slot_candidates_from_stmts(stmts, candidates);
            }
            HirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                collect_memory_slot_candidates_from_expr(expr, candidates);
                for case in cases {
                    collect_memory_slot_candidates_from_stmts(&case.body, candidates);
                }
                collect_memory_slot_candidates_from_stmts(default, candidates);
            }
            HirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                collect_memory_slot_candidates_from_expr(cond, candidates);
                collect_memory_slot_candidates_from_stmts(then_body, candidates);
                collect_memory_slot_candidates_from_stmts(else_body, candidates);
            }
            HirStmt::Label(_) | HirStmt::Goto(_) | HirStmt::Return(None) | HirStmt::Break | HirStmt::Continue => {}
        }
    }
}

fn collect_memory_slot_candidates_from_expr(
    expr: &HirExpr,
    candidates: &mut HashMap<MemorySlotKey, MemorySlotCandidate>,
) {
    match expr {
        HirExpr::Load { ptr, ty } => {
            collect_memory_slot_candidate_from_ptr(ptr, ty, candidates);
            collect_memory_slot_candidates_from_expr(ptr, candidates);
        }
        HirExpr::Cast { expr, .. } | HirExpr::Unary { expr, .. } | HirExpr::AggregateCopy { src: expr, .. } => {
            collect_memory_slot_candidates_from_expr(expr, candidates);
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            collect_memory_slot_candidates_from_expr(lhs, candidates);
            collect_memory_slot_candidates_from_expr(rhs, candidates);
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                collect_memory_slot_candidates_from_expr(arg, candidates);
            }
        }
        HirExpr::PtrOffset { base, .. } => collect_memory_slot_candidates_from_expr(base, candidates),
        HirExpr::Index { base, index, .. } => {
            collect_memory_slot_candidates_from_expr(base, candidates);
            collect_memory_slot_candidates_from_expr(index, candidates);
        }
        HirExpr::Var(_) | HirExpr::Const(_, _) => {}
    }
}

fn collect_memory_slot_candidate_from_ptr(
    ptr: &HirExpr,
    elem_ty: &NirType,
    candidates: &mut HashMap<MemorySlotKey, MemorySlotCandidate>,
) {
    let Some(pattern) = parse_memory_slot_pattern(ptr, elem_ty) else {
        return;
    };
    candidates
        .entry(pattern.key.clone())
        .and_modify(|candidate| candidate.count += 1)
        .or_insert_with(|| MemorySlotCandidate {
            key: pattern.key.clone(),
            base: pattern.base.clone(),
            offset: pattern.key.offset,
            elem_ty: pattern.elem_ty.clone(),
            access_size: pattern.key.access_size,
            count: 1,
        });
}

fn rewrite_memory_slot_stmts(
    stmts: &mut [HirStmt],
    aliases: &HashMap<MemorySlotKey, MemorySlotAlias>,
) -> bool {
    let mut changed = false;
    for stmt in stmts {
        match stmt {
            HirStmt::Assign { lhs, rhs } => {
                changed |= rewrite_memory_slot_lvalue(lhs, aliases);
                changed |= rewrite_memory_slot_expr(rhs, aliases);
            }
            HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => {
                changed |= rewrite_memory_slot_expr(expr, aliases);
            }
            HirStmt::Block(stmts)
            | HirStmt::While { body: stmts, .. }
            | HirStmt::DoWhile { body: stmts, .. } => {
                changed |= rewrite_memory_slot_stmts(stmts, aliases);
            }
            HirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                changed |= rewrite_memory_slot_expr(expr, aliases);
                for case in cases {
                    changed |= rewrite_memory_slot_stmts(&mut case.body, aliases);
                }
                changed |= rewrite_memory_slot_stmts(default, aliases);
            }
            HirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                changed |= rewrite_memory_slot_expr(cond, aliases);
                changed |= rewrite_memory_slot_stmts(then_body, aliases);
                changed |= rewrite_memory_slot_stmts(else_body, aliases);
            }
            HirStmt::Label(_) | HirStmt::Goto(_) | HirStmt::Return(None) | HirStmt::Break | HirStmt::Continue => {}
        }
    }
    changed
}

fn rewrite_memory_slot_lvalue(
    lhs: &mut HirLValue,
    aliases: &HashMap<MemorySlotKey, MemorySlotAlias>,
) -> bool {
    match lhs {
        HirLValue::Var(_) => false,
        HirLValue::Deref { ptr, ty } => {
            let changed = rewrite_memory_slot_expr(ptr, aliases);
            if let Some(pattern) = parse_memory_slot_pattern(ptr, ty)
                && let Some(alias) = aliases.get(&pattern.key)
            {
                let index = pattern.index.unwrap_or_else(zero_index_expr);
                *lhs = HirLValue::Index {
                    base: Box::new(HirExpr::Var(alias.alias.clone())),
                    index: Box::new(index),
                    elem_ty: alias.elem_ty.clone(),
                };
                return true;
            }
            changed
        }
        HirLValue::Index { base, index, .. } => {
            let mut changed = rewrite_memory_slot_expr(base, aliases);
            changed |= rewrite_memory_slot_expr(index, aliases);
            changed
        }
    }
}

fn rewrite_memory_slot_expr(
    expr: &mut HirExpr,
    aliases: &HashMap<MemorySlotKey, MemorySlotAlias>,
) -> bool {
    let mut changed = false;
    match expr {
        HirExpr::Load { ptr, ty } => {
            changed |= rewrite_memory_slot_expr(ptr, aliases);
            if let Some(pattern) = parse_memory_slot_pattern(ptr, ty)
                && let Some(alias) = aliases.get(&pattern.key)
            {
                let index = pattern.index.unwrap_or_else(|| zero_index_expr());
                *expr = HirExpr::Index {
                    base: Box::new(HirExpr::Var(alias.alias.clone())),
                    index: Box::new(index),
                    elem_ty: ty.clone(),
                };
                return true;
            }
        }
        HirExpr::Cast { expr, .. } | HirExpr::Unary { expr, .. } | HirExpr::AggregateCopy { src: expr, .. } => {
            changed |= rewrite_memory_slot_expr(expr, aliases);
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            changed |= rewrite_memory_slot_expr(lhs, aliases);
            changed |= rewrite_memory_slot_expr(rhs, aliases);
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                changed |= rewrite_memory_slot_expr(arg, aliases);
            }
        }
        HirExpr::PtrOffset { base, .. } => {
            changed |= rewrite_memory_slot_expr(base, aliases);
        }
        HirExpr::Index { base, index, .. } => {
            changed |= rewrite_memory_slot_expr(base, aliases);
            changed |= rewrite_memory_slot_expr(index, aliases);
        }
        HirExpr::Var(_) | HirExpr::Const(_, _) => {}
    }
    changed
}

fn parse_memory_slot_pattern(ptr: &HirExpr, elem_ty: &NirType) -> Option<MemorySlotPattern> {
    let access_size = type_byte_size(elem_ty)?;
    let elem_size = i64::from(access_size);
    let mut parts = AddressParts::default();
    collect_address_parts(ptr, &mut parts, 1)?;
    let base = parts.base?;
    if expr_has_side_effects(&base) {
        return None;
    }
    let stride = parts.scaled_index.as_ref().map(|(_, stride)| *stride);
    let index = match parts.scaled_index {
        Some((index, stride)) if stride == elem_size => Some(index),
        Some((index, stride)) if stride > elem_size && stride % elem_size == 0 => Some(index),
        Some(_) => return None,
        None => None,
    };
    let key = MemorySlotKey {
        base_repr: print_expr(&base),
        offset: parts.const_offset,
        access_size,
        stride,
    };
    Some(MemorySlotPattern {
        key,
        base,
        elem_ty: elem_ty.clone(),
        index,
    })
}

fn collect_address_parts(expr: &HirExpr, parts: &mut AddressParts, sign: i64) -> Option<()> {
    match expr {
        HirExpr::Const(value, _) => {
            parts.const_offset += sign * *value;
            Some(())
        }
        HirExpr::Cast { expr, .. } => collect_address_parts(expr, parts, sign),
        HirExpr::PtrOffset { base, offset } => {
            parts.const_offset += sign * *offset;
            collect_address_parts(base, parts, sign)
        }
        HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs,
            rhs,
            ..
        } => {
            collect_address_parts(lhs, parts, sign)?;
            collect_address_parts(rhs, parts, sign)
        }
        HirExpr::Binary {
            op: HirBinaryOp::Sub,
            lhs,
            rhs,
            ..
        } => {
            collect_address_parts(lhs, parts, sign)?;
            collect_address_parts(rhs, parts, -sign)
        }
        HirExpr::Binary {
            op: HirBinaryOp::Mul,
            lhs,
            rhs,
            ..
        } => {
            if let HirExpr::Const(value, _) = lhs.as_ref() {
                return add_scaled_index_expr(parts, rhs, sign * *value);
            }
            if let HirExpr::Const(value, _) = rhs.as_ref() {
                return add_scaled_index_expr(parts, lhs, sign * *value);
            }
            add_base_expr(parts, expr.clone(), sign)
        }
        HirExpr::Binary {
            op: HirBinaryOp::Shl,
            lhs,
            rhs,
            ..
        } => {
            let HirExpr::Const(shift, _) = rhs.as_ref() else {
                return add_base_expr(parts, expr.clone(), sign);
            };
            if *shift < 0 || *shift > 30 {
                return add_base_expr(parts, expr.clone(), sign);
            }
            add_scaled_index_expr(parts, lhs, sign * (1_i64 << shift))
        }
        _ => add_base_expr(parts, expr.clone(), sign),
    }
}

fn add_scaled_index_expr(parts: &mut AddressParts, expr: &HirExpr, stride: i64) -> Option<()> {
    if let HirExpr::Const(value, _) = expr {
        parts.const_offset += stride * *value;
        return Some(());
    }
    if let Some((index, bias)) = extract_index_bias(expr) {
        parts.const_offset += stride * bias;
        return add_scaled_index(parts, index, stride);
    }
    add_scaled_index(parts, expr.clone(), stride)
}

fn extract_index_bias(expr: &HirExpr) -> Option<(HirExpr, i64)> {
    match expr {
        HirExpr::Cast { expr, .. } => extract_index_bias(expr),
        HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs,
            rhs,
            ..
        } => {
            if let HirExpr::Const(value, _) = lhs.as_ref() {
                let (index, bias) = extract_index_bias(rhs)?;
                return Some((index, bias + *value));
            }
            if let HirExpr::Const(value, _) = rhs.as_ref() {
                let (index, bias) = extract_index_bias(lhs)?;
                return Some((index, bias + *value));
            }
            if !expr_has_side_effects(expr) {
                Some((expr.clone(), 0))
            } else {
                None
            }
        }
        HirExpr::Binary {
            op: HirBinaryOp::Sub,
            lhs,
            rhs,
            ..
        } => {
            if let HirExpr::Const(value, _) = rhs.as_ref() {
                let (index, bias) = extract_index_bias(lhs)?;
                return Some((index, bias - *value));
            }
            if !expr_has_side_effects(expr) {
                Some((expr.clone(), 0))
            } else {
                None
            }
        }
        _ if !expr_has_side_effects(expr) => Some((expr.clone(), 0)),
        _ => None,
    }
}

fn add_base_expr(parts: &mut AddressParts, expr: HirExpr, sign: i64) -> Option<()> {
    if sign != 1 || matches!(expr, HirExpr::Const(_, _)) {
        return None;
    }
    match &parts.base {
        Some(existing) if existing != &expr => None,
        Some(_) => Some(()),
        None => {
            parts.base = Some(expr);
            Some(())
        }
    }
}

fn add_scaled_index(parts: &mut AddressParts, expr: HirExpr, stride: i64) -> Option<()> {
    if stride <= 0 || expr_has_side_effects(&expr) {
        return None;
    }
    match &parts.scaled_index {
        Some((existing, existing_stride)) if existing != &expr || *existing_stride != stride => None,
        Some(_) => Some(()),
        None => {
            parts.scaled_index = Some((expr, stride));
            Some(())
        }
    }
}

fn next_slot_alias_name(
    key: &MemorySlotKey,
    family_base: Option<i64>,
    used_names: &mut HashSet<String>,
) -> String {
    let (family_offset, lane) = slot_family_name_layout(key, family_base);
    let base = if family_offset >= 0 {
        format!("slot_{family_offset:x}")
    } else {
        format!("slot_neg_{:x}", family_offset.unsigned_abs())
    };
    let base = if lane > 0 {
        format!("{base}_lane{lane}")
    } else {
        base
    };
    if used_names.insert(base.clone()) {
        return base;
    }
    let sized = format!("{base}_{}", key.access_size);
    if used_names.insert(sized.clone()) {
        return sized;
    }
    let mut idx = 1usize;
    loop {
        let candidate = format!("{sized}_{idx}");
        if used_names.insert(candidate.clone()) {
            return candidate;
        }
        idx += 1;
    }
}

fn slot_family_name_layout(key: &MemorySlotKey, family_base: Option<i64>) -> (i64, i64) {
    if let Some(family_base) = family_base
        && key.offset >= family_base
    {
        let lane_bytes = key.offset - family_base;
        if lane_bytes % i64::from(key.access_size) == 0 {
            return (family_base, lane_bytes / i64::from(key.access_size));
        }
    }
    slot_family_layout(key)
}

fn slot_family_layout(key: &MemorySlotKey) -> (i64, i64) {
    let Some(stride) = key.stride else {
        return (key.offset, 0);
    };
    if stride <= i64::from(key.access_size) {
        return (key.offset, 0);
    }
    let lane_bytes = key.offset.rem_euclid(stride);
    if lane_bytes % i64::from(key.access_size) != 0 {
        return (key.offset, 0);
    }
    let family_offset = key.offset - lane_bytes;
    let lane = lane_bytes / i64::from(key.access_size);
    (family_offset, lane)
}

fn zero_index_expr() -> HirExpr {
    HirExpr::Const(
        0,
        NirType::Int {
            bits: 64,
            signed: false,
        },
    )
}

fn type_byte_size(ty: &NirType) -> Option<u32> {
    match ty {
        NirType::Bool => Some(1),
        NirType::Int { bits, .. } => Some(bits / 8),
        NirType::Ptr(_) => Some(8),
        NirType::Aggregate { size } => Some(*size),
        NirType::Float { bits } => Some(bits / 8),
        NirType::Unknown => None,
    }
}

fn apply_bitstream_idioms(func: &mut HirFunction) -> bool {
    let state_roots = build_slot_state_roots(func);
    let default_state = infer_default_state_expr(&state_roots);
    rewrite_bitstream_stmt_list(&mut func.body, &state_roots, default_state.as_ref())
}

fn build_slot_state_roots(func: &HirFunction) -> HashMap<String, HirExpr> {
    let mut roots = HashMap::new();
    for binding in &func.locals {
        let Some(initializer) = &binding.initializer else {
            continue;
        };
        let Some(root) = peel_state_root_expr(initializer) else {
            continue;
        };
        roots.insert(binding.name.clone(), root);
    }
    roots
}

fn infer_default_state_expr(state_roots: &HashMap<String, HirExpr>) -> Option<HirExpr> {
    let mut roots = state_roots.values();
    let first = roots.next()?.clone();
    if roots.all(|root| *root == first) {
        Some(first)
    } else {
        None
    }
}

fn peel_state_root_expr(expr: &HirExpr) -> Option<HirExpr> {
    match expr {
        HirExpr::Cast { expr, .. } => peel_state_root_expr(expr),
        HirExpr::PtrOffset { base, .. } => Some((**base).clone()),
        HirExpr::Var(_) => Some(expr.clone()),
        _ => None,
    }
}

fn rewrite_bitstream_stmt_list(
    stmts: &mut Vec<HirStmt>,
    state_roots: &HashMap<String, HirExpr>,
    default_state: Option<&HirExpr>,
) -> bool {
    let mut changed = false;
    let mut idx = 0usize;
    while idx < stmts.len() {
        if let Some(rewritten) = rewrite_flush_bits_if(&stmts[idx], state_roots, default_state) {
            stmts[idx] = rewritten;
            changed = true;
        }
        if idx + 1 < stmts.len()
            && let Some((call_target, value, width, state)) =
                match_write_bits_pair(&stmts[idx], &stmts[idx + 1], state_roots, default_state)
        {
            stmts.splice(
                idx..=idx + 1,
                [HirStmt::Expr(HirExpr::Call {
                    target: call_target,
                    args: vec![state, value, width],
                    ty: NirType::Unknown,
                })],
            );
            changed = true;
            continue;
        }
        match &mut stmts[idx] {
            HirStmt::Block(body)
            | HirStmt::While { body, .. }
            | HirStmt::DoWhile { body, .. } => {
                changed |= rewrite_bitstream_stmt_list(body, state_roots, default_state);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= rewrite_bitstream_stmt_list(&mut case.body, state_roots, default_state);
                }
                changed |= rewrite_bitstream_stmt_list(default, state_roots, default_state);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= rewrite_bitstream_stmt_list(then_body, state_roots, default_state);
                changed |= rewrite_bitstream_stmt_list(else_body, state_roots, default_state);
            }
            HirStmt::Assign { .. }
            | HirStmt::Expr(_)
            | HirStmt::Label(_)
            | HirStmt::Goto(_)
            | HirStmt::Return(_)
            | HirStmt::Break
            | HirStmt::Continue => {}
        }
        idx += 1;
    }
    changed
}

fn rewrite_flush_bits_if(
    stmt: &HirStmt,
    state_roots: &HashMap<String, HirExpr>,
    default_state: Option<&HirExpr>,
) -> Option<HirStmt> {
    let HirStmt::If {
        cond,
        then_body,
        else_body,
    } = stmt
    else {
        return None;
    };
    if !else_body.is_empty() || !is_flush_condition(cond) {
        return None;
    }
    if !then_body.iter().any(is_output_store_stmt) {
        return None;
    }
    if !then_body.iter().any(is_pointer_increment_stmt) {
        return None;
    }
    if !then_body.iter().any(is_shift_byte_stmt) {
        return None;
    }
    if !then_body.iter().any(is_bitcount_adjust_stmt) {
        return None;
    }
    let state = infer_state_for_stmts(then_body, state_roots, default_state)?;
    Some(HirStmt::If {
        cond: cond.clone(),
        then_body: vec![HirStmt::Expr(HirExpr::Call {
            target: "FLUSH_BITS".to_string(),
            args: vec![state],
            ty: NirType::Unknown,
        })],
        else_body: Vec::new(),
    })
}

fn match_write_bits_pair(
    first: &HirStmt,
    second: &HirStmt,
    state_roots: &HashMap<String, HirExpr>,
    default_state: Option<&HirExpr>,
) -> Option<(String, HirExpr, HirExpr, HirExpr)> {
    let (bitcount_key, value) = parse_write_bits_accumulator(first)?;
    let width = parse_bitcount_increment(second, &bitcount_key)?;
    let state = infer_state_for_stmts(&[first.clone(), second.clone()], state_roots, default_state)?;
    let call_target = if is_table_lookup_expr(&value) && is_table_lookup_expr(&width) {
        "EMIT_CODE"
    } else {
        "WRITE_BITS"
    };
    Some((call_target.to_string(), value, width, state))
}

fn parse_write_bits_accumulator(stmt: &HirStmt) -> Option<(String, HirExpr)> {
    let HirStmt::Assign {
        lhs,
        rhs,
    } = stmt
    else {
        return None;
    };
    let accum_key = lvalue_location_key(lhs)?;
    let HirExpr::Binary {
        op: HirBinaryOp::Or | HirBinaryOp::Add,
        lhs,
        rhs,
        ..
    } = rhs
    else {
        return None;
    };
    let (value, bitcount_key) = if let Some(parsed) = parse_shifted_value(lhs, rhs, &accum_key) {
        parsed
    } else if let Some(parsed) = parse_shifted_value(rhs, lhs, &accum_key) {
        parsed
    } else {
        return None;
    };
    Some((bitcount_key, value))
}

fn parse_shifted_value<'a>(
    candidate: &'a HirExpr,
    other: &'a HirExpr,
    accum_key: &str,
) -> Option<(HirExpr, String)> {
    if expr_location_key(other).as_deref() != Some(accum_key) {
        return None;
    }
    let HirExpr::Binary {
        op: HirBinaryOp::Shl,
        lhs,
        rhs,
        ..
    } = candidate
    else {
        return None;
    };
    let bitcount_key = expr_location_key(rhs)?;
    Some(((**lhs).clone(), bitcount_key))
}

fn parse_bitcount_increment(stmt: &HirStmt, bitcount_key: &str) -> Option<HirExpr> {
    let HirStmt::Assign {
        lhs,
        rhs,
    } = stmt
    else {
        return None;
    };
    if lvalue_location_key(lhs).as_deref() != Some(bitcount_key) {
        return None;
    }
    let HirExpr::Binary {
        op: HirBinaryOp::Add,
        lhs,
        rhs,
        ..
    } = rhs
    else {
        return None;
    };
    if expr_location_key(lhs).as_deref() == Some(bitcount_key) {
        return Some((**rhs).clone());
    }
    if expr_location_key(rhs).as_deref() == Some(bitcount_key) {
        return Some((**lhs).clone());
    }
    None
}

fn infer_state_for_stmts(
    stmts: &[HirStmt],
    state_roots: &HashMap<String, HirExpr>,
    default_state: Option<&HirExpr>,
) -> Option<HirExpr> {
    if let Some(default_state) = default_state {
        return Some(default_state.clone());
    }
    for stmt in stmts {
        if let Some(state) = infer_state_from_stmt(stmt, state_roots) {
            return Some(state);
        }
    }
    None
}

fn infer_state_from_stmt(stmt: &HirStmt, state_roots: &HashMap<String, HirExpr>) -> Option<HirExpr> {
    match stmt {
        HirStmt::Assign { lhs, rhs } => infer_state_from_lvalue(lhs, state_roots)
            .or_else(|| infer_state_from_expr(rhs, state_roots)),
        HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => infer_state_from_expr(expr, state_roots),
        HirStmt::Block(body)
        | HirStmt::While { body, .. }
        | HirStmt::DoWhile { body, .. } => {
            for stmt in body {
                if let Some(state) = infer_state_from_stmt(stmt, state_roots) {
                    return Some(state);
                }
            }
            None
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => infer_state_from_expr(cond, state_roots)
            .or_else(|| infer_state_for_stmts(then_body, state_roots, None))
            .or_else(|| infer_state_for_stmts(else_body, state_roots, None)),
        HirStmt::Switch { expr, cases, default } => infer_state_from_expr(expr, state_roots).or_else(|| {
            for case in cases {
                if let Some(state) = infer_state_for_stmts(&case.body, state_roots, None) {
                    return Some(state);
                }
            }
            infer_state_for_stmts(default, state_roots, None)
        }),
        HirStmt::Label(_) | HirStmt::Goto(_) | HirStmt::Return(None) | HirStmt::Break | HirStmt::Continue => None,
    }
}

fn infer_state_from_lvalue(lhs: &HirLValue, state_roots: &HashMap<String, HirExpr>) -> Option<HirExpr> {
    match lhs {
        HirLValue::Var(var) => state_roots.get(var).cloned(),
        HirLValue::Deref { ptr, .. } => infer_state_from_expr(ptr, state_roots),
        HirLValue::Index { base, index, .. } => infer_state_from_expr(base, state_roots)
            .or_else(|| infer_state_from_expr(index, state_roots)),
    }
}

fn infer_state_from_expr(expr: &HirExpr, state_roots: &HashMap<String, HirExpr>) -> Option<HirExpr> {
    match expr {
        HirExpr::Var(var) => state_roots.get(var).cloned(),
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::Load { ptr: expr, .. }
        | HirExpr::PtrOffset { base: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. } => infer_state_from_expr(expr, state_roots),
        HirExpr::Binary { lhs, rhs, .. } => {
            infer_state_from_expr(lhs, state_roots).or_else(|| infer_state_from_expr(rhs, state_roots))
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                if let Some(state) = infer_state_from_expr(arg, state_roots) {
                    return Some(state);
                }
            }
            None
        }
        HirExpr::Index { base, index, .. } => infer_state_from_expr(base, state_roots)
            .or_else(|| infer_state_from_expr(index, state_roots)),
        HirExpr::Const(_, _) => None,
    }
}

fn is_flush_condition(expr: &HirExpr) -> bool {
    match expr {
        HirExpr::Binary {
            op: HirBinaryOp::Lt | HirBinaryOp::SLt,
            lhs,
            rhs,
            ..
        } if is_const_int(lhs, 7) => !matches!(rhs.as_ref(), HirExpr::Const(_, _)),
        HirExpr::Binary {
            op: HirBinaryOp::Le | HirBinaryOp::SLe,
            lhs,
            rhs,
            ..
        } if is_const_int(lhs, 8) => !matches!(rhs.as_ref(), HirExpr::Const(_, _)),
        HirExpr::Binary {
            op: HirBinaryOp::Lt | HirBinaryOp::SLt,
            lhs,
            rhs,
            ..
        } if is_const_int(rhs, 8) => !matches!(lhs.as_ref(), HirExpr::Const(_, _)),
        HirExpr::Binary {
            op: HirBinaryOp::Le | HirBinaryOp::SLe,
            lhs,
            rhs,
            ..
        } if is_const_int(rhs, 7) => !matches!(lhs.as_ref(), HirExpr::Const(_, _)),
        _ => false,
    }
}

fn is_output_store_stmt(stmt: &HirStmt) -> bool {
    matches!(
        stmt,
        HirStmt::Assign {
            lhs: HirLValue::Deref { .. } | HirLValue::Index { .. },
            ..
        }
    )
}

fn is_pointer_increment_stmt(stmt: &HirStmt) -> bool {
    let HirStmt::Assign {
        lhs,
        rhs,
    } = stmt
    else {
        return false;
    };
    let Some(lhs_key) = lvalue_location_key(lhs) else {
        return false;
    };
    let HirExpr::Binary {
        op: HirBinaryOp::Add | HirBinaryOp::Sub,
        lhs,
        rhs,
        ..
    } = rhs
    else {
        return false;
    };
    (expr_location_key(lhs).as_deref() == Some(lhs_key.as_str()) && is_const_int(rhs, 1))
        || (expr_location_key(rhs).as_deref() == Some(lhs_key.as_str()) && is_const_int(lhs, 1))
}

fn is_shift_byte_stmt(stmt: &HirStmt) -> bool {
    let HirStmt::Assign {
        lhs,
        rhs,
    } = stmt
    else {
        return false;
    };
    let Some(lhs_key) = lvalue_location_key(lhs) else {
        return false;
    };
    let HirExpr::Binary {
        op: HirBinaryOp::Shr | HirBinaryOp::Sar | HirBinaryOp::Shl | HirBinaryOp::Div | HirBinaryOp::Mul,
        lhs,
        rhs,
        ..
    } = rhs
    else {
        return false;
    };
    let matches_self = expr_location_key(lhs).as_deref() == Some(lhs_key.as_str());
    if !matches_self {
        return false;
    }
    is_const_int(rhs, 8) || is_const_int(rhs, 256)
}

fn is_bitcount_adjust_stmt(stmt: &HirStmt) -> bool {
    let HirStmt::Assign {
        lhs,
        rhs,
    } = stmt
    else {
        return false;
    };
    let Some(lhs_key) = lvalue_location_key(lhs) else {
        return false;
    };
    let HirExpr::Binary {
        op: HirBinaryOp::Sub | HirBinaryOp::Add,
        lhs,
        rhs,
        ..
    } = rhs
    else {
        return false;
    };
    expr_location_key(lhs).as_deref() == Some(lhs_key.as_str()) && is_const_int(rhs, 8)
}

fn is_table_lookup_expr(expr: &HirExpr) -> bool {
    match expr {
        HirExpr::Index { .. } | HirExpr::Load { .. } => true,
        HirExpr::Cast { expr, .. } => is_table_lookup_expr(expr),
        _ => false,
    }
}

fn is_const_int(expr: &HirExpr, expected: i64) -> bool {
    matches!(expr, HirExpr::Const(value, _) if *value == expected)
}

fn lvalue_location_key(lhs: &HirLValue) -> Option<String> {
    match lhs {
        HirLValue::Var(name) => Some(name.clone()),
        HirLValue::Index { base, index, .. } => {
            Some(format!("{}[{}]", print_expr(base), print_expr(index)))
        }
        HirLValue::Deref { .. } => None,
    }
}

fn expr_location_key(expr: &HirExpr) -> Option<String> {
    match expr {
        HirExpr::Var(name) => Some(name.clone()),
        HirExpr::Index { base, index, .. } => {
            Some(format!("{}[{}]", print_expr(base), print_expr(index)))
        }
        HirExpr::Cast { expr, .. } => expr_location_key(expr),
        HirExpr::PtrOffset { base, offset } if *offset == 0 => expr_location_key(base),
        _ => None,
    }
}
