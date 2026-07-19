use crate::prelude::*; // For accessing normalizer helpers
use crate::HashMap;

pub fn apply_double_precision_reconstruction_pass(func: &mut HirFunction) -> bool {
    let mut changed = false;

    // Phase 1: Collapse contiguous loads and stores in the function body
    if collapse_contiguous_mem_ops(&mut func.body, &mut func.locals) {
        changed = true;
    }

    // Phase 2: Build definition map of single-assignment variables
    let mut defs = HashMap::default();
    collect_single_defs(&func.body, &mut defs);

    // Phase 3: Recursively rewrite Or-Shift reconstruction expressions
    if rewrite_recombine_exprs(&mut func.body, &defs) {
        changed = true;
    }

    changed
}

fn collect_single_defs(stmts: &[HirStmt], defs: &mut HashMap<String, HirExpr>) {
    let mut def_counts = HashMap::default();
    count_defs_recursive(stmts, &mut def_counts);
    collect_defs_recursive(stmts, &def_counts, defs);
}

fn count_defs_recursive(stmts: &[HirStmt], counts: &mut HashMap<String, usize>) {
    for stmt in stmts {
        match stmt {
            HirStmt::Assign {
                lhs: HirLValue::Var(name),
                ..
            } => {
                *counts.entry(name.clone()).or_insert(0) += 1;
            }
            HirStmt::Block(body)
            | HirStmt::While { body, .. }
            | HirStmt::DoWhile { body, .. }
            | HirStmt::For { body, .. } => {
                count_defs_recursive(body, counts);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                count_defs_recursive(then_body, counts);
                count_defs_recursive(else_body, counts);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    count_defs_recursive(&case.body, counts);
                }
                count_defs_recursive(default, counts);
            }
            _ => {}
        }
    }
}

fn collect_defs_recursive(
    stmts: &[HirStmt],
    counts: &HashMap<String, usize>,
    defs: &mut HashMap<String, HirExpr>,
) {
    for stmt in stmts {
        match stmt {
            HirStmt::Assign {
                lhs: HirLValue::Var(name),
                rhs,
            } => {
                if counts.get(name).copied().unwrap_or(0) == 1 {
                    defs.insert(name.clone(), rhs.clone());
                }
            }
            HirStmt::Block(body)
            | HirStmt::While { body, .. }
            | HirStmt::DoWhile { body, .. }
            | HirStmt::For { body, .. } => {
                collect_defs_recursive(body, counts, defs);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_defs_recursive(then_body, counts, defs);
                collect_defs_recursive(else_body, counts, defs);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_defs_recursive(&case.body, counts, defs);
                }
                collect_defs_recursive(default, counts, defs);
            }
            _ => {}
        }
    }
}

// ── Expression Rewriting ──────────────────────────────────────────────────

fn rewrite_recombine_exprs(stmts: &mut [HirStmt], defs: &HashMap<String, HirExpr>) -> bool {
    let mut changed = false;
    for stmt in stmts {
        match stmt {
            HirStmt::Assign { lhs, rhs } => {
                changed |= rewrite_lvalue(lhs, defs);
                changed |= rewrite_expr(rhs, defs);
            }
            HirStmt::VaStart { va_list, .. } => {
                changed |= rewrite_expr(va_list, defs);
            }
            HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => {
                changed |= rewrite_expr(expr, defs);
            }
            HirStmt::Block(body)
            | HirStmt::While { body, .. }
            | HirStmt::DoWhile { body, .. }
            | HirStmt::For { body, .. } => {
                changed |= rewrite_recombine_exprs(body, defs);
            }
            HirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                changed |= rewrite_expr(expr, defs);
                for case in cases {
                    changed |= rewrite_recombine_exprs(&mut case.body, defs);
                }
                changed |= rewrite_recombine_exprs(default, defs);
            }
            HirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                changed |= rewrite_expr(cond, defs);
                changed |= rewrite_recombine_exprs(then_body, defs);
                changed |= rewrite_recombine_exprs(else_body, defs);
            }
            _ => {}
        }
    }
    changed
}

fn rewrite_lvalue(lval: &mut HirLValue, defs: &HashMap<String, HirExpr>) -> bool {
    match lval {
        HirLValue::Var(_) => false,
        HirLValue::Deref { ptr, .. } => rewrite_expr(ptr, defs),
        HirLValue::Index { base, index, .. } => {
            let mut changed = rewrite_expr(base, defs);
            changed |= rewrite_expr(index, defs);
            changed
        }
        HirLValue::FieldAccess { base, .. } => rewrite_expr(base, defs),
    }
}

