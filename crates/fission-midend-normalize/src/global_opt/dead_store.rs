use crate::prelude::*;
/// Dead store elimination using Memory SSA.
///
/// Removes `HirStmt::Assign { lhs: Deref { .. } | Index { .. }, rhs }` nodes
/// that write to a stack slot which is:
/// 1. Never read after the write (use_count == 0 in MemSSA), AND
/// 2. No `MemPhi` depends on this def, AND
/// 3. The stack slot address does not escape to a function call.
///
/// This is the "no-escape stack slot" variant of dead store elimination,
/// which is conservative and sound: only slots provably local to this
/// function frame are eligible for removal.
///
/// ## Integration
///
/// Called after `apply_ptr_arith_recovery_pass` so that `Deref`/`PtrOffset`
/// patterns are already normalised, and before `apply_aggregate_fields_pass`.
///
/// References:
/// - LLVM `DeadStoreElimination.cpp`
/// - RetDec `reaching_definitions.h`: UD/DU chain based DSE
use super::mem_ssa::{AliasKey, MemDef, build_mem_ssa};

/// Apply dead store elimination and return `true` if any stores were removed.
pub fn apply_dead_store_elimination(func: &mut HirFunction) -> bool {
    let mem_ssa = build_mem_ssa(func);

    // Collect the def ids that are eligible for removal:
    // - use_count == 0 (no loads observe this store)
    // - key is a stack slot (no escape possible)
    // - no MemPhi depends on this def
    let phi_inputs: crate::HashSet<usize> = mem_ssa
        .phis
        .iter()
        .flat_map(|p| p.inputs.iter().copied())
        .collect();

    let dead_def_ids: crate::HashSet<usize> = mem_ssa
        .defs
        .iter()
        .filter(|def| {
            def.use_count == 0
                && !def.may_escape
                && !phi_inputs.contains(&def.id)
                && matches!(&def.key, AliasKey::Partition(partition) if partition.is_promotable_stack_like())
        })
        .map(|def| def.id)
        .collect();

    if dead_def_ids.is_empty() {
        return false;
    }

    // We need to map def IDs back to statement positions.
    // Rebuild a linear scan to collect statement indices of dead stores.
    let mut collector = DeadStoreCollector {
        dead_defs: &mem_ssa.defs,
        dead_ids: &dead_def_ids,
        current_def_idx: 0,
    };
    let stmts_to_remove = collector.collect_stmts(&func.body);

    if stmts_to_remove.is_empty() {
        return false;
    }

    remove_dead_stores(&mut func.body, &stmts_to_remove);
    true
}

/// Identifies which statement positions (by path) correspond to dead stores.
struct DeadStoreCollector<'a> {
    dead_defs: &'a [MemDef],
    dead_ids: &'a crate::HashSet<usize>,
    /// Tracks which MemDef the current store maps to (linearised scan order).
    current_def_idx: usize,
}

impl<'a> DeadStoreCollector<'a> {
    fn collect_stmts(&mut self, stmts: &[HirStmt]) -> Vec<StmtPath> {
        let mut result = Vec::new();
        for (i, stmt) in stmts.iter().enumerate() {
            self.collect_stmt(stmt, vec![i], &mut result);
        }
        result
    }

