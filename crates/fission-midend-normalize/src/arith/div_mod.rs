use crate::prelude::*;
use super::util::*;

pub fn recognize_mod_div_power_of_two(expr: &HirExpr) -> Option<HirExpr> {
    normalize_signed_power_of_two_mod(expr)
        .or_else(|| normalize_signed_power_of_two_div(expr))
        .or_else(|| normalize_unsigned_power_of_two_mod(expr))
        .or_else(|| normalize_unsigned_power_of_two_div(expr))
        .or_else(|| collapse_cdq_style_signed_mod_div(expr))
}

/// Collapse CDQ/IDIV-style wide dividends: `((hi << k) | lo) % d` / `/ d`
/// when `hi` is a sign-fill of `lo` (SAR by k-1 or k).
///
/// Measured on x86 `cdq; idiv` remainder for signed `a % b` (gcd-class loops).
/// Fold CDQ-style wide dividends across `t = wide; …; x = t % d` in a stmt list.
pub fn collapse_cdq_signed_mod_in_stmts(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;
    let mut pure: std::collections::HashMap<String, HirExpr> = std::collections::HashMap::new();
    for stmt in stmts.iter() {
        if let HirStmt::Assign {
            lhs: HirLValue::Var(name),
            rhs,
        } = stmt
        {
            if extract_cdq_low_from_wide_dividend(rhs).is_some() {
                pure.insert(name.clone(), rhs.clone());
            }
        }
    }
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Assign { lhs, rhs } => {
                // Direct collapse: x = wide % d
                if let Some(collapsed) = collapse_cdq_style_signed_mod_div(rhs) {
                    *rhs = collapsed;
                    changed = true;
                    continue;
                }
                // Across temp: t = wide; x = t % d
                if let HirExpr::Binary {
                    op: bin_op @ (HirBinaryOp::Mod | HirBinaryOp::Div),
                    lhs: mod_lhs,
                    rhs: div,
                    ty,
                } = rhs
                {
                    if let HirExpr::Var(name) = mod_lhs.as_ref() {
                        if let Some(wide) = pure.get(name) {
                            let candidate = HirExpr::Binary {
                                op: *bin_op,
                                lhs: Box::new(wide.clone()),
                                rhs: div.clone(),
                                ty: ty.clone(),
                            };
                            if let Some(collapsed) = collapse_cdq_style_signed_mod_div(&candidate) {
                                *rhs = collapsed;
                                let _ = lhs;
                                changed = true;
                            }
                        }
                    }
                }
            }
            HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
                changed |= collapse_cdq_signed_mod_in_stmts(body);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= collapse_cdq_signed_mod_in_stmts(then_body);
                changed |= collapse_cdq_signed_mod_in_stmts(else_body);
            }
            HirStmt::For {
                init, update, body, ..
            } => {
                if let Some(i) = init {
                    if let HirStmt::Block(b) = i.as_mut() {
                        changed |= collapse_cdq_signed_mod_in_stmts(b);
                    }
                }
                if let Some(u) = update {
                    if let HirStmt::Block(b) = u.as_mut() {
                        changed |= collapse_cdq_signed_mod_in_stmts(b);
                    }
                }
                changed |= collapse_cdq_signed_mod_in_stmts(body);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= collapse_cdq_signed_mod_in_stmts(&mut case.body);
                }
                changed |= collapse_cdq_signed_mod_in_stmts(default);
            }
            _ => {}
        }
    }
    changed
}
pub fn collapse_cdq_style_signed_mod_div(expr: &HirExpr) -> Option<HirExpr> {
    let HirExpr::Binary {
        op,
        lhs,
        rhs,
        ty: _,
    } = expr
    else {
        return None;
    };
    if !matches!(op, HirBinaryOp::Mod | HirBinaryOp::Div) {
        return None;
    }
    let low = extract_cdq_low_from_wide_dividend(lhs)?;
    let bits = match expr_type(&low) {
        NirType::Int { bits, .. } => bits.max(32),
        _ => 32,
    };
    let signed_ty = NirType::Int {
        bits,
        signed: true,
    };
    Some(HirExpr::Binary {
        op: *op,
        lhs: Box::new(HirExpr::Cast {
            ty: signed_ty.clone(),
            expr: Box::new(low),
        }),
        rhs: Box::new(HirExpr::Cast {
            ty: signed_ty.clone(),
            expr: Box::new((**rhs).clone()),
        }),
        ty: signed_ty,
    })
}