fn rewrite_expr(expr: &mut HirExpr, defs: &HashMap<String, HirExpr>) -> bool {
    let mut changed = false;

    // Recursively rewrite sub-expressions first
    match expr {
        HirExpr::Cast { expr: inner, .. }
        | HirExpr::Unary { expr: inner, .. }
        | HirExpr::Load { ptr: inner, .. }
        | HirExpr::PtrOffset { base: inner, .. }
        | HirExpr::AggregateCopy { src: inner, .. }
        | HirExpr::FieldAccess { base: inner, .. } => {
            changed |= rewrite_expr(inner, defs);
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            changed |= rewrite_expr(lhs, defs);
            changed |= rewrite_expr(rhs, defs);
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                changed |= rewrite_expr(arg, defs);
            }
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            changed |= rewrite_expr(cond, defs);
            changed |= rewrite_expr(then_expr, defs);
            changed |= rewrite_expr(else_expr, defs);
        }
        HirExpr::Index { base, index, .. } => {
            changed |= rewrite_expr(base, defs);
            changed |= rewrite_expr(index, defs);
        }
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => {}
    }

    // Try to recombine hi/lo pairs at this node
    if let Some((hi, lo)) = match_recombine(expr) {
        if let (Some(hi_name), Some(lo_name)) = (get_var_name(&hi), get_var_name(&lo)) {
            if let Some(new_expr) = try_reconstruct(&hi_name, &lo_name, defs) {
                *expr = new_expr;
                changed = true;
            }
        }
    }

    changed
}

fn match_recombine(expr: &HirExpr) -> Option<(HirExpr, HirExpr)> {
    match expr {
        HirExpr::Binary {
            op: HirBinaryOp::Or | HirBinaryOp::Add,
            lhs,
            rhs,
            ..
        } => {
            if let Some(hi) = match_hi_shift(lhs) {
                return Some((hi, *rhs.clone()));
            }
            if let Some(hi) = match_hi_shift(rhs) {
                return Some((hi, *lhs.clone()));
            }
            None
        }
        _ => None,
    }
}

fn match_hi_shift(expr: &HirExpr) -> Option<HirExpr> {
    match expr {
        HirExpr::Binary {
            op: HirBinaryOp::Shl,
            lhs,
            rhs,
            ..
        } => {
            if let HirExpr::Const(32, _) = rhs.as_ref() {
                Some(*lhs.clone())
            } else {
                None
            }
        }
        HirExpr::Cast { expr: inner, .. } => match_hi_shift(inner),
        _ => None,
    }
}

fn strip_casts(expr: &HirExpr) -> &HirExpr {
    match expr {
        HirExpr::Cast { expr: inner, .. } => strip_casts(inner),
        _ => expr,
    }
}

fn get_var_name(expr: &HirExpr) -> Option<String> {
    match strip_casts(expr) {
        HirExpr::Var(name) => Some(name.clone()),
        _ => None,
    }
}

