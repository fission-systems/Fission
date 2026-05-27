//! Pseudocode rendering from structured HIR/NIR (`render_nir`, layout helpers).
//!
//! Downstream of semantics; avoid fixing structuring failures by tweaking format-only paths.
//! Policy: `crates/fission-pcode/src/nir/AGENTS.md`.

use super::*;
use std::collections::HashMap;

const MAX_PRINT_STMT_DEPTH: usize = 512;
const MAX_PRINT_EXPR_DEPTH: usize = 512;

/// Printing context: maps variable name → NirType for struct-member rendering.
struct PrintCtx<'a> {
    /// variable name → declared type
    var_types: HashMap<&'a str, &'a NirType>,
    return_type: &'a NirType,
    inline_guard_goto: bool,
    global_names: Option<&'a HashMap<u64, String>>,
}

impl<'a> PrintCtx<'a> {
    fn build(func: &'a HirFunction) -> Self {
        let mut var_types = HashMap::new();
        for b in func.locals.iter().chain(func.params.iter()) {
            var_types.insert(b.name.as_str(), &b.ty);
        }
        Self {
            var_types,
            return_type: &func.return_type,
            inline_guard_goto: func.body.len() <= 6,
            global_names: None,
        }
    }

    fn expr_is_pointer(&self, expr: &HirExpr) -> bool {
        match expr {
            HirExpr::AddressOfGlobal(_) => true,
            HirExpr::Var(name) => self
                .var_types
                .get(name.as_str())
                .is_some_and(|ty| matches!(ty, NirType::Ptr(_))),
            HirExpr::Cast {
                ty: NirType::Ptr(_),
                ..
            } => true,
            _ => false,
        }
    }
}

pub(in crate::nir) fn print_hir_function(func: &HirFunction) -> String {
    let ctx = PrintCtx::build(func);
    print_hir_function_impl(func, ctx)
}

pub(in crate::nir) fn print_hir_function_with_global_names(
    func: &HirFunction,
    global_names: &HashMap<u64, String>,
) -> String {
    let mut ctx = PrintCtx::build(func);
    ctx.global_names = Some(global_names);
    print_hir_function_impl(func, ctx)
}

fn print_hir_function_impl(func: &HirFunction, ctx: PrintCtx<'_>) -> String {
    let mut out = String::new();
    let return_type = func
        .surface_return_type_name
        .clone()
        .unwrap_or_else(|| print_type(&func.return_type));
    out.push_str(&format!("{return_type} {}(", func.name));
    if func.params.is_empty() {
        out.push_str("void");
    } else {
        for (idx, param) in func.params.iter().enumerate() {
            if idx > 0 {
                out.push_str(", ");
            }
            out.push_str(&format!("{} {}", print_binding_type(param), param.name));
        }
    }
    out.push_str(")\n{\n");
    for local in &func.locals {
        if let Some(initializer) = &local.initializer {
            out.push_str(&format!(
                "    {} {} = {};\n",
                print_binding_type(local),
                local.name,
                print_expr_with_ctx(initializer, &ctx)
            ));
        } else {
            out.push_str(&format!(
                "    {} {};\n",
                print_binding_type(local),
                local.name
            ));
        }
    }
    if !func.locals.is_empty() {
        out.push('\n');
    }
    for stmt in &func.body {
        print_stmt_with_indent_ctx(stmt, 1, 0, &ctx, &mut out);
    }
    out.push_str("}\n");
    out
}

fn print_binding_type(binding: &NirBinding) -> String {
    binding
        .surface_type_name
        .clone()
        .unwrap_or_else(|| print_type(&binding.ty))
}

pub(in crate::nir) fn print_stmt(stmt: &HirStmt) -> String {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            format!(
                "{} = {};",
                print_lvalue(lhs, 0),
                print_expr(expr_fallback(rhs, 0))
            )
        }
        HirStmt::VaStart {
            va_list,
            last_named_param,
        } => format!(
            "va_start({}, {});",
            print_expr(expr_fallback(va_list, 0)),
            last_named_param
        ),
        HirStmt::Expr(expr) => format!("{};", print_expr(expr_fallback(expr, 0))),
        HirStmt::Label(label) => format!("{}:", label),
        HirStmt::Goto(label) if label == crate::nir::structuring::SWITCH_FALLTHROUGH_SENTINEL => {
            "/* fallthrough */".to_string()
        }
        HirStmt::Goto(label) => format!("goto {};", label),
        HirStmt::Block(_) => "{ ... }".to_string(),
        HirStmt::Switch { .. } => "switch (...) { ... }".to_string(),
        HirStmt::If { .. } => "if (...) { ... }".to_string(),
        HirStmt::While { .. } => "while (...) { ... }".to_string(),
        HirStmt::DoWhile { .. } => "do { ... } while (...);".to_string(),
        HirStmt::For { .. } => "for (...) { ... }".to_string(),
        HirStmt::Return(Some(expr)) => format!("return {};", print_expr(expr_fallback(expr, 0))),
        HirStmt::Return(None) => "return;".to_string(),
        HirStmt::Break => "break;".to_string(),
        HirStmt::Continue => "continue;".to_string(),
    }
}

