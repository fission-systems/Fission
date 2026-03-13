use super::*;

pub(super) fn print_hir_function(func: &HirFunction) -> String {
    let mut out = String::new();
    out.push_str(&format!("{} {}(", print_type(&func.return_type), func.name));
    for (idx, param) in func.params.iter().enumerate() {
        if idx > 0 {
            out.push_str(", ");
        }
        out.push_str(&format!("{} {}", print_binding_type(param), param.name));
    }
    out.push_str(")\n{\n");
    for local in &func.locals {
        out.push_str(&format!("    {} {};\n", print_binding_type(local), local.name));
    }
    if !func.locals.is_empty() {
        out.push('\n');
    }
    for stmt in &func.body {
        print_stmt_with_indent(stmt, 1, &mut out);
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
        HirStmt::Assign { lhs, rhs } => format!("{} = {};", print_lvalue(lhs), print_expr(rhs)),
        HirStmt::Expr(expr) => format!("{};", print_expr(expr)),
        HirStmt::Label(label) => format!("{}:", label),
        HirStmt::Goto(label) => format!("goto {};", label),
        HirStmt::Block(_) => "{ ... }".to_string(),
        HirStmt::Switch { .. } => "switch (...) { ... }".to_string(),
        HirStmt::If { .. } => "if (...) { ... }".to_string(),
        HirStmt::While { .. } => "while (...) { ... }".to_string(),
        HirStmt::DoWhile { .. } => "do { ... } while (...);".to_string(),
        HirStmt::Return(Some(expr)) => format!("return {};", print_expr(expr)),
        HirStmt::Return(None) => "return;".to_string(),
        HirStmt::Break => "break;".to_string(),
        HirStmt::Continue => "continue;".to_string(),
    }
}

fn print_stmt_with_indent(stmt: &HirStmt, indent: usize, out: &mut String) {
    let pad = "    ".repeat(indent);
    match stmt {
        HirStmt::Assign { .. }
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
                print_stmt_with_indent(stmt, indent + 1, out);
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
                    print_stmt_with_indent(stmt, indent + 2, out);
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
                    print_stmt_with_indent(stmt, indent + 2, out);
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
            out.push_str(&format!("if ({}) {{\n", print_expr(cond)));
            for stmt in then_body {
                print_stmt_with_indent(stmt, indent + 1, out);
            }
            out.push_str(&pad);
            out.push('}');
            if else_body.is_empty() {
                out.push('\n');
            } else {
                out.push_str(" else {\n");
                for stmt in else_body {
                    print_stmt_with_indent(stmt, indent + 1, out);
                }
                out.push_str(&pad);
                out.push_str("}\n");
            }
        }
        HirStmt::While { cond, body } => {
            out.push_str(&pad);
            out.push_str(&format!("while ({}) {{\n", print_expr(cond)));
            for stmt in body {
                print_stmt_with_indent(stmt, indent + 1, out);
            }
            out.push_str(&pad);
            out.push_str("}\n");
        }
        HirStmt::DoWhile { body, cond } => {
            out.push_str(&pad);
            out.push_str("do {\n");
            for stmt in body {
                print_stmt_with_indent(stmt, indent + 1, out);
            }
            out.push_str(&pad);
            out.push_str(&format!("}} while ({});\n", print_expr(cond)));
        }
    }
}

fn print_lvalue(lhs: &HirLValue) -> String {
    match lhs {
        HirLValue::Var(name) => name.clone(),
        HirLValue::Deref { ptr, ty } => format!("*({} *)({})", print_type(ty), print_expr(ptr)),
    }
}

pub(super) fn print_expr(expr: &HirExpr) -> String {
    print_expr_prec(expr, 0)
}

fn print_expr_prec(expr: &HirExpr, parent_prec: u8) -> String {
    let (text, prec) = match expr {
        HirExpr::Var(name) => (name.clone(), 100),
        HirExpr::Const(value, _) => (value.to_string(), 100),
        HirExpr::Cast { ty, expr } => {
            let inner = print_expr_prec(expr, 90);
            (format!("({}){}", print_type(ty), inner), 90)
        }
        HirExpr::Unary { op, expr, .. } => {
            let symbol = match op {
                HirUnaryOp::Neg => "-",
                HirUnaryOp::Not => "!",
                HirUnaryOp::BitNot => "~",
            };
            let inner = print_expr_prec(expr, 85);
            (format!("{symbol}{inner}"), 85)
        }
        HirExpr::Binary { op, lhs, rhs, .. } => {
            let prec = binary_precedence(*op);
            let lhs = print_expr_prec(lhs, prec);
            let rhs = print_expr_prec(rhs, prec + 1);
            (format!("{lhs} {} {rhs}", print_binary_op(*op)), prec)
        }
        HirExpr::Call { target, args, .. } => {
            let args = args.iter().map(print_expr).collect::<Vec<_>>().join(", ");
            (format!("{target}({args})"), 100)
        }
        HirExpr::Load { ptr, ty } => {
            let inner = print_expr(ptr);
            (format!("*({} *)({inner})", print_type(ty)), 95)
        }
        HirExpr::PtrOffset { base, offset } => {
            let inner = print_expr(base);
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
            let inner = print_expr(base);
            (format!("(({} *)({inner}))[{index}]", print_type(elem_ty)), 95)
        }
        HirExpr::AggregateCopy { src, size } => {
            let inner = print_expr(src);
            (format!("*(fission_agg{} *)({inner})", size), 95)
        }
    };

    if prec < parent_prec {
        format!("({text})")
    } else {
        text
    }
}

fn binary_precedence(op: HirBinaryOp) -> u8 {
    match op {
        HirBinaryOp::LogicalOr => 10,
        HirBinaryOp::LogicalAnd => 20,
        HirBinaryOp::Eq | HirBinaryOp::Ne | HirBinaryOp::Lt | HirBinaryOp::Le | HirBinaryOp::SLt | HirBinaryOp::SLe => 30,
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
        HirBinaryOp::Shr | HirBinaryOp::Sar => ">>",
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
        NirType::Aggregate { size } => format!("fission_agg{}", size),
        NirType::Float { bits } => match *bits {
            32 => "float".to_string(),
            64 => "double".to_string(),
            _ => format!("float{}", bits),
        },
    }
}