fn extract_cdq_low_from_wide_dividend(expr: &HirExpr) -> Option<HirExpr> {
    let expr = strip_casts(expr);
    // (hi << k) | lo
    let HirExpr::Binary {
        op: HirBinaryOp::Or,
        lhs: or_lhs,
        rhs: or_rhs,
        ..
    } = expr
    else {
        return None;
    };
    let (hi_expr, shift_expr, low_expr) = match (or_lhs.as_ref(), or_rhs.as_ref()) {
        (
            HirExpr::Binary {
                op: HirBinaryOp::Shl,
                lhs: hi,
                rhs: shift,
                ..
            },
            low,
        ) => (hi.as_ref(), shift.as_ref(), low),
        (
            low,
            HirExpr::Binary {
                op: HirBinaryOp::Shl,
                lhs: hi,
                rhs: shift,
                ..
            },
        ) => (hi.as_ref(), shift.as_ref(), low),
        _ => return None,
    };
    let HirExpr::Const(shift_amt, _) = shift_expr else {
        return None;
    };
    if !(32..=64).contains(shift_amt) {
        return None;
    }
    let low = strip_casts(low_expr);
    // hi should be sign-related to low: SAR(low, …) or cast of SAR.
    let hi = strip_casts(hi_expr);
    if expr_is_sign_fill_of(&hi, &low, *shift_amt) {
        return Some(low);
    }
    None
}

fn expr_is_sign_fill_of(hi: &HirExpr, low: &HirExpr, shift_amt: i64) -> bool {
    let hi = strip_casts(hi);
    match hi {
        HirExpr::Binary {
            op: HirBinaryOp::Sar | HirBinaryOp::Shr,
            lhs,
            rhs: shift,
            ..
        } => {
            let HirExpr::Const(s, _) = shift.as_ref() else {
                return false;
            };
            let base = strip_casts(lhs.as_ref());
            // Allow SAR base to be cast/widen of low.
            let base_ok = base == *low
                || matches!(
                    &base,
                    HirExpr::Cast { expr, .. } if strip_casts(expr.as_ref()) == *low
                );
            let shift_ok = *s == shift_amt || *s == shift_amt - 1 || *s == 31 || *s == 63;
            base_ok && shift_ok
        }
        _ => false,
    }
}

pub fn recognize_compiler_runtime_division(expr: &HirExpr) -> Option<HirExpr> {
    let HirExpr::Call {
        target, args, ty, ..
    } = expr
    else {
        return None;
    };
    let (op, signed) = match target.as_str() {
        "__aeabi_uidiv" | "__aeabi_uidivmod" => (HirBinaryOp::Div, false),
        "__aeabi_idiv" | "__aeabi_idivmod" => (HirBinaryOp::Div, true),
        _ => return None,
    };
    if args.len() < 2 {
        return None;
    }
    let bits = match ty {
        NirType::Int { bits, .. } => *bits,
        _ => 32,
    };
    Some(HirExpr::Binary {
        op,
        lhs: Box::new(cast_runtime_div_arg(args[0].clone(), bits, signed)),
        rhs: Box::new(cast_runtime_div_arg(args[1].clone(), bits, signed)),
        ty: NirType::Int { bits, signed },
    })
}