fn print_stmt_with_indent(stmt: &HirStmt, indent: usize, depth: usize, out: &mut String) {
    let pad = "    ".repeat(indent);
    if depth > MAX_PRINT_STMT_DEPTH {
        out.push_str(&pad);
        out.push_str("/* [FISSION] RECURSION TOO DEEP (statement printer guard) */\n");
        return;
    }
    match stmt {
        HirStmt::Assign { .. }
        | HirStmt::VaStart { .. }
        | HirStmt::Expr(_)
        | HirStmt::Return(_)
        | HirStmt::Break
        | HirStmt::Continue
        | HirStmt::Goto(_) => {
            out.push_str(&pad);
            out.push_str(&print_stmt(stmt));
            out.push('\n');
        }
        HirStmt::Label(label) => {
            out.push_str(label);
            out.push_str(":\n");
        }
        HirStmt::Block(stmts) => {
            out.push_str(&pad);
            out.push_str("{\n");
            for stmt in stmts {
                print_stmt_with_indent(stmt, indent + 1, depth + 1, out);
            }
            out.push_str(&pad);
            out.push_str("}\n");
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            out.push_str(&pad);
            out.push_str(&format!("switch ({}) {{\n", print_expr(expr)));
            for (c_idx, case) in cases.iter().enumerate() {
                let next_label = if c_idx + 1 < cases.len() {
                    cases[c_idx + 1].body.first().and_then(|s| match s {
                        HirStmt::Label(l) => Some(l.as_str()),
                        _ => None,
                    })
                } else if !default.is_empty() {
                    default.first().and_then(|s| match s {
                        HirStmt::Label(l) => Some(l.as_str()),
                        _ => None,
                    })
                } else {
                    None
                };

                for value in &case.values {
                    out.push_str(&pad);
                    out.push_str("    ");
                    out.push_str(&format!("case {}:\n", value));
                }
                for (s_idx, stmt) in case.body.iter().enumerate() {
                    if s_idx + 1 == case.body.len() {
                        if let HirStmt::Goto(label) = stmt {
                            if Some(label.as_str()) == next_label {
                                out.push_str(&pad);
                                out.push_str("        /* fallthrough */\n");
                                continue;
                            }
                        }
                    }
                    print_stmt_with_indent(stmt, indent + 2, depth + 1, out);
                }
                if !matches!(
                    case.body.last(),
                    Some(HirStmt::Break | HirStmt::Return(_) | HirStmt::Goto(_))
                ) {
                    out.push_str(&pad);
                    out.push_str("        break;\n");
                }
            }
            if !default.is_empty() {
                out.push_str(&pad);
                out.push_str("    default:\n");
                for stmt in default {
                    print_stmt_with_indent(stmt, indent + 2, depth + 1, out);
                }
                if !matches!(
                    default.last(),
                    Some(HirStmt::Break | HirStmt::Return(_) | HirStmt::Goto(_))
                ) {
                    out.push_str(&pad);
                    out.push_str("        break;\n");
                }
            }
            out.push_str(&pad);
            out.push_str("}\n");
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            out.push_str(&pad);
            out.push_str(&format!("if ({}) {{\n", print_expr_prec(cond, 0, 0)));
            for stmt in then_body {
                print_stmt_with_indent(stmt, indent + 1, depth + 1, out);
            }
            out.push_str(&pad);
            out.push('}');
            if else_body.is_empty() {
                out.push('\n');
            } else {
                out.push_str(" else {\n");
                for stmt in else_body {
                    print_stmt_with_indent(stmt, indent + 1, depth + 1, out);
                }
                out.push_str(&pad);
                out.push_str("}\n");
            }
        }
        HirStmt::While { cond, body } => {
            out.push_str(&pad);
            out.push_str(&format!("while ({}) {{\n", print_expr_prec(cond, 0, 0)));
            for stmt in body {
                print_stmt_with_indent(stmt, indent + 1, depth + 1, out);
            }
            out.push_str(&pad);
            out.push_str("}\n");
        }
        HirStmt::DoWhile { body, cond } => {
            out.push_str(&pad);
            out.push_str("do {\n");
            for stmt in body {
                print_stmt_with_indent(stmt, indent + 1, depth + 1, out);
            }
            out.push_str(&pad);
            out.push_str(&format!("}} while ({});\n", print_expr_prec(cond, 0, 0)));
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            out.push_str(&pad);
            out.push_str("for (");
            if let Some(i) = init {
                let init_s = print_stmt(i);
                out.push_str(init_s.trim_end_matches(';'));
            }
            out.push_str("; ");
            if let Some(c) = cond {
                out.push_str(&print_expr_prec(c, 0, 0));
            }
            out.push_str("; ");
            if let Some(u) = update {
                let upd_s = print_stmt(u);
                out.push_str(upd_s.trim_end_matches(';'));
            }
            out.push_str(") {\n");
            for stmt in body {
                print_stmt_with_indent(stmt, indent + 1, depth + 1, out);
            }
            out.push_str(&pad);
            out.push_str("}\n");
        }
    }
}

