use crate::prelude::*;
use crate::{HashMap, HashSet};

pub(super) struct LiveRangeCollector {
    pub(super) stmt_counter: usize,
    pub(super) ranges: HashMap<String, (usize, usize)>,
    pub(super) reads: HashMap<String, HashSet<usize>>,
    pub(super) writes: HashMap<String, HashSet<usize>>,
    pub(super) labels: HashMap<String, usize>,
    pub(super) backedges: Vec<(usize, usize)>,
    pub(super) control_intervals: Vec<(usize, usize)>,
}

impl LiveRangeCollector {
    pub(super) fn visit_stmts(&mut self, stmts: &[PreHirStmt]) {
        for stmt in stmts {
            self.stmt_counter += 1;
            self.visit_stmt(stmt);
        }
    }

    fn visit_stmt(&mut self, stmt: &PreHirStmt) {
        match stmt {
            PreHirStmt::Assign { lhs, rhs } => {
                self.visit_lvalue(lhs);
                self.visit_expr(rhs);
            }
            PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => {
                self.visit_expr(expr);
            }
            PreHirStmt::Block(body) => {
                self.visit_stmts(body);
            }
            PreHirStmt::While { cond, body } => {
                let loop_start = self.stmt_counter;
                self.visit_expr(cond);
                self.visit_stmts(body);
                let loop_end = self.stmt_counter;
                self.control_intervals.push((loop_start, loop_end));
                self.extend_loop_ranges(loop_start, loop_end);
            }
            PreHirStmt::DoWhile { body, cond } => {
                let loop_start = self.stmt_counter;
                self.visit_stmts(body);
                self.visit_expr(cond);
                let loop_end = self.stmt_counter;
                self.control_intervals.push((loop_start, loop_end));
                self.extend_loop_ranges(loop_start, loop_end);
            }
            PreHirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                let loop_start = self.stmt_counter;
                if let Some(init_stmt) = init {
                    self.visit_stmt(init_stmt);
                }
                if let Some(cond_expr) = cond {
                    self.visit_expr(cond_expr);
                }
                if let Some(update_stmt) = update {
                    self.visit_stmt(update_stmt);
                }
                self.visit_stmts(body);
                let loop_end = self.stmt_counter;
                self.control_intervals.push((loop_start, loop_end));
                self.extend_loop_ranges(loop_start, loop_end);
            }
            PreHirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                let switch_start = self.stmt_counter;
                self.visit_expr(expr);
                for case in cases {
                    self.visit_stmts(&case.body);
                }
                self.visit_stmts(default);
                let switch_end = self.stmt_counter;
                self.control_intervals.push((switch_start, switch_end));
            }
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                self.visit_expr(cond);
                self.visit_stmts(then_body);
                self.visit_stmts(else_body);
            }
            PreHirStmt::VaStart { va_list, .. } => {
                self.visit_expr(va_list);
            }
            PreHirStmt::Label(name) => {
                self.labels.insert(name.clone(), self.stmt_counter);
            }
            PreHirStmt::Goto(name) => {
                if let Some(&label_counter) = self.labels.get(name) {
                    if label_counter < self.stmt_counter {
                        self.backedges.push((label_counter, self.stmt_counter));
                    }
                }
            }
            _ => {}
        }
    }

    fn visit_lvalue(&mut self, lval: &PreHirLValue) {
        match lval {
            PreHirLValue::Var(name) => {
                self.mark_write(name);
            }
            PreHirLValue::Deref { ptr, .. } => {
                self.visit_expr(ptr);
            }
            PreHirLValue::Index { base, index, .. } => {
                self.visit_expr(base);
                self.visit_expr(index);
            }
            PreHirLValue::FieldAccess { base, .. } => {
                self.visit_expr(base);
            }
        }
    }

    fn visit_expr(&mut self, expr: &PreHirExpr) {
        match expr {
            PreHirExpr::Var(name) => {
                self.mark_read(name);
            }
            PreHirExpr::Cast { expr: inner, .. }
            | PreHirExpr::Unary { expr: inner, .. }
            | PreHirExpr::Load { ptr: inner, .. }
            | PreHirExpr::PtrOffset { base: inner, .. }
            | PreHirExpr::AggregateCopy { src: inner, .. }
            | PreHirExpr::FieldAccess { base: inner, .. } => {
                self.visit_expr(inner);
            }
            PreHirExpr::Binary { lhs, rhs, .. } => {
                self.visit_expr(lhs);
                self.visit_expr(rhs);
            }
            PreHirExpr::Call { args, .. } => {
                for arg in args {
                    self.visit_expr(arg);
                }
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
            PreHirExpr::Index { base, index, .. } => {
                self.visit_expr(base);
                self.visit_expr(index);
            }
            PreHirExpr::AddressOfGlobal(_)
            | PreHirExpr::AddressOfLocal(_)
            | PreHirExpr::Const(_, _) => {}
        }
    }

    pub(super) fn extend_loop_ranges(&mut self, loop_start: usize, loop_end: usize) {
        let loop_carried = self
            .reads
            .iter()
            .filter_map(|(name, reads)| {
                let writes = self.writes.get(name)?;
                let read_in_loop = reads
                    .iter()
                    .any(|&idx| (loop_start..=loop_end).contains(&idx));
                let write_in_loop = writes
                    .iter()
                    .any(|&idx| (loop_start..=loop_end).contains(&idx));
                (read_in_loop && write_in_loop).then(|| name.clone())
            })
            .collect::<HashSet<_>>();

        for (name, range) in &mut self.ranges {
            if loop_carried.contains(name) {
                range.0 = range.0.min(loop_start);
                range.1 = range.1.max(loop_end);
            } else if range.0 < loop_start && range.1 >= loop_start {
                range.1 = range.1.max(loop_end);
            }
        }
    }

    fn mark_read(&mut self, name: &str) {
        self.mark_seen(name);
        self.reads
            .entry(name.to_string())
            .or_default()
            .insert(self.stmt_counter);
    }

    fn mark_write(&mut self, name: &str) {
        self.mark_seen(name);
        self.writes
            .entry(name.to_string())
            .or_default()
            .insert(self.stmt_counter);
    }

    fn mark_seen(&mut self, name: &str) {
        let idx = self.stmt_counter;
        let entry = self.ranges.entry(name.to_string()).or_insert((idx, idx));
        entry.1 = idx;
    }
}
