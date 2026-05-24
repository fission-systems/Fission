use super::super::*;// For accessing normalizer structures
use crate::nir::var_rename::rename_vars_in_stmts;
use std::collections::HashMap;

pub(crate) fn apply_variable_merge_pass(func: &mut HirFunction) -> bool {
    let mut changed = false;

    // Step 1: Collect lexical live ranges of variables
    let mut collector = LiveRangeCollector {
        stmt_counter: 0,
        ranges: HashMap::new(),
    };
    collector.visit_stmts(&func.body);

    // Keep track of parameters to avoid merging them
    let param_names: std::collections::HashSet<String> = func
        .params
        .iter()
        .map(|p| p.name.clone())
        .collect();

    // Group local variables by their exact type
    // Since NirType does not implement Hash, we use a Vec of pairs and check equality using PartialEq.
    let mut type_groups: Vec<(NirType, Vec<VarMeta>)> = Vec::new();
    for local in &func.locals {
        if param_names.contains(&local.name) {
            continue;
        }
        if local.initializer.is_some() {
            continue;
        }
        if let Some(&(start, end)) = collector.ranges.get(&local.name) {
            let meta = VarMeta {
                name: local.name.clone(),
                start,
                end,
            };
            if let Some(pos) = type_groups.iter().position(|(t, _)| *t == local.ty) {
                type_groups[pos].1.push(meta);
            } else {
                type_groups.push((local.ty.clone(), vec![meta]));
            }
        }
    }

    let mut renames = Vec::new();

    // Step 2: Speculatively merge variables within each type group
    for (_ty, vars) in type_groups.iter_mut() {
        // Sort by first seen index to make linear checks simpler
        vars.sort_by_key(|v| v.start);

        let mut merged = vec![false; vars.len()];
        for i in 0..vars.len() {
            if merged[i] {
                continue;
            }
            for j in (i + 1)..vars.len() {
                if merged[j] {
                    continue;
                }
                let v1 = &vars[i];
                let v2 = &vars[j];

                // Check if live ranges are disjoint: v1.end < v2.start (since sorted, v1.start <= v2.start)
                if v1.end < v2.start {
                    // Decide which name to preserve based on descriptive priority
                    let p1 = name_priority(&v1.name);
                    let p2 = name_priority(&v2.name);

                    if p1 >= p2 {
                        // Merge v2 into v1 (rename v2 to v1)
                        renames.push((v2.name.clone(), v1.name.clone()));
                        // Update v1's live range to cover the union of both ranges
                        vars[i].end = vars[j].end;
                        merged[j] = true;
                    } else {
                        // Merge v1 into v2 (rename v1 to v2)
                        renames.push((v1.name.clone(), v2.name.clone()));
                        // Update v2's live range to cover the union
                        vars[j].start = vars[i].start;
                        // Since v1 is merged into v2, update vars[i] to be v2's identity
                        vars[i] = vars[j].clone();
                        merged[j] = true;
                    }
                    changed = true;
                }
            }
        }
    }

    // Step 3: Apply the accumulated variable renames in-place to the function body
    if !renames.is_empty() {
        rename_vars_in_stmts(&mut func.body, &renames);

        // Keep only variables that were not merged into another variable
        let renamed_from: std::collections::HashSet<String> = renames
            .iter()
            .map(|(from, _)| from.clone())
            .collect();
        func.locals.retain(|local| !renamed_from.contains(&local.name));
    }

    changed
}

#[derive(Clone, Debug)]
struct VarMeta {
    name: String,
    start: usize,
    end: usize,
}

struct LiveRangeCollector {
    stmt_counter: usize,
    ranges: HashMap<String, (usize, usize)>,
}

impl LiveRangeCollector {
    fn visit_stmts(&mut self, stmts: &[HirStmt]) {
        for stmt in stmts {
            self.stmt_counter += 1;
            self.visit_stmt(stmt);
        }
    }

    fn visit_stmt(&mut self, stmt: &HirStmt) {
        match stmt {
            HirStmt::Assign { lhs, rhs } => {
                self.visit_lvalue(lhs);
                self.visit_expr(rhs);
            }
            HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => {
                self.visit_expr(expr);
            }
            HirStmt::Block(body) => {
                self.visit_stmts(body);
            }
            HirStmt::While { cond, body } => {
                self.visit_expr(cond);
                self.visit_stmts(body);
            }
            HirStmt::DoWhile { body, cond } => {
                self.visit_stmts(body);
                self.visit_expr(cond);
            }
            HirStmt::For { init, cond, update, body } => {
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
            }
            HirStmt::Switch { expr, cases, default } => {
                self.visit_expr(expr);
                for case in cases {
                    self.visit_stmts(&case.body);
                }
                self.visit_stmts(default);
            }
            HirStmt::If { cond, then_body, else_body } => {
                self.visit_expr(cond);
                self.visit_stmts(then_body);
                self.visit_stmts(else_body);
            }
            HirStmt::VaStart { va_list, .. } => {
                self.visit_expr(va_list);
            }
            _ => {}
        }
    }

    fn visit_lvalue(&mut self, lval: &HirLValue) {
        match lval {
            HirLValue::Var(name) => {
                self.mark_seen(name);
            }
            HirLValue::Deref { ptr, .. } => {
                self.visit_expr(ptr);
            }
            HirLValue::Index { base, index, .. } => {
                self.visit_expr(base);
                self.visit_expr(index);
            }
        }
    }

    fn visit_expr(&mut self, expr: &HirExpr) {
        match expr {
            HirExpr::Var(name) => {
                self.mark_seen(name);
            }
            HirExpr::Cast { expr: inner, .. }
            | HirExpr::Unary { expr: inner, .. }
            | HirExpr::Load { ptr: inner, .. }
            | HirExpr::PtrOffset { base: inner, .. }
            | HirExpr::AggregateCopy { src: inner, .. } => {
                self.visit_expr(inner);
            }
            HirExpr::Binary { lhs, rhs, .. } => {
                self.visit_expr(lhs);
                self.visit_expr(rhs);
            }
            HirExpr::Call { args, .. } => {
                for arg in args {
                    self.visit_expr(arg);
                }
            }
            HirExpr::Select { cond, then_expr, else_expr, .. } => {
                self.visit_expr(cond);
                self.visit_expr(then_expr);
                self.visit_expr(else_expr);
            }
            HirExpr::Index { base, index, .. } => {
                self.visit_expr(base);
                self.visit_expr(index);
            }
            HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => {}
        }
    }

    fn mark_seen(&mut self, name: &str) {
        let idx = self.stmt_counter;
        let entry = self.ranges.entry(name.to_string()).or_insert((idx, idx));
        entry.1 = idx;
    }
}

fn name_priority(name: &str) -> usize {
    if name.starts_with("uVar_dp_") {
        return 0; // lowest priority (dp temp variables)
    }
    if name.starts_with("uVar") || name.starts_with("iVar") || name.starts_with("xVar") || name.starts_with("bVar") {
        return 1;
    }
    if name.starts_with("slot_") {
        return 1;
    }
    if name == "result" || name == "retval" {
        return 2;
    }
    3 // highest priority: meaningful recovered symbols
}
