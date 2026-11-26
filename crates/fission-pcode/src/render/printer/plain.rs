//! Context-free statement, lvalue and expression printing (no declarations
//! consulted).

use super::*;

pub(crate) fn print_stmt(stmt: &HirStmt) -> String {
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
        HirStmt::Goto(label) if label == crate::render::SWITCH_FALLTHROUGH_SENTINEL => {
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

pub(super) fn print_stmt_with_indent(
    stmt: &HirStmt,
    indent: usize,
    depth: usize,
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

pub(super) fn print_lvalue(lhs: &HirLValue, depth: usize) -> String {
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
            // Always cast when the element type is a recovered aggregate so
            // `base[i].field_0` compiles even if `base` is still declared as
            // `uint *` / `int *` in the binding list.
            if matches!(elem_ty, NirType::Aggregate { .. }) {
                format!("(({} *)({inner}))[{index}]", print_type(elem_ty))
            } else {
                match base.as_ref() {
                    HirExpr::Var(name)
                    | HirExpr::AddressOfGlobal(name)
                    | HirExpr::AddressOfLocal(name) => {
                        format!("{name}[{index}]")
                    }
                    _ => format!("(({} *)({inner}))[{index}]", print_type(elem_ty)),
                }
            }
        }
        HirLValue::FieldAccess {
            base, field_name, ..
        } => {
            let mut inner = print_expr_prec(base, 110, depth + 1);
            if matches!(base.as_ref(), HirExpr::Cast { .. }) {
                inner = format!("({inner})");
            }
            let is_ptr = matches!(expr_type(base), NirType::Ptr(_));
            let op = if is_ptr { "->" } else { "." };
            format!("{inner}{op}{field_name}")
        }
    }
}

pub(crate) fn print_expr(expr: &HirExpr) -> String {
    print_expr_prec(expr, 0, 0)
}

pub(super) fn print_expr_prec(expr: &HirExpr, parent_prec: u8, depth: usize) -> String {
    if depth > MAX_PRINT_EXPR_DEPTH {
        return "0 /* [FISSION] RECURSION TOO DEEP (expression printer guard) */".to_string();
    }
    let (text, prec) = match expr {
        HirExpr::AddressOfLocal(name) => (format!("&{name}"), 110),
        HirExpr::AddressOfGlobal(name) => {
            if name.starts_with('"') {
                (name.clone(), 120)
            } else {
                (format!("&{name}"), 110)
            }
        }
        HirExpr::Var(name) => (name.clone(), 120),
        HirExpr::Const(value, ty) => (print_integer_const(*value, ty), 120),
        HirExpr::Cast { ty, expr } => {
            if matches!(ty, NirType::Ptr(_))
                && let HirExpr::Const(value, source_ty) = expr.as_ref()
                && source_ty == ty
            {
                (format!("({})0x{:x}", print_type(ty), *value as u64), 110)
            } else {
                let inner = print_expr_prec(expr, 110, depth + 1);
                (format!("({}){}", print_type(ty), inner), 110)
            }
        }
        HirExpr::Unary { op, expr, .. } => {
            let symbol = match op {
                HirUnaryOp::Neg => "-",
                HirUnaryOp::Not => "!",
                HirUnaryOp::BitNot => "~",
            };
            let inner = separate_unary_minus(symbol, print_expr_prec(expr, 110, depth + 1));
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
                let mut lhs_str = lhs_str;
                let mut rhs_str = print_expr_prec(rhs, rhs_parent_prec, depth + 1);
                // Same rule the ctx-aware path applies: a pointer cannot be an
                // operand of `* / % & | ^ << >>`, and cannot sit on the right
                // of `-` or opposite another pointer under `+`.
                if expr_prints_as_pointer_ctx_free(rhs)
                    && (rejects_pointer_operands(*op)
                        || matches!(*op, HirBinaryOp::Sub)
                        || (matches!(*op, HirBinaryOp::Add)
                            && expr_prints_as_pointer_ctx_free(lhs)))
                {
                    rhs_str = format!("(unsigned long long){rhs_str}");
                }
                if rejects_pointer_operands(*op) && expr_prints_as_pointer_ctx_free(lhs) {
                    lhs_str = format!("(unsigned long long){lhs_str}");
                }
                if *op == HirBinaryOp::Shr {
                    if let Some(count) = width_minus_shift_count(rhs) {
                        let count_str = print_expr_prec(count, 60, depth + 1);
                        (
                            format!(
                                "{count_str} == 0 ? 0 : {lhs_str} {} {rhs_str}",
                                print_binary_op(*op)
                            ),
                            20,
                        )
                    } else {
                        (
                            format!("{lhs_str} {} {rhs_str}", print_binary_op(*op)),
                            prec,
                        )
                    }
                } else {
                    (
                        format!("{lhs_str} {} {rhs_str}", print_binary_op(*op)),
                        prec,
                    )
                }
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
        HirExpr::Call {
            target, args, ty, ..
        } => {
            if target == "__fission_callind_opaque" && !args.is_empty() {
                let fn_ptr = print_expr_prec(&args[0], 0, depth + 1);
                let remaining_args = args[1..]
                    .iter()
                    .map(|arg| print_expr_prec(arg, 0, depth + 1))
                    .collect::<Vec<_>>()
                    .join(", ");
                // Cast scalar/unknown function-pointer carriers to a callable
                // type so `(*(ulonglong)fp)(...)` is valid C (register CallInd).
                let ret_ty = match ty {
                    NirType::Unknown => "unsigned long long".to_string(),
                    other => {
                        let printed = print_type(other);
                        if printed.is_empty() || printed == "void" {
                            "unsigned long long".to_string()
                        } else {
                            printed
                        }
                    }
                };
                (
                    // An empty function-pointer parameter list is a C11
                    // non-prototype type, so the call keeps its observed args
                    // without asserting a signature that was not recovered.
                    format!("(({ret_ty} (*)())({fn_ptr}))({remaining_args})"),
                    120,
                )
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
            let text = if matches!(elem_ty, NirType::Aggregate { .. }) {
                format!("(({} *)({inner}))[{index}]", print_type(elem_ty))
            } else {
                match base.as_ref() {
                    HirExpr::Var(name)
                    | HirExpr::AddressOfGlobal(name)
                    | HirExpr::AddressOfLocal(name) => {
                        format!("{name}[{index}]")
                    }
                    _ => format!("(({} *)({inner}))[{index}]", print_type(elem_ty)),
                }
            };
            (text, 120)
        }
        HirExpr::FieldAccess {
            base, field_name, ..
        } => {
            let mut inner = print_expr_prec(base, 110, depth + 1);
            if matches!(base.as_ref(), HirExpr::Cast { .. }) {
                inner = format!("({inner})");
            }
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

pub(super) fn peel_simple_deref_target(expr: &HirExpr) -> Option<&str> {
    match expr {
        // `*&g` is `g` itself, not `*g`, which reads through `g`'s value.
        // A same-width access was already spelled `g` by global symbol
        // recovery; what reaches here reads part of `g`, and keeps its cast.
        HirExpr::AddressOfGlobal(_) => None,
        HirExpr::Var(name) | HirExpr::AddressOfLocal(name) => Some(name),
        HirExpr::Cast { expr, .. } => peel_simple_deref_target(expr),
        HirExpr::PtrOffset { base, offset } if *offset == 0 => peel_simple_deref_target(base),
        HirExpr::AggregateCopy { src, .. } => peel_simple_deref_target(src),
        _ => None,
    }
}

pub(super) fn expr_fallback<'a>(expr: &'a HirExpr, _depth: usize) -> &'a HirExpr {
    expr
}