fn print_lvalue(lhs: &HirLValue, depth: usize) -> String {
    if depth > MAX_PRINT_EXPR_DEPTH {
        return "/* [FISSION] RECURSION TOO DEEP */".to_string();
    }
    match lhs {
        HirLValue::Var(name) => name.clone(),
        HirLValue::Deref { ptr, ty } => {
            if let Some(target) = peel_simple_deref_target(ptr) {
                format!("*{target}")
            } else {
                format!(
                    "*({} *)({})",
                    print_type(ty),
                    print_expr_prec(ptr, 0, depth + 1)
                )
            }
        }
        HirLValue::Index {
            base,
            index,
            elem_ty,
        } => {
            let inner = print_expr_prec(base, 0, depth + 1);
            let index = print_expr_prec(index, 0, depth + 1);
            match base.as_ref() {
                HirExpr::Var(name) | HirExpr::AddressOfGlobal(name) => format!("{name}[{index}]"),
                _ => format!("(({} *)({inner}))[{index}]", print_type(elem_ty)),
            }
        }
        HirLValue::FieldAccess { base, field_name, .. } => {
            let inner = print_expr_prec(base, 110, depth + 1);
            let is_ptr = matches!(expr_type(base), NirType::Ptr(_));
            let op = if is_ptr { "->" } else { "." };
            format!("{inner}{op}{field_name}")
        }
    }
}

pub(in crate::nir) fn print_expr(expr: &HirExpr) -> String {
    print_expr_prec(expr, 0, 0)
}

fn print_expr_prec(expr: &HirExpr, parent_prec: u8, depth: usize) -> String {
    if depth > MAX_PRINT_EXPR_DEPTH {
        return "0 /* [FISSION] RECURSION TOO DEEP (expression printer guard) */".to_string();
    }
    let (text, prec) = match expr {
        HirExpr::AddressOfGlobal(name) => {
            if name.starts_with('"') {
                (name.clone(), 120)
            } else {
                (format!("&{name}"), 110)
            }
        }
        HirExpr::Var(name) => (name.clone(), 120),
        HirExpr::Const(value, _) => (value.to_string(), 120),
        HirExpr::Cast { ty, expr } => {
            let inner = print_expr_prec(expr, 110, depth + 1);
            (format!("({}){}", print_type(ty), inner), 110)
        }
        HirExpr::Unary { op, expr, .. } => {
            let symbol = match op {
                HirUnaryOp::Neg => "-",
                HirUnaryOp::Not => "!",
                HirUnaryOp::BitNot => "~",
            };
            let inner = print_expr_prec(expr, 110, depth + 1);
            (format!("{symbol}{inner}"), 110)
        }
        HirExpr::Binary { op, lhs, rhs, ty } => {
            let prec = binary_precedence(*op);
            // Arithmetic right shift (Sar) requires the left operand to be
            // a signed integer.  If the expression type is unsigned (or unknown)
            // we emit an explicit signed cast on the lhs so the semantics are
            // preserved in the C output.
            if *op == HirBinaryOp::Sar {
                let bits = match ty {
                    NirType::Int { bits, .. } => *bits,
                    _ => 32,
                };
                let lhs_is_signed = matches!(ty, NirType::Int { signed: true, .. });
                let lhs_str = print_expr_prec(lhs, prec, depth + 1);
                let rhs_str = print_expr_prec(rhs, prec + 1, depth + 1);
                let lhs_out = if lhs_is_signed {
                    lhs_str
                } else {
                    format!("(int{bits}_t){lhs_str}")
                };
                (format!("{lhs_out} >> {rhs_str}"), prec)
            } else {
                let lhs_str = print_expr_prec(lhs, prec, depth + 1);
                let rhs_parent_prec = binary_rhs_parent_precedence(*op, rhs, prec + 1);
                let rhs_str = print_expr_prec(rhs, rhs_parent_prec, depth + 1);
                (
                    format!("{lhs_str} {} {rhs_str}", print_binary_op(*op)),
                    prec,
                )
            }
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            let prec = 20;
            let cond = print_expr_prec(cond, prec, depth + 1);
            let then_expr = print_expr_prec(then_expr, prec, depth + 1);
            let else_expr = print_expr_prec(else_expr, prec, depth + 1);
            (format!("{cond} ? {then_expr} : {else_expr}"), prec)
        }
        HirExpr::Call { target, args, ty } => {
            if target == "__fission_callind_opaque" && !args.is_empty() {
                let fn_ptr = print_expr_prec(&args[0], 0, depth + 1);
                let remaining_args = args[1..]
                    .iter()
                    .map(|arg| print_expr_prec(arg, 0, depth + 1))
                    .collect::<Vec<_>>()
                    .join(", ");
                (format!("(*({fn_ptr}))({remaining_args})"), 120)
            } else {
                let target = print_callable_target(target, args, ty, None);
                let args = args
                    .iter()
                    .map(|arg| print_expr_prec(arg, 0, depth + 1))
                    .collect::<Vec<_>>()
                    .join(", ");
                (format!("{target}({args})"), 120)
            }
        }
        HirExpr::Load { ptr, ty } => {
            if let Some(target) = peel_simple_deref_target(ptr) {
                (format!("*{target}"), 110)
            } else {
                let inner = print_expr_prec(ptr, 0, depth + 1);
                (format!("*({} *)({inner})", print_type(ty)), 110)
            }
        }
        HirExpr::PtrOffset { base, offset } => {
            let inner = print_expr_prec(base, 0, depth + 1);
            let text = if *offset == 0 {
                inner
            } else if *offset > 0 {
                format!("(uint8_t *)({inner}) + {offset}")
            } else {
                format!("(uint8_t *)({inner}) - {}", offset.unsigned_abs())
            };
            (text, 60)
        }
        HirExpr::Index {
            base,
            index,
            elem_ty,
        } => {
            let inner = print_expr_prec(base, 0, depth + 1);
            let index = print_expr_prec(index, 0, depth + 1);
            let text = match base.as_ref() {
                HirExpr::Var(name) | HirExpr::AddressOfGlobal(name) => format!("{name}[{index}]"),
                _ => format!("(({} *)({inner}))[{index}]", print_type(elem_ty)),
            };
            (text, 120)
        }
        HirExpr::FieldAccess { base, field_name, .. } => {
            let inner = print_expr_prec(base, 110, depth + 1);
            let is_ptr = matches!(expr_type(base), NirType::Ptr(_));
            let op = if is_ptr { "->" } else { "." };
            (format!("{inner}{op}{field_name}"), 110)
        }
        HirExpr::AggregateCopy { src, size } => {
            let inner = print_expr_prec(src, 0, depth + 1);
            (format!("*(fission_agg{} *)({inner})", size), 110)
        }
    };

    if prec < parent_prec {
        format!("({text})")
    } else {
        text
    }
}