fn try_reconstruct(
    hi_name: &str,
    lo_name: &str,
    defs: &HashMap<String, HirExpr>,
) -> Option<HirExpr> {
    let hi_def = defs.get(hi_name)?;
    let lo_def = defs.get(lo_name)?;

    // 1. Logical operations: AND, OR, XOR
    if let (
        HirExpr::Binary {
            op: op_hi,
            lhs: lhs_hi,
            rhs: rhs_hi,
            ty: ty_hi,
        },
        HirExpr::Binary {
            op: op_lo,
            lhs: lhs_lo,
            rhs: rhs_lo,
            ..
        },
    ) = (hi_def, lo_def)
    {
        if op_hi == op_lo && matches!(op_hi, HirBinaryOp::And | HirBinaryOp::Or | HirBinaryOp::Xor)
        {
            let val1 = make_recombine((**lhs_hi).clone(), (**lhs_lo).clone(), ty_hi);
            let val2 = make_recombine((**rhs_hi).clone(), (**rhs_lo).clone(), ty_hi);
            return Some(HirExpr::Binary {
                op: *op_hi,
                lhs: Box::new(val1),
                rhs: Box::new(val2),
                ty: NirType::Int {
                    bits: 64,
                    signed: false,
                },
            });
        }
    }

    // 2. Add with carry
    if let HirExpr::Binary {
        op: HirBinaryOp::Add,
        lhs: lo1,
        rhs: lo2,
        ..
    } = lo_def
    {
        if let Some((hi1, hi2, carry)) = match_hi_add_carry(hi_def) {
            if is_carry_of(&carry, lo1, lo2, lo_name, defs) {
                let val1 = make_recombine(
                    hi1,
                    lo1.as_ref().clone(),
                    &NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                );
                let val2 = make_recombine(
                    hi2,
                    lo2.as_ref().clone(),
                    &NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                );
                return Some(HirExpr::Binary {
                    op: HirBinaryOp::Add,
                    lhs: Box::new(val1),
                    rhs: Box::new(val2),
                    ty: NirType::Int {
                        bits: 64,
                        signed: false,
                    },
                });
            }
        }
    }

    // 3. Sub with borrow
    if let HirExpr::Binary {
        op: HirBinaryOp::Sub,
        lhs: lo1,
        rhs: lo2,
        ..
    } = lo_def
    {
        if let Some((hi1, hi2, borrow)) = match_hi_sub_borrow(hi_def) {
            if is_borrow_of(&borrow, lo1, lo2, lo_name, defs) {
                let val1 = make_recombine(
                    hi1,
                    lo1.as_ref().clone(),
                    &NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                );
                let val2 = make_recombine(
                    hi2,
                    lo2.as_ref().clone(),
                    &NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                );
                return Some(HirExpr::Binary {
                    op: HirBinaryOp::Sub,
                    lhs: Box::new(val1),
                    rhs: Box::new(val2),
                    ty: NirType::Int {
                        bits: 64,
                        signed: false,
                    },
                });
            }
        }
    }

    // 4. Shift Left
    if let HirExpr::Binary {
        op: HirBinaryOp::Shl,
        lhs: lo_src,
        rhs: lo_shift,
        ..
    } = lo_def
    {
        if let HirExpr::Const(shift_amt, _) = lo_shift.as_ref() {
            if let Some((hi_src, hi_shift_amt)) = match_hi_shl_mix(hi_def, lo_src, *shift_amt) {
                if hi_shift_amt == *shift_amt {
                    let val_src = make_recombine(
                        hi_src,
                        lo_src.as_ref().clone(),
                        &NirType::Int {
                            bits: 32,
                            signed: false,
                        },
                    );
                    return Some(HirExpr::Binary {
                        op: HirBinaryOp::Shl,
                        lhs: Box::new(val_src),
                        rhs: lo_shift.clone(),
                        ty: NirType::Int {
                            bits: 64,
                            signed: false,
                        },
                    });
                }
            }
        }
    }

    // 5. Shift Right
    if let HirExpr::Binary {
        op: op_hi @ (HirBinaryOp::Shr | HirBinaryOp::Sar),
        lhs: hi_src,
        rhs: hi_shift,
        ..
    } = hi_def
    {
        if let HirExpr::Const(shift_amt, _) = hi_shift.as_ref() {
            if let Some((lo_src, lo_shift_amt)) = match_lo_shr_mix(lo_def, hi_src, *shift_amt) {
                if lo_shift_amt == *shift_amt {
                    let val_src = make_recombine(
                        hi_src.as_ref().clone(),
                        lo_src,
                        &NirType::Int {
                            bits: 32,
                            signed: false,
                        },
                    );
                    return Some(HirExpr::Binary {
                        op: *op_hi,
                        lhs: Box::new(val_src),
                        rhs: hi_shift.clone(),
                        ty: NirType::Int {
                            bits: 64,
                            signed: false,
                        },
                    });
                }
            }
        }
    }

    None
}

fn make_recombine(hi: HirExpr, lo: HirExpr, _ty: &NirType) -> HirExpr {
    HirExpr::Binary {
        op: HirBinaryOp::Or,
        lhs: Box::new(HirExpr::Binary {
            op: HirBinaryOp::Shl,
            lhs: Box::new(HirExpr::Cast {
                ty: NirType::Int {
                    bits: 64,
                    signed: false,
                },
                expr: Box::new(hi),
            }),
            rhs: Box::new(HirExpr::Const(
                32,
                NirType::Int {
                    bits: 32,
                    signed: false,
                },
            )),
            ty: NirType::Int {
                bits: 64,
                signed: false,
            },
        }),
        rhs: Box::new(HirExpr::Cast {
            ty: NirType::Int {
                bits: 64,
                signed: false,
            },
            expr: Box::new(lo),
        }),
        ty: NirType::Int {
            bits: 64,
            signed: false,
        },
    }
}

