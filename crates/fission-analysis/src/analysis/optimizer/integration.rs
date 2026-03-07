//! Integration module for applying optimizer to decompiler output
//!
//! This module provides utilities to parse decompiled C code into AST,
//! apply optimizations, and regenerate improved C code.

use super::{BinOpKind, Expr, Optimizer, OptimizerConfig, Stmt, UnaryOpKind};

/// Parse decompiled C code into statements
pub fn parse_c_to_ast(c_code: &str) -> Vec<Stmt> {
    let lines: Vec<&str> = c_code.lines().collect();
    parse_lines_to_stmts(&lines, 0).0
}

/// Recursive line-based parser that respects brace-delimited blocks.
/// Returns (parsed statements, number of lines consumed).
fn parse_lines_to_stmts(lines: &[&str], start: usize) -> (Vec<Stmt>, usize) {
    let mut stmts = Vec::new();
    let mut i = start;

    while i < lines.len() {
        let line = lines[i].trim();

        // Stop at a closing brace — the caller will consume it
        if line == "}" || line == "} else {" || line.starts_with("} else") {
            break;
        }

        // Skip empty lines, comments, opening braces, and function signatures
        if line.is_empty() || line.starts_with("//") || line.starts_with("/*") || line == "{" {
            i += 1;
            continue;
        }

        // Parse if statements with block awareness
        if line.starts_with("if") || line.starts_with("if ") || line.starts_with("if(") {
            let (stmt, consumed) = parse_if_block(lines, i);
            if let Some(s) = stmt {
                stmts.push(s);
            }
            i += consumed;
            continue;
        }

        // Parse variable declarations: type name = expr;
        if let Some(stmt) = try_parse_var_decl(line) {
            stmts.push(stmt);
            i += 1;
            continue;
        }

        // Parse assignments: name = expr;
        if let Some(stmt) = try_parse_assignment(line) {
            stmts.push(stmt);
            i += 1;
            continue;
        }

        // Parse return statements
        if line.starts_with("return") {
            if let Some(stmt) = try_parse_return(line) {
                stmts.push(stmt);
            }
            i += 1;
            continue;
        }

        i += 1;
    }

    let consumed = i - start;
    (stmts, consumed)
}

/// Parse an if/else block spanning multiple lines.
/// Returns (optional statement, number of lines consumed).
fn parse_if_block(lines: &[&str], start: usize) -> (Option<Stmt>, usize) {
    let line = lines[start].trim();

    // Extract condition from "if (condition) {"  or  "if (condition)"
    let condition = match extract_if_condition(line) {
        Some(c) => c,
        None => return (None, 1),
    };

    let mut i = start + 1;

    // Check if the opening brace is on the same line or the next line
    let has_brace = line.ends_with('{');

    if !has_brace {
        // Check next line for a lone opening brace
        if i < lines.len() && lines[i].trim() == "{" {
            i += 1;
        }
    }

    // Parse then-block lines until closing brace
    let (then_block, consumed) = parse_lines_to_stmts(lines, i);
    i += consumed;

    // Consume closing brace (could be "}" or "} else {")
    let mut else_block = None;
    if i < lines.len() {
        let closing = lines[i].trim();
        if closing == "} else {" || closing.starts_with("} else") {
            i += 1;
            let (else_stmts, else_consumed) = parse_lines_to_stmts(lines, i);
            i += else_consumed;
            if !else_stmts.is_empty() {
                else_block = Some(else_stmts);
            }
            // Consume final "}"
            if i < lines.len() && lines[i].trim() == "}" {
                i += 1;
            }
        } else if closing == "}" {
            i += 1;
        }
    }

    let stmt = Stmt::If {
        condition,
        then_block,
        else_block,
    };

    (Some(stmt), i - start)
}

