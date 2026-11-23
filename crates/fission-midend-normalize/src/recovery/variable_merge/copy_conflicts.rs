use super::copy_aliases::sorted_var_pair;
use crate::HashSet;
use crate::prelude::*;

struct PendingCopyMerge {
    destination: String,
    source: String,
    destination_changed: bool,
    source_changed: bool,
}

struct CopyMergeConflictCollector<'a> {
    direct_copies: &'a HashSet<(String, String)>,
    pending: Vec<PendingCopyMerge>,
    blocked_pairs: HashSet<(String, String)>,
}

impl<'a> CopyMergeConflictCollector<'a> {
    fn new(direct_copies: &'a HashSet<(String, String)>) -> Self {
        Self {
            direct_copies,
            pending: Vec::new(),
            blocked_pairs: HashSet::default(),
        }
    }

    fn visit_stmts(&mut self, stmts: &[PreHirStmt], under_control: bool) {
        for stmt in stmts {
            self.visit_stmt(stmt, under_control);
        }
    }

    fn visit_stmt(&mut self, stmt: &PreHirStmt, under_control: bool) {
        match stmt {
            PreHirStmt::Assign { lhs, rhs } => {
                self.visit_lvalue_reads(lhs);
                self.visit_expr(rhs);
                if let PreHirLValue::Var(destination) = lhs {
                    if let PreHirExpr::Var(source) = rhs
                        && !under_control
                        && self
                            .direct_copies
                            .contains(&(destination.clone(), source.clone()))
                    {
                        // A direct copy starts a merge candidate. It is still
                        // a write to the destination for older candidates.
                        self.note_write(destination);
                        self.pending.push(PendingCopyMerge {
                            destination: destination.clone(),
                            source: source.clone(),
                            destination_changed: false,
                            source_changed: false,
                        });
                    } else {
                        self.note_write(destination);
                    }
                }
            }
            PreHirStmt::VaStart { va_list, .. }
            | PreHirStmt::Expr(va_list)
            | PreHirStmt::Return(Some(va_list)) => self.visit_expr(va_list),
            PreHirStmt::Block(body) => self.visit_stmts(body, under_control),
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                self.visit_expr(cond);
                self.visit_stmts(then_body, true);
                self.visit_stmts(else_body, true);
            }
            PreHirStmt::While { cond, body } => {
                self.visit_expr(cond);
                self.visit_stmts(body, true);
            }
            PreHirStmt::DoWhile { body, cond } => {
                self.visit_stmts(body, true);
                self.visit_expr(cond);
            }
            PreHirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                if let Some(init) = init {
                    self.visit_stmt(init, under_control);
                }
                if let Some(cond) = cond {
                    self.visit_expr(cond);
                }
                if let Some(update) = update {
                    self.visit_stmt(update, true);
                }
                self.visit_stmts(body, true);
            }
            PreHirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                self.visit_expr(expr);
                for case in cases {
                    self.visit_stmts(&case.body, true);
                }
                self.visit_stmts(default, true);
            }
            PreHirStmt::Label(_)
            | PreHirStmt::Goto(_)
            | PreHirStmt::Return(None)
            | PreHirStmt::Break
            | PreHirStmt::Continue => {}
        }
    }

    fn visit_lvalue_reads(&mut self, lvalue: &PreHirLValue) {
        match lvalue {
            PreHirLValue::Var(_) => {}
            PreHirLValue::Deref { ptr, .. } => self.visit_expr(ptr),
            PreHirLValue::Index { base, index, .. } => {
                self.visit_expr(base);
                self.visit_expr(index);
            }
            PreHirLValue::FieldAccess { base, .. } => self.visit_expr(base),
        }
    }

    fn visit_expr(&mut self, expr: &PreHirExpr) {
        match expr {
            PreHirExpr::Var(name) | PreHirExpr::AddressOfLocal(name) => self.note_read(name),
            PreHirExpr::Cast { expr, .. }
            | PreHirExpr::Unary { expr, .. }
            | PreHirExpr::Load { ptr: expr, .. }
            | PreHirExpr::PtrOffset { base: expr, .. }
            | PreHirExpr::AggregateCopy { src: expr, .. }
            | PreHirExpr::FieldAccess { base: expr, .. } => self.visit_expr(expr),
            PreHirExpr::Binary { lhs, rhs, .. } => {
                self.visit_expr(lhs);
                self.visit_expr(rhs);
            }
            PreHirExpr::Call { args, .. } => {
                for arg in args {
                    self.visit_expr(arg);
                }
            }
            PreHirExpr::Index { base, index, .. } => {
                self.visit_expr(base);
                self.visit_expr(index);
            }
            PreHirExpr::Select {
                cond,
                then_expr,
                else_expr,
                ..
            } => {
                self.visit_expr(cond);
                self.visit_expr(then_expr);
                self.visit_expr(else_expr);
            }
            PreHirExpr::Const(_, _) | PreHirExpr::AddressOfGlobal(_) => {}
        }
    }

    fn note_read(&mut self, name: &str) {
        let conflicts = self
            .pending
            .iter()
            .filter(|copy| {
                (copy.destination_changed && copy.source == name)
                    || (copy.source_changed && copy.destination == name)
            })
            .map(|copy| sorted_var_pair(&copy.destination, &copy.source))
            .collect::<Vec<_>>();
        self.blocked_pairs.extend(conflicts);
    }

    fn note_write(&mut self, name: &str) {
        for copy in &mut self.pending {
            if copy.destination == name {
                copy.destination_changed = true;
            }
            if copy.source == name {
                copy.source_changed = true;
            }
        }
    }
}

pub(super) fn collect_copy_merge_conflicts(
    stmts: &[PreHirStmt],
    direct_copies: &HashSet<(String, String)>,
) -> HashSet<(String, String)> {
    let mut collector = CopyMergeConflictCollector::new(direct_copies);
    collector.visit_stmts(stmts, false);
    collector.blocked_pairs
}
