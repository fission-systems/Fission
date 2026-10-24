use crate::HashMap;
use crate::prelude::*; // For accessing normalizer helpers

pub fn apply_double_precision_reconstruction_pass(func: &mut PreHirFunction) -> bool {
    let mut changed = false;

    // Phase 1: Collapse contiguous loads and stores in the function body
    if collapse_contiguous_mem_ops(&mut func.body, &mut func.locals) {
        changed = true;
    }

    // Phase 2: Build definition map of single-assignment variables
    let mut def_counts = HashMap::default();
    count_defs_recursive(&func.body, &mut def_counts);
    let mut defs = HashMap::default();

    // Phase 3: Recursively rewrite Or-Shift reconstruction expressions
    if rewrite_recombine_exprs(&mut func.body, &mut defs, &def_counts) {
        changed = true;
    }

    changed
}

fn expr_reads_var(expr: &PreHirExpr, name: &str) -> bool {
    match expr {
        PreHirExpr::Var(n) => n == name,
        PreHirExpr::Cast { expr: inner, .. }
        | PreHirExpr::Unary { expr: inner, .. }
        | PreHirExpr::Load { ptr: inner, .. }
        | PreHirExpr::PtrOffset { base: inner, .. }
        | PreHirExpr::FieldAccess { base: inner, .. }
        | PreHirExpr::AggregateCopy { src: inner, .. } => expr_reads_var(inner, name),
        PreHirExpr::Binary { lhs, rhs, .. } => {
            expr_reads_var(lhs, name) || expr_reads_var(rhs, name)
        }
        PreHirExpr::Call { args, .. } => args.iter().any(|a| expr_reads_var(a, name)),
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            expr_reads_var(cond, name)
                || expr_reads_var(then_expr, name)
                || expr_reads_var(else_expr, name)
        }
        PreHirExpr::Index { base, index, .. } => {
            expr_reads_var(base, name) || expr_reads_var(index, name)
        }
        _ => false,
    }
}

fn expr_reads_memory(expr: &PreHirExpr) -> bool {
    match expr {
        PreHirExpr::Load { .. } | PreHirExpr::Call { .. } => true,
        PreHirExpr::Cast { expr: inner, .. }
        | PreHirExpr::Unary { expr: inner, .. }
        | PreHirExpr::PtrOffset { base: inner, .. }
        | PreHirExpr::FieldAccess { base: inner, .. }
        | PreHirExpr::AggregateCopy { src: inner, .. } => expr_reads_memory(inner),
        PreHirExpr::Binary { lhs, rhs, .. } => expr_reads_memory(lhs) || expr_reads_memory(rhs),
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            expr_reads_memory(cond) || expr_reads_memory(then_expr) || expr_reads_memory(else_expr)
        }
        PreHirExpr::Index { base, index, .. } => {
            expr_reads_memory(base) || expr_reads_memory(index)
        }
        _ => false,
    }
}

fn collect_written_vars(stmts: &[PreHirStmt], written: &mut crate::HashSet<String>) {
    for stmt in stmts {
        match stmt {
            PreHirStmt::Assign { lhs, .. } => {
                if let PreHirLValue::Var(name) = lhs {
                    written.insert(name.clone());
                }
            }
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. }
            | PreHirStmt::For { body, .. } => {
                collect_written_vars(body, written);
            }
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_written_vars(then_body, written);
                collect_written_vars(else_body, written);
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_written_vars(&case.body, written);
                }
                collect_written_vars(default, written);
            }
            _ => {}
        }
    }
}

fn count_defs_recursive(stmts: &[PreHirStmt], counts: &mut HashMap<String, usize>) {
    for stmt in stmts {
        match stmt {
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var(name),
                ..
            } => {
                *counts.entry(name.clone()).or_insert(0) += 1;
            }
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. }
            | PreHirStmt::For { body, .. } => {
                count_defs_recursive(body, counts);
            }
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                count_defs_recursive(then_body, counts);
                count_defs_recursive(else_body, counts);
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    count_defs_recursive(&case.body, counts);
                }
                count_defs_recursive(default, counts);
            }
            _ => {}
        }
    }
}

// ── Expression Rewriting ──────────────────────────────────────────────────