/// Extract the condition expression from an if-line like "if (x > 0) {"
fn extract_if_condition(line: &str) -> Option<Expr> {
    let if_start = line.find("if")?;
    let paren_start = line[if_start..].find('(')?;

    // Find matching closing paren (handle nested parens)
    let after_if = &line[if_start + paren_start..];
    let mut depth = 0i32;
    let mut end = None;
    for (idx, ch) in after_if.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(idx);
                    break;
                }
            }
            _ => {}
        }
    }

    let paren_end = end?;
    let cond_str = &after_if[1..paren_end];
    Some(parse_expr(cond_str))
}

/// Try to parse variable declaration: type name = expr;
fn try_parse_var_decl(line: &str) -> Option<Stmt> {
    // Simple pattern: "type name = expr;" or "type name;"
    let parts: Vec<&str> = line.split('=').collect();
    if parts.len() >= 2 {
        let lhs = parts[0].trim();
        let rhs = parts[1].trim().trim_end_matches(';').trim();

        // Extract variable name (last word before =)
        let lhs_parts: Vec<&str> = lhs.split_whitespace().collect();
        if let Some(var_name) = lhs_parts.last() {
            let expr = parse_expr(rhs);
            return Some(Stmt::Expr(Expr::Assign {
                target: var_name.trim_matches('*').to_string(),
                value: Box::new(expr),
            }));
        }
    }
    None
}

/// Try to parse assignment: name = expr;
fn try_parse_assignment(line: &str) -> Option<Stmt> {
    if let Some(eq_pos) = line.find('=') {
        let lhs = line[..eq_pos].trim();
        let rhs = line[eq_pos + 1..].trim().trim_end_matches(';').trim();

        // Skip if it looks like a declaration (has type keywords)
        if lhs.contains(" ")
            && (lhs.contains("int") || lhs.contains("char") || lhs.contains("void"))
        {
            return None;
        }

        let rhs_expr = parse_expr(rhs);

        return Some(Stmt::Expr(Expr::Assign {
            target: lhs.to_string(),
            value: Box::new(rhs_expr),
        }));
    }
    None
}

/// Try to parse if statement from a single line (fallback for single-line ifs)
#[allow(dead_code)]
fn try_parse_if(line: &str) -> Option<Stmt> {
    let condition = extract_if_condition(line)?;

    Some(Stmt::If {
        condition,
        then_block: vec![],
        else_block: None,
    })
}

/// Try to parse return statement: return expr;
fn try_parse_return(line: &str) -> Option<Stmt> {
    let expr_str = line
        .strip_prefix("return")?
        .trim()
        .trim_end_matches(';')
        .trim();

    if expr_str.is_empty() || expr_str == "void" {
        return Some(Stmt::Return(None));
    }

    Some(Stmt::Return(Some(parse_expr(expr_str))))
}

/// Parse expression from string
pub fn parse_expr(s: &str) -> Expr {
    let s = s.trim();

    // Parse binary operations
    if let Some(expr) = try_parse_binop(s) {
        return expr;
    }

    // Parse unary operations
    if let Some(stripped) = s.strip_prefix('!') {
        return Expr::UnaryOp {
            op: UnaryOpKind::Not,
            operand: Box::new(parse_expr(stripped)),
        };
    }

    if let Some(stripped) = s.strip_prefix('-') {
        let is_number = stripped
            .chars()
            .next()
            .map(|c| c.is_numeric())
            .unwrap_or(false);
        if !stripped.is_empty() && !is_number {
            return Expr::UnaryOp {
                op: UnaryOpKind::Neg,
                operand: Box::new(parse_expr(stripped)),
            };
        }
    }

    // Parse literals
    if let Ok(val) = s.parse::<i64>() {
        return Expr::Const(val);
    }

    // Hex literals
    if s.starts_with("0x") || s.starts_with("0X") {
        if let Ok(val) = i64::from_str_radix(&s[2..], 16) {
            return Expr::Const(val);
        }
    }

    // Variable reference
    Expr::Var(s.to_string())
}

