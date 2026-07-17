//! Composable backward-liveness summaries for structured HIR statements.

use crate::midend::normalize::analysis::defuse::collect_expr_vars;
use crate::midend::{HirExpr, HirLValue, HirStmt};
use std::collections::HashSet;

/// Transfer summary for `live_in = uses_before_definition U (live_out - must_definitions)`.
///
/// The fields remain private so callers cannot manufacture a proof by combining
/// unrelated name sets. Summaries are built from HIR structure and composed in
/// execution order.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct LivenessTransfer {
    uses_before_definition: HashSet<String>,
    must_definitions: HashSet<String>,
    may_diverge: bool,
}

impl LivenessTransfer {
    pub(crate) fn for_stmt(stmt: &HirStmt) -> Self {
        match stmt {
            HirStmt::Assign { lhs, rhs } => {
                let mut uses = HashSet::new();
                collect_lvalue_reads(lhs, &mut uses);
                collect_expr_vars(rhs, &mut uses);
                let must_definitions = match lhs {
                    HirLValue::Var(name) => HashSet::from([name.clone()]),
                    _ => HashSet::new(),
                };
                Self {
                    uses_before_definition: uses,
                    must_definitions,
                    may_diverge: false,
                }
            }
            HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => {
                let mut uses = HashSet::new();
                collect_expr_vars(expr, &mut uses);
                Self {
                    uses_before_definition: uses,
                    may_diverge: matches!(stmt, HirStmt::Return(_)),
                    ..Self::default()
                }
            }
            HirStmt::VaStart { va_list, .. } => {
                let mut uses = HashSet::new();
                collect_expr_vars(va_list, &mut uses);
                Self {
                    uses_before_definition: uses,
                    ..Self::default()
                }
            }
            HirStmt::Return(None) | HirStmt::Goto(_) | HirStmt::Break | HirStmt::Continue => Self {
                may_diverge: true,
                ..Self::default()
            },
            HirStmt::Label(_) => Self::default(),
            HirStmt::Block(body) => Self::for_stmts(body),
            HirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                let mut uses = HashSet::new();
                collect_expr_vars(cond, &mut uses);
                let then_transfer = Self::for_stmts(then_body);
                let else_transfer = Self::for_stmts(else_body);
                uses.extend(then_transfer.uses_before_definition.iter().cloned());
                uses.extend(else_transfer.uses_before_definition.iter().cloned());
                let must_definitions = then_transfer
                    .must_definitions
                    .intersection(&else_transfer.must_definitions)
                    .cloned()
                    .collect();
                Self {
                    uses_before_definition: uses,
                    must_definitions,
                    may_diverge: then_transfer.may_diverge || else_transfer.may_diverge,
                }
            }
            HirStmt::While { cond, body } => {
                let mut uses = HashSet::new();
                collect_expr_vars(cond, &mut uses);
                let body_transfer = Self::for_stmts(body);
                uses.extend(body_transfer.uses_before_definition);
                Self {
                    uses_before_definition: uses,
                    must_definitions: HashSet::new(),
                    may_diverge: body_transfer.may_diverge,
                }
            }
            HirStmt::DoWhile { body, cond } => {
                let body_transfer = Self::for_stmts(body);
                let mut cond_uses = HashSet::new();
                collect_expr_vars(cond, &mut cond_uses);
                body_transfer.then(Self {
                    uses_before_definition: cond_uses,
                    must_definitions: HashSet::new(),
                    may_diverge: false,
                })
            }
            HirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                let init_transfer = init.as_deref().map(Self::for_stmt).unwrap_or_default();
                let mut loop_uses = HashSet::new();
                if let Some(cond) = cond {
                    collect_expr_vars(cond, &mut loop_uses);
                }
                let body_transfer = Self::for_stmts(body);
                loop_uses.extend(body_transfer.uses_before_definition);
                if let Some(update) = update {
                    loop_uses.extend(Self::for_stmt(update).uses_before_definition);
                }
                init_transfer.then(Self {
                    uses_before_definition: loop_uses,
                    must_definitions: HashSet::new(),
                    may_diverge: body_transfer.may_diverge,
                })
            }
            HirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                let mut uses = HashSet::new();
                collect_expr_vars(expr, &mut uses);
                let mut arms: Vec<Self> = cases
                    .iter()
                    .map(|case| Self::for_stmts(&case.body))
                    .collect();
                arms.push(Self::for_stmts(default));
                for arm in &arms {
                    uses.extend(arm.uses_before_definition.iter().cloned());
                }
                let must_definitions = arms
                    .iter()
                    .map(|arm| arm.must_definitions.clone())
                    .reduce(|left, right| left.intersection(&right).cloned().collect())
                    .unwrap_or_default();
                Self {
                    uses_before_definition: uses,
                    must_definitions,
                    may_diverge: arms.iter().any(|arm| arm.may_diverge),
                }
            }
        }
    }

    pub(crate) fn for_stmts(stmts: &[HirStmt]) -> Self {
        stmts
            .iter()
            .map(Self::for_stmt)
            .fold(Self::default(), Self::then)
    }

    pub(crate) fn uses_before_definition(&self) -> impl Iterator<Item = &str> {
        self.uses_before_definition.iter().map(String::as_str)
    }

    fn then(self, next: Self) -> Self {
        let mut uses = self.uses_before_definition;
        if self.may_diverge {
            uses.extend(next.uses_before_definition.iter().cloned());
        } else {
            uses.extend(
                next.uses_before_definition
                    .difference(&self.must_definitions)
                    .cloned(),
            );
        }
        let must_definitions = if self.may_diverge || next.may_diverge {
            HashSet::new()
        } else {
            self.must_definitions
                .union(&next.must_definitions)
                .cloned()
                .collect()
        };
        Self {
            uses_before_definition: uses,
            must_definitions,
            may_diverge: self.may_diverge || next.may_diverge,
        }
    }
}

