use super::*;
use std::collections::HashMap;

const MAX_PRINT_STMT_DEPTH: usize = 512;
const MAX_PRINT_EXPR_DEPTH: usize = 512;

/// Printing context: maps variable name → NirType for struct-member rendering.
struct PrintCtx<'a> {
    /// variable name → pointer-pointee type (Aggregate with fields)
    agg_ptr: HashMap<&'a str, &'a NirType>,
}

impl<'a> PrintCtx<'a> {
    fn build(func: &'a HirFunction) -> Self {
        let mut agg_ptr = HashMap::new();
        for b in func.locals.iter().chain(func.params.iter()) {
            if let NirType::Ptr(inner) = &b.ty {
                if let NirType::Aggregate { fields, .. } = inner.as_ref() {
                    if !fields.is_empty() {
                        agg_ptr.insert(b.name.as_str(), inner.as_ref());
                    }
                }
            }
        }
        Self { agg_ptr }
    }

    /// If `base_name` is a known Ptr(Aggregate{fields}) and `offset` matches a
    /// field, return the field name; otherwise return None.
    fn field_name(&self, base_name: &str, offset: i64) -> Option<&str> {
        if offset < 0 {
            return None;
        }
        let agg = self.agg_ptr.get(base_name)?;
        let NirType::Aggregate { fields, .. } = agg else {
            return None;
        };
        let f = fields.iter().find(|f| f.offset as i64 == offset)?;
        Some(f.name.as_str())
    }
}

pub(super) fn print_hir_function(func: &HirFunction) -> String {
    let ctx = PrintCtx::build(func);
    let mut out = String::new();
    let return_type = func
        .surface_return_type_name
        .clone()
        .unwrap_or_else(|| print_type(&func.return_type));
    out.push_str(&format!("{return_type} {}(", func.name));
    for (idx, param) in func.params.iter().enumerate() {
        if idx > 0 {
            out.push_str(", ");
        }
        out.push_str(&format!("{} {}", print_binding_type(param), param.name));
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

pub(super) fn print_stmt(stmt: &HirStmt) -> String {
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
            for case in cases {
                for value in &case.values {
                    out.push_str(&pad);
                    out.push_str("    ");
                    out.push_str(&format!("case {}:\n", value));
                }
                for stmt in &case.body {
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
                HirExpr::Var(name) => format!("{name}[{index}]"),
                _ => format!("(({} *)({inner}))[{index}]", print_type(elem_ty)),
            }
        }
    }
}

pub(super) fn print_expr(expr: &HirExpr) -> String {
    print_expr_prec(expr, 0, 0)
}

fn print_expr_prec(expr: &HirExpr, parent_prec: u8, depth: usize) -> String {
    if depth > MAX_PRINT_EXPR_DEPTH {
        return "0 /* [FISSION] RECURSION TOO DEEP (expression printer guard) */".to_string();
    }
    let (text, prec) = match expr {
        HirExpr::Var(name) => (name.clone(), 100),
        HirExpr::Const(value, _) => (value.to_string(), 100),
        HirExpr::Cast { ty, expr } => {
            let inner = print_expr_prec(expr, 90, depth + 1);
            (format!("({}){}", print_type(ty), inner), 90)
        }
        HirExpr::Unary { op, expr, .. } => {
            let symbol = match op {
                HirUnaryOp::Neg => "-",
                HirUnaryOp::Not => "!",
                HirUnaryOp::BitNot => "~",
            };
            let inner = print_expr_prec(expr, 85, depth + 1);
            (format!("{symbol}{inner}"), 85)
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
                let rhs_str = print_expr_prec(rhs, prec + 1, depth + 1);
                (
                    format!("{lhs_str} {} {rhs_str}", print_binary_op(*op)),
                    prec,
                )
            }
        }
        HirExpr::Call { target, args, .. } => {
            let args = args
                .iter()
                .map(|arg| print_expr_prec(arg, 0, depth + 1))
                .collect::<Vec<_>>()
                .join(", ");
            (format!("{target}({args})"), 100)
        }
        HirExpr::Load { ptr, ty } => {
            if let Some(target) = peel_simple_deref_target(ptr) {
                (format!("*{target}"), 95)
            } else {
                let inner = print_expr_prec(ptr, 0, depth + 1);
                (format!("*({} *)({inner})", print_type(ty)), 95)
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
                HirExpr::Var(name) => format!("{name}[{index}]"),
                _ => format!("(({} *)({inner}))[{index}]", print_type(elem_ty)),
            };
            (text, 95)
        }
        HirExpr::AggregateCopy { src, size } => {
            let inner = print_expr_prec(src, 0, depth + 1);
            (format!("*(fission_agg{} *)({inner})", size), 95)
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
        HirExpr::Var(name) => Some(name),
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
        HirBinaryOp::Eq
        | HirBinaryOp::Ne
        | HirBinaryOp::Lt
        | HirBinaryOp::Le
        | HirBinaryOp::SLt
        | HirBinaryOp::SLe => 30,
        HirBinaryOp::Or => 40,
        HirBinaryOp::Xor => 45,
        HirBinaryOp::And => 50,
        HirBinaryOp::Shl | HirBinaryOp::Shr | HirBinaryOp::Sar => 60,
        HirBinaryOp::Add | HirBinaryOp::Sub => 70,
        HirBinaryOp::Mul | HirBinaryOp::Div | HirBinaryOp::Mod => 80,
    }
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
    }
}