fn peel_simple_deref_target(expr: &HirExpr) -> Option<&str> {
    match expr {
        HirExpr::Var(name) | HirExpr::AddressOfGlobal(name) => Some(name),
        HirExpr::Cast { expr, .. } => peel_simple_deref_target(expr),
        HirExpr::PtrOffset { base, offset } if *offset == 0 => peel_simple_deref_target(base),
        _ => None,
    }
}

fn expr_fallback<'a>(expr: &'a HirExpr, _depth: usize) -> &'a HirExpr {
    expr
}

fn binary_precedence(op: HirBinaryOp) -> u8 {
    match op {
        HirBinaryOp::LogicalOr => 10,
        HirBinaryOp::LogicalAnd => 20,
        HirBinaryOp::Or => 30,
        HirBinaryOp::Xor => 40,
        HirBinaryOp::And => 50,
        HirBinaryOp::Eq | HirBinaryOp::Ne => 60,
        HirBinaryOp::Lt
        | HirBinaryOp::Le
        | HirBinaryOp::Gt
        | HirBinaryOp::Ge
        | HirBinaryOp::SLt
        | HirBinaryOp::SLe
        | HirBinaryOp::SGt
        | HirBinaryOp::SGe => 70,
        HirBinaryOp::Shl | HirBinaryOp::Shr | HirBinaryOp::Sar => 80,
        HirBinaryOp::Add | HirBinaryOp::Sub => 90,
        HirBinaryOp::Mul | HirBinaryOp::Div | HirBinaryOp::Mod => 100,
    }
}

fn binary_rhs_parent_precedence(parent_op: HirBinaryOp, rhs: &HirExpr, fallback: u8) -> u8 {
    let HirExpr::Binary { op: rhs_op, .. } = rhs else {
        return fallback;
    };
    if matches!(parent_op, HirBinaryOp::Eq | HirBinaryOp::Ne)
        && matches!(
            rhs_op,
            HirBinaryOp::Lt
                | HirBinaryOp::Le
                | HirBinaryOp::Gt
                | HirBinaryOp::Ge
                | HirBinaryOp::SLt
                | HirBinaryOp::SLe
                | HirBinaryOp::SGt
                | HirBinaryOp::SGe
        )
    {
        return binary_precedence(*rhs_op) + 1;
    }
    fallback
}

fn print_binary_op(op: HirBinaryOp) -> &'static str {
    match op {
        HirBinaryOp::Add => "+",
        HirBinaryOp::Sub => "-",
        HirBinaryOp::Mul => "*",
        HirBinaryOp::Div => "/",
        HirBinaryOp::Mod => "%",
        HirBinaryOp::LogicalAnd => "&&",
        HirBinaryOp::LogicalOr => "||",
        HirBinaryOp::And => "&",
        HirBinaryOp::Or => "|",
        HirBinaryOp::Xor => "^",
        HirBinaryOp::Shl => "<<",
        HirBinaryOp::Shr => ">>",
        HirBinaryOp::Sar => ">>", // Handled specially in print_expr_prec with signed cast.
        HirBinaryOp::Eq => "==",
        HirBinaryOp::Ne => "!=",
        HirBinaryOp::Lt | HirBinaryOp::SLt => "<",
        HirBinaryOp::Le | HirBinaryOp::SLe => "<=",
        HirBinaryOp::Gt | HirBinaryOp::SGt => ">",
        HirBinaryOp::Ge | HirBinaryOp::SGe => ">=",
    }
}

pub(in crate::nir) fn print_type(ty: &NirType) -> String {
    match ty {
        NirType::Unknown => "undefined".to_string(),
        NirType::Bool => "bool".to_string(),
        NirType::Int { bits, signed } => match (*bits, *signed) {
            (8, false) => "uchar".to_string(),
            (8, true) => "char".to_string(),
            (16, false) => "ushort".to_string(),
            (16, true) => "short".to_string(),
            (32, false) => "uint".to_string(),
            (32, true) => "int".to_string(),
            (64, false) => "ulonglong".to_string(),
            (64, true) => "longlong".to_string(),
            _ => format!("int{}", bits),
        },
        NirType::Ptr(inner) if matches!(inner.as_ref(), NirType::Unknown) => "void *".to_string(),
        NirType::Ptr(inner) => format!("{} *", print_type(inner)),
        NirType::Aggregate { size, .. } => format!("fission_agg{}", size),
        NirType::Float { bits } => match *bits {
            32 => "float".to_string(),
            64 => "double".to_string(),
            _ => format!("float{}", bits),
        },
    }
}

