//! Composable backward-liveness summaries for structured HIR statements.

use crate::analysis::defuse::collect_expr_vars;
use fission_midend_prehir::{PreHirExpr, PreHirLValue, PreHirStmt};
use crate::HashSet;

/// Transfer summary for `live_in = uses_before_definition U (live_out - must_definitions)`.
///
/// The fields remain private so callers cannot manufacture a proof by combining
/// unrelated name sets. Summaries are built from HIR structure and composed in
/// execution order.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LivenessTransfer {
    uses_before_definition: HashSet<String>,
    must_definitions: HashSet<String>,
    may_diverge: bool,
}

impl LivenessTransfer {
    pub fn for_stmt(stmt: &PreHirStmt) -> Self {
        match stmt {
            PreHirStmt::Assign { lhs, rhs } => {
                let mut uses = HashSet::default();
                collect_lvalue_reads(lhs, &mut uses);
                collect_expr_vars(rhs, &mut uses);
                let must_definitions = match lhs {
                    PreHirLValue::Var(name) => [name.clone()].into_iter().collect::<HashSet<_>>(),
                    _ => HashSet::default(),
                };
                Self {
                    uses_before_definition: uses,
                    must_definitions,
                    may_diverge: false,
                }
            }
            PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => {
                let mut uses = HashSet::default();
                collect_expr_vars(expr, &mut uses);
                Self {
                    uses_before_definition: uses,
                    may_diverge: matches!(stmt, PreHirStmt::Return(_)),
                    ..Self::default()
                }
            }
            PreHirStmt::VaStart { va_list, .. } => {
                let mut uses = HashSet::default();
                collect_expr_vars(va_list, &mut uses);
                Self {
                    uses_before_definition: uses,
                    ..Self::default()
                }
            }
            PreHirStmt::Return(None) | PreHirStmt::Goto(_) | PreHirStmt::Break | PreHirStmt::Continue => Self {
                may_diverge: true,
                ..Self::default()
            },
            PreHirStmt::Label(_) => Self::default(),
            PreHirStmt::Block(body) => Self::for_stmts(body),
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                let mut uses = HashSet::default();
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
            PreHirStmt::While { cond, body } => {
                let mut uses = HashSet::default();
                collect_expr_vars(cond, &mut uses);
                let body_transfer = Self::for_stmts(body);
                uses.extend(body_transfer.uses_before_definition);
                Self {
                    uses_before_definition: uses,
                    must_definitions: HashSet::default(),
                    may_diverge: body_transfer.may_diverge,
                }
            }
            PreHirStmt::DoWhile { body, cond } => {
                let body_transfer = Self::for_stmts(body);
                let mut cond_uses = HashSet::default();
                collect_expr_vars(cond, &mut cond_uses);
                body_transfer.then(Self {
                    uses_before_definition: cond_uses,
                    must_definitions: HashSet::default(),
                    may_diverge: false,
                })
            }
            PreHirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                let init_transfer = init.as_deref().map(Self::for_stmt).unwrap_or_default();
                let mut loop_uses = HashSet::default();
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
                    must_definitions: HashSet::default(),
                    may_diverge: body_transfer.may_diverge,
                })
            }
            PreHirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                let mut uses = HashSet::default();
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

    pub fn for_stmts(stmts: &[PreHirStmt]) -> Self {
        stmts
            .iter()
            .map(Self::for_stmt)
            .fold(Self::default(), Self::then)
    }

    pub fn uses_before_definition(&self) -> impl Iterator<Item = &str> {
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
            HashSet::default()
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

fn collect_lvalue_reads(lhs: &PreHirLValue, out: &mut HashSet<String>) {
    match lhs {
        PreHirLValue::Var(_) => {}
        PreHirLValue::Deref { ptr, .. } => collect_expr_vars(ptr, out),
        PreHirLValue::Index { base, index, .. } => {
            collect_expr_vars(base, out);
            collect_expr_vars(index, out);
        }
        PreHirLValue::FieldAccess { base, .. } => collect_expr_vars(base, out),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
// prelude via parent
    use fission_midend_core::NirType;
    use fission_midend_prehir::PreHirBinaryOp;

    fn var(name: &str) -> PreHirExpr {
        PreHirExpr::Var(name.to_string())
    }

    fn assign(name: &str, rhs: PreHirExpr) -> PreHirStmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name.to_string()),
            rhs,
        }
    }

    fn lt(lhs: &str, rhs: &str) -> PreHirExpr {
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Lt,
            lhs: Box::new(var(lhs)),
            rhs: Box::new(var(rhs)),
            ty: NirType::Bool,
        }
    }

    #[test]
    fn inner_definition_hides_structured_use_from_entry() {
        let stmt = PreHirStmt::While {
            cond: PreHirExpr::Const(1, NirType::Bool),
            body: vec![
                assign("cf", lt("value", "limit")),
                PreHirStmt::If {
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
        let stmt = PreHirStmt::While {
            cond: PreHirExpr::Const(1, NirType::Bool),
            body: vec![
                PreHirStmt::If {
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