fn cast_runtime_div_arg(expr: HirExpr, bits: u32, signed: bool) -> HirExpr {
    let target_ty = NirType::Int { bits, signed };
    if expr_type(&expr) == target_ty {
        expr
    } else {
        HirExpr::Cast {
            ty: target_ty,
            expr: Box::new(expr),
        }
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
    if is_full_mask_const(rhs.as_ref(), &expr_type(lhs)) {
        return None;
    }
    let divisor = (*mask as i128) + 1;
    if divisor <= 1 || (divisor & (divisor - 1)) != 0 {
        return None;
    }
    // Do not normalize `x & 1` to `x % 2` to preserve bitwise operations.
    if divisor == 2 {
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
    // Do not normalize `x >> 1` to `x / 2` to preserve bitwise operations.
    if *shift_amount == 1 {
        return None;
    }
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
    if sign_source != add_lhs.as_ref() && sign_source != add_rhs.as_ref() {
        return None;
    }
    let div_lhs = if sign_source == add_lhs.as_ref() {
        add_lhs.clone()
    } else {
        add_rhs.clone()
    };

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
        lhs: div_lhs,
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

pub fn recognize_magic_number_division(expr: &HirExpr) -> Option<HirExpr> {
    let mut current = expr;
    let mut n = 0u32;
    let mut x_size_bits = None;
    let mut ext_ty_bits = None;

    if let HirExpr::Cast { ty, expr: inner } = current {
        if let Some(bits) = int_type_bits(ty) {
            ext_ty_bits = Some(bits);
            current = inner.as_ref();
        }
    }

    let mut is_signed_shift = false;
    if let HirExpr::Binary { op, lhs, rhs, .. } = current {
        if matches!(op, HirBinaryOp::Shr | HirBinaryOp::Sar) {
            if let HirExpr::Const(shift_amt, _) = rhs.as_ref() {
                n += *shift_amt as u32;
                is_signed_shift = matches!(op, HirBinaryOp::Sar);
                current = lhs.as_ref();
            }
        }
    }

    if let HirExpr::Binary {
        op: HirBinaryOp::Mul,
        lhs,
        rhs,
        ty: mul_ty,
    } = current
    {
        let (x_ext, y_expr) = if let HirExpr::Const(_, _) = rhs.as_ref() {
            (lhs.as_ref(), rhs.as_ref())
        } else if let HirExpr::Const(_, _) = lhs.as_ref() {
            (rhs.as_ref(), lhs.as_ref())
        } else {
            return None;
        };

        let HirExpr::Const(y_val, _) = y_expr else {
            return None;
        };

        if let Some(bits) = int_type_bits(mul_ty) {
            let mask = if bits == 64 {
                u64::MAX
            } else {
                (1u64 << bits) - 1
            };
            let y_128 = ((*y_val as u64) & mask) as u128;

            let mut x_val = x_ext;
            let mut is_sext = false;

            if let HirExpr::Cast {
                ty: cast_ty,
                expr: original_x,
            } = x_ext
            {
                if let NirType::Int {
                    bits: orig_bits,
                    signed,
                } = expr_type(original_x.as_ref())
                {
                    x_size_bits = Some(orig_bits);
                    is_sext = signed;
                }
                x_val = original_x.as_ref();
            } else if let NirType::Int {
                bits: orig_bits,
                signed,
            } = expr_type(x_ext)
            {
                x_size_bits = Some(orig_bits);
                is_sext = signed;
            }

            if let Some(x_bits) = x_size_bits {
                if n <= 127 && x_bits <= 64 && y_128 > 1 {
                    let y_minus_1 = y_128 - 1;
                    let power = 1u128 << n;
                    let mut q = power / y_minus_1;
                    let mut r = power % y_minus_1;

                    if q <= u64::MAX as u128 && y_minus_1 >= q {
                        let mut diff = 0;
                        if r >= q {
                            q += 1;
                            r = r.wrapping_sub(y_minus_1).wrapping_add(q);
                            if r >= q {
                                // invalid magic
                            } else {
                                diff = q;
                            }
                        } else {
                            diff = 0;
                        }

                        let mut maxx = if x_bits == 64 { 0 } else { 1u128 << x_bits };
                        maxx = maxx.wrapping_sub(1);
                        diff += q.saturating_sub(r);

                        if diff != 0 {
                            let tmp = power / diff;
                            if tmp > maxx {
                                let divisor = q as u64;
                                let _out_bits = ext_ty_bits.unwrap_or(x_bits);
                                // Return the recovered division
                                let div = HirExpr::Binary {
                                    op: HirBinaryOp::Div,
                                    lhs: Box::new(x_val.clone()),
                                    rhs: Box::new(HirExpr::Const(divisor as i64, expr_type(x_val))),
                                    ty: expr_type(x_val),
                                };
                                return Some(if expr_type(expr) == expr_type(x_val) {
                                    div
                                } else {
                                    HirExpr::Cast {
                                        ty: expr_type(expr),
                                        expr: Box::new(div),
                                    }
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
// prelude via parent

    fn var(name: &str) -> HirExpr {
        HirExpr::Var(name.to_string())
    }

    #[test]
    fn recognizes_arm_eabi_unsigned_division_helper() {
        let expr = HirExpr::Call {
            target: "__aeabi_uidiv".to_string(),
            args: vec![var("numerator"), var("denominator"), var("dead_r2")],
            ty: NirType::Unknown,
        };

        let normalized = recognize_compiler_runtime_division(&expr).expect("runtime div");

        assert_eq!(
            normalized,
            HirExpr::Binary {
                op: HirBinaryOp::Div,
                lhs: Box::new(HirExpr::Cast {
                    ty: NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                    expr: Box::new(var("numerator")),
                }),
                rhs: Box::new(HirExpr::Cast {
                    ty: NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                    expr: Box::new(var("denominator")),
                }),
                ty: NirType::Int {
                    bits: 32,
                    signed: false,
                },
            }
        );
    }
}

#[cfg(test)]
mod cdq_tests {
    use super::*;

    fn i32_ty() -> NirType {
        NirType::Int {
            bits: 32,
            signed: true,
        }
    }
    fn u64_ty() -> NirType {
        NirType::Int {
            bits: 64,
            signed: false,
        }
    }

    #[test]
    fn collapse_cdq_style_mod_from_piece_or_pattern() {
        // ((longlong)x >> 32)<<32 | (ulonglong)x  % y
        let x = HirExpr::Var("param_10".into());
        let y = HirExpr::Var("param_18".into());
        let sar = HirExpr::Binary {
            op: HirBinaryOp::Sar,
            lhs: Box::new(HirExpr::Cast {
                ty: NirType::Int {
                    bits: 64,
                    signed: true,
                },
                expr: Box::new(x.clone()),
            }),
            rhs: Box::new(HirExpr::Const(32, i32_ty())),
            ty: NirType::Int {
                bits: 64,
                signed: true,
            },
        };
        let hi = HirExpr::Cast {
            ty: u64_ty(),
            expr: Box::new(HirExpr::Cast {
                ty: i32_ty(),
                expr: Box::new(sar),
            }),
        };
        let wide = HirExpr::Binary {
            op: HirBinaryOp::Or,
            lhs: Box::new(HirExpr::Binary {
                op: HirBinaryOp::Shl,
                lhs: Box::new(hi),
                rhs: Box::new(HirExpr::Const(32, i32_ty())),
                ty: u64_ty(),
            }),
            rhs: Box::new(HirExpr::Cast {
                ty: u64_ty(),
                expr: Box::new(x),
            }),
            ty: u64_ty(),
        };
        let expr = HirExpr::Binary {
            op: HirBinaryOp::Mod,
            lhs: Box::new(wide),
            rhs: Box::new(y),
            ty: u64_ty(),
        };
        let out = collapse_cdq_style_signed_mod_div(&expr).expect("collapse");
        match out {
            HirExpr::Binary {
                op: HirBinaryOp::Mod,
                lhs,
                rhs,
                ty: NirType::Int { signed: true, .. },
            } => {
                assert!(matches!(strip_casts(&lhs), HirExpr::Var(n) if n == "param_10"));
                assert!(matches!(strip_casts(&rhs), HirExpr::Var(n) if n == "param_18"));
            }
            other => panic!("unexpected: {other:?}"),
        }
    }
}
