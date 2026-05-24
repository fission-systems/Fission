use super::super::*;
use std::collections::HashMap;
use crate::nir::support::expr_type;

struct SimpleAssign {
    lhs: String,
    rhs: HirExpr,
}

fn collect_assignments(stmts: &[HirStmt], assigns: &mut Vec<SimpleAssign>) {
    for stmt in stmts {
        match stmt {
            HirStmt::Assign { lhs: HirLValue::Var(name), rhs } => {
                assigns.push(SimpleAssign { lhs: name.clone(), rhs: rhs.clone() });
            }
            HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
                collect_assignments(body, assigns);
            }
            HirStmt::For { init, update, body, .. } => {
                if let Some(i) = init {
                    collect_assignments(std::slice::from_ref(i.as_ref()), assigns);
                }
                if let Some(u) = update {
                    collect_assignments(std::slice::from_ref(u.as_ref()), assigns);
                }
                collect_assignments(body, assigns);
            }
            HirStmt::If { then_body, else_body, .. } => {
                collect_assignments(then_body, assigns);
                collect_assignments(else_body, assigns);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_assignments(&case.body, assigns);
                }
                collect_assignments(default, assigns);
            }
            _ => {}
        }
    }
}

fn type_bits(ty: &NirType) -> u32 {
    match ty {
        NirType::Bool => 1,
        NirType::Int { bits, .. } => *bits,
        _ => 64,
    }
}

fn match_piece_concat(expr: &HirExpr) -> Option<(HirExpr, HirExpr, u32)> {
    if let HirExpr::Binary { op: HirBinaryOp::Or, lhs, rhs, .. } = expr {
        if let Some((hi, shift_bits)) = match_shifted_high(lhs) {
            let lo = match_unshifted_low(rhs);
            return Some((hi, lo, shift_bits));
        }
        if let Some((hi, shift_bits)) = match_shifted_high(rhs) {
            let lo = match_unshifted_low(lhs);
            return Some((hi, lo, shift_bits));
        }
    }
    None
}

fn match_shifted_high(expr: &HirExpr) -> Option<(HirExpr, u32)> {
    if let HirExpr::Binary { op: HirBinaryOp::Shl, lhs, rhs, .. } = expr {
        if let HirExpr::Const(shift_bits, _) = &**rhs {
            let hi = if let HirExpr::Cast { expr: inner, .. } = &**lhs {
                (**inner).clone()
            } else {
                (**lhs).clone()
            };
            return Some((hi, *shift_bits as u32));
        }
    }
    None
}

fn match_unshifted_low(expr: &HirExpr) -> HirExpr {
    if let HirExpr::Cast { expr: inner, .. } = expr {
        (**inner).clone()
    } else {
        expr.clone()
    }
}

fn match_zext_write(expr: &HirExpr, x_bits: u32) -> Option<(HirExpr, HirExpr, u32)> {
    if let HirExpr::Cast { ty, expr: inner } = expr {
        let inner_bits = type_bits(&expr_type(inner));
        if x_bits > inner_bits {
            let hi = HirExpr::Const(0, NirType::Int { bits: x_bits - inner_bits, signed: false });
            return Some((hi, (**inner).clone(), inner_bits));
        }
    }
    None
}

fn is_valid_low_extract(expr: &HirExpr, x_name: &str, shift_bits: u32) -> bool {
    if let HirExpr::Cast { ty, expr: inner } = expr {
        if let HirExpr::Var(name) = &**inner {
            if name == x_name {
                if let NirType::Int { bits, .. } = ty {
                    return *bits <= shift_bits;
                }
                if let NirType::Bool = ty {
                    return 1 <= shift_bits;
                }
            }
        }
    }
    false
}