fn print_callable_target(
    target: &str,
    args: &[HirExpr],
    return_ty: &NirType,
    ctx: Option<&PrintCtx<'_>>,
) -> String {
    let Some(target_expr) = target
        .strip_prefix("((code *)")
        .and_then(|rest| rest.strip_suffix(')'))
    else {
        return target.to_string();
    };
    let ret_ty = if matches!(return_ty, NirType::Unknown) {
        ctx.map(|ctx| ctx.return_type).unwrap_or(return_ty)
    } else {
        return_ty
    };
    if matches!(ret_ty, NirType::Unknown) {
        return target.to_string();
    }
    let arg_types = args
        .iter()
        .map(|arg| callable_arg_type_name(arg, ctx))
        .collect::<Vec<_>>()
        .join(", ");
    format!("(({} (*)({arg_types})){target_expr})", print_type(ret_ty))
}

fn callable_arg_type_name(arg: &HirExpr, ctx: Option<&PrintCtx<'_>>) -> String {
    if let Some(ctx) = ctx
        && let HirExpr::Var(name) = arg
        && let Some(ty) = ctx.var_types.get(name.as_str())
    {
        return print_type(ty);
    }
    match arg {
        HirExpr::Const(_, ty)
        | HirExpr::Cast { ty, .. }
        | HirExpr::Unary { ty, .. }
        | HirExpr::Binary { ty, .. }
        | HirExpr::Call { ty, .. }
        | HirExpr::Load { ty, .. }
        | HirExpr::FieldAccess { ty, .. }
        | HirExpr::Select { ty, .. } => {
            if matches!(ty, NirType::Unknown) {
                "uint".to_string()
            } else {
                print_type(ty)
            }
        }
        HirExpr::Index { elem_ty, .. } => print_type(elem_ty),
        HirExpr::Var(_)
        | HirExpr::AddressOfGlobal(_)
        | HirExpr::PtrOffset { .. }
        | HirExpr::AggregateCopy { .. } => "uint".to_string(),
    }
}

// ── Context-aware printing (struct field access) ──────────────────────────────

fn print_expr_with_ctx(expr: &HirExpr, ctx: &PrintCtx<'_>) -> String {
    print_expr_prec_ctx(expr, 0, 0, ctx)
}

fn print_expr_prec_ctx(
    expr: &HirExpr,
    parent_prec: u8,
    depth: usize,
    ctx: &PrintCtx<'_>,
) -> String {
    if depth > MAX_PRINT_EXPR_DEPTH {
        return "0 /* [FISSION] RECURSION TOO DEEP (expression printer guard) */".to_string();
    }
    let (text, prec) = match expr {
        HirExpr::PtrOffset { base, offset } => {
            let inner = print_expr_prec_ctx(base, 0, depth + 1, ctx);
            let text = if *offset == 0 {
                inner
            } else if *offset > 0 {
                format!("(uint8_t *)({inner}) + {offset}")
            } else {
                format!("(uint8_t *)({inner}) - {}", offset.unsigned_abs())
            };
            (text, 60)
        }
        HirExpr::FieldAccess { base, field_name, .. } => {
            let inner = print_expr_prec_ctx(base, 110, depth + 1, ctx);
            let is_ptr = ctx.expr_is_pointer(base);
            let op = if is_ptr { "->" } else { "." };
            (format!("{inner}{op}{field_name}"), 110)
        }
        HirExpr::AddressOfGlobal(name) => {
            if name.starts_with('"') {
                (name.clone(), 120)
            } else {
                (format!("&{name}"), 110)
            }
        }
        HirExpr::Var(name) => (name.clone(), 120),
        HirExpr::Const(value, _) => {
            let name = ctx.global_names.and_then(|names| {
                names.get(&((*value) as u64)).cloned()
            });
            if let Some(name) = name {
                (name, 120)
            } else {
                (value.to_string(), 120)
            }
        }
        HirExpr::Cast { ty, expr } => {
            if let Some(pointer_diff) = print_pointer_diff_cast(ty, expr, depth, ctx) {
                return pointer_diff;
            }
            let inner = print_expr_prec_ctx(expr, 110, depth + 1, ctx);
            (format!("({}){}", print_type(ty), inner), 110)
        }
        HirExpr::Unary { op, expr, .. } => {
            let symbol = match op {
                HirUnaryOp::Neg => "-",
                HirUnaryOp::Not => "!",
                HirUnaryOp::BitNot => "~",
            };
            let inner = print_expr_prec_ctx(expr, 110, depth + 1, ctx);
            (format!("{symbol}{inner}"), 110)
        }
        HirExpr::Binary { op, lhs, rhs, .. } => {
            let prec = binary_precedence(*op);
            let lhs = print_expr_prec_ctx(lhs, prec, depth + 1, ctx);
            let rhs_parent_prec = binary_rhs_parent_precedence(*op, rhs, prec + 1);
            let rhs = print_expr_prec_ctx(rhs, rhs_parent_prec, depth + 1, ctx);
            (format!("{lhs} {} {rhs}", print_binary_op(*op)), prec)
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            let prec = 20;
            let cond = print_expr_prec_ctx(cond, prec, depth + 1, ctx);
            let then_expr = print_expr_prec_ctx(then_expr, prec, depth + 1, ctx);
            let else_expr = print_expr_prec_ctx(else_expr, prec, depth + 1, ctx);
            (format!("{cond} ? {then_expr} : {else_expr}"), prec)
        }
        HirExpr::Call { target, args, ty } => {
            if target == "__fission_callind_opaque" && !args.is_empty() {
                let fn_ptr = print_expr_prec_ctx(&args[0], 0, depth + 1, ctx);
                let remaining_args = args[1..]
                    .iter()
                    .map(|arg| print_expr_prec_ctx(arg, 0, depth + 1, ctx))
                    .collect::<Vec<_>>()
                    .join(", ");
                (format!("(*({fn_ptr}))({remaining_args})"), 120)
            } else {
                let target = print_callable_target(target, args, ty, Some(ctx));
                let args = args
                    .iter()
                    .map(|arg| print_expr_prec_ctx(arg, 0, depth + 1, ctx))
                    .collect::<Vec<_>>()
                    .join(", ");
                (format!("{target}({args})"), 120)
            }
        }
        HirExpr::Load { ptr, ty } => {
            if let Some(target) = peel_simple_deref_target(ptr) {
                (format!("*{target}"), 110)
            } else {
                let inner = print_expr_prec_ctx(ptr, 0, depth + 1, ctx);
                (format!("*({} *)({inner})", print_type(ty)), 110)
            }
        }
        HirExpr::Index {
            base,
            index,
            elem_ty,
        } => {
            let inner = print_expr_prec_ctx(base, 0, depth + 1, ctx);
            let index = print_expr_prec_ctx(index, 0, depth + 1, ctx);
            let text = match base.as_ref() {
                HirExpr::Var(name) | HirExpr::AddressOfGlobal(name) => format!("{name}[{index}]"),
                _ => format!("(({} *)({inner}))[{index}]", print_type(elem_ty)),
            };
            (text, 120)
        }
        HirExpr::AggregateCopy { src, size } => {
            let inner = print_expr_prec_ctx(src, 0, depth + 1, ctx);
            (format!("*(fission_agg{} *)({inner})", size), 110)
        }
    };
    if prec < parent_prec {
        format!("({text})")
    } else {
        text
    }
}

