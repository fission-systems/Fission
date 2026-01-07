//! Temporary Variable Inlining
//!
//! Implements single-use temporary variable elimination:
//! - temp = x + 1; return temp; → return x + 1;
//! - Reduces intermediate variables for cleaner output
//! - Similar to Ghidra's ActionMarkImplied

use super::{Expr, Stmt};
use std::collections::HashMap;
use std::hash::BuildHasher;

/// Inline temporary variables that are only used once
pub fn inline_temps<S: BuildHasher>(
    stmts: Vec<Stmt>,
    var_usage: &HashMap<String, usize, S>,
) -> Vec<Stmt> {
    let mut inliner = TempInliner::new(var_usage);
    inliner.process(stmts)
}

struct TempInliner<'a, S: BuildHasher> {
    var_usage: &'a HashMap<String, usize, S>,
    var_defs: HashMap<String, Expr>,
}

impl<'a, S: BuildHasher> TempInliner<'a, S> {
    fn new(var_usage: &'a HashMap<String, usize, S>) -> Self {
        Self {
            var_usage,
            var_defs: HashMap::new(),
        }
    }

    fn process(&mut self, stmts: Vec<Stmt>) -> Vec<Stmt> {
        let mut result = Vec::new();

        for stmt in stmts {
            if let Some(processed) = self.process_stmt(stmt) {
                result.push(processed);
            }
        }

        result
    }

    fn process_stmt(&mut self, stmt: Stmt) -> Option<Stmt> {
        match stmt {
            Stmt::Expr(Expr::Assign { target, value }) => {
                let value = self.inline_in_expr(*value);

                // Check if this is a single-use temp variable
                let usage_count = self.var_usage.get(&target).copied().unwrap_or(0);

                if usage_count == 1 && is_temp_var(&target) {
                    // Store the definition for later inlining
                    self.var_defs.insert(target.clone(), value);
                    // Don't emit this statement
                    None
                } else {
                    // Keep the assignment
                    Some(Stmt::Expr(Expr::Assign {
                        target,
                        value: Box::new(value),
                    }))
                }
            }
            Stmt::If {
                condition,
                then_block,
                else_block,
            } => {
                let condition = self.inline_in_expr(condition);
                let then_block = self.process(then_block);
                let else_block = else_block.map(|b| self.process(b));

                Some(Stmt::If {
                    condition,
                    then_block,
                    else_block,
                })
            }
            Stmt::While { condition, body } => {
                let condition = self.inline_in_expr(condition);
                let body = self.process(body);

                Some(Stmt::While { condition, body })
            }
            Stmt::Return(Some(expr)) => {
                let expr = self.inline_in_expr(expr);
                Some(Stmt::Return(Some(expr)))
            }
            other => Some(other),
        }
    }

    fn inline_in_expr(&self, expr: Expr) -> Expr {
        match expr {
            Expr::Var(name) => {
                // If this is a single-use temp, replace with its definition
                if let Some(def) = self.var_defs.get(&name) {
                    def.clone()
                } else {
                    Expr::Var(name)
                }
            }
            Expr::BinOp { op, left, right } => Expr::BinOp {
                op,
                left: Box::new(self.inline_in_expr(*left)),
                right: Box::new(self.inline_in_expr(*right)),
            },
            Expr::UnaryOp { op, operand } => Expr::UnaryOp {
                op,
                operand: Box::new(self.inline_in_expr(*operand)),
            },
            Expr::Call { name, args } => Expr::Call {
                name,
                args: args.into_iter().map(|a| self.inline_in_expr(a)).collect(),
            },
            Expr::Assign { target, value } => Expr::Assign {
                target,
                value: Box::new(self.inline_in_expr(*value)),
            },
            other => other,
        }
    }
}

/// Check if a variable name looks like a compiler-generated temporary
fn is_temp_var(name: &str) -> bool {
    // Common patterns:
    // - temp_N
    // - uVar_N
    // - iVar_N
    // - _tmp_N
    name.starts_with("temp_")
        || name.starts_with("uVar")
        || name.starts_with("iVar")
        || name.starts_with("_tmp")
        || name.starts_with("tmp")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::optimizer::{BinOpKind, Expr, Stmt};

    #[test]
    fn test_is_temp_var() {
        assert!(is_temp_var("temp_1"));
        assert!(is_temp_var("uVar5"));
        assert!(is_temp_var("iVar3"));
        assert!(is_temp_var("_tmp_x"));
        assert!(!is_temp_var("result"));
        assert!(!is_temp_var("count"));
    }

    #[test]
    fn test_inline_single_use() {
        let mut usage = HashMap::new();
        usage.insert("temp_1".to_string(), 1);

        let stmts = vec![
            Stmt::Expr(Expr::Assign {
                target: "temp_1".to_string(),
                value: Box::new(Expr::BinOp {
                    op: BinOpKind::Add,
                    left: Box::new(Expr::Var("x".to_string())),
                    right: Box::new(Expr::Const(1)),
                }),
            }),
            Stmt::Return(Some(Expr::Var("temp_1".to_string()))),
        ];

        let result = inline_temps(stmts, &usage);

        assert_eq!(result.len(), 1);
        if let Stmt::Return(Some(expr)) = &result[0] {
            assert!(matches!(expr, Expr::BinOp { .. }));
        } else {
            panic!("Expected return with inlined expression");
        }
    }

    #[test]
    fn test_keep_multi_use() {
        let mut usage = HashMap::new();
        usage.insert("temp_1".to_string(), 2);

        let stmts = vec![
            Stmt::Expr(Expr::Assign {
                target: "temp_1".to_string(),
                value: Box::new(Expr::Const(5)),
            }),
            Stmt::Return(Some(Expr::Var("temp_1".to_string()))),
        ];

        let result = inline_temps(stmts, &usage);

        // Should keep both statements since temp_1 is used twice
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_keep_non_temp() {
        let mut usage = HashMap::new();
        usage.insert("result".to_string(), 1);

        let stmts = vec![
            Stmt::Expr(Expr::Assign {
                target: "result".to_string(),
                value: Box::new(Expr::Const(42)),
            }),
            Stmt::Return(Some(Expr::Var("result".to_string()))),
        ];

        let result = inline_temps(stmts, &usage);

        // Should keep assignment since "result" is not a temp variable
        assert_eq!(result.len(), 2);
    }
}