fn collect_lvalue_reads(lhs: &HirLValue, out: &mut HashSet<String>) {
    match lhs {
        HirLValue::Var(_) => {}
        HirLValue::Deref { ptr, .. } => collect_expr_vars(ptr, out),
        HirLValue::Index { base, index, .. } => {
            collect_expr_vars(base, out);
            collect_expr_vars(index, out);
        }
        HirLValue::FieldAccess { base, .. } => collect_expr_vars(base, out),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midend::{HirBinaryOp, NirType};

    fn var(name: &str) -> HirExpr {
        HirExpr::Var(name.to_string())
    }

    fn assign(name: &str, rhs: HirExpr) -> HirStmt {
        HirStmt::Assign {
            lhs: HirLValue::Var(name.to_string()),
            rhs,
        }
    }

    fn lt(lhs: &str, rhs: &str) -> HirExpr {
        HirExpr::Binary {
            op: HirBinaryOp::Lt,
            lhs: Box::new(var(lhs)),
            rhs: Box::new(var(rhs)),
            ty: NirType::Bool,
        }
    }

    #[test]
    fn inner_definition_hides_structured_use_from_entry() {
        let stmt = HirStmt::While {
            cond: HirExpr::Const(1, NirType::Bool),
            body: vec![
                assign("cf", lt("value", "limit")),
                HirStmt::If {
                    cond: var("cf"),
                    then_body: Vec::new(),
                    else_body: Vec::new(),
                },
            ],
        };

        let transfer = LivenessTransfer::for_stmt(&stmt);
        assert!(!transfer.uses_before_definition().any(|name| name == "cf"));
    }

    #[test]
    fn structured_use_before_definition_remains_live_in() {
        let stmt = HirStmt::While {
            cond: HirExpr::Const(1, NirType::Bool),
            body: vec![
                HirStmt::If {
                    cond: var("cf"),
                    then_body: Vec::new(),
                    else_body: Vec::new(),
                },
                assign("cf", lt("value", "limit")),
            ],
        };

        let transfer = LivenessTransfer::for_stmt(&stmt);
        assert!(transfer.uses_before_definition().any(|name| name == "cf"));
    }
}
