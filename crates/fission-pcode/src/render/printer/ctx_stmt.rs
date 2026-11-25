//! Context-aware statement printing.

use super::*;

pub(super) fn print_stmt_ctx(stmt: &HirStmt, ctx: &PrintCtx<'_>) -> String {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            if let Some(compound) = try_compound_assignment(lhs, rhs, ctx) {
                compound
            } else {
                format!(
                    "{} = {};",
                    print_lvalue_ctx(lhs, 0, ctx),
                    print_assignment_rhs(lhs, rhs, ctx)
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
        HirStmt::Return(Some(expr)) => format!("return {};", print_return_expr(expr, ctx)),
        HirStmt::Return(None) => "return;".to_string(),
        HirStmt::Break => "break;".to_string(),
        HirStmt::Continue => "continue;".to_string(),
        HirStmt::Label(label) => format!("{}:", label),
        HirStmt::Goto(label) if label == crate::render::SWITCH_FALLTHROUGH_SENTINEL => {
            "/* fallthrough */".to_string()
        }
        HirStmt::Goto(label) => format!("goto {};", label),
        _ => print_stmt(stmt),
    }
}

pub(super) fn print_stmt_with_indent_ctx(
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
    if let HirStmt::Assign {
        lhs: HirLValue::Var(name),
        ..
    } = stmt
    {
        if let Some(trace) = ctx.line_trace {
            trace.definition(name, out);
        }
    }
    // Only simple statement lines with one call and no memory LHS claim
    // a call origin. Headers and nested/multiple calls deliberately abstain.
    if let Some(trace) = ctx.line_trace {
        match stmt {
            HirStmt::Assign {
                lhs: HirLValue::Var(_),
                rhs,
            } => trace.call(rhs, out),
            HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => trace.call(expr, out),
            _ => {}
        }
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
                    if s_idx == 0 {
                        if let HirStmt::Label(label) = s {
                            if !ctx.goto_targets.contains(label.as_str()) {
                                continue;
                            }
                        }
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
                    if s_idx == 0 {
                        if let HirStmt::Label(label) = s {
                            if !ctx.goto_targets.contains(label.as_str()) {
                                continue;
                            }
                        }
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
