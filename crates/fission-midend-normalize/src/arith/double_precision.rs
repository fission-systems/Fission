use crate::prelude::*; // For accessing normalizer helpers
use crate::HashMap;

pub fn apply_double_precision_reconstruction_pass(func: &mut DirFunction) -> bool {
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

fn collect_single_defs(stmts: &[DirStmt], defs: &mut HashMap<String, DirExpr>) {
    let mut def_counts = HashMap::default();
    count_defs_recursive(stmts, &mut def_counts);
    collect_defs_recursive(stmts, &def_counts, defs);
}

fn count_defs_recursive(stmts: &[DirStmt], counts: &mut HashMap<String, usize>) {
    for stmt in stmts {
        match stmt {
            DirStmt::Assign {
                lhs: DirLValue::Var(name),
                ..
            } => {
                *counts.entry(name.clone()).or_insert(0) += 1;
            }
            DirStmt::Block(body)
            | DirStmt::While { body, .. }
            | DirStmt::DoWhile { body, .. }
            | DirStmt::For { body, .. } => {
                count_defs_recursive(body, counts);
            }
            DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                count_defs_recursive(then_body, counts);
                count_defs_recursive(else_body, counts);
            }
            DirStmt::Switch { cases, default, .. } => {
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
    stmts: &[DirStmt],
    counts: &HashMap<String, usize>,
    defs: &mut HashMap<String, DirExpr>,
) {
    for stmt in stmts {
        match stmt {
            DirStmt::Assign {
                lhs: DirLValue::Var(name),
                rhs,
            } => {
                if counts.get(name).copied().unwrap_or(0) == 1 {
                    defs.insert(name.clone(), rhs.clone());
                }
            }
            DirStmt::Block(body)
            | DirStmt::While { body, .. }
            | DirStmt::DoWhile { body, .. }
            | DirStmt::For { body, .. } => {
                collect_defs_recursive(body, counts, defs);
            }
            DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_defs_recursive(then_body, counts, defs);
                collect_defs_recursive(else_body, counts, defs);
            }
            DirStmt::Switch { cases, default, .. } => {
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

fn rewrite_recombine_exprs(stmts: &mut [DirStmt], defs: &HashMap<String, DirExpr>) -> bool {
    let mut changed = false;
    for stmt in stmts {
        match stmt {
            DirStmt::Assign { lhs, rhs } => {
                changed |= rewrite_lvalue(lhs, defs);
                changed |= rewrite_expr(rhs, defs);
            }
            DirStmt::VaStart { va_list, .. } => {
                changed |= rewrite_expr(va_list, defs);
            }
            DirStmt::Expr(expr) | DirStmt::Return(Some(expr)) => {
                changed |= rewrite_expr(expr, defs);
            }
            DirStmt::Block(body)
            | DirStmt::While { body, .. }
            | DirStmt::DoWhile { body, .. }
            | DirStmt::For { body, .. } => {
                changed |= rewrite_recombine_exprs(body, defs);
            }
            DirStmt::Switch {
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
            DirStmt::If {
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

fn rewrite_lvalue(lval: &mut DirLValue, defs: &HashMap<String, DirExpr>) -> bool {
    match lval {
        DirLValue::Var(_) => false,
        DirLValue::Deref { ptr, .. } => rewrite_expr(ptr, defs),
        DirLValue::Index { base, index, .. } => {
            let mut changed = rewrite_expr(base, defs);
            changed |= rewrite_expr(index, defs);
            changed
        }
        DirLValue::FieldAccess { base, .. } => rewrite_expr(base, defs),
    }
}

fn rewrite_expr(expr: &mut DirExpr, defs: &HashMap<String, DirExpr>) -> bool {
    let mut changed = false;

    // Recursively rewrite sub-expressions first
    match expr {
        DirExpr::Cast { expr: inner, .. }
        | DirExpr::Unary { expr: inner, .. }
        | DirExpr::Load { ptr: inner, .. }
        | DirExpr::PtrOffset { base: inner, .. }
        | DirExpr::AggregateCopy { src: inner, .. }
        | DirExpr::FieldAccess { base: inner, .. } => {
            changed |= rewrite_expr(inner, defs);
        }
        DirExpr::Binary { lhs, rhs, .. } => {
            changed |= rewrite_expr(lhs, defs);
            changed |= rewrite_expr(rhs, defs);
        }
        DirExpr::Call { args, .. } => {
            for arg in args {
                changed |= rewrite_expr(arg, defs);
            }
        }
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            changed |= rewrite_expr(cond, defs);
            changed |= rewrite_expr(then_expr, defs);
            changed |= rewrite_expr(else_expr, defs);
        }
        DirExpr::Index { base, index, .. } => {
            changed |= rewrite_expr(base, defs);
            changed |= rewrite_expr(index, defs);
        }
        DirExpr::Var(_) | DirExpr::AddressOfGlobal(_) | DirExpr::Const(_, _) => {}
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

fn match_recombine(expr: &DirExpr) -> Option<(DirExpr, DirExpr)> {
    match expr {
        DirExpr::Binary {
            op: DirBinaryOp::Or | DirBinaryOp::Add,
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

fn match_hi_shift(expr: &DirExpr) -> Option<DirExpr> {
    match expr {
        DirExpr::Binary {
            op: DirBinaryOp::Shl,
            lhs,
            rhs,
            ..
        } => {
            if let DirExpr::Const(32, _) = rhs.as_ref() {
                Some(*lhs.clone())
            } else {
                None
            }
        }
        DirExpr::Cast { expr: inner, .. } => match_hi_shift(inner),
        _ => None,
    }
}

fn strip_casts(expr: &DirExpr) -> &DirExpr {
    match expr {
        DirExpr::Cast { expr: inner, .. } => strip_casts(inner),
        _ => expr,
    }
}

fn get_var_name(expr: &DirExpr) -> Option<String> {
    match strip_casts(expr) {
        DirExpr::Var(name) => Some(name.clone()),
        _ => None,
    }
}

fn try_reconstruct(
    hi_name: &str,
    lo_name: &str,
    defs: &HashMap<String, DirExpr>,
) -> Option<DirExpr> {
    let hi_def = defs.get(hi_name)?;
    let lo_def = defs.get(lo_name)?;

    // 1. Logical operations: AND, OR, XOR
    if let (
        DirExpr::Binary {
            op: op_hi,
            lhs: lhs_hi,
            rhs: rhs_hi,
            ty: ty_hi,
        },
        DirExpr::Binary {
            op: op_lo,
            lhs: lhs_lo,
            rhs: rhs_lo,
            ..
        },
    ) = (hi_def, lo_def)
    {
        if op_hi == op_lo && matches!(op_hi, DirBinaryOp::And | DirBinaryOp::Or | DirBinaryOp::Xor)
        {
            let val1 = make_recombine((**lhs_hi).clone(), (**lhs_lo).clone(), ty_hi);
            let val2 = make_recombine((**rhs_hi).clone(), (**rhs_lo).clone(), ty_hi);
            return Some(DirExpr::Binary {
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
    if let DirExpr::Binary {
        op: DirBinaryOp::Add,
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
                return Some(DirExpr::Binary {
                    op: DirBinaryOp::Add,
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
    if let DirExpr::Binary {
        op: DirBinaryOp::Sub,
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
                return Some(DirExpr::Binary {
                    op: DirBinaryOp::Sub,
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
    if let DirExpr::Binary {
        op: DirBinaryOp::Shl,
        lhs: lo_src,
        rhs: lo_shift,
        ..
    } = lo_def
    {
        if let DirExpr::Const(shift_amt, _) = lo_shift.as_ref() {
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
                    return Some(DirExpr::Binary {
                        op: DirBinaryOp::Shl,
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
    if let DirExpr::Binary {
        op: op_hi @ (DirBinaryOp::Shr | DirBinaryOp::Sar),
        lhs: hi_src,
        rhs: hi_shift,
        ..
    } = hi_def
    {
        if let DirExpr::Const(shift_amt, _) = hi_shift.as_ref() {
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
                    return Some(DirExpr::Binary {
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

fn make_recombine(hi: DirExpr, lo: DirExpr, _ty: &NirType) -> DirExpr {
    DirExpr::Binary {
        op: DirBinaryOp::Or,
        lhs: Box::new(DirExpr::Binary {
            op: DirBinaryOp::Shl,
            lhs: Box::new(DirExpr::Cast {
                ty: NirType::Int {
                    bits: 64,
                    signed: false,
                },
                expr: Box::new(hi),
            }),
            rhs: Box::new(DirExpr::Const(
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
        rhs: Box::new(DirExpr::Cast {
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

fn match_hi_add_carry(expr: &DirExpr) -> Option<(DirExpr, DirExpr, DirExpr)> {
    match expr {
        DirExpr::Binary {
            op: DirBinaryOp::Add,
            lhs,
            rhs,
            ..
        } => {
            if let DirExpr::Binary {
                op: DirBinaryOp::Add,
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
            if let DirExpr::Binary {
                op: DirBinaryOp::Add,
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
    carry_expr: &DirExpr,
    lo1: &DirExpr,
    lo2: &DirExpr,
    lo_name: &str,
    defs: &HashMap<String, DirExpr>,
) -> bool {
    let expr = strip_casts(carry_expr);
    if let DirExpr::Binary {
        op: DirBinaryOp::Lt | DirBinaryOp::SLt,
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
    if let DirExpr::Call { target, args, .. } = expr {
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
    if let DirExpr::Var(name) = expr {
        if let Some(def) = defs.get(name) {
            return is_carry_of(def, lo1, lo2, lo_name, defs);
        }
    }
    false
}

fn match_hi_sub_borrow(expr: &DirExpr) -> Option<(DirExpr, DirExpr, DirExpr)> {
    match expr {
        DirExpr::Binary {
            op: DirBinaryOp::Sub,
            lhs,
            rhs,
            ..
        } => {
            if let DirExpr::Binary {
                op: DirBinaryOp::Sub,
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
    borrow_expr: &DirExpr,
    lo1: &DirExpr,
    lo2: &DirExpr,
    lo_name: &str,
    defs: &HashMap<String, DirExpr>,
) -> bool {
    let expr = strip_casts(borrow_expr);
    if let DirExpr::Binary {
        op: DirBinaryOp::Lt | DirBinaryOp::SLt,
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
    if let DirExpr::Call { target, args, .. } = expr {
        if target == "__sborrow" && args.len() >= 2 {
            let a = get_var_name(&args[0]);
            let b = get_var_name(&args[1]);
            if a == get_var_name(lo1) && b == get_var_name(lo2) {
                return true;
            }
        }
    }
    if let DirExpr::Var(name) = expr {
        if let Some(def) = defs.get(name) {
            return is_borrow_of(def, lo1, lo2, lo_name, defs);
        }
    }
    false
}

fn match_shl(expr: &DirExpr) -> Option<(DirExpr, i64)> {
    let expr = strip_casts(expr);
    if let DirExpr::Binary {
        op: DirBinaryOp::Shl,
        lhs,
        rhs,
        ..
    } = expr
    {
        if let DirExpr::Const(shift, _) = rhs.as_ref() {
            return Some((lhs.as_ref().clone(), *shift));
        }
    }
    None
}

fn match_shr(expr: &DirExpr) -> Option<(DirExpr, i64)> {
    let expr = strip_casts(expr);
    if let DirExpr::Binary {
        op: DirBinaryOp::Shr | DirBinaryOp::Sar,
        lhs,
        rhs,
        ..
    } = expr
    {
        if let DirExpr::Const(shift, _) = rhs.as_ref() {
            return Some((lhs.as_ref().clone(), *shift));
        }
    }
    None
}

fn match_hi_shl_mix(
    expr: &DirExpr,
    expected_lo_src: &DirExpr,
    expected_shift: i64,
) -> Option<(DirExpr, i64)> {
    let expr = strip_casts(expr);
    if let DirExpr::Binary {
        op: DirBinaryOp::Or,
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
    expr: &DirExpr,
    expected_hi_src: &DirExpr,
    expected_shift: i64,
) -> Option<(DirExpr, i64)> {
    let expr = strip_casts(expr);
    if let DirExpr::Binary {
        op: DirBinaryOp::Or,
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

fn collapse_contiguous_mem_ops(stmts: &mut Vec<DirStmt>, locals: &mut Vec<DirBinding>) -> bool {
    let mut changed = false;

    // First recurse into nested blocks
    for stmt in stmts.iter_mut() {
        match stmt {
            DirStmt::Block(body)
            | DirStmt::While { body, .. }
            | DirStmt::DoWhile { body, .. }
            | DirStmt::For { body, .. } => {
                changed |= collapse_contiguous_mem_ops(body, locals);
            }
            DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= collapse_contiguous_mem_ops(then_body, locals);
                changed |= collapse_contiguous_mem_ops(else_body, locals);
            }
            DirStmt::Switch { cases, default, .. } => {
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
                DirStmt::Assign {
                    lhs: DirLValue::Var(lo_var),
                    rhs:
                        DirExpr::Load {
                            ptr: lo_ptr,
                            ty: lo_ty,
                        },
                },
                DirStmt::Assign {
                    lhs: DirLValue::Var(hi_var),
                    rhs:
                        DirExpr::Load {
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
                    locals.push(DirBinding {
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
                    new_stmts.push(DirStmt::Assign {
                        lhs: DirLValue::Var(temp_64_name.clone()),
                        rhs: DirExpr::Load {
                            ptr: lo_ptr.clone(),
                            ty: NirType::Int {
                                bits: 64,
                                signed: false,
                            },
                        },
                    });
                    // lo_var = cast(temp_64)
                    new_stmts.push(DirStmt::Assign {
                        lhs: DirLValue::Var(lo_var.clone()),
                        rhs: DirExpr::Cast {
                            ty: lo_ty.clone(),
                            expr: Box::new(DirExpr::Var(temp_64_name.clone())),
                        },
                    });
                    // hi_var = cast(temp_64 >> 32)
                    new_stmts.push(DirStmt::Assign {
                        lhs: DirLValue::Var(hi_var.clone()),
                        rhs: DirExpr::Cast {
                            ty: hi_ty.clone(),
                            expr: Box::new(DirExpr::Binary {
                                op: DirBinaryOp::Shr,
                                lhs: Box::new(DirExpr::Var(temp_64_name.clone())),
                                rhs: Box::new(DirExpr::Const(
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
                DirStmt::Assign {
                    lhs:
                        DirLValue::Deref {
                            ptr: lo_ptr,
                            ty: lo_ty,
                        },
                    rhs: lo_val,
                },
                DirStmt::Assign {
                    lhs:
                        DirLValue::Deref {
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
                        new_stmts.push(DirStmt::Assign {
                            lhs: DirLValue::Deref {
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

fn is_ptr_offset_by_4(hi_ptr: &DirExpr, lo_ptr: &DirExpr) -> bool {
    match hi_ptr {
        DirExpr::PtrOffset { base, offset: 4 } => base.as_ref() == lo_ptr,
        DirExpr::Binary {
            op: DirBinaryOp::Add,
            lhs,
            rhs,
            ..
        } => {
            if let DirExpr::Const(4, _) = rhs.as_ref() {
                lhs.as_ref() == lo_ptr
            } else {
                false
            }
        }
        _ => false,
    }
}

fn match_split_stores(lo_val: &DirExpr, hi_val: &DirExpr) -> Option<DirExpr> {
    // lo_val is Cast(val_64) or subpiece
    // hi_val is Cast(val_64 >> 32)
    let lo_inner = strip_casts(lo_val);
    let hi_inner = strip_casts(hi_val);
    if let DirExpr::Binary {
        op: DirBinaryOp::Shr | DirBinaryOp::Sar,
        lhs: hi_src,
        rhs: shift,
        ..
    } = hi_inner
    {
        if let DirExpr::Const(32, _) = shift.as_ref() {
            if get_var_name(lo_inner) == get_var_name(hi_src) {
                return Some(hi_src.as_ref().clone());
            }
        }
    }
    None
}