pub(super) fn print_type(ty: &NirType) -> String {
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
        NirType::Ptr(inner) => format!("{} *", print_type(inner)),
        NirType::Aggregate { size, .. } => format!("fission_agg{}", size),
        NirType::Float { bits } => match *bits {
            32 => "float".to_string(),
            64 => "double".to_string(),
            _ => format!("float{}", bits),
        },
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
            // If the base is a known Ptr(Aggregate{fields}) variable and the
            // offset matches a field, render as `base->field_X`.
            if let HirExpr::Var(name) = base.as_ref() {
                if let Some(field_name) = ctx.field_name(name, *offset) {
                    return format!("{name}->{field_name}");
                }
            }
            // Fallback: raw byte-offset form.
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
        HirExpr::Var(name) => (name.clone(), 100),
        HirExpr::Const(value, _) => (value.to_string(), 100),
        HirExpr::Cast { ty, expr } => {
            let inner = print_expr_prec_ctx(expr, 90, depth + 1, ctx);
            (format!("({}){}", print_type(ty), inner), 90)
        }
        HirExpr::Unary { op, expr, .. } => {
            let symbol = match op {
                HirUnaryOp::Neg => "-",
                HirUnaryOp::Not => "!",
                HirUnaryOp::BitNot => "~",
            };
            let inner = print_expr_prec_ctx(expr, 85, depth + 1, ctx);
            (format!("{symbol}{inner}"), 85)
        }
        HirExpr::Binary { op, lhs, rhs, .. } => {
            let prec = binary_precedence(*op);
            let lhs = print_expr_prec_ctx(lhs, prec, depth + 1, ctx);
            let rhs = print_expr_prec_ctx(rhs, prec + 1, depth + 1, ctx);
            (format!("{lhs} {} {rhs}", print_binary_op(*op)), prec)
        }
        HirExpr::Call { target, args, .. } => {
            let args = args
                .iter()
                .map(|arg| print_expr_prec_ctx(arg, 0, depth + 1, ctx))
                .collect::<Vec<_>>()
                .join(", ");
            (format!("{target}({args})"), 100)
        }
        HirExpr::Load { ptr, ty } => {
            // Check if `ptr` is a PtrOffset with a known field.
            if let HirExpr::PtrOffset { base, offset } = ptr.as_ref() {
                if let HirExpr::Var(name) = base.as_ref() {
                    if let Some(field_name) = ctx.field_name(name, *offset) {
                        return format!("{name}->{field_name}");
                    }
                }
            }
            if let Some(target) = peel_simple_deref_target(ptr) {
                (format!("*{target}"), 95)
            } else {
                let inner = print_expr_prec_ctx(ptr, 0, depth + 1, ctx);
                (format!("*({} *)({inner})", print_type(ty)), 95)
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
                HirExpr::Var(name) => format!("{name}[{index}]"),
                _ => format!("(({} *)({inner}))[{index}]", print_type(elem_ty)),
            };
            (text, 95)
        }
        HirExpr::AggregateCopy { src, size } => {
            let inner = print_expr_prec_ctx(src, 0, depth + 1, ctx);
            (format!("*(fission_agg{} *)({inner})", size), 95)
        }
    };
    if prec < parent_prec {
        format!("({text})")
    } else {
        text
    }
}

fn print_lvalue_ctx(lhs: &HirLValue, depth: usize, ctx: &PrintCtx<'_>) -> String {
    if depth > MAX_PRINT_EXPR_DEPTH {
        return "/* [FISSION] RECURSION TOO DEEP */".to_string();
    }
    match lhs {
        HirLValue::Var(name) => name.clone(),
        HirLValue::Deref { ptr, ty } => {
            // Check for struct member store: `*(&base->field_X) = ...`
            if let HirExpr::PtrOffset { base, offset } = ptr.as_ref() {
                if let HirExpr::Var(name) = base.as_ref() {
                    if let Some(field_name) = ctx.field_name(name, *offset) {
                        return format!("{name}->{field_name}");
                    }
                }
            }
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
                HirExpr::Var(name) => format!("{name}[{index}]"),
                _ => format!("(({} *)({inner}))[{index}]", print_type(elem_ty)),
            }
        }
    }
}

fn print_stmt_ctx(stmt: &HirStmt, ctx: &PrintCtx<'_>) -> String {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            format!(
                "{} = {};",
                print_lvalue_ctx(lhs, 0, ctx),
                print_expr_with_ctx(rhs, ctx)
            )
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
            for case in cases {
                for value in &case.values {
                    out.push_str(&pad);
                    out.push_str("    ");
                    out.push_str(&format!("case {}:\n", value));
                }
                for s in &case.body {
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
                for s in default {
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