/// Try to parse binary operation
fn try_parse_binop(s: &str) -> Option<Expr> {
    // Find operators (in order of precedence)
    let operators = vec![
        ("==", BinOpKind::Eq),
        ("!=", BinOpKind::Ne),
        ("<=", BinOpKind::Le),
        (">=", BinOpKind::Ge),
        ("<", BinOpKind::Lt),
        (">", BinOpKind::Gt),
        ("<<", BinOpKind::Shl),
        (">>", BinOpKind::Shr),
        ("+", BinOpKind::Add),
        ("-", BinOpKind::Sub),
        ("*", BinOpKind::Mul),
        ("/", BinOpKind::Div),
        ("&", BinOpKind::And),
        ("|", BinOpKind::Or),
        ("^", BinOpKind::Xor),
    ];

    for (op_str, op_kind) in operators {
        if let Some(pos) = s.rfind(op_str) {
            // Avoid matching inside hex literals
            if op_str == "x" && pos > 0 && &s[pos - 1..pos] == "0" {
                continue;
            }

            let left = s[..pos].trim();
            let right = s[pos + op_str.len()..].trim();

            // Skip if empty
            if left.is_empty() || right.is_empty() {
                continue;
            }

            return Some(Expr::BinOp {
                left: Box::new(parse_expr(left)),
                op: op_kind,
                right: Box::new(parse_expr(right)),
            });
        }
    }

    None
}

/// Convert AST back to C code
pub fn ast_to_c(stmts: &[Stmt]) -> String {
    let mut output = String::new();

    for stmt in stmts {
        output.push_str(&stmt_to_c(stmt, 2)); // indent level 2 (inside function)
    }

    output
}

fn stmt_to_c(stmt: &Stmt, indent: usize) -> String {
    let ind = "  ".repeat(indent);

    match stmt {
        Stmt::Expr(expr) => {
            // Check if it's an assignment
            if let Expr::Assign { target, value } = expr {
                format!("{}{} = {};\n", ind, target, expr_to_c(value))
            } else {
                format!("{}{};\n", ind, expr_to_c(expr))
            }
        }
        Stmt::If {
            condition,
            then_block,
            else_block,
        } => {
            let mut s = format!("{}if ({}) {{\n", ind, expr_to_c(condition));
            for stmt in then_block {
                s.push_str(&stmt_to_c(stmt, indent + 1));
            }
            s.push_str(&format!("{}}}", ind));

            if let Some(else_stmts) = else_block {
                s.push_str(" else {\n");
                for stmt in else_stmts {
                    s.push_str(&stmt_to_c(stmt, indent + 1));
                }
                s.push_str(&format!("{}}}", ind));
            }
            s.push('\n');
            s
        }
        Stmt::Return(expr) => {
            if let Some(e) = expr {
                format!("{}return {};\n", ind, expr_to_c(e))
            } else {
                format!("{}return;\n", ind)
            }
        }
        _ => format!("{}/* unsupported statement */\n", ind),
    }
}

fn expr_to_c(expr: &Expr) -> String {
    match expr {
        Expr::Const(val) => {
            if *val >= 0 && *val <= 9 {
                format!("{}", val)
            } else {
                format!("0x{:x}", val)
            }
        }
        Expr::Var(name) => name.clone(),
        Expr::BinOp { left, op, right } => {
            format!(
                "({} {} {})",
                expr_to_c(left),
                binop_to_str(op),
                expr_to_c(right)
            )
        }
        Expr::UnaryOp { op, operand } => {
            format!("({}{})", unaryop_to_str(op), expr_to_c(operand))
        }
        Expr::Assign { target, value } => {
            format!("{} = {}", target, expr_to_c(value))
        }
        Expr::Call { name, args } => {
            let args_str = args
                .iter()
                .map(|a| expr_to_c(a))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}({})", name, args_str)
        }
    }
}

