use super::util::*;
use crate::prelude::*;

pub fn recognize_mod_div_power_of_two(expr: &DirExpr) -> Option<DirExpr> {
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
///
/// Sequential scan (not whole-block last-wins):
/// - bind `t → wide` only while `t` is live with that CDQ RHS;
/// - kill `t` on any reassign of `t`;
/// - kill entries whose free vars are redefined before the mod/div use.
pub fn collapse_cdq_signed_mod_in_stmts(stmts: &mut Vec<DirStmt>) -> bool {
    let mut changed = false;
    // Live CDQ temps: name → (wide expr, free vars of wide).
    let mut live: crate::HashMap<String, (DirExpr, crate::HashSet<String>)> =
        crate::HashMap::default();

    for i in 0..stmts.len() {
        // Nested structures first (independent scopes).
        match &mut stmts[i] {
            DirStmt::Block(body) | DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
                changed |= collapse_cdq_signed_mod_in_stmts(body);
                // Conservative: control transfer may clobber linear live set.
                live.clear();
                continue;
            }
            DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= collapse_cdq_signed_mod_in_stmts(then_body);
                changed |= collapse_cdq_signed_mod_in_stmts(else_body);
                live.clear();
                continue;
            }
            DirStmt::For {
                init, update, body, ..
            } => {
                if let Some(init_stmt) = init {
                    if let DirStmt::Block(b) = init_stmt.as_mut() {
                        changed |= collapse_cdq_signed_mod_in_stmts(b);
                    }
                }
                if let Some(upd) = update {
                    if let DirStmt::Block(b) = upd.as_mut() {
                        changed |= collapse_cdq_signed_mod_in_stmts(b);
                    }
                }
                changed |= collapse_cdq_signed_mod_in_stmts(body);
                live.clear();
                continue;
            }
            DirStmt::Switch { cases, default, .. } => {
                for case in cases.iter_mut() {
                    changed |= collapse_cdq_signed_mod_in_stmts(&mut case.body);
                }
                changed |= collapse_cdq_signed_mod_in_stmts(default);
                live.clear();
                continue;
            }
            _ => {}
        }

        // Kill live temps whose free vars are written by this stmt's LHS.
        if let DirStmt::Assign {
            lhs: DirLValue::Var(written),
            ..
        } = &stmts[i]
        {
            let written = written.clone();
            live.retain(|_k, (_wide, frees)| !frees.contains(&written));
            // Kill the temp itself on any reassignment.
            live.remove(&written);
        }

        let DirStmt::Assign { lhs, rhs } = &mut stmts[i] else {
            continue;
        };

        // Direct collapse: x = wide % d
        if let Some(collapsed) = collapse_cdq_style_signed_mod_div(rhs) {
            *rhs = collapsed;
            changed = true;
            // After collapse, if lhs is a name, it is not a CDQ wide temp.
            if let DirLValue::Var(name) = lhs {
                live.remove(name);
            }
            continue;
        }

        // Across temp: t = wide; …; x = t % d  (t still live, free vars not killed).
        if let DirExpr::Binary {
            op: bin_op @ (DirBinaryOp::Mod | DirBinaryOp::Div),
            lhs: mod_lhs,
            rhs: div,
            ty,
        } = rhs
        {
            if let DirExpr::Var(name) = mod_lhs.as_ref() {
                if let Some((wide, _frees)) = live.get(name) {
                    let candidate = DirExpr::Binary {
                        op: *bin_op,
                        lhs: Box::new(wide.clone()),
                        rhs: div.clone(),
                        ty: ty.clone(),
                    };
                    if let Some(collapsed) = collapse_cdq_style_signed_mod_div(&candidate) {
                        *rhs = collapsed;
                        changed = true;
                    }
                }
            }
        }

        // Bind or rebind after processing uses (so same-stmt x = t % d cannot
        // use a pure just bound on this line).
        if let DirLValue::Var(name) = lhs {
            if extract_cdq_low_from_wide_dividend(rhs).is_some() {
                let mut frees = crate::HashSet::default();
                collect_free_var_names(rhs, &mut frees);
                live.insert(name.clone(), (rhs.clone(), frees));
            } else {
                live.remove(name);
            }
        }
    }
    changed
}