fn rewrite_recombine_exprs(
    stmts: &mut [PreHirStmt],
    defs: &mut HashMap<String, PreHirExpr>,
    counts: &HashMap<String, usize>,
) -> bool {
    let mut changed = false;
    for stmt in stmts {
        match stmt {
            PreHirStmt::Assign { lhs, rhs } => {
                changed |= rewrite_lvalue(lhs, defs);
                changed |= rewrite_expr(rhs, defs);
                match lhs {
                    PreHirLValue::Var(name) => {
                        defs.retain(|_, expr| !expr_reads_var(expr, name));
                        defs.remove(name);
                        if counts.get(name).copied().unwrap_or(0) == 1 {
                            defs.insert(name.clone(), rhs.clone());
                        }
                    }
                    PreHirLValue::Deref { .. }
                    | PreHirLValue::Index { .. }
                    | PreHirLValue::FieldAccess { .. } => {
                        defs.retain(|_, expr| !expr_reads_memory(expr));
                    }
                }
                if expr_reads_memory(rhs) {
                    defs.retain(|_, expr| !expr_reads_memory(expr));
                }
            }
            PreHirStmt::Expr(expr) => {
                changed |= rewrite_expr(expr, defs);
                if expr_reads_memory(expr) {
                    defs.retain(|_, expr| !expr_reads_memory(expr));
                }
            }
            PreHirStmt::Return(Some(expr)) => {
                changed |= rewrite_expr(expr, defs);
            }
            PreHirStmt::VaStart { va_list, .. } => {
                changed |= rewrite_expr(va_list, defs);
            }
            PreHirStmt::Block(body) => {
                changed |= rewrite_recombine_exprs(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    defs,
                    counts,
                );
            }
            PreHirStmt::While { cond, body } | PreHirStmt::DoWhile { cond, body } => {
                let mut written = crate::HashSet::default();
                collect_written_vars(body, &mut written);
                for v in &written {
                    defs.retain(|_, expr| !expr_reads_var(expr, v));
                    defs.remove(v);
                }
                changed |= rewrite_expr(cond, defs);
                let mut loop_defs = defs.clone();
                changed |= rewrite_recombine_exprs(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    &mut loop_defs,
                    counts,
                );
                for v in &written {
                    defs.retain(|_, expr| !expr_reads_var(expr, v));
                    defs.remove(v);
                }
            }
            PreHirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                if let Some(i) = init {
                    changed |= rewrite_stmt(i.as_mut(), defs, counts);
                }
                let mut written = crate::HashSet::default();
                if let Some(u) = update {
                    collect_written_vars(std::slice::from_ref(u.as_ref()), &mut written);
                }
                collect_written_vars(body, &mut written);
                for v in &written {
                    defs.retain(|_, expr| !expr_reads_var(expr, v));
                    defs.remove(v);
                }
                if let Some(c) = cond {
                    changed |= rewrite_expr(c, defs);
                }
                if let Some(u) = update {
                    changed |= rewrite_stmt(u.as_mut(), defs, counts);
                }
                let mut loop_defs = defs.clone();
                changed |= rewrite_recombine_exprs(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    &mut loop_defs,
                    counts,
                );
                for v in &written {
                    defs.retain(|_, expr| !expr_reads_var(expr, v));
                    defs.remove(v);
                }
            }
            PreHirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                changed |= rewrite_expr(expr, defs);
                let mut written = crate::HashSet::default();
                for case in cases.iter_mut() {
                    collect_written_vars(&case.body, &mut written);
                    let mut case_defs = defs.clone();
                    changed |= rewrite_recombine_exprs(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                        &mut case_defs,
                        counts,
                    );
                }
                collect_written_vars(default, &mut written);
                let mut def_defs = defs.clone();
                changed |= rewrite_recombine_exprs(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                    &mut def_defs,
                    counts,
                );
                for v in &written {
                    defs.retain(|_, expr| !expr_reads_var(expr, v));
                    defs.remove(v);
                }
            }
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                changed |= rewrite_expr(cond, defs);
                let mut then_defs = defs.clone();
                let mut else_defs = defs.clone();
                changed |= rewrite_recombine_exprs(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                    &mut then_defs,
                    counts,
                );
                changed |= rewrite_recombine_exprs(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                    &mut else_defs,
                    counts,
                );
                let mut written = crate::HashSet::default();
                collect_written_vars(then_body, &mut written);
                collect_written_vars(else_body, &mut written);
                for v in &written {
                    defs.retain(|_, expr| !expr_reads_var(expr, v));
                    defs.remove(v);
                }
            }
            _ => {}
        }
    }
    changed
}