fn print_pointer_diff_cast(
    cast_ty: &NirType,
    expr: &HirExpr,
    depth: usize,
    ctx: &PrintCtx<'_>,
) -> Option<String> {
    if !matches!(cast_ty, NirType::Int { .. } | NirType::Bool) {
        return None;
    }
    let HirExpr::Binary {
        op: HirBinaryOp::Sub,
        lhs,
        rhs,
        ..
    } = expr
    else {
        return None;
    };
    if !ctx.expr_is_pointer(lhs) || !ctx.expr_is_pointer(rhs) {
        return None;
    }
    let lhs = print_expr_prec_ctx(lhs, 0, depth + 1, ctx);
    let rhs = print_expr_prec_ctx(rhs, 0, depth + 1, ctx);
    Some(format!(
        "({})((uint8_t *)({lhs}) - (uint8_t *)({rhs}))",
        print_type(cast_ty)
    ))
}

fn print_lvalue_ctx(lhs: &HirLValue, depth: usize, ctx: &PrintCtx<'_>) -> String {
    if depth > MAX_PRINT_EXPR_DEPTH {
        return "/* [FISSION] RECURSION TOO DEEP */".to_string();
    }
    match lhs {
        HirLValue::Var(name) => name.clone(),
        HirLValue::FieldAccess { base, field_name, .. } => {
            let inner = print_expr_prec_ctx(base, 110, depth + 1, ctx);
            let is_ptr = ctx.expr_is_pointer(base);
            let op = if is_ptr { "->" } else { "." };
            format!("{inner}{op}{field_name}")
        }
        HirLValue::Deref { ptr, ty } => {
            if let Some(target) = peel_simple_deref_target(ptr) {
                format!("*{target}")
            } else {
                format!(
                    "*({} *)({})",
                    print_type(ty),
                    print_expr_prec_ctx(ptr, 0, depth + 1, ctx)
                )
            }
        }
        HirLValue::Index {
            base,
            index,
            elem_ty,
        } => {
            let inner = print_expr_prec_ctx(base, 0, depth + 1, ctx);
            let index = print_expr_prec_ctx(index, 0, depth + 1, ctx);
            match base.as_ref() {
                HirExpr::Var(name) | HirExpr::AddressOfGlobal(name) => format!("{name}[{index}]"),
                _ => format!("(({} *)({inner}))[{index}]", print_type(elem_ty)),
            }
        }
    }
}

fn is_integer_bitop(op: HirBinaryOp) -> bool {
    matches!(
        op,
        HirBinaryOp::And | HirBinaryOp::Or | HirBinaryOp::Xor
            | HirBinaryOp::Shl | HirBinaryOp::Shr | HirBinaryOp::Sar
    )
}