fn collect_free_var_names(expr: &DirExpr, out: &mut crate::HashSet<String>) {
    match expr {
        DirExpr::Var(n) => {
            out.insert(n.clone());
        }
        DirExpr::AddressOfGlobal(_) | DirExpr::Const(_, _) => {}
        DirExpr::Unary { expr, .. } | DirExpr::Cast { expr, .. } => {
            collect_free_var_names(expr, out);
        }
        DirExpr::Binary { lhs, rhs, .. } => {
            collect_free_var_names(lhs, out);
            collect_free_var_names(rhs, out);
        }
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_free_var_names(cond, out);
            collect_free_var_names(then_expr, out);
            collect_free_var_names(else_expr, out);
        }
        DirExpr::Call { args, .. } => {
            for a in args {
                collect_free_var_names(a, out);
            }
        }
        DirExpr::Load { ptr, .. }
        | DirExpr::PtrOffset { base: ptr, .. }
        | DirExpr::FieldAccess { base: ptr, .. }
        | DirExpr::AggregateCopy { src: ptr, .. } => collect_free_var_names(ptr, out),
        DirExpr::Index { base, index, .. } => {
            collect_free_var_names(base, out);
            collect_free_var_names(index, out);
        }
    }
}
pub fn collapse_cdq_style_signed_mod_div(expr: &DirExpr) -> Option<DirExpr> {
    let DirExpr::Binary {
        op,
        lhs,
        rhs,
        ty: _,
    } = expr
    else {
        return None;
    };
    if !matches!(op, DirBinaryOp::Mod | DirBinaryOp::Div) {
        return None;
    }
    let low = extract_cdq_low_from_wide_dividend(lhs)?;
    let bits = match expr_type(&low) {
        NirType::Int { bits, .. } => bits.max(32),
        _ => 32,
    };
    let signed_ty = NirType::Int { bits, signed: true };
    Some(DirExpr::Binary {
        op: *op,
        lhs: Box::new(DirExpr::Cast {
            ty: signed_ty.clone(),
            expr: Box::new(low),
        }),
        rhs: Box::new(DirExpr::Cast {
            ty: signed_ty.clone(),
            expr: Box::new((**rhs).clone()),
        }),
        ty: signed_ty,
    })
}