fn rewrite_stmt(
    stmt: &mut PreHirStmt,
    defs: &mut HashMap<String, PreHirExpr>,
    counts: &HashMap<String, usize>,
) -> bool {
    rewrite_recombine_exprs(std::slice::from_mut(stmt), defs, counts)
}

fn rewrite_lvalue(lval: &mut PreHirLValue, defs: &HashMap<String, PreHirExpr>) -> bool {
    match lval {
        PreHirLValue::Var(_) => false,
        PreHirLValue::Deref { ptr, .. } => rewrite_expr(ptr, defs),
        PreHirLValue::Index { base, index, .. } => {
            let mut changed = rewrite_expr(base, defs);
            changed |= rewrite_expr(index, defs);
            changed
        }
        PreHirLValue::FieldAccess { base, .. } => rewrite_expr(base, defs),
    }
}

fn rewrite_expr(expr: &mut PreHirExpr, defs: &HashMap<String, PreHirExpr>) -> bool {
    let mut changed = false;

    // Recursively rewrite sub-expressions first
    match expr {
        PreHirExpr::Cast { expr: inner, .. }
        | PreHirExpr::Unary { expr: inner, .. }
        | PreHirExpr::Load { ptr: inner, .. }
        | PreHirExpr::PtrOffset { base: inner, .. }
        | PreHirExpr::AggregateCopy { src: inner, .. }
        | PreHirExpr::FieldAccess { base: inner, .. } => {
            changed |= rewrite_expr(inner, defs);
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            changed |= rewrite_expr(lhs, defs);
            changed |= rewrite_expr(rhs, defs);
        }
        PreHirExpr::Call { args, .. } => {
            for arg in args {
                changed |= rewrite_expr(arg, defs);
            }
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            changed |= rewrite_expr(cond, defs);
            changed |= rewrite_expr(then_expr, defs);
            changed |= rewrite_expr(else_expr, defs);
        }
        PreHirExpr::Index { base, index, .. } => {
            changed |= rewrite_expr(base, defs);
            changed |= rewrite_expr(index, defs);
        }
        PreHirExpr::Var(_)
        | PreHirExpr::AddressOfGlobal(_)
        | PreHirExpr::AddressOfLocal(_)
        | PreHirExpr::Const(_, _) => {}
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

fn match_recombine(expr: &PreHirExpr) -> Option<(PreHirExpr, PreHirExpr)> {
    match expr {
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Or | PreHirBinaryOp::Add,
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

fn match_hi_shift(expr: &PreHirExpr) -> Option<PreHirExpr> {
    match expr {
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Shl,
            lhs,
            rhs,
            ..
        } => {
            if let PreHirExpr::Const(32, _) = rhs.as_ref() {
                Some(*lhs.clone())
            } else {
                None
            }
        }
        PreHirExpr::Cast { expr: inner, .. } => match_hi_shift(inner),
        _ => None,
    }
}

fn strip_casts(expr: &PreHirExpr) -> &PreHirExpr {
    match expr {
        PreHirExpr::Cast { expr: inner, .. } => strip_casts(inner),
        _ => expr,
    }
}

fn get_var_name(expr: &PreHirExpr) -> Option<String> {
    match strip_casts(expr) {
        PreHirExpr::Var(name) => Some(name.clone()),
        _ => None,
    }
}

fn try_reconstruct(
    hi_name: &str,
    lo_name: &str,
    defs: &HashMap<String, PreHirExpr>,
) -> Option<PreHirExpr> {
    let hi_def = defs.get(hi_name)?;
    let lo_def = defs.get(lo_name)?;

    // 1. Logical operations: AND, OR, XOR
    if let (
        PreHirExpr::Binary {
            op: op_hi,
            lhs: lhs_hi,
            rhs: rhs_hi,
            ty: ty_hi,
        },
        PreHirExpr::Binary {
            op: op_lo,
            lhs: lhs_lo,
            rhs: rhs_lo,
            ..
        },
    ) = (hi_def, lo_def)
    {
        if op_hi == op_lo
            && matches!(
                op_hi,
                PreHirBinaryOp::And | PreHirBinaryOp::Or | PreHirBinaryOp::Xor
            )
        {
            let val1 = make_recombine((**lhs_hi).clone(), (**lhs_lo).clone(), ty_hi);
            let val2 = make_recombine((**rhs_hi).clone(), (**rhs_lo).clone(), ty_hi);
            return Some(PreHirExpr::Binary {
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
    if let PreHirExpr::Binary {
        op: PreHirBinaryOp::Add,
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
                return Some(PreHirExpr::Binary {
                    op: PreHirBinaryOp::Add,
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
    if let PreHirExpr::Binary {
        op: PreHirBinaryOp::Sub,
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
                return Some(PreHirExpr::Binary {
                    op: PreHirBinaryOp::Sub,
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
    if let PreHirExpr::Binary {
        op: PreHirBinaryOp::Shl,
        lhs: lo_src,
        rhs: lo_shift,
        ..
    } = lo_def
    {
        if let PreHirExpr::Const(shift_amt, _) = lo_shift.as_ref() {
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
                    return Some(PreHirExpr::Binary {
                        op: PreHirBinaryOp::Shl,
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
    if let PreHirExpr::Binary {
        op: op_hi @ (PreHirBinaryOp::Shr | PreHirBinaryOp::Sar),
        lhs: hi_src,
        rhs: hi_shift,
        ..
    } = hi_def
    {
        if let PreHirExpr::Const(shift_amt, _) = hi_shift.as_ref() {
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
                    return Some(PreHirExpr::Binary {
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

fn make_recombine(hi: PreHirExpr, lo: PreHirExpr, _ty: &NirType) -> PreHirExpr {
    PreHirExpr::Binary {
        op: PreHirBinaryOp::Or,
        lhs: Box::new(PreHirExpr::Binary {
            op: PreHirBinaryOp::Shl,
            lhs: Box::new(PreHirExpr::Cast {
                ty: NirType::Int {
                    bits: 64,
                    signed: false,
                },
                expr: Box::new(hi),
            }),
            rhs: Box::new(PreHirExpr::Const(
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
        rhs: Box::new(PreHirExpr::Cast {
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

fn match_hi_add_carry(expr: &PreHirExpr) -> Option<(PreHirExpr, PreHirExpr, PreHirExpr)> {
    match expr {
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs,
            rhs,
            ..
        } => {
            if let PreHirExpr::Binary {
                op: PreHirBinaryOp::Add,
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
            if let PreHirExpr::Binary {
                op: PreHirBinaryOp::Add,
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
    carry_expr: &PreHirExpr,
    lo1: &PreHirExpr,
    lo2: &PreHirExpr,
    lo_name: &str,
    defs: &HashMap<String, PreHirExpr>,
) -> bool {
    is_carry_of_depth(carry_expr, lo1, lo2, lo_name, defs, 0)
}

fn is_carry_of_depth(
    carry_expr: &PreHirExpr,
    lo1: &PreHirExpr,
    lo2: &PreHirExpr,
    lo_name: &str,
    defs: &HashMap<String, PreHirExpr>,
    depth: usize,
) -> bool {
    if depth > 8 {
        return false;
    }
    let expr = strip_casts(carry_expr);
    if let PreHirExpr::Binary {
        op: PreHirBinaryOp::Lt | PreHirBinaryOp::SLt,
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
    if let PreHirExpr::Call { target, args, .. } = expr {
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
    if let PreHirExpr::Var(name) = expr {
        if let Some(def) = defs.get(name) {
            return is_carry_of_depth(def, lo1, lo2, lo_name, defs, depth + 1);
        }
    }
    false
}

fn match_hi_sub_borrow(expr: &PreHirExpr) -> Option<(PreHirExpr, PreHirExpr, PreHirExpr)> {
    match expr {
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Sub,
            lhs,
            rhs,
            ..
        } => {
            if let PreHirExpr::Binary {
                op: PreHirBinaryOp::Sub,
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
    borrow_expr: &PreHirExpr,
    lo1: &PreHirExpr,
    lo2: &PreHirExpr,
    lo_name: &str,
    defs: &HashMap<String, PreHirExpr>,
) -> bool {
    is_borrow_of_depth(borrow_expr, lo1, lo2, lo_name, defs, 0)
}

fn is_borrow_of_depth(
    borrow_expr: &PreHirExpr,
    lo1: &PreHirExpr,
    lo2: &PreHirExpr,
    lo_name: &str,
    defs: &HashMap<String, PreHirExpr>,
    depth: usize,
) -> bool {
    if depth > 8 {
        return false;
    }
    let expr = strip_casts(borrow_expr);
    if let PreHirExpr::Binary {
        op: PreHirBinaryOp::Lt | PreHirBinaryOp::SLt,
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
    if let PreHirExpr::Call { target, args, .. } = expr {
        if target == "__sborrow" && args.len() >= 2 {
            let a = get_var_name(&args[0]);
            let b = get_var_name(&args[1]);
            if a == get_var_name(lo1) && b == get_var_name(lo2) {
                return true;
            }
        }
    }
    if let PreHirExpr::Var(name) = expr {
        if let Some(def) = defs.get(name) {
            return is_borrow_of_depth(def, lo1, lo2, lo_name, defs, depth + 1);
        }
    }
    false
}

fn match_shl(expr: &PreHirExpr) -> Option<(PreHirExpr, i64)> {
    let expr = strip_casts(expr);
    if let PreHirExpr::Binary {
        op: PreHirBinaryOp::Shl,
        lhs,
        rhs,
        ..
    } = expr
    {
        if let PreHirExpr::Const(shift, _) = rhs.as_ref() {
            return Some((lhs.as_ref().clone(), *shift));
        }
    }
    None
}

fn match_shr(expr: &PreHirExpr) -> Option<(PreHirExpr, i64)> {
    let expr = strip_casts(expr);
    if let PreHirExpr::Binary {
        op: PreHirBinaryOp::Shr | PreHirBinaryOp::Sar,
        lhs,
        rhs,
        ..
    } = expr
    {
        if let PreHirExpr::Const(shift, _) = rhs.as_ref() {
            return Some((lhs.as_ref().clone(), *shift));
        }
    }
    None
}

fn match_hi_shl_mix(
    expr: &PreHirExpr,
    expected_lo_src: &PreHirExpr,
    expected_shift: i64,
) -> Option<(PreHirExpr, i64)> {
    let expr = strip_casts(expr);
    if let PreHirExpr::Binary {
        op: PreHirBinaryOp::Or,
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
    expr: &PreHirExpr,
    expected_hi_src: &PreHirExpr,
    expected_shift: i64,
) -> Option<(PreHirExpr, i64)> {
    let expr = strip_casts(expr);
    if let PreHirExpr::Binary {
        op: PreHirBinaryOp::Or,
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

fn collapse_contiguous_mem_ops(
    stmts: &mut Vec<PreHirStmt>,
    locals: &mut Vec<PreHirBinding>,
) -> bool {
    let mut changed = false;

    // First recurse into nested blocks
    for stmt in stmts.iter_mut() {
        match stmt {
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. }
            | PreHirStmt::For { body, .. } => {
                changed |= collapse_contiguous_mem_ops(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    locals,
                );
            }
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= collapse_contiguous_mem_ops(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                    locals,
                );
                changed |= collapse_contiguous_mem_ops(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                    locals,
                );
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= collapse_contiguous_mem_ops(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                        locals,
                    );
                }
                changed |= collapse_contiguous_mem_ops(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                    locals,
                );
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
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var(lo_var),
                    rhs:
                        PreHirExpr::Load {
                            ptr: lo_ptr,
                            ty: lo_ty,
                        },
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var(hi_var),
                    rhs:
                        PreHirExpr::Load {
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
                    locals.push(PreHirBinding {
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
                    new_stmts.push(PreHirStmt::Assign {
                        lhs: PreHirLValue::Var(temp_64_name.clone()),
                        rhs: PreHirExpr::Load {
                            ptr: lo_ptr.clone(),
                            ty: NirType::Int {
                                bits: 64,
                                signed: false,
                            },
                        },
                    });
                    // lo_var = cast(temp_64)
                    new_stmts.push(PreHirStmt::Assign {
                        lhs: PreHirLValue::Var(lo_var.clone()),
                        rhs: PreHirExpr::Cast {
                            ty: lo_ty.clone(),
                            expr: Box::new(PreHirExpr::Var(temp_64_name.clone())),
                        },
                    });
                    // hi_var = cast(temp_64 >> 32)
                    new_stmts.push(PreHirStmt::Assign {
                        lhs: PreHirLValue::Var(hi_var.clone()),
                        rhs: PreHirExpr::Cast {
                            ty: hi_ty.clone(),
                            expr: Box::new(PreHirExpr::Binary {
                                op: PreHirBinaryOp::Shr,
                                lhs: Box::new(PreHirExpr::Var(temp_64_name.clone())),
                                rhs: Box::new(PreHirExpr::Const(
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
                PreHirStmt::Assign {
                    lhs:
                        PreHirLValue::Deref {
                            ptr: lo_ptr,
                            ty: lo_ty,
                        },
                    rhs: lo_val,
                },
                PreHirStmt::Assign {
                    lhs:
                        PreHirLValue::Deref {
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
                        new_stmts.push(PreHirStmt::Assign {
                            lhs: PreHirLValue::Deref {
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

fn is_ptr_offset_by_4(hi_ptr: &PreHirExpr, lo_ptr: &PreHirExpr) -> bool {
    match hi_ptr {
        PreHirExpr::PtrOffset { base, offset: 4 } => base.as_ref() == lo_ptr,
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs,
            rhs,
            ..
        } => {
            if let PreHirExpr::Const(4, _) = rhs.as_ref() {
                lhs.as_ref() == lo_ptr
            } else {
                false
            }
        }
        _ => false,
    }
}

fn match_split_stores(lo_val: &PreHirExpr, hi_val: &PreHirExpr) -> Option<PreHirExpr> {
    // lo_val is Cast(val_64) or subpiece
    // hi_val is Cast(val_64 >> 32)
    let lo_inner = strip_casts(lo_val);
    let hi_inner = strip_casts(hi_val);
    if let PreHirExpr::Binary {
        op: PreHirBinaryOp::Shr | PreHirBinaryOp::Sar,
        lhs: hi_src,
        rhs: shift,
        ..
    } = hi_inner
    {
        if let PreHirExpr::Const(32, _) = shift.as_ref() {
            let lo_name = get_var_name(lo_inner);
            let hi_name = get_var_name(hi_src);
            if lo_name.is_some() && lo_name == hi_name {
                return Some(hi_src.as_ref().clone());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u32_ty() -> NirType {
        NirType::Int {
            bits: 32,
            signed: false,
        }
    }

    #[test]
    fn match_split_stores_rejects_non_var_expressions() {
        let lo_val = PreHirExpr::Load {
            ptr: Box::new(PreHirExpr::Var("p".to_string())),
            ty: u32_ty(),
        };
        let hi_val = PreHirExpr::Binary {
            op: PreHirBinaryOp::Shr,
            lhs: Box::new(PreHirExpr::Load {
                ptr: Box::new(PreHirExpr::Var("q".to_string())),
                ty: u32_ty(),
            }),
            rhs: Box::new(PreHirExpr::Const(32, u32_ty())),
            ty: u32_ty(),
        };
        assert!(match_split_stores(&lo_val, &hi_val).is_none());
    }

    #[test]
    fn is_carry_of_handles_cyclic_defs_without_stack_overflow() {
        let mut defs = HashMap::default();
        defs.insert("x".to_string(), PreHirExpr::Var("x".to_string()));
        let carry_expr = PreHirExpr::Var("x".to_string());
        let lo1 = PreHirExpr::Var("a".to_string());
        let lo2 = PreHirExpr::Var("b".to_string());
        assert!(!is_carry_of(&carry_expr, &lo1, &lo2, "lo", &defs));
    }
}