fn match_hi_add_carry(expr: &HirExpr) -> Option<(HirExpr, HirExpr, HirExpr)> {
    match expr {
        HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs,
            rhs,
            ..
        } => {
            if let HirExpr::Binary {
                op: HirBinaryOp::Add,
                lhs: hi1,
                rhs: hi2,
                ..
            } = lhs.as_ref()
            {
                return Some((
                    hi1.as_ref().clone(),
                    hi2.as_ref().clone(),
                    rhs.as_ref().clone(),
                ));
            }
            if let HirExpr::Binary {
                op: HirBinaryOp::Add,
                lhs: hi2,
                rhs: carry,
                ..
            } = rhs.as_ref()
            {
                return Some((
                    lhs.as_ref().clone(),
                    hi2.as_ref().clone(),
                    carry.as_ref().clone(),
                ));
            }
            None
        }
        _ => None,
    }
}

fn is_carry_of(
    carry_expr: &HirExpr,
    lo1: &HirExpr,
    lo2: &HirExpr,
    lo_name: &str,
    defs: &HashMap<String, HirExpr>,
) -> bool {
    let expr = strip_casts(carry_expr);
    if let HirExpr::Binary {
        op: HirBinaryOp::Lt | HirBinaryOp::SLt,
        lhs,
        rhs,
        ..
    } = expr
    {
        let l = get_var_name(lhs);
        let r = get_var_name(rhs);
        if l.as_deref() == Some(lo_name) && (r == get_var_name(lo1) || r == get_var_name(lo2)) {
            return true;
        }
    }
    if let HirExpr::Call { target, args, .. } = expr {
        if target == "__carry" && args.len() >= 2 {
            let a = get_var_name(&args[0]);
            let b = get_var_name(&args[1]);
            let lo1_var = get_var_name(lo1);
            let lo2_var = get_var_name(lo2);
            if (a == lo1_var && b == lo2_var) || (a == lo2_var && b == lo1_var) {
                return true;
            }
        }
    }
    if let HirExpr::Var(name) = expr {
        if let Some(def) = defs.get(name) {
            return is_carry_of(def, lo1, lo2, lo_name, defs);
        }
    }
    false
}

fn match_hi_sub_borrow(expr: &HirExpr) -> Option<(HirExpr, HirExpr, HirExpr)> {
    match expr {
        HirExpr::Binary {
            op: HirBinaryOp::Sub,
            lhs,
            rhs,
            ..
        } => {
            if let HirExpr::Binary {
                op: HirBinaryOp::Sub,
                lhs: hi1,
                rhs: hi2,
                ..
            } = lhs.as_ref()
            {
                return Some((
                    hi1.as_ref().clone(),
                    hi2.as_ref().clone(),
                    rhs.as_ref().clone(),
                ));
            }
            None
        }
        _ => None,
    }
}

fn is_borrow_of(
    borrow_expr: &HirExpr,
    lo1: &HirExpr,
    lo2: &HirExpr,
    lo_name: &str,
    defs: &HashMap<String, HirExpr>,
) -> bool {
    let expr = strip_casts(borrow_expr);
    if let HirExpr::Binary {
        op: HirBinaryOp::Lt | HirBinaryOp::SLt,
        lhs,
        rhs,
        ..
    } = expr
    {
        let l = get_var_name(lhs);
        let r = get_var_name(rhs);
        if l == get_var_name(lo1) && r == get_var_name(lo2) {
            return true;
        }
    }
    if let HirExpr::Call { target, args, .. } = expr {
        if target == "__sborrow" && args.len() >= 2 {
            let a = get_var_name(&args[0]);
            let b = get_var_name(&args[1]);
            if a == get_var_name(lo1) && b == get_var_name(lo2) {
                return true;
            }
        }
    }
    if let HirExpr::Var(name) = expr {
        if let Some(def) = defs.get(name) {
            return is_borrow_of(def, lo1, lo2, lo_name, defs);
        }
    }
    false
}

fn match_shl(expr: &HirExpr) -> Option<(HirExpr, i64)> {
    let expr = strip_casts(expr);
    if let HirExpr::Binary {
        op: HirBinaryOp::Shl,
        lhs,
        rhs,
        ..
    } = expr
    {
        if let HirExpr::Const(shift, _) = rhs.as_ref() {
            return Some((lhs.as_ref().clone(), *shift));
        }
    }
    None
}