fn extract_cdq_low_from_wide_dividend(expr: &DirExpr) -> Option<DirExpr> {
    let expr = strip_casts(expr);
    // (hi << k) | lo
    let DirExpr::Binary {
        op: DirBinaryOp::Or,
        lhs: or_lhs,
        rhs: or_rhs,
        ..
    } = expr
    else {
        return None;
    };
    let (hi_expr, shift_expr, low_expr) = match (or_lhs.as_ref(), or_rhs.as_ref()) {
        (
            DirExpr::Binary {
                op: DirBinaryOp::Shl,
                lhs: hi,
                rhs: shift,
                ..
            },
            low,
        ) => (hi.as_ref(), shift.as_ref(), low),
        (
            low,
            DirExpr::Binary {
                op: DirBinaryOp::Shl,
                lhs: hi,
                rhs: shift,
                ..
            },
        ) => (hi.as_ref(), shift.as_ref(), low),
        _ => return None,
    };
    let DirExpr::Const(shift_amt, _) = shift_expr else {
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

fn expr_is_sign_fill_of(hi: &DirExpr, low: &DirExpr, shift_amt: i64) -> bool {
    // Only arithmetic SAR is CDQ-class sign-fill. Logical Shr is not (negative
    // low logical high ≠ sign bits). Materialize emits Sar for SubPiece of
    // signed IntSExt; residual must match that and reject bare Shr.
    let hi = strip_casts(hi);
    match hi {
        DirExpr::Binary {
            op: DirBinaryOp::Sar,
            lhs,
            rhs: shift,
            ..
        } => {
            let DirExpr::Const(s, _) = shift.as_ref() else {
                return false;
            };
            let base = strip_casts(lhs.as_ref());
            let base_ok = base == *low
                || matches!(
                    &base,
                    DirExpr::Cast { expr, .. } if strip_casts(expr.as_ref()) == *low
                );
            let shift_ok = *s == shift_amt || *s == shift_amt - 1 || *s == 31 || *s == 63;
            base_ok && shift_ok
        }
        _ => false,
    }
}

pub fn recognize_compiler_runtime_division(expr: &DirExpr) -> Option<DirExpr> {
    let DirExpr::Call {
        target, args, ty, ..
    } = expr
    else {
        return None;
    };
    let (op, signed) = match target.as_str() {
        "__aeabi_uidiv" | "__aeabi_uidivmod" => (DirBinaryOp::Div, false),
        "__aeabi_idiv" | "__aeabi_idivmod" => (DirBinaryOp::Div, true),
        _ => return None,
    };
    if args.len() < 2 {
        return None;
    }
    let bits = match ty {
        NirType::Int { bits, .. } => *bits,
        _ => 32,
    };
    Some(DirExpr::Binary {
        op,
        lhs: Box::new(cast_runtime_div_arg(args[0].clone(), bits, signed)),
        rhs: Box::new(cast_runtime_div_arg(args[1].clone(), bits, signed)),
        ty: NirType::Int { bits, signed },
    })
}

fn cast_runtime_div_arg(expr: DirExpr, bits: u32, signed: bool) -> DirExpr {
    let target_ty = NirType::Int { bits, signed };
    if expr_type(&expr) == target_ty {
        expr
    } else {
        DirExpr::Cast {
            ty: target_ty,
            expr: Box::new(expr),
        }
    }
}

fn normalize_unsigned_power_of_two_mod(expr: &DirExpr) -> Option<DirExpr> {
    let DirExpr::Binary {
        op: DirBinaryOp::And,
        lhs,
        rhs,
        ..
    } = expr
    else {
        return None;
    };
    let DirExpr::Const(
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
    Some(DirExpr::Binary {
        op: DirBinaryOp::Mod,
        lhs: lhs.clone(),
        rhs: Box::new(DirExpr::Const(
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

fn normalize_unsigned_power_of_two_div(expr: &DirExpr) -> Option<DirExpr> {
    let DirExpr::Binary {
        op: DirBinaryOp::Shr,
        lhs,
        rhs,
        ty,
    } = expr
    else {
        return None;
    };
    let DirExpr::Const(shift_amount, _) = rhs.as_ref() else {
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
    Some(DirExpr::Binary {
        op: DirBinaryOp::Div,
        lhs: lhs.clone(),
        rhs: Box::new(DirExpr::Const(
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

fn normalize_signed_power_of_two_mod(expr: &DirExpr) -> Option<DirExpr> {
    let DirExpr::Binary {
        op: DirBinaryOp::Sub,
        lhs,
        rhs,
        ty,
    } = expr
    else {
        return None;
    };
    if let DirExpr::Binary {
        op: DirBinaryOp::Shl,
        lhs: shl_inner,
        rhs: shl_rhs,
        ..
    } = rhs.as_ref()
    {
        let DirExpr::Const(shift_amount, _) = shl_rhs.as_ref() else {
            return None;
        };
        let DirExpr::Binary {
            op: DirBinaryOp::Div,
            lhs: div_lhs,
            rhs: div_rhs,
            ..
        } = shl_inner.as_ref()
        else {
            return None;
        };
        let DirExpr::Const(divisor, _) = div_rhs.as_ref() else {
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
            return Some(DirExpr::Binary {
                op: DirBinaryOp::Mod,
                lhs: lhs.clone(),
                rhs: Box::new(DirExpr::Const(
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
    let DirExpr::Binary {
        op: DirBinaryOp::Shl,
        lhs: shl_inner,
        rhs: shl_rhs,
        ..
    } = rhs.as_ref()
    else {
        return None;
    };
    let DirExpr::Const(shift_amount, _) = shl_rhs.as_ref() else {
        return None;
    };
    let DirExpr::Binary {
        op: DirBinaryOp::Sar,
        lhs: sar_inner,
        rhs: sar_rhs,
        ..
    } = shl_inner.as_ref()
    else {
        return None;
    };
    let DirExpr::Const(sar_shift, _) = sar_rhs.as_ref() else {
        return None;
    };
    if sar_shift != shift_amount {
        return None;
    }
    let DirExpr::Binary {
        op: DirBinaryOp::Add,
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
        DirExpr::Binary {
            op: DirBinaryOp::And,
            lhs: and_lhs,
            rhs: and_rhs,
            ..
        } => {
            let DirExpr::Binary {
                op: DirBinaryOp::Shr,
                lhs: shr_lhs,
                rhs: shr_rhs,
                ..
            } = and_lhs.as_ref()
            else {
                return None;
            };
            let DirExpr::Const(sign_shift, _) = shr_rhs.as_ref() else {
                return None;
            };
            let DirExpr::Const(mask, _) = and_rhs.as_ref() else {
                return None;
            };
            (shr_lhs.as_ref(), *sign_shift, *mask)
        }
        DirExpr::Binary {
            op: DirBinaryOp::Mod,
            lhs: mod_lhs,
            rhs: mod_rhs,
            ..
        } => {
            let DirExpr::Binary {
                op: DirBinaryOp::Shr,
                lhs: shr_lhs,
                rhs: shr_rhs,
                ..
            } = mod_lhs.as_ref()
            else {
                return None;
            };
            let DirExpr::Const(sign_shift, _) = shr_rhs.as_ref() else {
                return None;
            };
            let DirExpr::Const(divisor, _) = mod_rhs.as_ref() else {
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

    Some(DirExpr::Binary {
        op: DirBinaryOp::Mod,
        lhs: lhs.clone(),
        rhs: Box::new(DirExpr::Const(
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

fn normalize_signed_power_of_two_div(expr: &DirExpr) -> Option<DirExpr> {
    let DirExpr::Binary {
        op: DirBinaryOp::Sar,
        lhs,
        rhs,
        ty,
    } = expr
    else {
        return None;
    };
    let DirExpr::Const(shift_amount, _) = rhs.as_ref() else {
        return None;
    };
    let DirExpr::Binary {
        op: DirBinaryOp::Add,
        lhs: add_lhs,
        rhs: add_rhs,
        ..
    } = lhs.as_ref()
    else {
        return None;
    };
    let (sign_source, sign_shift, mask) = match add_rhs.as_ref() {
        DirExpr::Binary {
            op: DirBinaryOp::And,
            lhs: and_lhs,
            rhs: and_rhs,
            ..
        } => {
            let DirExpr::Binary {
                op: DirBinaryOp::Shr,
                lhs: shr_lhs,
                rhs: shr_rhs,
                ..
            } = and_lhs.as_ref()
            else {
                return None;
            };
            let DirExpr::Const(sign_shift, _) = shr_rhs.as_ref() else {
                return None;
            };
            let DirExpr::Const(mask, _) = and_rhs.as_ref() else {
                return None;
            };
            (shr_lhs.as_ref(), *sign_shift, *mask)
        }
        DirExpr::Binary {
            op: DirBinaryOp::Mod,
            lhs: mod_lhs,
            rhs: mod_rhs,
            ..
        } => {
            let DirExpr::Binary {
                op: DirBinaryOp::Shr,
                lhs: shr_lhs,
                rhs: shr_rhs,
                ..
            } = mod_lhs.as_ref()
            else {
                return None;
            };
            let DirExpr::Const(sign_shift, _) = shr_rhs.as_ref() else {
                return None;
            };
            let DirExpr::Const(divisor, _) = mod_rhs.as_ref() else {
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

    Some(DirExpr::Binary {
        op: DirBinaryOp::Div,
        lhs: div_lhs,
        rhs: Box::new(DirExpr::Const(
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

pub fn recognize_magic_number_division(expr: &DirExpr) -> Option<DirExpr> {
    let mut current = expr;
    let mut n = 0u32;
    let mut x_size_bits = None;
    let mut ext_ty_bits = None;

    if let DirExpr::Cast { ty, expr: inner } = current {
        if let Some(bits) = int_type_bits(ty) {
            ext_ty_bits = Some(bits);
            current = inner.as_ref();
        }
    }

    let mut is_signed_shift = false;
    if let DirExpr::Binary { op, lhs, rhs, .. } = current {
        if matches!(op, DirBinaryOp::Shr | DirBinaryOp::Sar) {
            if let DirExpr::Const(shift_amt, _) = rhs.as_ref() {
                n += *shift_amt as u32;
                is_signed_shift = matches!(op, DirBinaryOp::Sar);
                current = lhs.as_ref();
            }
        }
    }

    if let DirExpr::Binary {
        op: DirBinaryOp::Mul,
        lhs,
        rhs,
        ty: mul_ty,
    } = current
    {
        let (x_ext, y_expr) = if let DirExpr::Const(_, _) = rhs.as_ref() {
            (lhs.as_ref(), rhs.as_ref())
        } else if let DirExpr::Const(_, _) = lhs.as_ref() {
            (rhs.as_ref(), lhs.as_ref())
        } else {
            return None;
        };

        let DirExpr::Const(y_val, _) = y_expr else {
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

            if let DirExpr::Cast {
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
                                let div = DirExpr::Binary {
                                    op: DirBinaryOp::Div,
                                    lhs: Box::new(x_val.clone()),
                                    rhs: Box::new(DirExpr::Const(divisor as i64, expr_type(x_val))),
                                    ty: expr_type(x_val),
                                };
                                return Some(if expr_type(expr) == expr_type(x_val) {
                                    div
                                } else {
                                    DirExpr::Cast {
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

    fn var(name: &str) -> DirExpr {
        DirExpr::Var(name.to_string())
    }

    #[test]
    fn recognizes_arm_eabi_unsigned_division_helper() {
        let expr = DirExpr::Call {
            target: "__aeabi_uidiv".to_string(),
            args: vec![var("numerator"), var("denominator"), var("dead_r2")],
            ty: NirType::Unknown,
        };

        let normalized = recognize_compiler_runtime_division(&expr).expect("runtime div");

        assert_eq!(
            normalized,
            DirExpr::Binary {
                op: DirBinaryOp::Div,
                lhs: Box::new(DirExpr::Cast {
                    ty: NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                    expr: Box::new(var("numerator")),
                }),
                rhs: Box::new(DirExpr::Cast {
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

    fn cdq_wide(low_name: &str) -> DirExpr {
        let x = DirExpr::Var(low_name.into());
        let sar = DirExpr::Binary {
            op: DirBinaryOp::Sar,
            lhs: Box::new(DirExpr::Cast {
                ty: NirType::Int {
                    bits: 64,
                    signed: true,
                },
                expr: Box::new(x.clone()),
            }),
            rhs: Box::new(DirExpr::Const(32, i32_ty())),
            ty: NirType::Int {
                bits: 64,
                signed: true,
            },
        };
        let hi = DirExpr::Cast {
            ty: u64_ty(),
            expr: Box::new(DirExpr::Cast {
                ty: i32_ty(),
                expr: Box::new(sar),
            }),
        };
        DirExpr::Binary {
            op: DirBinaryOp::Or,
            lhs: Box::new(DirExpr::Binary {
                op: DirBinaryOp::Shl,
                lhs: Box::new(hi),
                rhs: Box::new(DirExpr::Const(32, i32_ty())),
                ty: u64_ty(),
            }),
            rhs: Box::new(DirExpr::Cast {
                ty: u64_ty(),
                expr: Box::new(x),
            }),
            ty: u64_ty(),
        }
    }

    fn mod_vars(lhs: DirExpr, rhs_name: &str) -> DirExpr {
        DirExpr::Binary {
            op: DirBinaryOp::Mod,
            lhs: Box::new(lhs),
            rhs: Box::new(DirExpr::Var(rhs_name.into())),
            ty: u64_ty(),
        }
    }

    #[test]
    fn collapse_cdq_style_mod_from_piece_or_pattern() {
        let expr = mod_vars(cdq_wide("param_10"), "param_18");
        let out = collapse_cdq_style_signed_mod_div(&expr).expect("collapse");
        match out {
            DirExpr::Binary {
                op: DirBinaryOp::Mod,
                lhs,
                rhs,
                ty: NirType::Int { signed: true, .. },
            } => {
                assert!(matches!(strip_casts(&lhs), DirExpr::Var(n) if n == "param_10"));
                assert!(matches!(strip_casts(&rhs), DirExpr::Var(n) if n == "param_18"));
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn stmt_collapse_adjacent_temp_fold() {
        // t = wide(a); x = t % d  →  x = (int)a % (int)d
        let mut stmts = vec![
            DirStmt::Assign {
                lhs: DirLValue::Var("t".into()),
                rhs: cdq_wide("a"),
            },
            DirStmt::Assign {
                lhs: DirLValue::Var("x".into()),
                rhs: mod_vars(DirExpr::Var("t".into()), "d"),
            },
        ];
        assert!(collapse_cdq_signed_mod_in_stmts(&mut stmts));
        match &stmts[1] {
            DirStmt::Assign {
                rhs:
                    DirExpr::Binary {
                        op: DirBinaryOp::Mod,
                        lhs,
                        rhs,
                        ..
                    },
                ..
            } => {
                assert!(
                    matches!(strip_casts(lhs), DirExpr::Var(n) if n == "a"),
                    "expected low half a, got {lhs:?}"
                );
                assert!(matches!(strip_casts(rhs), DirExpr::Var(n) if n == "d"));
            }
            other => panic!("unexpected stmt: {other:?}"),
        }
    }

    #[test]
    fn stmt_collapse_multi_def_kill_does_not_mis_substitute() {
        // t = wide(a); x = t % d1; t = wide(b); y = t % d2
        // x must use a; y must use b (not last-wins for x).
        let mut stmts = vec![
            DirStmt::Assign {
                lhs: DirLValue::Var("t".into()),
                rhs: cdq_wide("a"),
            },
            DirStmt::Assign {
                lhs: DirLValue::Var("x".into()),
                rhs: mod_vars(DirExpr::Var("t".into()), "d1"),
            },
            DirStmt::Assign {
                lhs: DirLValue::Var("t".into()),
                rhs: cdq_wide("b"),
            },
            DirStmt::Assign {
                lhs: DirLValue::Var("y".into()),
                rhs: mod_vars(DirExpr::Var("t".into()), "d2"),
            },
        ];
        assert!(collapse_cdq_signed_mod_in_stmts(&mut stmts));
        let low_of = |stmt: &DirStmt| -> String {
            match stmt {
                DirStmt::Assign {
                    rhs:
                        DirExpr::Binary {
                            op: DirBinaryOp::Mod,
                            lhs,
                            ..
                        },
                    ..
                } => match strip_casts(lhs) {
                    DirExpr::Var(n) => n,
                    other => panic!("expected var low, got {other:?}"),
                },
                other => panic!("expected assign mod, got {other:?}"),
            }
        };
        assert_eq!(low_of(&stmts[1]), "a");
        assert_eq!(low_of(&stmts[3]), "b");
    }

    #[test]
    fn stmt_collapse_free_var_redef_kills_live_temp() {
        // t = wide(a); a = 0; x = t % d  → must NOT rewrite x to use new a.
        let mut stmts = vec![
            DirStmt::Assign {
                lhs: DirLValue::Var("t".into()),
                rhs: cdq_wide("a"),
            },
            DirStmt::Assign {
                lhs: DirLValue::Var("a".into()),
                rhs: DirExpr::Const(0, i32_ty()),
            },
            DirStmt::Assign {
                lhs: DirLValue::Var("x".into()),
                rhs: mod_vars(DirExpr::Var("t".into()), "d"),
            },
        ];
        let _ = collapse_cdq_signed_mod_in_stmts(&mut stmts);
        // x should still be t % d (Var form), not (int)a % …
        match &stmts[2] {
            DirStmt::Assign {
                rhs:
                    DirExpr::Binary {
                        op: DirBinaryOp::Mod,
                        lhs,
                        ..
                    },
                ..
            } => {
                assert!(
                    matches!(lhs.as_ref(), DirExpr::Var(n) if n == "t"),
                    "free-var redef must kill temp fold; got {lhs:?}"
                );
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn logical_shr_is_not_cdq_sign_fill() {
        // ( (x >> 32 logical) << 32 | x ) % y must NOT collapse.
        let x = DirExpr::Var("x".into());
        let shr = DirExpr::Binary {
            op: DirBinaryOp::Shr,
            lhs: Box::new(DirExpr::Cast {
                ty: u64_ty(),
                expr: Box::new(x.clone()),
            }),
            rhs: Box::new(DirExpr::Const(32, i32_ty())),
            ty: u64_ty(),
        };
        let wide = DirExpr::Binary {
            op: DirBinaryOp::Or,
            lhs: Box::new(DirExpr::Binary {
                op: DirBinaryOp::Shl,
                lhs: Box::new(shr),
                rhs: Box::new(DirExpr::Const(32, i32_ty())),
                ty: u64_ty(),
            }),
            rhs: Box::new(DirExpr::Cast {
                ty: u64_ty(),
                expr: Box::new(x),
            }),
            ty: u64_ty(),
        };
        let expr = mod_vars(wide, "y");
        assert!(
            collapse_cdq_style_signed_mod_div(&expr).is_none(),
            "logical Shr must not count as CDQ sign-fill"
        );
    }

    #[test]
    fn bare_copy_high_half_is_not_cdq_sign_fill() {
        // (copy(x) << 32 | x) % y — high is not SAR/SExt of x.
        let x = DirExpr::Var("x".into());
        let wide = DirExpr::Binary {
            op: DirBinaryOp::Or,
            lhs: Box::new(DirExpr::Binary {
                op: DirBinaryOp::Shl,
                lhs: Box::new(x.clone()),
                rhs: Box::new(DirExpr::Const(32, i32_ty())),
                ty: u64_ty(),
            }),
            rhs: Box::new(DirExpr::Cast {
                ty: u64_ty(),
                expr: Box::new(x),
            }),
            ty: u64_ty(),
        };
        assert!(
            collapse_cdq_style_signed_mod_div(&mod_vars(wide, "y")).is_none(),
            "bare copy/var high half must not collapse as CDQ"
        );
    }
}
