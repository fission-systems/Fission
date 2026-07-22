//! Composable backward-liveness summaries for structured HIR statements.

use crate::analysis::defuse::collect_expr_vars;
use fission_midend_dir::{DirExpr, DirLValue, DirStmt};
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
    pub fn for_stmt(stmt: &DirStmt) -> Self {
        match stmt {
            DirStmt::Assign { lhs, rhs } => {
                let mut uses = HashSet::default();
                collect_lvalue_reads(lhs, &mut uses);
                collect_expr_vars(rhs, &mut uses);
                let must_definitions = match lhs {
                    DirLValue::Var(name) => [name.clone()].into_iter().collect::<HashSet<_>>(),
                    _ => HashSet::default(),
                };
                Self {
                    uses_before_definition: uses,
                    must_definitions,
                    may_diverge: false,
                }
            }
            DirStmt::Expr(expr) | DirStmt::Return(Some(expr)) => {
                let mut uses = HashSet::default();
                collect_expr_vars(expr, &mut uses);
                Self {
                    uses_before_definition: uses,
                    may_diverge: matches!(stmt, DirStmt::Return(_)),
                    ..Self::default()
                }
            }
            DirStmt::VaStart { va_list, .. } => {
                let mut uses = HashSet::default();
                collect_expr_vars(va_list, &mut uses);
                Self {
                    uses_before_definition: uses,
                    ..Self::default()
                }
            }
            DirStmt::Return(None) | DirStmt::Goto(_) | DirStmt::Break | DirStmt::Continue => Self {
                may_diverge: true,
                ..Self::default()
            },
            DirStmt::Label(_) => Self::default(),
            DirStmt::Block(body) => Self::for_stmts(body),
            DirStmt::If {
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
            DirStmt::While { cond, body } => {
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
            DirStmt::DoWhile { body, cond } => {
                let body_transfer = Self::for_stmts(body);
                let mut cond_uses = HashSet::default();
                collect_expr_vars(cond, &mut cond_uses);
                body_transfer.then(Self {
                    uses_before_definition: cond_uses,
                    must_definitions: HashSet::default(),
                    may_diverge: false,
                })
            }
            DirStmt::For {
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
            DirStmt::Switch {
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

    pub fn for_stmts(stmts: &[DirStmt]) -> Self {
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

fn collect_lvalue_reads(lhs: &DirLValue, out: &mut HashSet<String>) {
    match lhs {
        DirLValue::Var(_) => {}
        DirLValue::Deref { ptr, .. } => collect_expr_vars(ptr, out),
        DirLValue::Index { base, index, .. } => {
            collect_expr_vars(base, out);
            collect_expr_vars(index, out);
        }
        DirLValue::FieldAccess { base, .. } => collect_expr_vars(base, out),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
// prelude via parent
    use fission_midend_core::NirType;
    use fission_midend_dir::DirBinaryOp;

    fn var(name: &str) -> DirExpr {
        DirExpr::Var(name.to_string())
    }

    fn assign(name: &str, rhs: DirExpr) -> DirStmt {
        DirStmt::Assign {
            lhs: DirLValue::Var(name.to_string()),
            rhs,
        }
    }

    fn lt(lhs: &str, rhs: &str) -> DirExpr {
        DirExpr::Binary {
            op: DirBinaryOp::Lt,
            lhs: Box::new(var(lhs)),
            rhs: Box::new(var(rhs)),
            ty: NirType::Bool,
        }
    }

    #[test]
    fn inner_definition_hides_structured_use_from_entry() {
        let stmt = DirStmt::While {
            cond: DirExpr::Const(1, NirType::Bool),
            body: vec![
                assign("cf", lt("value", "limit")),
                DirStmt::If {
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
        let stmt = DirStmt::While {
            cond: DirExpr::Const(1, NirType::Bool),
            body: vec![
                DirStmt::If {
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