fn is_valid_high_extract(expr: &HirExpr, x_name: &str, shift_bits: u32) -> bool {
    if let HirExpr::Cast { expr: inner, .. } = expr {
        if let HirExpr::Binary { op: HirBinaryOp::Shr | HirBinaryOp::Sar, lhs, rhs, .. } = &**inner {
            if let (HirExpr::Var(name), HirExpr::Const(sa, _)) = (&**lhs, &**rhs) {
                if name == x_name && *sa as u32 == shift_bits {
                    return true;
                }
            }
        }
    }
    if let HirExpr::Binary { op: HirBinaryOp::Shr | HirBinaryOp::Sar, lhs, rhs, .. } = expr {
        if let (HirExpr::Var(name), HirExpr::Const(sa, _)) = (&**lhs, &**rhs) {
            if name == x_name && *sa as u32 == shift_bits {
                return true;
            }
        }
    }
    false
}

fn for_each_child_expr_ref<F>(expr: &HirExpr, mut f: F)
where
    F: FnMut(&HirExpr),
{
    match expr {
        HirExpr::Cast { expr: inner, .. }
        | HirExpr::Unary { expr: inner, .. }
        | HirExpr::Load { ptr: inner, .. }
        | HirExpr::PtrOffset { base: inner, .. }
        | HirExpr::AggregateCopy { src: inner, .. } => {
            f(inner);
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            f(lhs);
            f(rhs);
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                f(arg);
            }
        }
        HirExpr::Index { base, index, .. } => {
            f(base);
            f(index);
        }
        HirExpr::Select { cond, then_expr, else_expr, .. } => {
            f(cond);
            f(then_expr);
            f(else_expr);
        }
        _ => {}
    }
}

fn verify_reads(expr: &HirExpr, x_name: &str, shift_bits: u32, valid: &mut bool) {
    if !*valid {
        return;
    }
    if is_valid_low_extract(expr, x_name, shift_bits) {
        return;
    }
    if is_valid_high_extract(expr, x_name, shift_bits) {
        return;
    }
    if let HirExpr::Var(name) = expr {
        if name == x_name {
            *valid = false;
            return;
        }
    }
    for_each_child_expr_ref(expr, |child| verify_reads(child, x_name, shift_bits, valid));
}

fn verify_reads_in_stmt(stmt: &HirStmt, x_name: &str, shift_bits: u32, valid: &mut bool) {
    if !*valid {
        return;
    }
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            match lhs {
                HirLValue::Var(_) => {}
                HirLValue::Deref { ptr, .. } => {
                    verify_reads(ptr, x_name, shift_bits, valid);
                }
                HirLValue::Index { base, index, .. } => {
                    verify_reads(base, x_name, shift_bits, valid);
                    verify_reads(index, x_name, shift_bits, valid);
                }
            }
            verify_reads(rhs, x_name, shift_bits, valid);
        }
        HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => {
            verify_reads(expr, x_name, shift_bits, valid);
        }
        HirStmt::VaStart { va_list, .. } => {
            verify_reads(va_list, x_name, shift_bits, valid);
        }
        HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            for s in body {
                verify_reads_in_stmt(s, x_name, shift_bits, valid);
            }
        }
        HirStmt::For { init, cond, update, body } => {
            if let Some(i) = init {
                verify_reads_in_stmt(i, x_name, shift_bits, valid);
            }
            if let Some(c) = cond {
                verify_reads(c, x_name, shift_bits, valid);
            }
            if let Some(u) = update {
                verify_reads_in_stmt(u, x_name, shift_bits, valid);
            }
            for s in body {
                verify_reads_in_stmt(s, x_name, shift_bits, valid);
            }
        }
        HirStmt::If { cond, then_body, else_body } => {
            verify_reads(cond, x_name, shift_bits, valid);
            for s in then_body {
                verify_reads_in_stmt(s, x_name, shift_bits, valid);
            }
            for s in else_body {
                verify_reads_in_stmt(s, x_name, shift_bits, valid);
            }
        }
        HirStmt::Switch { expr, cases, default } => {
            verify_reads(expr, x_name, shift_bits, valid);
            for case in cases {
                for s in &case.body {
                    verify_reads_in_stmt(s, x_name, shift_bits, valid);
                }
            }
            for s in default {
                verify_reads_in_stmt(s, x_name, shift_bits, valid);
            }
        }
        _ => {}
    }
}