fn binop_to_str(op: &BinOpKind) -> &str {
    match op {
        BinOpKind::Add => "+",
        BinOpKind::Sub => "-",
        BinOpKind::Mul => "*",
        BinOpKind::Div => "/",
        BinOpKind::Mod => "%",
        BinOpKind::And => "&",
        BinOpKind::Or => "|",
        BinOpKind::Xor => "^",
        BinOpKind::Shl => "<<",
        BinOpKind::Shr => ">>",
        BinOpKind::Eq => "==",
        BinOpKind::Ne => "!=",
        BinOpKind::Lt => "<",
        BinOpKind::Le => "<=",
        BinOpKind::Gt => ">",
        BinOpKind::Ge => ">=",
        BinOpKind::LogicalAnd => "&&",
        BinOpKind::LogicalOr => "||",
    }
}

fn unaryop_to_str(op: &UnaryOpKind) -> &str {
    match op {
        UnaryOpKind::Neg => "-",
        UnaryOpKind::Not => "!",
        UnaryOpKind::BitwiseNot => "~",
    }
}

/// Apply optimizer to decompiled C code
pub fn optimize_c_code(c_code: &str, config: OptimizerConfig) -> String {
    // Parse C to AST
    let stmts = parse_c_to_ast(c_code);

    // Apply optimizer
    let mut optimizer = Optimizer::with_config(config);
    let optimized = optimizer.optimize_stmts(stmts);

    // Convert back to C
    ast_to_c(&optimized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_const() {
        let expr = parse_expr("42");
        assert!(matches!(expr, Expr::Const(42)));
    }

    #[test]
    fn test_parse_hex() {
        let expr = parse_expr("0xFF");
        assert!(matches!(expr, Expr::Const(255)));
    }

    #[test]
    fn test_parse_var() {
        let expr = parse_expr("temp_1");
        assert!(matches!(expr, Expr::Var(ref s) if s == "temp_1"));
    }

    #[test]
    fn test_parse_binop() {
        let expr = parse_expr("x + 5");
        if let Expr::BinOp { op, .. } = expr {
            assert!(matches!(op, BinOpKind::Add));
        } else {
            panic!("Expected binary op");
        }
    }

    #[test]
    fn test_parse_assignment() {
        let line = "x = y + 5;";
        let stmt = try_parse_assignment(line);
        assert!(stmt.is_some());
    }

    #[test]
    fn test_optimize_simple() {
        let code = "  int temp_1 = x ^ 0;";
        let optimized = optimize_c_code(code, OptimizerConfig::default());
        // Should optimize x ^ 0 to x
        eprintln!("Optimized: {}", optimized);
        assert!(optimized.contains("x") && !optimized.contains("^ 0"));
    }

    #[test]
    fn test_parse_if_block() {
        let code = r#"
if (x > 0) {
    y = 1;
}
"#;
        let stmts = parse_c_to_ast(code);
        assert_eq!(stmts.len(), 1);
        if let Stmt::If {
            then_block,
            else_block,
            ..
        } = &stmts[0]
        {
            assert_eq!(then_block.len(), 1, "then_block should have 1 statement");
            assert!(else_block.is_none());
        } else {
            panic!("Expected If statement");
        }
    }

    #[test]
    fn test_parse_if_else_block() {
        let code = r#"
if (x > 0) {
    y = 1;
} else {
    y = 2;
}
"#;
        let stmts = parse_c_to_ast(code);
        assert_eq!(stmts.len(), 1);
        if let Stmt::If {
            then_block,
            else_block,
            ..
        } = &stmts[0]
        {
            assert_eq!(then_block.len(), 1);
            let Some(eb) = else_block.as_ref() else {
                panic!("should have else block")
            };
            assert_eq!(eb.len(), 1);
        } else {
            panic!("Expected If statement");
        }
    }

    #[test]
    fn test_parse_if_roundtrip() {
        let code = "if (x > 0) {\n    y = 1;\n}\n";
        let stmts = parse_c_to_ast(code);
        let output = ast_to_c(&stmts);
        assert!(output.contains("if"), "roundtrip must contain if");
        assert!(
            output.contains("y = 1"),
            "roundtrip must contain assignment"
        );
    }
}