fn try_compound_assignment(lhs: &HirLValue, rhs: &HirExpr, ctx: &PrintCtx<'_>) -> Option<String> {
    let HirLValue::Var(var_name) = lhs else {
        return None;
    };
    let HirExpr::Binary { op, lhs: lhs_expr, rhs: rhs_expr, .. } = rhs else {
        return None;
    };
    let HirExpr::Var(lhs_name) = lhs_expr.as_ref() else {
        return None;
    };
    if var_name != lhs_name {
        return None;
    }

    // If the LHS variable is a pointer type and the operation is a bitwise integer op,
    // we cannot emit `ptr &= val` (invalid C). Instead emit:
    //   var = (ptr_ty)((ulonglong)var OP val);
    let var_is_ptr = ctx
        .var_types
        .get(var_name.as_str())
        .is_some_and(|ty| matches!(ty, NirType::Ptr(_)));
    if var_is_ptr && is_integer_bitop(*op) {
        let ptr_ty = ctx
            .var_types
            .get(var_name.as_str())
            .map(|ty| print_type(ty))
            .unwrap_or_else(|| "void *".to_string());
        let op_str = match op {
            HirBinaryOp::And => "&",
            HirBinaryOp::Or => "|",
            HirBinaryOp::Xor => "^",
            HirBinaryOp::Shl => "<<",
            HirBinaryOp::Shr | HirBinaryOp::Sar => ">>",
            _ => return None,
        };
        let rhs_str = print_expr_with_ctx(rhs_expr, ctx);
        return Some(format!(
            "{var_name} = ({ptr_ty})((ulonglong){var_name} {op_str} {rhs_str});"
        ));
    }

    if matches!(op, HirBinaryOp::Add) && matches!(rhs_expr.as_ref(), HirExpr::Const(1, _)) {
        return Some(format!("{}++;", var_name));
    }
    if matches!(op, HirBinaryOp::Sub) && matches!(rhs_expr.as_ref(), HirExpr::Const(1, _)) {
        return Some(format!("{}--;", var_name));
    }
    let op_str = match op {
        HirBinaryOp::Add => "+=",
        HirBinaryOp::Sub => "-=",
        HirBinaryOp::Mul => "*=",
        HirBinaryOp::Div => "/=",
        HirBinaryOp::Mod => "%=",
        HirBinaryOp::And => "&=",
        HirBinaryOp::Or => "|=",
        HirBinaryOp::Xor => "^=",
        HirBinaryOp::Shl => "<<=",
        HirBinaryOp::Shr => ">>=",
        _ => return None,
    };
    Some(format!(
        "{} {} {};",
        var_name,
        op_str,
        print_expr_with_ctx(rhs_expr, ctx)
    ))
}

fn print_stmt_ctx(stmt: &HirStmt, ctx: &PrintCtx<'_>) -> String {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            if let Some(compound) = try_compound_assignment(lhs, rhs, ctx) {
                compound
            } else {
                format!(
                    "{} = {};",
                    print_lvalue_ctx(lhs, 0, ctx),
                    print_expr_with_ctx(rhs, ctx)
                )
            }
        }
        HirStmt::VaStart {
            va_list,
            last_named_param,
        } => format!(
            "va_start({}, {});",
            print_expr_with_ctx(va_list, ctx),
            last_named_param
        ),
        HirStmt::Expr(expr) => format!("{};", print_expr_with_ctx(expr, ctx)),
        HirStmt::Return(Some(expr)) => format!("return {};", print_expr_with_ctx(expr, ctx)),
        HirStmt::Return(None) => "return;".to_string(),
        HirStmt::Break => "break;".to_string(),
        HirStmt::Continue => "continue;".to_string(),
        HirStmt::Label(label) => format!("{}:", label),
        HirStmt::Goto(label) if label == crate::nir::structuring::SWITCH_FALLTHROUGH_SENTINEL => {
            "/* fallthrough */".to_string()
        }
        HirStmt::Goto(label) => format!("goto {};", label),
        _ => print_stmt(stmt),
    }
}