fn rewrite_expr(
    expr: &mut HirExpr,
    x_name: &str,
    x_low: &str,
    x_high: &str,
    shift_bits: u32,
) {
    if is_valid_low_extract(expr, x_name, shift_bits) {
        if let HirExpr::Cast { ty, .. } = expr {
            *expr = HirExpr::Cast {
                ty: ty.clone(),
                expr: Box::new(HirExpr::Var(x_low.to_string())),
            };
            return;
        }
    }
    if is_valid_high_extract(expr, x_name, shift_bits) {
        if let HirExpr::Cast { ty, .. } = expr {
            *expr = HirExpr::Cast {
                ty: ty.clone(),
                expr: Box::new(HirExpr::Var(x_high.to_string())),
            };
            return;
        }
        if let HirExpr::Binary { .. } = expr {
            *expr = HirExpr::Var(x_high.to_string());
            return;
        }
    }

    match expr {
        HirExpr::Cast { expr: inner, .. }
        | HirExpr::Unary { expr: inner, .. }
        | HirExpr::Load { ptr: inner, .. }
        | HirExpr::PtrOffset { base: inner, .. }
        | HirExpr::AggregateCopy { src: inner, .. } => {
            rewrite_expr(inner, x_name, x_low, x_high, shift_bits);
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            rewrite_expr(lhs, x_name, x_low, x_high, shift_bits);
            rewrite_expr(rhs, x_name, x_low, x_high, shift_bits);
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                rewrite_expr(arg, x_name, x_low, x_high, shift_bits);
            }
        }
        HirExpr::Index { base, index, .. } => {
            rewrite_expr(base, x_name, x_low, x_high, shift_bits);
            rewrite_expr(index, x_name, x_low, x_high, shift_bits);
        }
        HirExpr::Select { cond, then_expr, else_expr, .. } => {
            rewrite_expr(cond, x_name, x_low, x_high, shift_bits);
            rewrite_expr(then_expr, x_name, x_low, x_high, shift_bits);
            rewrite_expr(else_expr, x_name, x_low, x_high, shift_bits);
        }
        _ => {}
    }
}

fn rewrite_stmt(
    stmt: &mut HirStmt,
    x_name: &str,
    x_low: &str,
    x_high: &str,
    shift_bits: u32,
    x_bits: u32,
) {
    match stmt {
        HirStmt::Assign { lhs: HirLValue::Var(name), rhs } if name == x_name => {
            if let Some((hi, lo, _)) = match_piece_concat(rhs) {
                *stmt = HirStmt::Block(vec![
                    HirStmt::Assign {
                        lhs: HirLValue::Var(x_low.to_string()),
                        rhs: lo,
                    },
                    HirStmt::Assign {
                        lhs: HirLValue::Var(x_high.to_string()),
                        rhs: hi,
                    },
                ]);
            } else if let Some((hi, lo, _)) = match_zext_write(rhs, x_bits) {
                *stmt = HirStmt::Block(vec![
                    HirStmt::Assign {
                        lhs: HirLValue::Var(x_low.to_string()),
                        rhs: lo,
                    },
                    HirStmt::Assign {
                        lhs: HirLValue::Var(x_high.to_string()),
                        rhs: hi,
                    },
                ]);
            }
        }
        HirStmt::Assign { lhs, rhs } => {
            match lhs {
                HirLValue::Deref { ptr, .. } => {
                    rewrite_expr(ptr, x_name, x_low, x_high, shift_bits);
                }
                HirLValue::Index { base, index, .. } => {
                    rewrite_expr(base, x_name, x_low, x_high, shift_bits);
                    rewrite_expr(index, x_name, x_low, x_high, shift_bits);
                }
                _ => {}
            }
            rewrite_expr(rhs, x_name, x_low, x_high, shift_bits);
        }
        HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => {
            rewrite_expr(expr, x_name, x_low, x_high, shift_bits);
        }
        HirStmt::VaStart { va_list, .. } => {
            rewrite_expr(va_list, x_name, x_low, x_high, shift_bits);
        }
        HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            rewrite_stmts(body, x_name, x_low, x_high, shift_bits, x_bits);
        }
        HirStmt::For { init, cond, update, body } => {
            if let Some(i) = init {
                rewrite_stmt(i, x_name, x_low, x_high, shift_bits, x_bits);
            }
            if let Some(c) = cond {
                rewrite_expr(c, x_name, x_low, x_high, shift_bits);
            }
            if let Some(u) = update {
                rewrite_stmt(u, x_name, x_low, x_high, shift_bits, x_bits);
            }
            rewrite_stmts(body, x_name, x_low, x_high, shift_bits, x_bits);
        }
        HirStmt::If { cond, then_body, else_body } => {
            rewrite_expr(cond, x_name, x_low, x_high, shift_bits);
            rewrite_stmts(then_body, x_name, x_low, x_high, shift_bits, x_bits);
            rewrite_stmts(else_body, x_name, x_low, x_high, shift_bits, x_bits);
        }
        HirStmt::Switch { expr, cases, default } => {
            rewrite_expr(expr, x_name, x_low, x_high, shift_bits);
            for case in cases {
                rewrite_stmts(&mut case.body, x_name, x_low, x_high, shift_bits, x_bits);
            }
            rewrite_stmts(default, x_name, x_low, x_high, shift_bits, x_bits);
        }
        _ => {}
    }
}

