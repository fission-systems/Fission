use super::super::*;
use crate::midend::support::expr_type;
use std::collections::{HashMap, HashSet};

struct SimpleAssign {
    lhs: String,
    rhs: HirExpr,
}

fn collect_assignments(stmts: &[HirStmt], assigns: &mut Vec<SimpleAssign>) {
    for stmt in stmts {
        match stmt {
            HirStmt::Assign {
                lhs: HirLValue::Var(name),
                rhs,
            } => {
                assigns.push(SimpleAssign {
                    lhs: name.clone(),
                    rhs: rhs.clone(),
                });
            }
            HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
                collect_assignments(body, assigns);
            }
            HirStmt::For {
                init, update, body, ..
            } => {
                if let Some(i) = init {
                    collect_assignments(std::slice::from_ref(i.as_ref()), assigns);
                }
                if let Some(u) = update {
                    collect_assignments(std::slice::from_ref(u.as_ref()), assigns);
                }
                collect_assignments(body, assigns);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
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
    if let HirExpr::Binary {
        op: HirBinaryOp::Or,
        lhs,
        rhs,
        ..
    } = expr
    {
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
    if let HirExpr::Binary {
        op: HirBinaryOp::Shl,
        lhs,
        rhs,
        ..
    } = expr
    {
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
            let hi = HirExpr::Const(
                0,
                NirType::Int {
                    bits: x_bits - inner_bits,
                    signed: false,
                },
            );
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
        if let HirExpr::Binary {
            op: HirBinaryOp::Shr | HirBinaryOp::Sar,
            lhs,
            rhs,
            ..
        } = &**inner
        {
            if let (HirExpr::Var(name), HirExpr::Const(sa, _)) = (&**lhs, &**rhs) {
                if name == x_name && *sa as u32 == shift_bits {
                    return true;
                }
            }
        }
    }
    if let HirExpr::Binary {
        op: HirBinaryOp::Shr | HirBinaryOp::Sar,
        lhs,
        rhs,
        ..
    } = expr
    {
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
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
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
                HirLValue::FieldAccess { base, .. } => {
                    verify_reads(base, x_name, shift_bits, valid);
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
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
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
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            verify_reads(cond, x_name, shift_bits, valid);
            for s in then_body {
                verify_reads_in_stmt(s, x_name, shift_bits, valid);
            }
            for s in else_body {
                verify_reads_in_stmt(s, x_name, shift_bits, valid);
            }
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
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

fn rewrite_expr(expr: &mut HirExpr, x_name: &str, x_low: &str, x_high: &str, shift_bits: u32) {
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
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
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
        HirStmt::Assign {
            lhs: HirLValue::Var(name),
            rhs,
        } if name == x_name => {
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
                HirLValue::FieldAccess { base, .. } => {
                    rewrite_expr(base, x_name, x_low, x_high, shift_bits);
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
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
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
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            rewrite_expr(cond, x_name, x_low, x_high, shift_bits);
            rewrite_stmts(then_body, x_name, x_low, x_high, shift_bits, x_bits);
            rewrite_stmts(else_body, x_name, x_low, x_high, shift_bits, x_bits);
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
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

fn narrow_scalar_assignment(expr: &HirExpr) -> Option<(NirType, HirExpr)> {
    let scalar_expr = match expr {
        HirExpr::Cast {
            ty: NirType::Ptr(_),
            expr: inner,
        } => inner.as_ref(),
        other => other,
    };
    let scalar_ty = expr_type(scalar_expr);
    match scalar_ty {
        NirType::Bool => Some((NirType::Bool, scalar_expr.clone())),
        NirType::Int { bits, .. } if bits <= 16 => Some((scalar_ty, scalar_expr.clone())),
        _ => None,
    }
}

fn stmt_defines_name(stmt: &HirStmt, name: &str) -> bool {
    matches!(
        stmt,
        HirStmt::Assign {
            lhs: HirLValue::Var(lhs),
            ..
        } if lhs == name
    )
}

fn expr_reads_name(expr: &HirExpr, name: &str) -> bool {
    match expr {
        HirExpr::Var(var) => var == name,
        HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => false,
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::Load { ptr: expr, .. }
        | HirExpr::PtrOffset { base: expr, .. }
        | HirExpr::FieldAccess { base: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. } => expr_reads_name(expr, name),
        HirExpr::Binary { lhs, rhs, .. } => {
            expr_reads_name(lhs, name) || expr_reads_name(rhs, name)
        }
        HirExpr::Call { args, .. } => args.iter().any(|arg| expr_reads_name(arg, name)),
        HirExpr::Index { base, index, .. } => {
            expr_reads_name(base, name) || expr_reads_name(index, name)
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            expr_reads_name(cond, name)
                || expr_reads_name(then_expr, name)
                || expr_reads_name(else_expr, name)
        }
    }
}

fn lvalue_reads_name(lhs: &HirLValue, name: &str) -> bool {
    match lhs {
        HirLValue::Var(_) => false,
        HirLValue::Deref { ptr, .. } => expr_reads_name(ptr, name),
        HirLValue::Index { base, index, .. } => {
            expr_reads_name(base, name) || expr_reads_name(index, name)
        }
        HirLValue::FieldAccess { base, .. } => expr_reads_name(base, name),
    }
}

fn stmt_reads_name(stmt: &HirStmt, name: &str) -> bool {
    match stmt {
        HirStmt::Assign { lhs, rhs } => lvalue_reads_name(lhs, name) || expr_reads_name(rhs, name),
        HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => expr_reads_name(expr, name),
        HirStmt::VaStart { va_list, .. } => expr_reads_name(va_list, name),
        HirStmt::Return(None)
        | HirStmt::Label(_)
        | HirStmt::Goto(_)
        | HirStmt::Break
        | HirStmt::Continue => false,
        HirStmt::Block(_)
        | HirStmt::If { .. }
        | HirStmt::While { .. }
        | HirStmt::DoWhile { .. }
        | HirStmt::For { .. }
        | HirStmt::Switch { .. } => false,
    }
}

fn stmt_reads_name_deep(stmt: &HirStmt, name: &str) -> bool {
    match stmt {
        HirStmt::Assign { lhs, rhs } => lvalue_reads_name(lhs, name) || expr_reads_name(rhs, name),
        HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => expr_reads_name(expr, name),
        HirStmt::VaStart { va_list, .. } => expr_reads_name(va_list, name),
        HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            body.iter().any(|stmt| stmt_reads_name_deep(stmt, name))
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            expr_reads_name(cond, name)
                || then_body
                    .iter()
                    .any(|stmt| stmt_reads_name_deep(stmt, name))
                || else_body
                    .iter()
                    .any(|stmt| stmt_reads_name_deep(stmt, name))
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            init.as_deref()
                .is_some_and(|stmt| stmt_reads_name_deep(stmt, name))
                || cond
                    .as_ref()
                    .is_some_and(|expr| expr_reads_name(expr, name))
                || update
                    .as_deref()
                    .is_some_and(|stmt| stmt_reads_name_deep(stmt, name))
                || body.iter().any(|stmt| stmt_reads_name_deep(stmt, name))
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            expr_reads_name(expr, name)
                || cases.iter().any(|case| {
                    case.body
                        .iter()
                        .any(|stmt| stmt_reads_name_deep(stmt, name))
                })
                || default.iter().any(|stmt| stmt_reads_name_deep(stmt, name))
        }
        HirStmt::Return(None)
        | HirStmt::Label(_)
        | HirStmt::Goto(_)
        | HirStmt::Break
        | HirStmt::Continue => false,
    }
}

fn expr_reads_name_as_address(expr: &HirExpr, name: &str) -> bool {
    match expr {
        HirExpr::Load { ptr, .. }
        | HirExpr::PtrOffset { base: ptr, .. }
        | HirExpr::FieldAccess { base: ptr, .. } => {
            expr_reads_name(ptr, name) || expr_reads_name_as_address(ptr, name)
        }
        HirExpr::Index { base, index, .. } => {
            expr_reads_name(base, name)
                || expr_reads_name_as_address(base, name)
                || expr_reads_name_as_address(index, name)
        }
        HirExpr::Cast { expr, .. } | HirExpr::Unary { expr, .. } => {
            expr_reads_name_as_address(expr, name)
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            expr_reads_name_as_address(lhs, name) || expr_reads_name_as_address(rhs, name)
        }
        HirExpr::Call { args, .. } => args.iter().any(|arg| expr_reads_name_as_address(arg, name)),
        HirExpr::AggregateCopy { src, .. } => expr_reads_name(src, name),
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            expr_reads_name_as_address(cond, name)
                || expr_reads_name_as_address(then_expr, name)
                || expr_reads_name_as_address(else_expr, name)
        }
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => false,
    }
}

fn stmt_reads_name_as_address(stmt: &HirStmt, name: &str) -> bool {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            let lhs_address = match lhs {
                HirLValue::Var(_) => false,
                HirLValue::Deref { ptr, .. } => expr_reads_name(ptr, name),
                HirLValue::Index { base, .. } => expr_reads_name(base, name),
                HirLValue::FieldAccess { base, .. } => expr_reads_name(base, name),
            };
            lhs_address || expr_reads_name_as_address(rhs, name)
        }
        HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => expr_reads_name_as_address(expr, name),
        HirStmt::VaStart { va_list, .. } => expr_reads_name_as_address(va_list, name),
        _ => false,
    }
}

fn is_linear_phase_stmt(stmt: &HirStmt) -> bool {
    matches!(
        stmt,
        HirStmt::Assign { .. } | HirStmt::Expr(_) | HirStmt::Return(_) | HirStmt::VaStart { .. }
    )
}

fn rename_expr_var(expr: &mut HirExpr, old: &str, new: &str) {
    match expr {
        HirExpr::Var(name) if name == old => *name = new.to_string(),
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::Load { ptr: expr, .. }
        | HirExpr::PtrOffset { base: expr, .. }
        | HirExpr::FieldAccess { base: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. } => rename_expr_var(expr, old, new),
        HirExpr::Binary { lhs, rhs, .. } => {
            rename_expr_var(lhs, old, new);
            rename_expr_var(rhs, old, new);
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                rename_expr_var(arg, old, new);
            }
        }
        HirExpr::Index { base, index, .. } => {
            rename_expr_var(base, old, new);
            rename_expr_var(index, old, new);
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            rename_expr_var(cond, old, new);
            rename_expr_var(then_expr, old, new);
            rename_expr_var(else_expr, old, new);
        }
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => {}
    }
}

fn rename_stmt_reads(stmt: &mut HirStmt, old: &str, new: &str) {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            match lhs {
                HirLValue::Var(_) => {}
                HirLValue::Deref { ptr, .. } => rename_expr_var(ptr, old, new),
                HirLValue::Index { base, index, .. } => {
                    rename_expr_var(base, old, new);
                    rename_expr_var(index, old, new);
                }
                HirLValue::FieldAccess { base, .. } => rename_expr_var(base, old, new),
            }
            rename_expr_var(rhs, old, new);
        }
        HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => rename_expr_var(expr, old, new),
        HirStmt::VaStart { va_list, .. } => rename_expr_var(va_list, old, new),
        _ => {}
    }
}

fn fresh_phase_name(base: &str, used_names: &mut HashSet<String>) -> String {
    let stem = format!("{base}_value");
    if used_names.insert(stem.clone()) {
        return stem;
    }
    let mut suffix = 2usize;
    loop {
        let candidate = format!("{stem}_{suffix}");
        if used_names.insert(candidate.clone()) {
            return candidate;
        }
        suffix += 1;
    }
}

fn split_scalar_role_phases(
    stmts: &mut [HirStmt],
    pointer_locals: &HashSet<String>,
    used_names: &mut HashSet<String>,
    new_bindings: &mut Vec<NirBinding>,
) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
                changed |= split_scalar_role_phases(body, pointer_locals, used_names, new_bindings);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |=
                    split_scalar_role_phases(then_body, pointer_locals, used_names, new_bindings);
                changed |=
                    split_scalar_role_phases(else_body, pointer_locals, used_names, new_bindings);
            }
            HirStmt::For {
                init, update, body, ..
            } => {
                if let Some(init) = init {
                    changed |= split_scalar_role_phases(
                        std::slice::from_mut(init.as_mut()),
                        pointer_locals,
                        used_names,
                        new_bindings,
                    );
                }
                changed |= split_scalar_role_phases(body, pointer_locals, used_names, new_bindings);
                if let Some(update) = update {
                    changed |= split_scalar_role_phases(
                        std::slice::from_mut(update.as_mut()),
                        pointer_locals,
                        used_names,
                        new_bindings,
                    );
                }
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= split_scalar_role_phases(
                        &mut case.body,
                        pointer_locals,
                        used_names,
                        new_bindings,
                    );
                }
                changed |=
                    split_scalar_role_phases(default, pointer_locals, used_names, new_bindings);
            }
            _ => {}
        }
    }

    let mut index = 0usize;
    while index < stmts.len() {
        let candidate = match &stmts[index] {
            HirStmt::Assign {
                lhs: HirLValue::Var(name),
                rhs,
            } if pointer_locals.contains(name)
                && stmts[..index]
                    .iter()
                    .any(|prior| stmt_defines_name(prior, name)) =>
            {
                narrow_scalar_assignment(rhs).map(|(ty, scalar_rhs)| (name.clone(), ty, scalar_rhs))
            }
            _ => None,
        };
        let Some((old_name, scalar_ty, scalar_rhs)) = candidate else {
            index += 1;
            continue;
        };
        let next_definition = stmts[index + 1..]
            .iter()
            .position(|stmt| stmt_defines_name(stmt, &old_name))
            .map_or(stmts.len(), |offset| index + 1 + offset);
        let control_boundary = stmts[index + 1..]
            .iter()
            .position(|stmt| !is_linear_phase_stmt(stmt))
            .map_or(stmts.len(), |offset| index + 1 + offset);
        let end = next_definition.min(control_boundary);
        if control_boundary < next_definition
            && stmts[control_boundary..next_definition]
                .iter()
                .any(|stmt| stmt_reads_name_deep(stmt, &old_name))
        {
            index += 1;
            continue;
        }
        let suffix = &stmts[index + 1..end];
        if suffix.is_empty()
            || !suffix.iter().any(|stmt| stmt_reads_name(stmt, &old_name))
            || suffix
                .iter()
                .any(|stmt| stmt_reads_name_as_address(stmt, &old_name))
        {
            index += 1;
            continue;
        }

        let new_name = fresh_phase_name(&old_name, used_names);
        if let HirStmt::Assign {
            lhs: HirLValue::Var(lhs),
            rhs,
        } = &mut stmts[index]
        {
            *lhs = new_name.clone();
            *rhs = scalar_rhs;
        }
        for stmt in &mut stmts[index + 1..end] {
            rename_stmt_reads(stmt, &old_name, &new_name);
        }
        new_bindings.push(NirBinding {
            name: new_name,
            ty: scalar_ty,
            surface_type_name: None,
            origin: Some(NirBindingOrigin::TempPreserved),
            initializer: None,
        });
        changed = true;
        index = end;
    }
    changed
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
        let Some(rhs_exprs) = var_assigns.get(name) else {
            continue;
        };
        if rhs_exprs.is_empty() {
            continue;
        }

        let NirType::Int {
            bits: x_bits,
            signed,
        } = local.ty
        else {
            continue;
        };

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

        if !all_match {
            continue;
        }
        let Some(shift_bits) = first_shift else {
            continue;
        };
        if shift_bits >= x_bits {
            continue;
        }

        let mut is_valid = true;
        for stmt in &func.body {
            verify_reads_in_stmt(stmt, name, shift_bits, &mut is_valid);
        }

        if is_valid {
            let x_low = format!("{}_low", name);
            let x_high = format!("{}_high", name);

            let low_binding = NirBinding {
                name: x_low.clone(),
                ty: NirType::Int {
                    bits: shift_bits,
                    signed: false,
                },
                surface_type_name: None,
                origin: Some(NirBindingOrigin::Temp),
                initializer: None,
            };
            let high_binding = NirBinding {
                name: x_high.clone(),
                ty: NirType::Int {
                    bits: x_bits - shift_bits,
                    signed,
                },
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

    let pointer_locals: HashSet<String> = func
        .locals
        .iter()
        .filter(|binding| matches!(binding.ty, NirType::Ptr(_)))
        .map(|binding| binding.name.clone())
        .collect();
    let mut used_names: HashSet<String> = func
        .params
        .iter()
        .chain(func.locals.iter())
        .map(|binding| binding.name.clone())
        .collect();
    let mut new_bindings = Vec::new();
    changed |= split_scalar_role_phases(
        &mut func.body,
        &pointer_locals,
        &mut used_names,
        &mut new_bindings,
    );
    func.locals.extend(new_bindings);

    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uint(bits: u32) -> NirType {
        NirType::Int {
            bits,
            signed: false,
        }
    }

    fn binding(name: &str, ty: NirType) -> NirBinding {
        NirBinding {
            name: name.into(),
            ty,
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        }
    }

    fn narrow_pointer_cast(value: &str, pointer_ty: &NirType) -> HirExpr {
        HirExpr::Cast {
            ty: pointer_ty.clone(),
            expr: Box::new(HirExpr::Cast {
                ty: uint(8),
                expr: Box::new(HirExpr::Var(value.into())),
            }),
        }
    }

    #[test]
    fn splits_narrow_scalar_phase_from_pointer_storage() {
        let pointer_ty = NirType::Ptr(Box::new(uint(8)));
        let mut func = HirFunction {
            name: "phase_split".into(),
            locals: vec![
                binding("slot", pointer_ty.clone()),
                binding("base", pointer_ty.clone()),
                binding("value", uint(32)),
                binding("acc", uint(32)),
            ],
            body: vec![
                HirStmt::Assign {
                    lhs: HirLValue::Var("slot".into()),
                    rhs: HirExpr::Var("base".into()),
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("slot".into()),
                    rhs: narrow_pointer_cast("value", &pointer_ty),
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("acc".into()),
                    rhs: HirExpr::Binary {
                        op: HirBinaryOp::Add,
                        lhs: Box::new(HirExpr::Var("acc".into())),
                        rhs: Box::new(HirExpr::Var("slot".into())),
                        ty: uint(32),
                    },
                },
            ],
            ..Default::default()
        };

        assert!(apply_split_flow_pass(&mut func));
        assert!(func.locals.iter().any(|binding| {
            binding.name == "slot_value"
                && binding.ty == uint(8)
                && binding.origin == Some(NirBindingOrigin::TempPreserved)
        }));
        assert!(matches!(
            &func.body[1],
            HirStmt::Assign {
                lhs: HirLValue::Var(name),
                rhs: HirExpr::Cast {
                    ty: NirType::Int { bits: 8, .. },
                    ..
                },
            } if name == "slot_value"
        ));
        assert!(matches!(
            &func.body[2],
            HirStmt::Assign {
                rhs: HirExpr::Binary { rhs, .. },
                ..
            } if matches!(rhs.as_ref(), HirExpr::Var(name) if name == "slot_value")
        ));
    }

    #[test]
    fn splits_direct_narrow_assignment_to_pointer_storage() {
        let pointer_ty = NirType::Ptr(Box::new(uint(8)));
        let mut func = HirFunction {
            name: "direct_phase_split".into(),
            locals: vec![
                binding("slot", pointer_ty),
                binding("base", NirType::Ptr(Box::new(uint(8)))),
                binding("value", uint(32)),
                binding("acc", uint(32)),
            ],
            body: vec![
                HirStmt::Assign {
                    lhs: HirLValue::Var("slot".into()),
                    rhs: HirExpr::Var("base".into()),
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("slot".into()),
                    rhs: HirExpr::Cast {
                        ty: uint(8),
                        expr: Box::new(HirExpr::Var("value".into())),
                    },
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("acc".into()),
                    rhs: HirExpr::Binary {
                        op: HirBinaryOp::Add,
                        lhs: Box::new(HirExpr::Var("acc".into())),
                        rhs: Box::new(HirExpr::Var("slot".into())),
                        ty: uint(32),
                    },
                },
            ],
            ..Default::default()
        };

        assert!(apply_split_flow_pass(&mut func));
        assert!(func
            .locals
            .iter()
            .any(|binding| binding.name == "slot_value"));
        assert!(matches!(
            &func.body[2],
            HirStmt::Assign {
                rhs: HirExpr::Binary { rhs, .. },
                ..
            } if matches!(rhs.as_ref(), HirExpr::Var(name) if name == "slot_value")
        ));
    }

    #[test]
    fn splits_scalar_phase_that_ends_before_label_boundary() {
        let pointer_ty = NirType::Ptr(Box::new(uint(8)));
        let mut func = HirFunction {
            name: "label_phase_split".into(),
            locals: vec![
                binding("slot", pointer_ty),
                binding("base", NirType::Ptr(Box::new(uint(8)))),
                binding("value", uint(32)),
                binding("acc", uint(32)),
            ],
            body: vec![
                HirStmt::Assign {
                    lhs: HirLValue::Var("slot".into()),
                    rhs: HirExpr::Var("base".into()),
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("slot".into()),
                    rhs: HirExpr::Cast {
                        ty: uint(8),
                        expr: Box::new(HirExpr::Var("value".into())),
                    },
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("acc".into()),
                    rhs: HirExpr::Binary {
                        op: HirBinaryOp::Add,
                        lhs: Box::new(HirExpr::Var("acc".into())),
                        rhs: Box::new(HirExpr::Var("slot".into())),
                        ty: uint(32),
                    },
                },
                HirStmt::Label("next".into()),
                HirStmt::Return(Some(HirExpr::Var("acc".into()))),
            ],
            ..Default::default()
        };

        assert!(apply_split_flow_pass(&mut func));
        assert!(func
            .locals
            .iter()
            .any(|binding| binding.name == "slot_value"));
    }

    #[test]
    fn keeps_pointer_phase_when_suffix_uses_value_as_address() {
        let pointer_ty = NirType::Ptr(Box::new(uint(8)));
        let mut func = HirFunction {
            name: "phase_no_split".into(),
            locals: vec![
                binding("slot", pointer_ty.clone()),
                binding("base", pointer_ty.clone()),
                binding("value", uint(32)),
                binding("loaded", uint(8)),
            ],
            body: vec![
                HirStmt::Assign {
                    lhs: HirLValue::Var("slot".into()),
                    rhs: HirExpr::Var("base".into()),
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("slot".into()),
                    rhs: narrow_pointer_cast("value", &pointer_ty),
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("loaded".into()),
                    rhs: HirExpr::Load {
                        ptr: Box::new(HirExpr::Var("slot".into())),
                        ty: uint(8),
                    },
                },
            ],
            ..Default::default()
        };

        assert!(!apply_split_flow_pass(&mut func));
        assert!(!func
            .locals
            .iter()
            .any(|binding| binding.name == "slot_value"));
    }
}