fn match_shr(expr: &HirExpr) -> Option<(HirExpr, i64)> {
    let expr = strip_casts(expr);
    if let HirExpr::Binary {
        op: HirBinaryOp::Shr | HirBinaryOp::Sar,
        lhs,
        rhs,
        ..
    } = expr
    {
        if let HirExpr::Const(shift, _) = rhs.as_ref() {
            return Some((lhs.as_ref().clone(), *shift));
        }
    }
    None
}

fn match_hi_shl_mix(
    expr: &HirExpr,
    expected_lo_src: &HirExpr,
    expected_shift: i64,
) -> Option<(HirExpr, i64)> {
    let expr = strip_casts(expr);
    if let HirExpr::Binary {
        op: HirBinaryOp::Or,
        lhs,
        rhs,
        ..
    } = expr
    {
        if let Some((hi_src, shl_amt)) = match_shl(lhs) {
            if let Some((lo_src, shr_amt)) = match_shr(rhs) {
                if shl_amt + shr_amt == 32
                    && shl_amt == expected_shift
                    && get_var_name(&lo_src) == get_var_name(expected_lo_src)
                {
                    return Some((hi_src, shl_amt));
                }
            }
        }
        if let Some((hi_src, shl_amt)) = match_shl(rhs) {
            if let Some((lo_src, shr_amt)) = match_shr(lhs) {
                if shl_amt + shr_amt == 32
                    && shl_amt == expected_shift
                    && get_var_name(&lo_src) == get_var_name(expected_lo_src)
                {
                    return Some((hi_src, shl_amt));
                }
            }
        }
    }
    None
}

fn match_lo_shr_mix(
    expr: &HirExpr,
    expected_hi_src: &HirExpr,
    expected_shift: i64,
) -> Option<(HirExpr, i64)> {
    let expr = strip_casts(expr);
    if let HirExpr::Binary {
        op: HirBinaryOp::Or,
        lhs,
        rhs,
        ..
    } = expr
    {
        if let Some((lo_src, shr_amt)) = match_shr(lhs) {
            if let Some((hi_src, shl_amt)) = match_shl(rhs) {
                if shl_amt + shr_amt == 32
                    && shr_amt == expected_shift
                    && get_var_name(&hi_src) == get_var_name(expected_hi_src)
                {
                    return Some((lo_src, shr_amt));
                }
            }
        }
        if let Some((lo_src, shr_amt)) = match_shr(rhs) {
            if let Some((hi_src, shl_amt)) = match_shl(lhs) {
                if shl_amt + shr_amt == 32
                    && shr_amt == expected_shift
                    && get_var_name(&hi_src) == get_var_name(expected_hi_src)
                {
                    return Some((lo_src, shr_amt));
                }
            }
        }
    }
    None
}

// ── Contiguous Loads & Stores collapsing ───────────────────────────────────