fn rewrite_stmts(
    stmts: &mut [HirStmt],
    x_name: &str,
    x_low: &str,
    x_high: &str,
    shift_bits: u32,
    x_bits: u32,
) {
    for stmt in stmts {
        rewrite_stmt(stmt, x_name, x_low, x_high, shift_bits, x_bits);
    }
}

pub(crate) fn apply_split_flow_pass(func: &mut HirFunction) -> bool {
    let mut type_map = HashMap::new();
    for binding in func.params.iter().chain(func.locals.iter()) {
        type_map.insert(binding.name.clone(), binding.ty.clone());
    }

    let mut assigns = Vec::new();
    collect_assignments(&func.body, &mut assigns);

    let mut var_assigns: HashMap<String, Vec<HirExpr>> = HashMap::new();
    for assign in assigns {
        var_assigns.entry(assign.lhs).or_default().push(assign.rhs);
    }

    let mut changed = false;

    for local in func.locals.clone() {
        let name = &local.name;
        let Some(rhs_exprs) = var_assigns.get(name) else { continue; };
        if rhs_exprs.is_empty() { continue; }

        let NirType::Int { bits: x_bits, signed } = local.ty else { continue; };

        let mut first_shift = None;
        let mut all_match = true;

        for rhs in rhs_exprs {
            if let Some((_, _, shift_bits)) = match_piece_concat(rhs) {
                if let Some(first) = first_shift {
                    if first != shift_bits {
                        all_match = false;
                        break;
                    }
                } else {
                    first_shift = Some(shift_bits);
                }
            } else if let Some((_, _, shift_bits)) = match_zext_write(rhs, x_bits) {
                if let Some(first) = first_shift {
                    if first != shift_bits {
                        all_match = false;
                        break;
                    }
                } else {
                    first_shift = Some(shift_bits);
                }
            } else {
                all_match = false;
                break;
            }
        }

        if !all_match { continue; }
        let Some(shift_bits) = first_shift else { continue; };
        if shift_bits >= x_bits { continue; }

        let mut is_valid = true;
        for stmt in &func.body {
            verify_reads_in_stmt(stmt, name, shift_bits, &mut is_valid);
        }

        if is_valid {
            let x_low = format!("{}_low", name);
            let x_high = format!("{}_high", name);

            let low_binding = NirBinding {
                name: x_low.clone(),
                ty: NirType::Int { bits: shift_bits, signed: false },
                surface_type_name: None,
                origin: Some(NirBindingOrigin::Temp),
                initializer: None,
            };
            let high_binding = NirBinding {
                name: x_high.clone(),
                ty: NirType::Int { bits: x_bits - shift_bits, signed },
                surface_type_name: None,
                origin: Some(NirBindingOrigin::Temp),
                initializer: None,
            };
            func.locals.push(low_binding);
            func.locals.push(high_binding);

            rewrite_stmts(&mut func.body, name, &x_low, &x_high, shift_bits, x_bits);
            func.locals.retain(|l| &l.name != name);

            changed = true;
        }
    }

    changed
}
