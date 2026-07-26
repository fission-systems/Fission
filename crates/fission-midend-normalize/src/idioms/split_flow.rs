use crate::prelude::*;
use fission_midend_prehir::util::expr_type;
use crate::{HashMap, HashSet};

struct SimpleAssign {
    lhs: String,
    rhs: PreHirExpr,
}

fn collect_assignments(stmts: &[PreHirStmt], assigns: &mut Vec<SimpleAssign>) {
    for stmt in stmts {
        match stmt {
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var(name),
                rhs,
            } => {
                assigns.push(SimpleAssign {
                    lhs: name.clone(),
                    rhs: rhs.clone(),
                });
            }
            PreHirStmt::Block(body) | PreHirStmt::While { body, .. } | PreHirStmt::DoWhile { body, .. } => {
                collect_assignments(body, assigns);
            }
            PreHirStmt::For {
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
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_assignments(then_body, assigns);
                collect_assignments(else_body, assigns);
            }
            PreHirStmt::Switch { cases, default, .. } => {
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

fn match_piece_concat(expr: &PreHirExpr) -> Option<(PreHirExpr, PreHirExpr, u32)> {
    if let PreHirExpr::Binary {
        op: PreHirBinaryOp::Or,
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

fn match_shifted_high(expr: &PreHirExpr) -> Option<(PreHirExpr, u32)> {
    if let PreHirExpr::Binary {
        op: PreHirBinaryOp::Shl,
        lhs,
        rhs,
        ..
    } = expr
    {
        if let PreHirExpr::Const(shift_bits, _) = &**rhs {
            let hi = if let PreHirExpr::Cast { expr: inner, .. } = &**lhs {
                (**inner).clone()
            } else {
                (**lhs).clone()
            };
            return Some((hi, *shift_bits as u32));
        }
    }
    None
}

fn match_unshifted_low(expr: &PreHirExpr) -> PreHirExpr {
    if let PreHirExpr::Cast { expr: inner, .. } = expr {
        (**inner).clone()
    } else {
        expr.clone()
    }
}

fn match_zext_write(expr: &PreHirExpr, x_bits: u32) -> Option<(PreHirExpr, PreHirExpr, u32)> {
    if let PreHirExpr::Cast { ty, expr: inner } = expr {
        let inner_bits = type_bits(&expr_type(inner));
        if x_bits > inner_bits {
            let hi = PreHirExpr::Const(
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

fn is_valid_low_extract(expr: &PreHirExpr, x_name: &str, shift_bits: u32) -> bool {
    if let PreHirExpr::Cast { ty, expr: inner } = expr {
        if let PreHirExpr::Var(name) = &**inner {
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

fn is_valid_high_extract(expr: &PreHirExpr, x_name: &str, shift_bits: u32) -> bool {
    if let PreHirExpr::Cast { expr: inner, .. } = expr {
        if let PreHirExpr::Binary {
            op: PreHirBinaryOp::Shr | PreHirBinaryOp::Sar,
            lhs,
            rhs,
            ..
        } = &**inner
        {
            if let (PreHirExpr::Var(name), PreHirExpr::Const(sa, _)) = (&**lhs, &**rhs) {
                if name == x_name && *sa as u32 == shift_bits {
                    return true;
                }
            }
        }
    }
    if let PreHirExpr::Binary {
        op: PreHirBinaryOp::Shr | PreHirBinaryOp::Sar,
        lhs,
        rhs,
        ..
    } = expr
    {
        if let (PreHirExpr::Var(name), PreHirExpr::Const(sa, _)) = (&**lhs, &**rhs) {
            if name == x_name && *sa as u32 == shift_bits {
                return true;
            }
        }
    }
    false
}

fn for_each_child_expr_ref<F>(expr: &PreHirExpr, mut f: F)
where
    F: FnMut(&PreHirExpr),
{
    match expr {
        PreHirExpr::Cast { expr: inner, .. }
        | PreHirExpr::Unary { expr: inner, .. }
        | PreHirExpr::Load { ptr: inner, .. }
        | PreHirExpr::PtrOffset { base: inner, .. }
        | PreHirExpr::AggregateCopy { src: inner, .. } => {
            f(inner);
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            f(lhs);
            f(rhs);
        }
        PreHirExpr::Call { args, .. } => {
            for arg in args {
                f(arg);
            }
        }
        PreHirExpr::Index { base, index, .. } => {
            f(base);
            f(index);
        }
        PreHirExpr::Select {
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

fn verify_reads(expr: &PreHirExpr, x_name: &str, shift_bits: u32, valid: &mut bool) {
    if !*valid {
        return;
    }
    if is_valid_low_extract(expr, x_name, shift_bits) {
        return;
    }
    if is_valid_high_extract(expr, x_name, shift_bits) {
        return;
    }
    if let PreHirExpr::Var(name) = expr {
        if name == x_name {
            *valid = false;
            return;
        }
    }
    for_each_child_expr_ref(expr, |child| verify_reads(child, x_name, shift_bits, valid));
}

fn verify_reads_in_stmt(stmt: &PreHirStmt, x_name: &str, shift_bits: u32, valid: &mut bool) {
    if !*valid {
        return;
    }
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            match lhs {
                PreHirLValue::Var(_) => {}
                PreHirLValue::Deref { ptr, .. } => {
                    verify_reads(ptr, x_name, shift_bits, valid);
                }
                PreHirLValue::Index { base, index, .. } => {
                    verify_reads(base, x_name, shift_bits, valid);
                    verify_reads(index, x_name, shift_bits, valid);
                }
                PreHirLValue::FieldAccess { base, .. } => {
                    verify_reads(base, x_name, shift_bits, valid);
                }
            }
            verify_reads(rhs, x_name, shift_bits, valid);
        }
        PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => {
            verify_reads(expr, x_name, shift_bits, valid);
        }
        PreHirStmt::VaStart { va_list, .. } => {
            verify_reads(va_list, x_name, shift_bits, valid);
        }
        PreHirStmt::Block(body) | PreHirStmt::While { body, .. } | PreHirStmt::DoWhile { body, .. } => {
            for s in body {
                verify_reads_in_stmt(s, x_name, shift_bits, valid);
            }
        }
        PreHirStmt::For {
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
        PreHirStmt::If {
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
        PreHirStmt::Switch {
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

fn rewrite_expr(expr: &mut PreHirExpr, x_name: &str, x_low: &str, x_high: &str, shift_bits: u32) {
    if is_valid_low_extract(expr, x_name, shift_bits) {
        if let PreHirExpr::Cast { ty, .. } = expr {
            *expr = PreHirExpr::Cast {
                ty: ty.clone(),
                expr: Box::new(PreHirExpr::Var(x_low.to_string())),
            };
            return;
        }
    }
    if is_valid_high_extract(expr, x_name, shift_bits) {
        if let PreHirExpr::Cast { ty, .. } = expr {
            *expr = PreHirExpr::Cast {
                ty: ty.clone(),
                expr: Box::new(PreHirExpr::Var(x_high.to_string())),
            };
            return;
        }
        if let PreHirExpr::Binary { .. } = expr {
            *expr = PreHirExpr::Var(x_high.to_string());
            return;
        }
    }

    match expr {
        PreHirExpr::Cast { expr: inner, .. }
        | PreHirExpr::Unary { expr: inner, .. }
        | PreHirExpr::Load { ptr: inner, .. }
        | PreHirExpr::PtrOffset { base: inner, .. }
        | PreHirExpr::AggregateCopy { src: inner, .. } => {
            rewrite_expr(inner, x_name, x_low, x_high, shift_bits);
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            rewrite_expr(lhs, x_name, x_low, x_high, shift_bits);
            rewrite_expr(rhs, x_name, x_low, x_high, shift_bits);
        }
        PreHirExpr::Call { args, .. } => {
            for arg in args {
                rewrite_expr(arg, x_name, x_low, x_high, shift_bits);
            }
        }
        PreHirExpr::Index { base, index, .. } => {
            rewrite_expr(base, x_name, x_low, x_high, shift_bits);
            rewrite_expr(index, x_name, x_low, x_high, shift_bits);
        }
        PreHirExpr::Select {
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
    stmt: &mut PreHirStmt,
    x_name: &str,
    x_low: &str,
    x_high: &str,
    shift_bits: u32,
    x_bits: u32,
) {
    match stmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name),
            rhs,
        } if name == x_name => {
            if let Some((hi, lo, _)) = match_piece_concat(rhs) {
                *stmt = PreHirStmt::Block(vec![
                    PreHirStmt::Assign {
                        lhs: PreHirLValue::Var(x_low.to_string()),
                        rhs: lo,
                    },
                    PreHirStmt::Assign {
                        lhs: PreHirLValue::Var(x_high.to_string()),
                        rhs: hi,
                    },
                ]);
            } else if let Some((hi, lo, _)) = match_zext_write(rhs, x_bits) {
                *stmt = PreHirStmt::Block(vec![
                    PreHirStmt::Assign {
                        lhs: PreHirLValue::Var(x_low.to_string()),
                        rhs: lo,
                    },
                    PreHirStmt::Assign {
                        lhs: PreHirLValue::Var(x_high.to_string()),
                        rhs: hi,
                    },
                ]);
            }
        }
        PreHirStmt::Assign { lhs, rhs } => {
            match lhs {
                PreHirLValue::Deref { ptr, .. } => {
                    rewrite_expr(ptr, x_name, x_low, x_high, shift_bits);
                }
                PreHirLValue::Index { base, index, .. } => {
                    rewrite_expr(base, x_name, x_low, x_high, shift_bits);
                    rewrite_expr(index, x_name, x_low, x_high, shift_bits);
                }
                PreHirLValue::FieldAccess { base, .. } => {
                    rewrite_expr(base, x_name, x_low, x_high, shift_bits);
                }
                _ => {}
            }
            rewrite_expr(rhs, x_name, x_low, x_high, shift_bits);
        }
        PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => {
            rewrite_expr(expr, x_name, x_low, x_high, shift_bits);
        }
        PreHirStmt::VaStart { va_list, .. } => {
            rewrite_expr(va_list, x_name, x_low, x_high, shift_bits);
        }
        PreHirStmt::Block(body) | PreHirStmt::While { body, .. } | PreHirStmt::DoWhile { body, .. } => {
            rewrite_stmts(body, x_name, x_low, x_high, shift_bits, x_bits);
        }
        PreHirStmt::For {
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
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            rewrite_expr(cond, x_name, x_low, x_high, shift_bits);
            rewrite_stmts(then_body, x_name, x_low, x_high, shift_bits, x_bits);
            rewrite_stmts(else_body, x_name, x_low, x_high, shift_bits, x_bits);
        }
        PreHirStmt::Switch {
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
    stmts: &mut [PreHirStmt],
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

fn narrow_scalar_assignment(expr: &PreHirExpr) -> Option<(NirType, PreHirExpr)> {
    let scalar_expr = match expr {
        PreHirExpr::Cast {
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

fn stmt_defines_name(stmt: &PreHirStmt, name: &str) -> bool {
    matches!(
        stmt,
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(lhs),
            ..
        } if lhs == name
    )
}

fn expr_reads_name(expr: &PreHirExpr, name: &str) -> bool {
    match expr {
        PreHirExpr::Var(var) => var == name,
        PreHirExpr::AddressOfGlobal(_) | PreHirExpr::Const(_, _) => false,
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. } => expr_reads_name(expr, name),
        PreHirExpr::Binary { lhs, rhs, .. } => {
            expr_reads_name(lhs, name) || expr_reads_name(rhs, name)
        }
        PreHirExpr::Call { args, .. } => args.iter().any(|arg| expr_reads_name(arg, name)),
        PreHirExpr::Index { base, index, .. } => {
            expr_reads_name(base, name) || expr_reads_name(index, name)
        }
        PreHirExpr::Select {
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

fn lvalue_reads_name(lhs: &PreHirLValue, name: &str) -> bool {
    match lhs {
        PreHirLValue::Var(_) => false,
        PreHirLValue::Deref { ptr, .. } => expr_reads_name(ptr, name),
        PreHirLValue::Index { base, index, .. } => {
            expr_reads_name(base, name) || expr_reads_name(index, name)
        }
        PreHirLValue::FieldAccess { base, .. } => expr_reads_name(base, name),
    }
}

fn stmt_reads_name(stmt: &PreHirStmt, name: &str) -> bool {
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => lvalue_reads_name(lhs, name) || expr_reads_name(rhs, name),
        PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => expr_reads_name(expr, name),
        PreHirStmt::VaStart { va_list, .. } => expr_reads_name(va_list, name),
        PreHirStmt::Return(None)
        | PreHirStmt::Label(_)
        | PreHirStmt::Goto(_)
        | PreHirStmt::Break
        | PreHirStmt::Continue => false,
        PreHirStmt::Block(_)
        | PreHirStmt::If { .. }
        | PreHirStmt::While { .. }
        | PreHirStmt::DoWhile { .. }
        | PreHirStmt::For { .. }
        | PreHirStmt::Switch { .. } => false,
    }
}

fn stmt_reads_name_deep(stmt: &PreHirStmt, name: &str) -> bool {
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => lvalue_reads_name(lhs, name) || expr_reads_name(rhs, name),
        PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => expr_reads_name(expr, name),
        PreHirStmt::VaStart { va_list, .. } => expr_reads_name(va_list, name),
        PreHirStmt::Block(body) | PreHirStmt::While { body, .. } | PreHirStmt::DoWhile { body, .. } => {
            body.iter().any(|stmt| stmt_reads_name_deep(stmt, name))
        }
        PreHirStmt::If {
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
        PreHirStmt::For {
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
        PreHirStmt::Switch {
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
        PreHirStmt::Return(None)
        | PreHirStmt::Label(_)
        | PreHirStmt::Goto(_)
        | PreHirStmt::Break
        | PreHirStmt::Continue => false,
    }
}

fn expr_reads_name_as_address(expr: &PreHirExpr, name: &str) -> bool {
    match expr {
        PreHirExpr::Load { ptr, .. }
        | PreHirExpr::PtrOffset { base: ptr, .. }
        | PreHirExpr::FieldAccess { base: ptr, .. } => {
            expr_reads_name(ptr, name) || expr_reads_name_as_address(ptr, name)
        }
        PreHirExpr::Index { base, index, .. } => {
            expr_reads_name(base, name)
                || expr_reads_name_as_address(base, name)
                || expr_reads_name_as_address(index, name)
        }
        PreHirExpr::Cast { expr, .. } | PreHirExpr::Unary { expr, .. } => {
            expr_reads_name_as_address(expr, name)
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            expr_reads_name_as_address(lhs, name) || expr_reads_name_as_address(rhs, name)
        }
        PreHirExpr::Call { args, .. } => args.iter().any(|arg| expr_reads_name_as_address(arg, name)),
        PreHirExpr::AggregateCopy { src, .. } => expr_reads_name(src, name),
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            expr_reads_name_as_address(cond, name)
                || expr_reads_name_as_address(then_expr, name)
                || expr_reads_name_as_address(else_expr, name)
        }
        PreHirExpr::Var(_) | PreHirExpr::AddressOfGlobal(_) | PreHirExpr::Const(_, _) => false,
    }
}

fn stmt_reads_name_as_address(stmt: &PreHirStmt, name: &str) -> bool {
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            let lhs_address = match lhs {
                PreHirLValue::Var(_) => false,
                PreHirLValue::Deref { ptr, .. } => expr_reads_name(ptr, name),
                PreHirLValue::Index { base, .. } => expr_reads_name(base, name),
                PreHirLValue::FieldAccess { base, .. } => expr_reads_name(base, name),
            };
            lhs_address || expr_reads_name_as_address(rhs, name)
        }
        PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => expr_reads_name_as_address(expr, name),
        PreHirStmt::VaStart { va_list, .. } => expr_reads_name_as_address(va_list, name),
        _ => false,
    }
}

fn is_linear_phase_stmt(stmt: &PreHirStmt) -> bool {
    matches!(
        stmt,
        PreHirStmt::Assign { .. } | PreHirStmt::Expr(_) | PreHirStmt::Return(_) | PreHirStmt::VaStart { .. }
    )
}

fn rename_expr_var(expr: &mut PreHirExpr, old: &str, new: &str) {
    match expr {
        PreHirExpr::Var(name) if name == old => *name = new.to_string(),
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. } => rename_expr_var(expr, old, new),
        PreHirExpr::Binary { lhs, rhs, .. } => {
            rename_expr_var(lhs, old, new);
            rename_expr_var(rhs, old, new);
        }
        PreHirExpr::Call { args, .. } => {
            for arg in args {
                rename_expr_var(arg, old, new);
            }
        }
        PreHirExpr::Index { base, index, .. } => {
            rename_expr_var(base, old, new);
            rename_expr_var(index, old, new);
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            rename_expr_var(cond, old, new);
            rename_expr_var(then_expr, old, new);
            rename_expr_var(else_expr, old, new);
        }
        PreHirExpr::Var(_) | PreHirExpr::AddressOfGlobal(_) | PreHirExpr::Const(_, _) => {}
    }
}

fn rename_stmt_reads(stmt: &mut PreHirStmt, old: &str, new: &str) {
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            match lhs {
                PreHirLValue::Var(_) => {}
                PreHirLValue::Deref { ptr, .. } => rename_expr_var(ptr, old, new),
                PreHirLValue::Index { base, index, .. } => {
                    rename_expr_var(base, old, new);
                    rename_expr_var(index, old, new);
                }
                PreHirLValue::FieldAccess { base, .. } => rename_expr_var(base, old, new),
            }
            rename_expr_var(rhs, old, new);
        }
        PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => rename_expr_var(expr, old, new),
        PreHirStmt::VaStart { va_list, .. } => rename_expr_var(va_list, old, new),
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
    stmts: &mut [PreHirStmt],
    pointer_locals: &HashSet<String>,
    used_names: &mut HashSet<String>,
    new_bindings: &mut Vec<PreHirBinding>,
) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        match stmt {
            PreHirStmt::Block(body) | PreHirStmt::While { body, .. } | PreHirStmt::DoWhile { body, .. } => {
                changed |= split_scalar_role_phases(body, pointer_locals, used_names, new_bindings);
            }
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |=
                    split_scalar_role_phases(then_body, pointer_locals, used_names, new_bindings);
                changed |=
                    split_scalar_role_phases(else_body, pointer_locals, used_names, new_bindings);
            }
            PreHirStmt::For {
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
            PreHirStmt::Switch { cases, default, .. } => {
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
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var(name),
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
        if let PreHirStmt::Assign {
            lhs: PreHirLValue::Var(lhs),
            rhs,
        } = &mut stmts[index]
        {
            *lhs = new_name.clone();
            *rhs = scalar_rhs;
        }
        for stmt in &mut stmts[index + 1..end] {
            rename_stmt_reads(stmt, &old_name, &new_name);
        }
        new_bindings.push(PreHirBinding {
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

pub fn apply_split_flow_pass(func: &mut PreHirFunction) -> bool {
    let mut type_map = HashMap::default();
    for binding in func.params.iter().chain(func.locals.iter()) {
        type_map.insert(binding.name.clone(), binding.ty.clone());
    }

    let mut assigns = Vec::new();
    collect_assignments(&func.body, &mut assigns);

    let mut var_assigns: HashMap<String, Vec<PreHirExpr>> = HashMap::default();
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

            let low_binding = PreHirBinding {
                name: x_low.clone(),
                ty: NirType::Int {
                    bits: shift_bits,
                    signed: false,
                },
                surface_type_name: None,
                origin: Some(NirBindingOrigin::Temp),
                initializer: None,
            };
            let high_binding = PreHirBinding {
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
// prelude via parent

    fn uint(bits: u32) -> NirType {
        NirType::Int {
            bits,
            signed: false,
        }
    }

    fn binding(name: &str, ty: NirType) -> PreHirBinding {
        PreHirBinding {
            name: name.into(),
            ty,
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        }
    }

    fn narrow_pointer_cast(value: &str, pointer_ty: &NirType) -> PreHirExpr {
        PreHirExpr::Cast {
            ty: pointer_ty.clone(),
            expr: Box::new(PreHirExpr::Cast {
                ty: uint(8),
                expr: Box::new(PreHirExpr::Var(value.into())),
            }),
        }
    }

    #[test]
    fn splits_narrow_scalar_phase_from_pointer_storage() {
        let pointer_ty = NirType::Ptr(Box::new(uint(8)));
        let mut func = PreHirFunction {
            name: "phase_split".into(),
            locals: vec![
                binding("slot", pointer_ty.clone()),
                binding("base", pointer_ty.clone()),
                binding("value", uint(32)),
                binding("acc", uint(32)),
            ],
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("slot".into()),
                    rhs: PreHirExpr::Var("base".into()),
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("slot".into()),
                    rhs: narrow_pointer_cast("value", &pointer_ty),
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("acc".into()),
                    rhs: PreHirExpr::Binary {
                        op: PreHirBinaryOp::Add,
                        lhs: Box::new(PreHirExpr::Var("acc".into())),
                        rhs: Box::new(PreHirExpr::Var("slot".into())),
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
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var(name),
                rhs: PreHirExpr::Cast {
                    ty: NirType::Int { bits: 8, .. },
                    ..
                },
            } if name == "slot_value"
        ));
        assert!(matches!(
            &func.body[2],
            PreHirStmt::Assign {
                rhs: PreHirExpr::Binary { rhs, .. },
                ..
            } if matches!(rhs.as_ref(), PreHirExpr::Var(name) if name == "slot_value")
        ));
    }

    #[test]
    fn splits_direct_narrow_assignment_to_pointer_storage() {
        let pointer_ty = NirType::Ptr(Box::new(uint(8)));
        let mut func = PreHirFunction {
            name: "direct_phase_split".into(),
            locals: vec![
                binding("slot", pointer_ty),
                binding("base", NirType::Ptr(Box::new(uint(8)))),
                binding("value", uint(32)),
                binding("acc", uint(32)),
            ],
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("slot".into()),
                    rhs: PreHirExpr::Var("base".into()),
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("slot".into()),
                    rhs: PreHirExpr::Cast {
                        ty: uint(8),
                        expr: Box::new(PreHirExpr::Var("value".into())),
                    },
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("acc".into()),
                    rhs: PreHirExpr::Binary {
                        op: PreHirBinaryOp::Add,
                        lhs: Box::new(PreHirExpr::Var("acc".into())),
                        rhs: Box::new(PreHirExpr::Var("slot".into())),
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
            PreHirStmt::Assign {
                rhs: PreHirExpr::Binary { rhs, .. },
                ..
            } if matches!(rhs.as_ref(), PreHirExpr::Var(name) if name == "slot_value")
        ));
    }

    #[test]
    fn splits_scalar_phase_that_ends_before_label_boundary() {
        let pointer_ty = NirType::Ptr(Box::new(uint(8)));
        let mut func = PreHirFunction {
            name: "label_phase_split".into(),
            locals: vec![
                binding("slot", pointer_ty),
                binding("base", NirType::Ptr(Box::new(uint(8)))),
                binding("value", uint(32)),
                binding("acc", uint(32)),
            ],
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("slot".into()),
                    rhs: PreHirExpr::Var("base".into()),
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("slot".into()),
                    rhs: PreHirExpr::Cast {
                        ty: uint(8),
                        expr: Box::new(PreHirExpr::Var("value".into())),
                    },
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("acc".into()),
                    rhs: PreHirExpr::Binary {
                        op: PreHirBinaryOp::Add,
                        lhs: Box::new(PreHirExpr::Var("acc".into())),
                        rhs: Box::new(PreHirExpr::Var("slot".into())),
                        ty: uint(32),
                    },
                },
                PreHirStmt::Label("next".into()),
                PreHirStmt::Return(Some(PreHirExpr::Var("acc".into()))),
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
        let mut func = PreHirFunction {
            name: "phase_no_split".into(),
            locals: vec![
                binding("slot", pointer_ty.clone()),
                binding("base", pointer_ty.clone()),
                binding("value", uint(32)),
                binding("loaded", uint(8)),
            ],
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("slot".into()),
                    rhs: PreHirExpr::Var("base".into()),
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("slot".into()),
                    rhs: narrow_pointer_cast("value", &pointer_ty),
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("loaded".into()),
                    rhs: PreHirExpr::Load {
                        ptr: Box::new(PreHirExpr::Var("slot".into())),
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