fn collapse_contiguous_mem_ops(stmts: &mut Vec<HirStmt>, locals: &mut Vec<NirBinding>) -> bool {
    let mut changed = false;

    // First recurse into nested blocks
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Block(body)
            | HirStmt::While { body, .. }
            | HirStmt::DoWhile { body, .. }
            | HirStmt::For { body, .. } => {
                changed |= collapse_contiguous_mem_ops(body, locals);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= collapse_contiguous_mem_ops(then_body, locals);
                changed |= collapse_contiguous_mem_ops(else_body, locals);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= collapse_contiguous_mem_ops(&mut case.body, locals);
                }
                changed |= collapse_contiguous_mem_ops(default, locals);
            }
            _ => {}
        }
    }

    // Now try to collapse contiguous loads
    let mut i = 0;
    let mut new_stmts = Vec::new();
    while i < stmts.len() {
        if i + 1 < stmts.len() {
            if let (
                HirStmt::Assign {
                    lhs: HirLValue::Var(lo_var),
                    rhs:
                        HirExpr::Load {
                            ptr: lo_ptr,
                            ty: lo_ty,
                        },
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var(hi_var),
                    rhs:
                        HirExpr::Load {
                            ptr: hi_ptr,
                            ty: hi_ty,
                        },
                },
            ) = (&stmts[i], &stmts[i + 1])
            {
                if is_32bit_int(lo_ty) && is_32bit_int(hi_ty) && is_ptr_offset_by_4(hi_ptr, lo_ptr)
                {
                    // Contiguous load!
                    let temp_64_name = format!("uVar_dp_{}", locals.len());
                    locals.push(NirBinding {
                        name: temp_64_name.clone(),
                        ty: NirType::Int {
                            bits: 64,
                            signed: false,
                        },
                        surface_type_name: None,
                        origin: None,
                        initializer: None,
                    });

                    // Add: temp_64 = load(lo_ptr)
                    new_stmts.push(HirStmt::Assign {
                        lhs: HirLValue::Var(temp_64_name.clone()),
                        rhs: HirExpr::Load {
                            ptr: lo_ptr.clone(),
                            ty: NirType::Int {
                                bits: 64,
                                signed: false,
                            },
                        },
                    });
                    // lo_var = cast(temp_64)
                    new_stmts.push(HirStmt::Assign {
                        lhs: HirLValue::Var(lo_var.clone()),
                        rhs: HirExpr::Cast {
                            ty: lo_ty.clone(),
                            expr: Box::new(HirExpr::Var(temp_64_name.clone())),
                        },
                    });
                    // hi_var = cast(temp_64 >> 32)
                    new_stmts.push(HirStmt::Assign {
                        lhs: HirLValue::Var(hi_var.clone()),
                        rhs: HirExpr::Cast {
                            ty: hi_ty.clone(),
                            expr: Box::new(HirExpr::Binary {
                                op: HirBinaryOp::Shr,
                                lhs: Box::new(HirExpr::Var(temp_64_name.clone())),
                                rhs: Box::new(HirExpr::Const(
                                    32,
                                    NirType::Int {
                                        bits: 32,
                                        signed: false,
                                    },
                                )),
                                ty: NirType::Int {
                                    bits: 64,
                                    signed: false,
                                },
                            }),
                        },
                    });

                    changed = true;
                    i += 2;
                    continue;
                }
            }

            // Contiguous stores collapsing
            // *ptr = cast(val_64)
            // *(ptr + 4) = cast(val_64 >> 32)
            if let (
                HirStmt::Assign {
                    lhs:
                        HirLValue::Deref {
                            ptr: lo_ptr,
                            ty: lo_ty,
                        },
                    rhs: lo_val,
                },
                HirStmt::Assign {
                    lhs:
                        HirLValue::Deref {
                            ptr: hi_ptr,
                            ty: hi_ty,
                        },
                    rhs: hi_val,
                },
            ) = (&stmts[i], &stmts[i + 1])
            {
                if is_32bit_int(lo_ty) && is_32bit_int(hi_ty) && is_ptr_offset_by_4(hi_ptr, lo_ptr)
                {
                    if let Some(val_64) = match_split_stores(lo_val, hi_val) {
                        new_stmts.push(HirStmt::Assign {
                            lhs: HirLValue::Deref {
                                ptr: lo_ptr.clone(),
                                ty: NirType::Int {
                                    bits: 64,
                                    signed: false,
                                },
                            },
                            rhs: val_64,
                        });
                        changed = true;
                        i += 2;
                        continue;
                    }
                }
            }
        }

        new_stmts.push(stmts[i].clone());
        i += 1;
    }

    if changed {
        *stmts = new_stmts;
    }

    changed
}

fn is_32bit_int(ty: &NirType) -> bool {
    matches!(ty, NirType::Int { bits: 32, .. })
}

fn is_ptr_offset_by_4(hi_ptr: &HirExpr, lo_ptr: &HirExpr) -> bool {
    match hi_ptr {
        HirExpr::PtrOffset { base, offset: 4 } => base.as_ref() == lo_ptr,
        HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs,
            rhs,
            ..
        } => {
            if let HirExpr::Const(4, _) = rhs.as_ref() {
                lhs.as_ref() == lo_ptr
            } else {
                false
            }
        }
        _ => false,
    }
}

fn match_split_stores(lo_val: &HirExpr, hi_val: &HirExpr) -> Option<HirExpr> {
    // lo_val is Cast(val_64) or subpiece
    // hi_val is Cast(val_64 >> 32)
    let lo_inner = strip_casts(lo_val);
    let hi_inner = strip_casts(hi_val);
    if let HirExpr::Binary {
        op: HirBinaryOp::Shr | HirBinaryOp::Sar,
        lhs: hi_src,
        rhs: shift,
        ..
    } = hi_inner
    {
        if let HirExpr::Const(32, _) = shift.as_ref() {
            if get_var_name(lo_inner) == get_var_name(hi_src) {
                return Some(hi_src.as_ref().clone());
            }
        }
    }
    None
}
