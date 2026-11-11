/// Dead store elimination using Memory SSA.
///
/// Removes `PreHirStmt::Assign { lhs: Deref { .. } | Index { .. }, rhs }` nodes
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
use crate::prelude::*;

/// Apply dead store elimination and return `true` if any stores were removed.
pub fn apply_dead_store_elimination(func: &mut PreHirFunction) -> bool {
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
    fn collect_stmts(&mut self, stmts: &[PreHirStmt]) -> Vec<StmtPath> {
        let mut result = Vec::new();
        for (i, stmt) in stmts.iter().enumerate() {
            self.collect_stmt(stmt, vec![i], &mut result);
        }
        result
    }

    fn collect_stmt(&mut self, stmt: &PreHirStmt, path: Vec<usize>, out: &mut Vec<StmtPath>) {
        match stmt {
            PreHirStmt::Assign { lhs, .. } => {
                let is_mem_write = matches!(
                    lhs,
                    PreHirLValue::Deref { .. }
                        | PreHirLValue::Index { .. }
                        | PreHirLValue::FieldAccess { .. }
                );
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
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                let sub = self.collect_sub(then_body, path.clone(), 0);
                out.extend(sub);
                let sub = self.collect_sub(else_body, path.clone(), 1);
                out.extend(sub);
            }
            PreHirStmt::While { body, .. } | PreHirStmt::DoWhile { body, .. } => {
                let sub = self.collect_sub(body, path.clone(), 0);
                out.extend(sub);
            }
            PreHirStmt::For {
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
            PreHirStmt::Switch { cases, default, .. } => {
                for (i, case) in cases.iter().enumerate() {
                    let sub = self.collect_sub(&case.body, path.clone(), i);
                    out.extend(sub);
                }
                let sub = self.collect_sub(default, path.clone(), cases.len());
                out.extend(sub);
            }
            PreHirStmt::Block(stmts) => {
                let sub = self.collect_sub(stmts, path, 0);
                out.extend(sub);
            }
            _ => {}
        }
    }

    fn collect_sub(
        &mut self,
        stmts: &[PreHirStmt],
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

/// Whether `stmt` is a write through memory, the only kind of statement this
/// pass ever collects. Paths are applied to every list of the same shape, so a
/// path recorded for one store can land on a different statement in another
/// list; deleting that statement -- a label some `goto` still names, say --
/// is never what the pass set out to do, and leaving it alone costs only a
/// store that stays.
fn is_memory_store(stmt: &PreHirStmt) -> bool {
    matches!(
        stmt,
        PreHirStmt::Assign {
            lhs: PreHirLValue::Deref { .. }
                | PreHirLValue::Index { .. }
                | PreHirLValue::FieldAccess { .. },
            ..
        }
    )
}

/// A path to a statement in the nested HIR tree.
#[derive(Debug, Clone)]
struct StmtPath(Vec<usize>);

/// Remove statements at the given paths from the nested body.
///
/// We use a simple approach: rebuild each statement list, skipping
/// the statements marked for removal.
fn remove_dead_stores(stmts: &mut Vec<PreHirStmt>, paths: &[StmtPath]) {
    let top_level: crate::HashSet<usize> = paths
        .iter()
        .filter(|p| p.0.len() == 1)
        .map(|p| p.0[0])
        .collect();
    let deeper: Vec<&StmtPath> = paths.iter().filter(|p| p.0.len() > 1).collect();
    remove_listed_and_descend(stmts, &top_level, &deeper, 0);
}

/// Apply the removals recorded for one statement list.
///
/// A path names a statement by the position it had when the collector walked
/// the *original* tree, so a deeper path `[.., i, branch, ..]` belongs to the
/// statement that stood at original position `i` of this list and to no other:
/// descend into each statement with only its own paths, and do it before any
/// removal shifts the positions.
fn remove_listed_and_descend(
    body: &mut Vec<PreHirStmt>,
    remove_here: &crate::HashSet<usize>,
    deeper: &[&StmtPath],
    depth: usize,
) {
    if !deeper.is_empty() {
        for (original_idx, stmt) in body.iter_mut().enumerate() {
            let own: Vec<&StmtPath> = deeper
                .iter()
                .copied()
                .filter(|p| p.0[depth] == original_idx)
                .collect();
            if !own.is_empty() {
                recurse_remove(stmt, &own, depth + 1);
            }
        }
    }
    if !remove_here.is_empty() {
        let mut original_idx = 0;
        body.retain(|stmt| {
            let keep = !(remove_here.contains(&original_idx) && is_memory_store(stmt));
            original_idx += 1;
            keep
        });
    }
}

fn recurse_remove(stmt: &mut PreHirStmt, paths: &[&StmtPath], depth: usize) {
    match stmt {
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            remove_at_branch(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                paths,
                depth,
                0,
            );
            remove_at_branch(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                paths,
                depth,
                1,
            );
        }
        PreHirStmt::While { body, .. } | PreHirStmt::DoWhile { body, .. } => {
            remove_at_branch(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                paths,
                depth,
                0,
            );
        }
        PreHirStmt::For {
            init, body, update, ..
        } => {
            remove_at_optional_branch(init, paths, depth, 0);
            remove_at_branch(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                paths,
                depth,
                1,
            );
            remove_at_optional_branch(update, paths, depth, 2);
        }
        PreHirStmt::Switch { cases, default, .. } => {
            for (i, case) in cases.iter_mut().enumerate() {
                remove_at_branch(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                    paths,
                    depth,
                    i,
                );
            }
            remove_at_branch(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                paths,
                depth,
                cases.len(),
            );
        }
        PreHirStmt::Block(stmts) => {
            remove_at_branch(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(stmts),
                paths,
                depth,
                0,
            );
        }
        _ => {}
    }
}

/// Remove a selected statement from a single-statement `For` header, or
/// recurse into it when the collector recorded a nested path. `init` and
/// `update` use the same path branch convention as the vector-backed body, but
/// need an optional-slot adapter because they are not statement lists.
fn remove_at_optional_branch(
    slot: &mut Option<Box<PreHirStmt>>,
    paths: &[&StmtPath],
    depth: usize,
    branch: usize,
) {
    let relevant: Vec<&StmtPath> = paths
        .iter()
        .copied()
        .filter(|p| p.0.len() > depth && p.0[depth] == branch)
        .collect();
    if relevant.is_empty() {
        return;
    }
    if relevant.iter().any(|p| p.0.len() == depth + 1) {
        *slot = None;
        return;
    }
    if let Some(stmt) = slot.as_mut() {
        recurse_remove(stmt, &relevant, depth + 1);
    }
}

fn remove_at_branch(body: &mut Vec<PreHirStmt>, paths: &[&StmtPath], depth: usize, branch: usize) {
    let relevant: Vec<&StmtPath> = paths
        .iter()
        .copied()
        .filter(|p| p.0.len() > depth && p.0[depth] == branch)
        .collect();
    if relevant.is_empty() {
        return;
    }
    let next_depth = depth + 1;
    let remove_here: crate::HashSet<usize> = relevant
        .iter()
        .filter(|p| p.0.len() == next_depth + 1)
        .map(|p| p.0[next_depth])
        .collect();
    let deeper: Vec<&StmtPath> = relevant
        .iter()
        .copied()
        .filter(|p| p.0.len() > next_depth + 1)
        .collect();
    remove_listed_and_descend(body, &remove_here, &deeper, next_depth);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;

    fn ptr_binding(name: &str) -> PreHirBinding {
        PreHirBinding {
            name: name.to_string(),
            ty: NirType::Ptr(Box::new(NirType::Int {
                bits: 32,
                signed: false,
            })),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::StackOffset(-0x18)),
            initializer: None,
        }
    }

    fn int_binding(name: &str) -> PreHirBinding {
        PreHirBinding {
            name: name.to_string(),
            ty: NirType::Int {
                bits: 32,
                signed: true,
            },
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        }
    }

    fn dead_stack_store(name: &str, value: i64) -> PreHirStmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Deref {
                ptr: Box::new(PreHirExpr::Var(name.to_string())),
                ty: NirType::Int {
                    bits: 32,
                    signed: false,
                },
            },
            rhs: PreHirExpr::Const(
                value,
                NirType::Int {
                    bits: 32,
                    signed: false,
                },
            ),
        }
    }

    fn base_func(body: Vec<PreHirStmt>, locals: Vec<PreHirBinding>) -> PreHirFunction {
        PreHirFunction {
            variadic_fixed_arity: None,
            name: "f".to_string(),
            int_param_offsets: Vec::new(),
            float_param_offsets: Vec::new(),
            float_shares_int_slots: false,
            params: Vec::new(),
            locals,
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body,
            calling_convention: CallingConvention::default(),
            is_64bit: true,
            is_big_endian: false,
            suppress_entry_register_params: false,
            callee_observed_max_arity: Default::default(),
            callee_summaries: Default::default(),
        }
    }

    /// A slot whose address reaches a callee keeps its store.
    ///
    /// This pass documented an escape check it never performed: the set of
    /// escaping names was collected and never consulted, so `may_escape` came
    /// only from the partition key's own class. Nothing exercised it, because
    /// a taken address survived as arithmetic on a register that no partition
    /// key matched -- no store was ever attributed to the slot to begin with.
    /// `&local_18` makes the attribution work, and the missing check with it.
    #[test]
    fn dead_store_elimination_keeps_a_write_whose_address_reaches_a_callee() {
        let store = PreHirStmt::Assign {
            lhs: PreHirLValue::Deref {
                ptr: Box::new(PreHirExpr::AddressOfLocal("local_18".to_string())),
                ty: NirType::Int {
                    bits: 32,
                    signed: true,
                },
            },
            rhs: PreHirExpr::Const(
                7,
                NirType::Int {
                    bits: 32,
                    signed: true,
                },
            ),
        };
        let call = PreHirStmt::Expr(PreHirExpr::Call {
            origin: None,
            target: "callee".to_string(),
            args: vec![PreHirExpr::AddressOfLocal("local_18".to_string())],
            ty: NirType::Unknown,
        });
        let mut func = base_func(vec![store, call], vec![ptr_binding("local_18")]);
        apply_dead_store_elimination(&mut func);
        assert_eq!(
            func.body.len(),
            2,
            "the write must survive -- the callee reads it:\n{:?}",
            func.body
        );
    }

    /// A `local_XX`-named binding used as a Deref base plus a runtime-
    /// scaled index (`local_18 + i * 4`) is exactly the shape a spilled
    /// VLA base pointer produces -- confirmed via a real `int arr[n]`
    /// (genuinely dynamic `n`) fixture where this pattern's write was
    /// silently dropped as a "safely removable" write to "the stack slot
    /// `local_18`", when `local_18` actually just *holds* a pointer to an
    /// entirely different, unbounded region. With no read anywhere in the
    /// function (the worst case for this bug -- previously guaranteed
    /// elimination), the store must still survive.
    #[test]
    fn dead_store_elimination_keeps_write_through_stack_spilled_pointer_with_runtime_index() {
        let index_expr = PreHirExpr::Binary {
            op: PreHirBinaryOp::Mul,
            lhs: Box::new(PreHirExpr::Var("i".to_string())),
            rhs: Box::new(PreHirExpr::Const(
                4,
                NirType::Int {
                    bits: 32,
                    signed: true,
                },
            )),
            ty: NirType::Int {
                bits: 32,
                signed: true,
            },
        };
        let addr_expr = PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs: Box::new(PreHirExpr::Var("local_18".to_string())),
            rhs: Box::new(index_expr),
            ty: NirType::Ptr(Box::new(NirType::Int {
                bits: 32,
                signed: false,
            })),
        };
        let mut func = base_func(
            vec![PreHirStmt::Assign {
                lhs: PreHirLValue::Deref {
                    ptr: Box::new(addr_expr),
                    ty: NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                },
                rhs: PreHirExpr::Const(
                    1,
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                ),
            }],
            vec![ptr_binding("local_18"), int_binding("i")],
        );

        let changed = apply_dead_store_elimination(&mut func);

        assert!(
            !changed,
            "write through a stack-spilled pointer must never be treated \
             as a removable dead store to its own stack slot"
        );
        assert_eq!(
            func.body.len(),
            1,
            "the store must survive: {:?}",
            func.body
        );
    }

    /// After ptr_arith, `*(local_18 + i * 4)` becomes `local_18[i]`.
    /// MemSSA must not key that as a write to the stack slot that only
    /// *holds* the pointer — otherwise matrix out-param stores vanish.
    #[test]
    fn dead_store_elimination_keeps_field_store_through_stack_spilled_pointer() {
        // `local_18` holds a pointer (the spilled parameter); the store writes
        // its pointee, not the stack slot `local_18` lives in.
        let mut func = base_func(
            vec![PreHirStmt::Assign {
                lhs: PreHirLValue::FieldAccess {
                    base: Box::new(PreHirExpr::Var("local_18".to_string())),
                    field_name: "field_1a04".to_string(),
                    offset: 6660,
                    ty: NirType::Int {
                        bits: 8,
                        signed: false,
                    },
                },
                rhs: PreHirExpr::Const(
                    1,
                    NirType::Int {
                        bits: 8,
                        signed: false,
                    },
                ),
            }],
            vec![ptr_binding("local_18")],
        );

        let changed = apply_dead_store_elimination(&mut func);

        assert!(
            !changed,
            "a field store through a stack-spilled pointer must not be DSE'd: {:?}",
            func.body
        );
        assert_eq!(func.body.len(), 1, "{:?}", func.body);
    }

    #[test]
    fn dead_store_elimination_keeps_index_store_through_stack_spilled_pointer() {
        let mut func = base_func(
            vec![PreHirStmt::Assign {
                lhs: PreHirLValue::Index {
                    base: Box::new(PreHirExpr::Var("local_18".to_string())),
                    index: Box::new(PreHirExpr::Var("i".to_string())),
                    elem_ty: NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                },
                rhs: PreHirExpr::Const(
                    1,
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                ),
            }],
            vec![ptr_binding("local_18"), int_binding("i")],
        );

        let changed = apply_dead_store_elimination(&mut func);

        assert!(
            !changed,
            "Index store through a stack-spilled pointer must not be DSE'd: {:?}",
            func.body
        );
        assert_eq!(func.body.len(), 1, "{:?}", func.body);
    }

    /// Baseline: a genuine, provably-dead write to an *ordinary* stack
    /// local (no runtime index at all) must still be eliminated -- the
    /// fix above must not blunt this existing, legitimate optimization.
    #[test]
    fn dead_store_elimination_still_removes_genuinely_dead_local_write() {
        let mut func = base_func(
            vec![PreHirStmt::Assign {
                lhs: PreHirLValue::Deref {
                    ptr: Box::new(PreHirExpr::Var("local_10".to_string())),
                    ty: NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                },
                rhs: PreHirExpr::Const(
                    5,
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                ),
            }],
            vec![int_binding("local_10")],
        );

        let changed = apply_dead_store_elimination(&mut func);

        assert!(
            changed,
            "a provably-dead, never-read local write should still be removed"
        );
        assert!(func.body.is_empty(), "{:?}", func.body);
    }

    #[test]
    fn dead_store_elimination_removes_for_init_and_update_writes() {
        let mut func = base_func(
            vec![PreHirStmt::For {
                init: Some(Box::new(dead_stack_store("local_init", 1))),
                cond: Some(PreHirExpr::Const(1, NirType::Bool)),
                update: Some(Box::new(dead_stack_store("local_update", 2))),
                body: Rc::new(Vec::new()),
            }],
            vec![int_binding("local_init"), int_binding("local_update")],
        );

        let changed = apply_dead_store_elimination(&mut func);

        assert!(changed, "dead For-header stores should be removed");
        let PreHirStmt::For { init, update, .. } = &func.body[0] else {
            panic!("expected For statement, got {:?}", func.body);
        };
        assert!(init.is_none(), "dead For init should be removed");
        assert!(update.is_none(), "dead For update should be removed");
    }

    #[test]
    fn dead_store_elimination_removes_dead_store_in_switch() {
        let mut func = base_func(
            vec![PreHirStmt::Switch {
                expr: PreHirExpr::Var("sel".to_string()),
                cases: vec![PreHirSwitchCase {
                    values: vec![1],
                    body: Rc::new(vec![dead_stack_store("local_sw", 42)]),
                }],
                default: Rc::new(vec![]),
            }],
            vec![int_binding("local_sw")],
        );

        let changed = apply_dead_store_elimination(&mut func);
        assert!(changed, "dead store in switch case should be removed");
        let PreHirStmt::Switch { cases, .. } = &func.body[0] else {
            panic!();
        };
        assert!(cases[0].body.is_empty());
    }

    #[test]
    fn review_may_alias_store_does_not_kill_reaching_definition() {
        let ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let addr = PreHirExpr::Var("local_18".into());
        let store = PreHirStmt::Assign {
            lhs: PreHirLValue::Deref {
                ptr: Box::new(addr.clone()),
                ty: ty.clone(),
            },
            rhs: PreHirExpr::Const(7, ty.clone()),
        };
        let unknown_store = PreHirStmt::Assign {
            lhs: PreHirLValue::Deref {
                ptr: Box::new(PreHirExpr::Var("p".into())),
                ty: ty.clone(),
            },
            rhs: PreHirExpr::Const(9, ty.clone()),
        };
        let read = PreHirStmt::Return(Some(PreHirExpr::Load {
            ptr: Box::new(addr),
            ty,
        }));
        let mut func = base_func(
            vec![store.clone(), unknown_store, read],
            vec![ptr_binding("local_18")],
        );
        apply_dead_store_elimination(&mut func);
        assert_eq!(
            func.body.first(),
            Some(&store),
            "a may-alias store need not overwrite local_18; return must still read 7 when p differs"
        );
    }
    fn u32_ty() -> NirType {
        NirType::Int {
            bits: 32,
            signed: false,
        }
    }

    fn slot_store(value: i64) -> PreHirStmt {
        dead_stack_store("local_10", value)
    }

    fn slot_load_into(var: &str) -> PreHirStmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(var.to_string()),
            rhs: PreHirExpr::Load {
                ptr: Box::new(PreHirExpr::Var("local_10".to_string())),
                ty: u32_ty(),
            },
        }
    }

    fn locals_for_loop_tests() -> Vec<PreHirBinding> {
        vec![int_binding("local_10"), int_binding("v")]
    }

    /// Positive control for the loop test below: without it, "kept" could just
    /// mean the pass removes nothing in this shape.
    #[test]
    fn control_an_overwritten_store_is_removed() {
        let mut func = base_func(vec![slot_store(1), slot_store(2)], locals_for_loop_tests());
        apply_dead_store_elimination(&mut func);
        assert!(func.body.len() < 2, "{:?}", func.body);
    }

    /// `*slot = 0; while (c) { v = *slot; use(v); *slot = 5; }` -- the store in
    /// the body is read by the *next* iteration through the back edge, and by
    /// nothing after the loop. A single forward scan of the body sees that read
    /// before the store and never credits the store with it.
    #[test]
    fn dead_store_elimination_keeps_a_store_read_by_the_next_iteration() {
        let body = vec![
            slot_load_into("v"),
            PreHirStmt::Expr(PreHirExpr::Call {
                origin: None,
                target: "use".to_string(),
                args: vec![PreHirExpr::Var("v".to_string())],
                ty: NirType::Unknown,
            }),
            slot_store(5),
        ];
        let mut func = base_func(
            vec![
                slot_store(0),
                PreHirStmt::While {
                    cond: PreHirExpr::Var("c".to_string()),
                    body: Rc::new(body),
                },
            ],
            locals_for_loop_tests(),
        );
        apply_dead_store_elimination(&mut func);
        let PreHirStmt::While { body, .. } = &func.body[func.body.len() - 1] else {
            panic!("{:?}", func.body)
        };
        assert_eq!(
            body.len(),
            3,
            "the body's store is read on the next iteration:\n{:?}",
            func.body
        );
    }

    /// `if (c) { *slot = 1; } use(*slot);` -- a definition on one path only
    /// still reaches the read after the join (the other path reads the value
    /// from before the `if`, which is the entry value here).
    #[test]
    fn dead_store_elimination_keeps_a_one_sided_store_read_after_the_join() {
        let mut func = base_func(
            vec![
                PreHirStmt::If {
                    cond: PreHirExpr::Var("c".to_string()),
                    then_body: Rc::new(vec![slot_store(1)]),
                    else_body: Rc::new(Vec::new()),
                },
                slot_load_into("v"),
            ],
            locals_for_loop_tests(),
        );
        apply_dead_store_elimination(&mut func);
        let PreHirStmt::If { then_body, .. } = &func.body[0] else {
            panic!("{:?}", func.body)
        };
        assert_eq!(then_body.len(), 1, "{:?}", func.body);
    }

    /// The one-sided store is overwritten on every path before any read, so
    /// removing it is right -- the pass must not keep everything it cannot
    /// prove dead, or the checks above would hold for the wrong reason.
    #[test]
    fn control_a_one_sided_store_overwritten_before_any_read_is_removed() {
        let mut func = base_func(
            vec![
                PreHirStmt::If {
                    cond: PreHirExpr::Var("c".to_string()),
                    then_body: Rc::new(vec![slot_store(1)]),
                    else_body: Rc::new(Vec::new()),
                },
                slot_store(2),
            ],
            locals_for_loop_tests(),
        );
        apply_dead_store_elimination(&mut func);
        let PreHirStmt::If { then_body, .. } = &func.body[0] else {
            panic!("{:?}", func.body)
        };
        assert!(then_body.is_empty(), "{:?}", func.body);
    }

    fn mem_store(name: &str) -> PreHirStmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Deref {
                ptr: Box::new(PreHirExpr::Var(name.to_string())),
                ty: NirType::Int {
                    bits: 32,
                    signed: false,
                },
            },
            rhs: PreHirExpr::Const(
                0,
                NirType::Int {
                    bits: 32,
                    signed: false,
                },
            ),
        }
    }

    fn if_with(then: Vec<PreHirStmt>) -> PreHirStmt {
        PreHirStmt::If {
            cond: PreHirExpr::Var("c".to_string()),
            then_body: Rc::new(then),
            else_body: Rc::new(Vec::new()),
        }
    }

    /// A path recorded for the store in the second `if` is also tried on the
    /// first `if`'s list, where the same position holds a label some goto
    /// names. The label must survive; the store it was recorded for goes.
    #[test]
    fn a_removal_path_never_deletes_a_statement_that_is_not_a_store() {
        let mut body = vec![
            if_with(vec![
                PreHirStmt::Expr(PreHirExpr::Var("x".to_string())),
                PreHirStmt::Label("L".to_string()),
                PreHirStmt::Expr(PreHirExpr::Var("y".to_string())),
            ]),
            if_with(vec![
                PreHirStmt::Expr(PreHirExpr::Var("a".to_string())),
                mem_store("p"),
                PreHirStmt::Expr(PreHirExpr::Var("b".to_string())),
            ]),
        ];
        remove_dead_stores(&mut body, &[StmtPath(vec![1, 0, 1])]);

        let PreHirStmt::If {
            then_body: first, ..
        } = &body[0]
        else {
            panic!("expected if");
        };
        let PreHirStmt::If {
            then_body: second, ..
        } = &body[1]
        else {
            panic!("expected if");
        };
        assert!(
            first
                .iter()
                .any(|s| matches!(s, PreHirStmt::Label(l) if l == "L")),
            "the label was deleted: {first:?}"
        );
        assert_eq!(second.len(), 2, "the named store is gone: {second:?}");
    }

    #[test]
    fn a_removal_path_still_removes_the_store_it_names() {
        let mut body = vec![mem_store("p"), PreHirStmt::Return(None)];
        remove_dead_stores(&mut body, &[StmtPath(vec![0])]);
        assert_eq!(body, vec![PreHirStmt::Return(None)]);
    }

    /// A path names one statement: `[1, 0, 1]` is statement 1 of the top-level
    /// list, its then-arm, item 1. Another `if` with a *store* at the same
    /// position must keep it.
    #[test]
    fn a_nested_removal_touches_only_the_statement_its_path_names() {
        let mut body = vec![
            if_with(vec![
                PreHirStmt::Expr(PreHirExpr::Var("a1".to_string())),
                mem_store("p"),
                PreHirStmt::Expr(PreHirExpr::Var("a3".to_string())),
            ]),
            if_with(vec![
                PreHirStmt::Expr(PreHirExpr::Var("b1".to_string())),
                mem_store("q"),
                PreHirStmt::Expr(PreHirExpr::Var("b3".to_string())),
            ]),
        ];
        remove_dead_stores(&mut body, &[StmtPath(vec![1, 0, 1])]);

        let PreHirStmt::If {
            then_body: first, ..
        } = &body[0]
        else {
            panic!("expected if");
        };
        let PreHirStmt::If {
            then_body: second, ..
        } = &body[1]
        else {
            panic!("expected if");
        };
        assert_eq!(first.len(), 3, "the other if lost a store: {first:?}");
        assert_eq!(second.len(), 2, "the named store is gone: {second:?}");
    }

    /// Removing a top-level statement shifts the ones after it; a nested path
    /// still names the statement it was recorded for.
    #[test]
    fn a_nested_removal_survives_a_top_level_removal_before_it() {
        let mut body = vec![
            mem_store("dead"),
            if_with(vec![
                PreHirStmt::Expr(PreHirExpr::Var("keep".to_string())),
                mem_store("q"),
                PreHirStmt::Expr(PreHirExpr::Var("keep_too".to_string())),
            ]),
        ];
        remove_dead_stores(&mut body, &[StmtPath(vec![0]), StmtPath(vec![1, 0, 1])]);

        assert_eq!(body.len(), 1);
        let PreHirStmt::If { then_body, .. } = &body[0] else {
            panic!("expected if");
        };
        assert_eq!(then_body.len(), 2, "{then_body:?}");
    }

    #[test]
    fn dead_store_elimination_keeps_deref_store_at_offset_from_stack_spilled_pointer() {
        // `*(local_18 + 8) = 1` where `local_18` holds a pointer: the store
        // writes the pointee, not `local_18`'s own stack slot.
        let u32_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let mut func = base_func(
            vec![PreHirStmt::Assign {
                lhs: PreHirLValue::Deref {
                    ptr: Box::new(PreHirExpr::Binary {
                        op: PreHirBinaryOp::Add,
                        lhs: Box::new(PreHirExpr::Var("local_18".to_string())),
                        rhs: Box::new(PreHirExpr::Const(8, u32_ty.clone())),
                        ty: NirType::Ptr(Box::new(u32_ty.clone())),
                    }),
                    ty: u32_ty.clone(),
                },
                rhs: PreHirExpr::Const(1, u32_ty),
            }],
            vec![ptr_binding("local_18")],
        );

        let changed = apply_dead_store_elimination(&mut func);

        assert!(
            !changed,
            "a store at an offset from a stack-spilled pointer must not be DSE'd: {:?}",
            func.body
        );
        assert_eq!(func.body.len(), 1, "{:?}", func.body);
    }
}