fn print_stmt_with_indent_ctx(
    stmt: &HirStmt,
    indent: usize,
    depth: usize,
    ctx: &PrintCtx<'_>,
    out: &mut String,
) {
    let pad = "    ".repeat(indent);
    if depth > MAX_PRINT_STMT_DEPTH {
        out.push_str(&pad);
        out.push_str("/* [FISSION] RECURSION TOO DEEP (statement printer guard) */\n");
        return;
    }
    match stmt {
        HirStmt::Assign { .. }
        | HirStmt::VaStart { .. }
        | HirStmt::Expr(_)
        | HirStmt::Return(_)
        | HirStmt::Break
        | HirStmt::Continue
        | HirStmt::Goto(_) => {
            out.push_str(&pad);
            out.push_str(&print_stmt_ctx(stmt, ctx));
            out.push('\n');
        }
        HirStmt::Label(label) => {
            out.push_str(label);
            out.push_str(":\n");
        }
        HirStmt::Block(stmts) => {
            out.push_str(&pad);
            out.push_str("{\n");
            for s in stmts {
                print_stmt_with_indent_ctx(s, indent + 1, depth + 1, ctx, out);
            }
            out.push_str(&pad);
            out.push_str("}\n");
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            out.push_str(&pad);
            out.push_str(&format!("switch ({}) {{\n", print_expr_with_ctx(expr, ctx)));
            for (c_idx, case) in cases.iter().enumerate() {
                let next_label = if c_idx + 1 < cases.len() {
                    let next_case_body = &cases[c_idx + 1].body;
                    next_case_body.first().and_then(|s| match s {
                        HirStmt::Label(l) => Some(l.as_str()),
                        _ => None,
                    })
                } else if !default.is_empty() {
                    default.first().and_then(|s| match s {
                        HirStmt::Label(l) => Some(l.as_str()),
                        _ => None,
                    })
                } else {
                    None
                };

                for value in &case.values {
                    out.push_str(&pad);
                    out.push_str("    ");
                    out.push_str(&format!("case {}:\n", value));
                }
                for (s_idx, s) in case.body.iter().enumerate() {
                    if s_idx == 0 && matches!(s, HirStmt::Label(_)) {
                        continue;
                    }
                    if s_idx + 1 == case.body.len() {
                        if let HirStmt::Goto(label) = s {
                            if Some(label.as_str()) == next_label {
                                out.push_str(&pad);
                                out.push_str("        /* fallthrough */\n");
                                continue;
                            }
                        }
                    }
                    print_stmt_with_indent_ctx(s, indent + 2, depth + 1, ctx, out);
                }
                if !matches!(
                    case.body.last(),
                    Some(HirStmt::Break | HirStmt::Return(_) | HirStmt::Goto(_))
                ) {
                    out.push_str(&pad);
                    out.push_str("        break;\n");
                }
            }
            if !default.is_empty() {
                out.push_str(&pad);
                out.push_str("    default:\n");
                for (s_idx, s) in default.iter().enumerate() {
                    if s_idx == 0 && matches!(s, HirStmt::Label(_)) {
                        continue;
                    }
                    print_stmt_with_indent_ctx(s, indent + 2, depth + 1, ctx, out);
                }
                if !matches!(
                    default.last(),
                    Some(HirStmt::Break | HirStmt::Return(_) | HirStmt::Goto(_))
                ) {
                    out.push_str(&pad);
                    out.push_str("        break;\n");
                }
            }
            out.push_str(&pad);
            out.push_str("}\n");
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            if ctx.inline_guard_goto && else_body.is_empty() {
                if let [HirStmt::Goto(label)] = then_body.as_slice() {
                    out.push_str(&pad);
                    out.push_str(&format!(
                        "if ({}) goto {label};\n",
                        print_expr_prec_ctx(cond, 0, 0, ctx)
                    ));
                    return;
                }
            }
            out.push_str(&pad);
            out.push_str(&format!(
                "if ({}) {{\n",
                print_expr_prec_ctx(cond, 0, 0, ctx)
            ));
            for s in then_body {
                print_stmt_with_indent_ctx(s, indent + 1, depth + 1, ctx, out);
            }
            out.push_str(&pad);
            out.push('}');
            if else_body.is_empty() {
                out.push('\n');
            } else {
                out.push_str(" else {\n");
                for s in else_body {
                    print_stmt_with_indent_ctx(s, indent + 1, depth + 1, ctx, out);
                }
                out.push_str(&pad);
                out.push_str("}\n");
            }
        }
        HirStmt::While { cond, body } => {
            out.push_str(&pad);
            out.push_str(&format!(
                "while ({}) {{\n",
                print_expr_prec_ctx(cond, 0, 0, ctx)
            ));
            for s in body {
                print_stmt_with_indent_ctx(s, indent + 1, depth + 1, ctx, out);
            }
            out.push_str(&pad);
            out.push_str("}\n");
        }
        HirStmt::DoWhile { body, cond } => {
            out.push_str(&pad);
            out.push_str("do {\n");
            for s in body {
                print_stmt_with_indent_ctx(s, indent + 1, depth + 1, ctx, out);
            }
            out.push_str(&pad);
            out.push_str(&format!(
                "}} while ({});\n",
                print_expr_prec_ctx(cond, 0, 0, ctx)
            ));
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            out.push_str(&pad);
            out.push_str("for (");
            if let Some(i) = init {
                let init_s = print_stmt_ctx(i, ctx);
                out.push_str(init_s.trim_end_matches(';'));
            }
            out.push_str("; ");
            if let Some(c) = cond {
                out.push_str(&print_expr_prec_ctx(c, 0, 0, ctx));
            }
            out.push_str("; ");
            if let Some(u) = update {
                let upd_s = print_stmt_ctx(u, ctx);
                out.push_str(upd_s.trim_end_matches(';'));
            }
            out.push_str(") {\n");
            for s in body {
                print_stmt_with_indent_ctx(s, indent + 1, depth + 1, ctx, out);
            }
            out.push_str(&pad);
            out.push_str("}\n");
        }
    }
}

pub fn render_contracted_wrapper_summary(
    name: &str,
    summary: &crate::nir::ProcedureSummary,
) -> String {
    let target = summary
        .wrapper_contraction
        .as_ref()
        .map(|proof| proof.target.symbol.clone())
        .unwrap_or_else(|| "unknown_target".to_string());
    let mut hir = HirFunction {
        name: name.to_string(),
        return_type: NirType::Unknown,
        ..HirFunction::default()
    };
    hir.body = vec![HirStmt::Return(Some(HirExpr::Call {
        target,
        args: Vec::new(),
        ty: NirType::Unknown,
    }))];
    print_hir_function(&hir)
}
