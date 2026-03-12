use super::*;

pub(super) fn print_hir_function(func: &HirFunction) -> String {
    let mut out = String::new();
    out.push_str(&format!("{} {}(", print_type(&func.return_type), func.name));
    for (idx, param) in func.params.iter().enumerate() {
        if idx > 0 {
            out.push_str(", ");
        }
        out.push_str(&format!("{} {}", print_type(&param.ty), param.name));
    }
    out.push_str(")\n{\n");
    for local in &func.locals {
        out.push_str(&format!("    {} {};\n", print_type(&local.ty), local.name));
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

pub(super) fn print_stmt(stmt: &HirStmt) -> String {
    match stmt {
        HirStmt::Assign { lhs, rhs } => format!("{} = {};", print_lvalue(lhs), print_expr(rhs)),
        HirStmt::Expr(expr) => format!("{};", print_expr(expr)),
        HirStmt::Label(label) => format!("{}:", label),
        HirStmt::Goto(label) => format!("goto {};", label),
        HirStmt::Block(_) => "{ ... }".to_string(),
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
    match expr {
        HirExpr::Var(name) => name.clone(),
        HirExpr::Const(value, _) => value.to_string(),
        HirExpr::Cast { ty, expr } => format!("({})({})", print_type(ty), print_expr(expr)),
        HirExpr::Unary { op, expr, .. } => {
            let symbol = match op {
                HirUnaryOp::Neg => "-",
                HirUnaryOp::Not => "!",
                HirUnaryOp::BitNot => "~",
            };
            format!("{}({})", symbol, print_expr(expr))
        }
        HirExpr::Binary { op, lhs, rhs, .. } => {
            format!(
                "({} {} {})",
                print_expr(lhs),
                print_binary_op(*op),
                print_expr(rhs)
            )
        }
        HirExpr::Call { target, args, .. } => {
            let args = args.iter().map(print_expr).collect::<Vec<_>>().join(", ");
            format!("{target}({args})")
        }
        HirExpr::Load { ptr, ty } => format!("*({} *)({})", print_type(ty), print_expr(ptr)),
        HirExpr::PtrOffset { base, offset } => {
            if *offset == 0 {
                print_expr(base)
            } else if *offset > 0 {
                format!("((uint8_t *)({}) + {})", print_expr(base), offset)
            } else {
                format!(
                    "((uint8_t *)({}) - {})",
                    print_expr(base),
                    offset.unsigned_abs()
                )
            }
        }
        HirExpr::Index {
            base,
            index,
            elem_ty,
        } => format!(
            "(({} *)({}))[{}]",
            print_type(elem_ty),
            print_expr(base),
            index
        ),
        HirExpr::AggregateCopy { src, size } => {
            format!("*(fission_agg{} *)({})", size, print_expr(src))
        }
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