    fn collect_stmt(&mut self, stmt: &HirStmt, path: Vec<usize>, out: &mut Vec<StmtPath>) {
        match stmt {
            HirStmt::Assign { lhs, .. } => {
                let is_mem_write = matches!(lhs, HirLValue::Deref { .. } | HirLValue::Index { .. });
                if is_mem_write {
                    // Find the matching MemDef by scanning in order.
                    while self.current_def_idx < self.dead_defs.len()
                        && !matches!(
                            &self.dead_defs[self.current_def_idx].key,
                            AliasKey::Partition(_) | AliasKey::Unknown
                        )
                    {
                        self.current_def_idx += 1;
                    }
                    if self.current_def_idx < self.dead_defs.len() {
                        let def = &self.dead_defs[self.current_def_idx];
                        if self.dead_ids.contains(&def.id) {
                            out.push(StmtPath(path.clone()));
                        }
                        self.current_def_idx += 1;
                    }
                }
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                let sub = self.collect_sub(then_body, path.clone(), 0);
                out.extend(sub);
                let sub = self.collect_sub(else_body, path.clone(), 1);
                out.extend(sub);
            }
            HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
                let sub = self.collect_sub(body, path.clone(), 0);
                out.extend(sub);
            }
            HirStmt::For {
                init, body, update, ..
            } => {
                if let Some(s) = init {
                    self.collect_stmt(
                        s,
                        {
                            let mut p = path.clone();
                            p.push(0);
                            p
                        },
                        out,
                    );
                }
                let sub = self.collect_sub(body, path.clone(), 1);
                out.extend(sub);
                if let Some(s) = update {
                    self.collect_stmt(
                        s,
                        {
                            let mut p = path.clone();
                            p.push(2);
                            p
                        },
                        out,
                    );
                }
            }
            HirStmt::Switch { cases, default, .. } => {
                for (i, case) in cases.iter().enumerate() {
                    let sub = self.collect_sub(&case.body, path.clone(), i);
                    out.extend(sub);
                }
                let sub = self.collect_sub(default, path.clone(), cases.len());
                out.extend(sub);
            }
            HirStmt::Block(stmts) => {
                let sub = self.collect_sub(stmts, path, 0);
                out.extend(sub);
            }
            _ => {}
        }
    }

    fn collect_sub(
        &mut self,
        stmts: &[HirStmt],
        mut path: Vec<usize>,
        branch: usize,
    ) -> Vec<StmtPath> {
        path.push(branch);
        let mut result = Vec::new();
        for (i, stmt) in stmts.iter().enumerate() {
            let mut sp = path.clone();
            sp.push(i);
            self.collect_stmt(stmt, sp, &mut result);
        }
        result
    }
}

/// A path to a statement in the nested HIR tree.
#[derive(Debug, Clone)]
struct StmtPath(Vec<usize>);

/// Remove statements at the given paths from the nested body.
///
/// We use a simple approach: rebuild each statement list, skipping
/// the statements marked for removal.
fn remove_dead_stores(stmts: &mut Vec<HirStmt>, paths: &[StmtPath]) {
    // Collect top-level indices to remove.
    let top_level: crate::HashSet<usize> = paths
        .iter()
        .filter(|p| p.0.len() == 1)
        .map(|p| p.0[0])
        .collect();

    if !top_level.is_empty() {
        let mut i = 0;
        let mut original_idx = 0;
        while i < stmts.len() {
            if top_level.contains(&original_idx) {
                stmts.remove(i);
            } else {
                i += 1;
            }
            original_idx += 1;
        }
    }

    // Recurse for deeper paths.
    let deeper: Vec<&StmtPath> = paths.iter().filter(|p| p.0.len() > 1).collect();
    if deeper.is_empty() {
        return;
    }

    for stmt in stmts.iter_mut() {
        recurse_remove(stmt, &deeper, 1);
    }
}

fn recurse_remove(stmt: &mut HirStmt, paths: &[&StmtPath], depth: usize) {
    match stmt {
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            remove_at_branch(then_body, paths, depth, 0);
            remove_at_branch(else_body, paths, depth, 1);
        }
        HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            remove_at_branch(body, paths, depth, 0);
        }
        HirStmt::For { body, .. } => {
            remove_at_branch(body, paths, depth, 1);
        }
        HirStmt::Block(stmts) => {
            let top: crate::HashSet<usize> = paths
                .iter()
                .filter(|p| p.0.len() == depth + 1)
                .map(|p| p.0[depth])
                .collect();
            if !top.is_empty() {
                let mut i = 0;
                let mut original_idx = 0;
                while i < stmts.len() {
                    if top.contains(&original_idx) {
                        stmts.remove(i);
                    } else {
                        i += 1;
                    }
                    original_idx += 1;
                }
            }
        }
        _ => {}
    }
}

fn remove_at_branch(body: &mut Vec<HirStmt>, paths: &[&StmtPath], depth: usize, branch: usize) {
    let relevant: Vec<&StmtPath> = paths
        .iter()
        .copied()
        .filter(|p| p.0.len() > depth && p.0[depth] == branch)
        .collect();
    if relevant.is_empty() {
        return;
    }
    let next_depth = depth + 1;
    let top_level: crate::HashSet<usize> = relevant
        .iter()
        .filter(|p| p.0.len() == next_depth + 1)
        .map(|p| p.0[next_depth])
        .collect();
    if !top_level.is_empty() {
        let mut i = 0;
        let mut original_idx = 0;
        while i < body.len() {
            if top_level.contains(&original_idx) {
                body.remove(i);
            } else {
                i += 1;
            }
            original_idx += 1;
        }
    }
    for stmt in body.iter_mut() {
        recurse_remove(stmt, &relevant, next_depth);
    }
}
