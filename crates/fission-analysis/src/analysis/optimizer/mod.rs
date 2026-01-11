//! Decompiler Output Optimizer
//!
//! Post-processes decompiler output to improve readability through:
//! - Bit operation simplification
//! - Control flow normalization
//! - Temporary variable inlining

pub mod bitops;
pub mod control_flow;
pub mod integration;
pub mod temp_inline;

use std::collections::HashMap;

/// Represents a simplified AST node for optimization
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// Variable reference
    Var(String),
    /// Integer constant
    Const(i64),
    /// Binary operation
    BinOp {
        op: BinOpKind,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    /// Unary operation
    UnaryOp { op: UnaryOpKind, operand: Box<Expr> },
    /// Function call
    Call { name: String, args: Vec<Expr> },
    /// Assignment (for statement-level optimizations)
    Assign { target: String, value: Box<Expr> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOpKind {
    // Arithmetic
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    // Bitwise
    And,
    Or,
    Xor,
    Shl,
    Shr,
    // Comparison
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    // Logical
    LogicalAnd,
    LogicalOr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOpKind {
    Neg,
    Not,
    BitwiseNot,
}

/// Statement types for control flow
#[derive(Debug, Clone)]
pub enum Stmt {
    Expr(Expr),
    If {
        condition: Expr,
        then_block: Vec<Stmt>,
        else_block: Option<Vec<Stmt>>,
    },
    While {
        condition: Expr,
        body: Vec<Stmt>,
    },
    Return(Option<Expr>),
}

/// Main optimizer that applies all optimization passes
pub struct Optimizer {
    /// Track variable usage counts
    var_usage: HashMap<String, usize>,
    /// Enable/disable specific optimizations
    config: OptimizerConfig,
}

#[derive(Debug, Clone)]
pub struct OptimizerConfig {
    pub enable_bitops: bool,
    pub enable_control_flow: bool,
    pub enable_temp_inline: bool,
}

impl Default for OptimizerConfig {
    fn default() -> Self {
        Self {
            enable_bitops: true,
            enable_control_flow: true,
            enable_temp_inline: true,
        }
    }
}

impl Optimizer {
    pub fn new() -> Self {
        Self::with_config(OptimizerConfig::default())
    }

    pub fn with_config(config: OptimizerConfig) -> Self {
        Self {
            var_usage: HashMap::new(),
            config,
        }
    }

    /// Apply all enabled optimization passes to an expression
    pub fn optimize_expr(&mut self, expr: Expr) -> Expr {
        let mut result = expr;

        if self.config.enable_bitops {
            result = bitops::simplify_bitops(result);
        }

        result
    }

    /// Apply all enabled optimization passes to statements
    pub fn optimize_stmts(&mut self, stmts: Vec<Stmt>) -> Vec<Stmt> {
        let mut result = stmts;

        // First pass: count variable usage
        if self.config.enable_temp_inline {
            self.analyze_usage(&result);
            result = temp_inline::inline_temps(result, &self.var_usage);
        }

        // Second pass: normalize control flow
        if self.config.enable_control_flow {
            result = control_flow::normalize(result);
        }

        // Final pass: simplify bit operations across statements
        if self.config.enable_bitops {
            result = self.simplify_stmts(result);
        }

        result
    }

    fn simplify_stmts(&self, stmts: Vec<Stmt>) -> Vec<Stmt> {
        stmts
            .into_iter()
            .map(|stmt| self.simplify_stmt(stmt))
            .collect()
    }

    fn simplify_stmt(&self, stmt: Stmt) -> Stmt {
        match stmt {
            Stmt::Expr(expr) => Stmt::Expr(bitops::simplify_bitops(expr)),
            Stmt::If {
                condition,
                then_block,
                else_block,
            } => Stmt::If {
                condition: bitops::simplify_bitops(condition),
                then_block: self.simplify_stmts(then_block),
                else_block: else_block.map(|block| self.simplify_stmts(block)),
            },
            Stmt::While { condition, body } => Stmt::While {
                condition: bitops::simplify_bitops(condition),
                body: self.simplify_stmts(body),
            },
            Stmt::Return(Some(expr)) => Stmt::Return(Some(bitops::simplify_bitops(expr))),
            Stmt::Return(None) => Stmt::Return(None),
        }
    }

    fn analyze_usage(&mut self, stmts: &[Stmt]) {
        self.var_usage.clear();
        for stmt in stmts {
            self.count_expr_usage(stmt);
        }
    }

    fn count_expr_usage(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Expr(expr) => self.count_in_expr(expr),
            Stmt::If {
                condition,
                then_block,
                else_block,
            } => {
                self.count_in_expr(condition);
                for s in then_block {
                    self.count_expr_usage(s);
                }
                if let Some(else_stmts) = else_block {
                    for s in else_stmts {
                        self.count_expr_usage(s);
                    }
                }
            }
            Stmt::While { condition, body } => {
                self.count_in_expr(condition);
                for s in body {
                    self.count_expr_usage(s);
                }
            }
            Stmt::Return(Some(expr)) => self.count_in_expr(expr),
            Stmt::Return(None) => {}
        }
    }

    fn count_in_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Var(name) => {
                *self.var_usage.entry(name.clone()).or_insert(0) += 1;
            }
            Expr::BinOp { left, right, .. } => {
                self.count_in_expr(left);
                self.count_in_expr(right);
            }
            Expr::UnaryOp { operand, .. } => {
                self.count_in_expr(operand);
            }
            Expr::Call { args, .. } => {
                for arg in args {
                    self.count_in_expr(arg);
                }
            }
            Expr::Assign { value, .. } => {
                self.count_in_expr(value);
            }
            Expr::Const(_) => {}
        }
    }
}

impl Default for Optimizer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_optimizer_creation() {
        let opt = Optimizer::new();
        assert!(opt.config.enable_bitops);
        assert!(opt.config.enable_control_flow);
        assert!(opt.config.enable_temp_inline);
    }
}
