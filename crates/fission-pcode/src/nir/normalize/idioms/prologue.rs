/// Callee-saved register prologue/epilogue elimination pass.
///
/// Windows x64 and System V AMD64 require that callee-saved registers be
/// preserved across function calls.  Compilers emit save/restore sequences at
/// the start/end of every non-leaf function that uses such registers:
///
/// ```text
/// // Prologue
/// *spill_slot_ptr = r15;
/// *spill_slot_ptr2 = r14;
///
/// // Function body (uses r15, r14 for its own purposes)
/// ...
///
/// // Epilogue (each return path)
/// r15 = *spill_slot_ptr;
/// r14 = *spill_slot_ptr2;
/// return;
/// ```
///
/// These save/restore pairs are semantically transparent with respect to the
/// function's observable behavior and clutter every decompiled output.  This
/// pass detects and removes them using the following invariant:
///
/// A save/restore pair `(*p = reg, reg = *p)` can be eliminated when:
/// 1. `reg` is in the known callee-saved set for the active native ABI
///    family currently represented in HIR names.
/// 2. The spill pointer variable `p` is used **only** in those two operations:
///    the Deref-lhs assignment and the Load-rhs assignment.
/// 3. The register `reg` itself may be freely modified by the function body —
///    the only effect of removal is that we no longer track the entry value,
///    which is correct because callee-save conventions do not affect the
///    function's observable output.
///
/// Irreducible cases (any condition fails) are left unchanged.
use super::super::*;
use std::collections::{HashMap, HashSet};

/// Callee-saved register names that can appear after register naming. This
/// covers x86-64, AArch64, and ARM32 preserved GPR sets. Frame/link registers
/// are included here because compiler prologues save and restore them as part
/// of the same ABI-preserving stack scaffold.
const CALLEE_SAVED_REGS: &[&str] = &[
    "rbx", "rbp", "rsi", "rdi", "r12", "r13", "r14", "r15", "x19", "x20", "x21", "x22", "x23",
    "x24", "x25", "x26", "x27", "x28", "x29", "x30", "r4", "r5", "r6", "r7", "r8", "r9", "r10",
    "r11", "lr",
];

fn is_callee_saved(name: &str) -> bool {
    CALLEE_SAVED_REGS.contains(&name)
}

fn looks_like_stack_scaffold_name(name: &str) -> bool {
    name == "sp" || name.starts_with("var_") || name.starts_with("xVar") || name.starts_with("uVar")
}

fn stack_scaffold_ptr_expr(expr: &HirExpr) -> bool {
    match expr {
        HirExpr::Var(name) | HirExpr::AddressOfGlobal(name) => looks_like_stack_scaffold_name(name),
        HirExpr::PtrOffset { base, .. }
        | HirExpr::Cast { expr: base, .. }
        | HirExpr::Unary { expr: base, .. } => stack_scaffold_ptr_expr(base),
        HirExpr::Binary { lhs, rhs, .. } => {
            stack_scaffold_ptr_expr(lhs) || stack_scaffold_ptr_expr(rhs)
        }
        _ => false,
    }
}

fn is_entry_stack_scaffold_store(stmt: &HirStmt) -> bool {
    let HirStmt::Assign {
        lhs: HirLValue::Deref { ptr, .. },
        rhs: HirExpr::Var(_),
    } = stmt
    else {
        return false;
    };
    stack_scaffold_ptr_expr(ptr)
}

fn is_entry_stack_scaffold_alias_binding(stmt: &HirStmt) -> Option<&str> {
    let HirStmt::Assign {
        lhs: HirLValue::Var(lhs),
        rhs,
    } = stmt
    else {
        return None;
    };
    if looks_like_stack_scaffold_name(lhs) && stack_scaffold_ptr_expr(rhs) {
        return Some(lhs.as_str());
    }
    None
}

fn looks_like_stack_slot_name(name: &str) -> bool {
    name.starts_with("home_") || name.starts_with("local_") || name.starts_with("ret_scaffold_")
}

fn var_name_through_cast(expr: &HirExpr) -> Option<&str> {
    match expr {
        HirExpr::Var(name) | HirExpr::AddressOfGlobal(name) => Some(name.as_str()),
        HirExpr::Cast { expr, .. } => var_name_through_cast(expr),
        _ => None,
    }
}

fn is_entry_stack_slot_scaffold_store(stmt: &HirStmt) -> bool {
    matches!(
        stmt,
        HirStmt::Assign {
            lhs: HirLValue::Var(lhs),
            rhs,
        } if looks_like_stack_slot_name(lhs) && var_name_through_cast(rhs).is_some()
    )
}

fn is_entry_stack_slot_callee_saved_store(stmt: &HirStmt) -> bool {
    matches!(
        stmt,
        HirStmt::Assign {
            lhs: HirLValue::Var(lhs),
            rhs,
        } if looks_like_stack_slot_name(lhs)
            && var_name_through_cast(rhs).is_some_and(is_callee_saved)
    )
}

/// Remove leading stack-growth scaffold stores emitted from x86-64 prologue
/// pushes once they have survived raw p-code lowering as generic dereference
/// stores. These are only removed as a contiguous function-entry prefix and
/// only when the destination pointer is a synthetic stack scaffold name, so
/// ordinary early stores through parameters or globals are left intact.
pub(crate) fn remove_entry_stack_scaffold_stores(func: &mut HirFunction) -> bool {
    remove_entry_stack_scaffold_stores_from_body(&mut func.body)
}

fn remove_entry_stack_scaffold_stores_from_body(body: &mut Vec<HirStmt>) -> bool {
    let remove_count = body
        .iter()
        .take_while(|stmt| {
            is_entry_stack_scaffold_store(stmt)
                || is_entry_stack_slot_scaffold_store(stmt)
                || is_entry_stack_scaffold_alias_binding(stmt).is_some()
        })
        .count();
    if remove_count > 0 {
        let prefix = &body[..remove_count];
        let suffix = &body[remove_count..];
        let has_scaffold_evidence = prefix.iter().any(is_entry_stack_scaffold_store)
            || prefix.iter().any(is_entry_stack_slot_callee_saved_store);
        if !has_scaffold_evidence {
            return false;
        }
        let alias_escapes_prefix = prefix
            .iter()
            .filter_map(is_entry_stack_scaffold_alias_binding)
            .any(|alias| count_ptr_var_rvalue_uses(suffix, alias) > 0);
        if alias_escapes_prefix {
            return false;
        }
        body.drain(0..remove_count);
        return true;
    }

    if let Some(HirStmt::Block(inner)) = body.first_mut() {
        return remove_entry_stack_scaffold_stores_from_body(inner);
    }

    false
}

// ── Pattern matching ──────────────────────────────────────────────────────────

/// Attempt to match a prologue SAVE statement:
/// `*<ptr_var> = <callee_saved_reg>`
///
/// Returns `(ptr_var_name, reg_name)` on success.
fn match_prologue_save(stmt: &HirStmt) -> Option<(String, String)> {
    let HirStmt::Assign { lhs, rhs } = stmt else {
        return None;
    };
    let ptr_var = match lhs {
        HirLValue::Deref { ptr, .. } => {
            if let HirExpr::Var(v) = ptr.as_ref() {
                v.as_str()
            } else {
                return None;
            }
        }
        _ => return None,
    };
    let reg = match rhs {
        HirExpr::Var(r) if is_callee_saved(r) => r.as_str(),
        _ => return None,
    };
    Some((ptr_var.to_string(), reg.to_string()))
}

/// Attempt to match an epilogue RESTORE statement:
/// `<callee_saved_reg> = *<ptr_var>` (or Cast-wrapped variant)
///
/// Returns `(ptr_var_name, reg_name)` on success.
fn match_epilogue_restore(stmt: &HirStmt) -> Option<(String, String)> {
    let HirStmt::Assign { lhs, rhs } = stmt else {
        return None;
    };
    let reg = match lhs {
        HirLValue::Var(r) if is_callee_saved(r) => r.as_str(),
        _ => return None,
    };
    // Match `Load { ptr: Var(v) }` or `Cast { Load { ptr: Var(v) } }`.
    let ptr_var = match rhs {
        HirExpr::Load { ptr, .. } => {
            if let HirExpr::Var(v) = ptr.as_ref() {
                v.as_str()
            } else {
                return None;
            }
        }
        HirExpr::Cast { expr: inner, .. } => {
            if let HirExpr::Load { ptr, .. } = inner.as_ref() {
                if let HirExpr::Var(v) = ptr.as_ref() {
                    v.as_str()
                } else {
                    return None;
                }
            } else {
                return None;
            }
        }
        _ => return None,
    };
    Some((ptr_var.to_string(), reg.to_string()))
}

// ── Use counting ─────────────────────────────────────────────────────────────

/// Count how many times `ptr_var` appears as an Rvalue reference (i.e., as
/// `Var(ptr_var)` inside any expression, NOT counting the LHS Deref write).
fn count_ptr_var_rvalue_uses(stmts: &[HirStmt], ptr_var: &str) -> usize {
    stmts.iter().map(|s| count_ptr_in_stmt(s, ptr_var)).sum()
}

fn count_ptr_in_stmt(stmt: &HirStmt, name: &str) -> usize {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            let lhs_uses = match lhs {
                // The write itself (`*p = ...`) does NOT count as an rvalue use
                // of `p` for our purposes — we only care whether `p` is READ
                // beyond the prologue/epilogue pair.  However, the pointer load
                // `*p` in `reg = *p` is an rvalue load, counted in `rhs`.
                HirLValue::Deref { ptr, .. } => count_ptr_in_expr(ptr, name),
                HirLValue::Index { base, index, .. } => {
                    count_ptr_in_expr(base, name) + count_ptr_in_expr(index, name)
                }
                HirLValue::Var(_) => 0,
            };
            lhs_uses + count_ptr_in_expr(rhs, name)
        }
        HirStmt::Expr(e) | HirStmt::Return(Some(e)) => count_ptr_in_expr(e, name),
        HirStmt::VaStart { va_list, .. } => count_ptr_in_expr(va_list, name),
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            count_ptr_in_expr(cond, name)
                + count_ptr_var_rvalue_uses(then_body, name)
                + count_ptr_var_rvalue_uses(else_body, name)
        }
        HirStmt::While { cond, body } => {
            count_ptr_in_expr(cond, name) + count_ptr_var_rvalue_uses(body, name)
        }
        HirStmt::DoWhile { body, cond } => {
            count_ptr_var_rvalue_uses(body, name) + count_ptr_in_expr(cond, name)
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            let i = init
                .as_ref()
                .map(|s| count_ptr_in_stmt(s, name))
                .unwrap_or(0);
            let c = cond
                .as_ref()
                .map(|e| count_ptr_in_expr(e, name))
                .unwrap_or(0);
            let u = update
                .as_ref()
                .map(|s| count_ptr_in_stmt(s, name))
                .unwrap_or(0);
            i + c + u + count_ptr_var_rvalue_uses(body, name)
        }
        HirStmt::Block(body) => count_ptr_var_rvalue_uses(body, name),
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            let e = count_ptr_in_expr(expr, name);
            let c: usize = cases
                .iter()
                .map(|case| count_ptr_var_rvalue_uses(&case.body, name))
                .sum();
            let d = count_ptr_var_rvalue_uses(default, name);
            e + c + d
        }
        HirStmt::Return(None)
        | HirStmt::Break
        | HirStmt::Continue
        | HirStmt::Label(_)
        | HirStmt::Goto(_) => 0,
    }
}

fn count_ptr_in_expr(expr: &HirExpr, name: &str) -> usize {
    match expr {
        HirExpr::Var(v) | HirExpr::AddressOfGlobal(v) => usize::from(v == name),
        HirExpr::Const(_, _) => 0,
        HirExpr::Cast { expr: inner, .. }
        | HirExpr::Unary { expr: inner, .. }
        | HirExpr::Load { ptr: inner, .. }
        | HirExpr::PtrOffset { base: inner, .. }
        | HirExpr::AggregateCopy { src: inner, .. } => count_ptr_in_expr(inner, name),
        HirExpr::Binary { lhs, rhs, .. } => {
            count_ptr_in_expr(lhs, name) + count_ptr_in_expr(rhs, name)
        }
        HirExpr::Call { args, .. } => args.iter().map(|a| count_ptr_in_expr(a, name)).sum(),
        HirExpr::Index { base, index, .. } => {
            count_ptr_in_expr(base, name) + count_ptr_in_expr(index, name)
        }
    }
}

// ── Statement removal ─────────────────────────────────────────────────────────

/// Remove all statements that match the given `(ptr_var, reg)` pairs from
/// `stmts` at any nesting level (epilogues can appear inside conditional arms).
fn remove_matching_saves_restores(
    stmts: &mut Vec<HirStmt>,
    pairs: &HashMap<String, String>, // ptr_var → reg
    changed: &mut bool,
) {
    // Recurse into nested bodies.
    for stmt in stmts.iter_mut() {
        remove_nested(stmt, pairs, changed);
    }
    // Remove flat-level matches.
    stmts.retain(|stmt| {
        if let Some((ptr, _)) = match_prologue_save(stmt) {
            if pairs.contains_key(&ptr) {
                *changed = true;
                return false;
            }
        }
        if let Some((ptr, _)) = match_epilogue_restore(stmt) {
            if pairs.contains_key(&ptr) {
                *changed = true;
                return false;
            }
        }
        true
    });
}

fn remove_nested(stmt: &mut HirStmt, pairs: &HashMap<String, String>, changed: &mut bool) {
    match stmt {
        HirStmt::Block(body) => remove_matching_saves_restores(body, pairs, changed),
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            remove_matching_saves_restores(then_body, pairs, changed);
            remove_matching_saves_restores(else_body, pairs, changed);
        }
        HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            remove_matching_saves_restores(body, pairs, changed)
        }
        HirStmt::For { body, .. } => remove_matching_saves_restores(body, pairs, changed),
        HirStmt::Switch { cases, default, .. } => {
            for case in cases.iter_mut() {
                remove_matching_saves_restores(&mut case.body, pairs, changed);
            }
            remove_matching_saves_restores(default, pairs, changed);
        }
        _ => {}
    }
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Remove callee-saved register prologue/epilogue save-restore pairs from
/// `func`.  Returns `true` if any statements were removed.
pub(crate) fn remove_callee_save_prologue_epilogue(func: &mut HirFunction) -> bool {
    // ── Step 1: Discover prologue saves in the first few top-level statements.
    let max_prologue_scan = 16usize;
    let mut candidate_pairs: HashMap<String, String> = HashMap::new(); // ptr_var → reg

    for stmt in func.body.iter().take(max_prologue_scan) {
        if let Some((ptr, reg)) = match_prologue_save(stmt) {
            candidate_pairs.insert(ptr, reg);
        } else {
            // Stop scanning at the first non-save statement to avoid false
            // positives from mid-function register spills.
            break;
        }
    }

    if candidate_pairs.is_empty() {
        return false;
    }

    // ── Step 2: Validate each candidate pair.
    // A pair (ptr, reg) is valid if:
    //   a. At least one epilogue restore for (ptr, reg) exists anywhere in the body.
    //   b. The ptr variable appears exactly ONCE as an rvalue in the body
    //      (the epilogue restore's Load expression).  Any additional use means
    //      the spill slot is aliased or used for something else.
    let mut confirmed: HashMap<String, String> = HashMap::new();

    // Collect all epilogue restores anywhere in the body.
    let mut restores: HashMap<String, String> = HashMap::new(); // ptr_var → reg
    collect_restores(&func.body, &mut restores);

    for (ptr, reg) in &candidate_pairs {
        // Must have a matching restore.
        let Some(restore_reg) = restores.get(ptr) else {
            continue;
        };
        if restore_reg != reg {
            continue; // Mismatch — conservative: skip.
        }

        // The ptr variable must be used ONLY for the epilogue restore load.
        // We count all rvalue occurrences of `ptr` in the entire body;
        // it should equal exactly the number of restores for this ptr.
        let restore_count = count_restores_for_ptr(&func.body, ptr);
        let total_uses = count_ptr_var_rvalue_uses(&func.body, ptr);
        if total_uses != restore_count {
            // ptr is used beyond just the restore loads — keep the pair.
            continue;
        }

        confirmed.insert(ptr.clone(), reg.clone());
    }

    if confirmed.is_empty() {
        return false;
    }

    // ── Step 3: Remove all confirmed save and restore statements.
    let mut changed = false;
    remove_matching_saves_restores(&mut func.body, &confirmed, &mut changed);

    // ── Step 4: Remove now-unreferenced spill-slot bindings from locals.
    if changed {
        let eliminated_ptrs: HashSet<&str> = confirmed.keys().map(|s| s.as_str()).collect();
        func.locals
            .retain(|b| !eliminated_ptrs.contains(b.name.as_str()));
    }

    changed
}

// ── Helper: collect all epilogue restores ────────────────────────────────────

fn collect_restores(stmts: &[HirStmt], restores: &mut HashMap<String, String>) {
    for stmt in stmts {
        if let Some((ptr, reg)) = match_epilogue_restore(stmt) {
            restores.entry(ptr).or_insert(reg);
        }
        match stmt {
            HirStmt::Block(body) => collect_restores(body, restores),
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_restores(then_body, restores);
                collect_restores(else_body, restores);
            }
            HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
                collect_restores(body, restores)
            }
            HirStmt::For { body, .. } => collect_restores(body, restores),
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_restores(&case.body, restores);
                }
                collect_restores(default, restores);
            }
            _ => {}
        }
    }
}

fn count_restores_for_ptr(stmts: &[HirStmt], ptr: &str) -> usize {
    let mut count = 0;
    for stmt in stmts {
        if let Some((p, _)) = match_epilogue_restore(stmt) {
            if p == ptr {
                count += 1;
            }
        }
        match stmt {
            HirStmt::Block(body) => count += count_restores_for_ptr(body, ptr),
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                count += count_restores_for_ptr(then_body, ptr);
                count += count_restores_for_ptr(else_body, ptr);
            }
            HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
                count += count_restores_for_ptr(body, ptr)
            }
            HirStmt::For { body, .. } => count += count_restores_for_ptr(body, ptr),
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    count += count_restores_for_ptr(&case.body, ptr);
                }
                count += count_restores_for_ptr(default, ptr);
            }
            _ => {}
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u64_ty() -> NirType {
        NirType::Int {
            bits: 64,
            signed: false,
        }
    }

    fn u32_ty() -> NirType {
        NirType::Int {
            bits: 32,
            signed: false,
        }
    }

    fn scaffold_store(ptr: &str, rhs: &str) -> HirStmt {
        HirStmt::Assign {
            lhs: HirLValue::Deref {
                ptr: Box::new(HirExpr::PtrOffset {
                    base: Box::new(HirExpr::Var(ptr.to_owned())),
                    offset: -8,
                }),
                ty: u64_ty(),
            },
            rhs: HirExpr::Var(rhs.to_owned()),
        }
    }

    #[test]
    fn removes_contiguous_entry_stack_scaffold_stores() {
        let mut func = HirFunction {
            name: "test".to_owned(),
            body: vec![
                scaffold_store("var_20", "var_38"),
                scaffold_store("xVar0", "param_2"),
                HirStmt::Return(None),
            ],
            ..Default::default()
        };

        assert!(remove_entry_stack_scaffold_stores(&mut func));
        assert_eq!(func.body, vec![HirStmt::Return(None)]);
    }

    #[test]
    fn removes_aarch64_sp_based_entry_callee_saved_scaffold() {
        let mut func = HirFunction {
            name: "test".to_owned(),
            body: vec![
                scaffold_store("sp", "x29"),
                HirStmt::Assign {
                    lhs: HirLValue::Deref {
                        ptr: Box::new(HirExpr::PtrOffset {
                            base: Box::new(HirExpr::Var("sp".to_owned())),
                            offset: 8,
                        }),
                        ty: u64_ty(),
                    },
                    rhs: HirExpr::Var("x30".to_owned()),
                },
                HirStmt::Assign {
                    lhs: HirLValue::Deref {
                        ptr: Box::new(HirExpr::PtrOffset {
                            base: Box::new(HirExpr::Var("sp".to_owned())),
                            offset: 16,
                        }),
                        ty: u64_ty(),
                    },
                    rhs: HirExpr::Var("x20".to_owned()),
                },
                HirStmt::Return(Some(HirExpr::Var("param_1".to_owned()))),
            ],
            ..Default::default()
        };

        assert!(remove_entry_stack_scaffold_stores(&mut func));
        assert_eq!(
            func.body,
            vec![HirStmt::Return(Some(HirExpr::Var("param_1".to_owned())))]
        );
    }

    #[test]
    fn removes_aarch64_entry_stack_alias_callee_saved_scaffold() {
        let mut func = HirFunction {
            name: "test".to_owned(),
            body: vec![
                HirStmt::Assign {
                    lhs: HirLValue::Var("xVar2".to_owned()),
                    rhs: HirExpr::PtrOffset {
                        base: Box::new(HirExpr::Var("sp".to_owned())),
                        offset: 16,
                    },
                },
                HirStmt::Assign {
                    lhs: HirLValue::Deref {
                        ptr: Box::new(HirExpr::Var("xVar2".to_owned())),
                        ty: u64_ty(),
                    },
                    rhs: HirExpr::Var("x20".to_owned()),
                },
                HirStmt::Return(Some(HirExpr::Var("param_1".to_owned()))),
            ],
            ..Default::default()
        };

        assert!(remove_entry_stack_scaffold_stores(&mut func));
        assert_eq!(
            func.body,
            vec![HirStmt::Return(Some(HirExpr::Var("param_1".to_owned())))]
        );
    }

    #[test]
    fn removes_arm32_uvar_stack_alias_callee_saved_scaffold() {
        let mut func = HirFunction {
            name: "test".to_owned(),
            body: vec![
                HirStmt::Assign {
                    lhs: HirLValue::Var("uVar0".to_owned()),
                    rhs: HirExpr::Binary {
                        op: HirBinaryOp::Sub,
                        lhs: Box::new(HirExpr::Var("sp".to_owned())),
                        rhs: Box::new(HirExpr::Const(4, u32_ty())),
                        ty: u32_ty(),
                    },
                },
                HirStmt::Assign {
                    lhs: HirLValue::Deref {
                        ptr: Box::new(HirExpr::Var("uVar0".to_owned())),
                        ty: u32_ty(),
                    },
                    rhs: HirExpr::Var("lr".to_owned()),
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("uVar1".to_owned()),
                    rhs: HirExpr::Binary {
                        op: HirBinaryOp::Sub,
                        lhs: Box::new(HirExpr::Var("uVar0".to_owned())),
                        rhs: Box::new(HirExpr::Const(1, u32_ty())),
                        ty: u32_ty(),
                    },
                },
                HirStmt::Assign {
                    lhs: HirLValue::Deref {
                        ptr: Box::new(HirExpr::Var("uVar1".to_owned())),
                        ty: u32_ty(),
                    },
                    rhs: HirExpr::Var("r11".to_owned()),
                },
                HirStmt::Return(Some(HirExpr::Var("param_1".to_owned()))),
            ],
            ..Default::default()
        };

        assert!(remove_entry_stack_scaffold_stores(&mut func));
        assert_eq!(
            func.body,
            vec![HirStmt::Return(Some(HirExpr::Var("param_1".to_owned())))]
        );
    }

    #[test]
    fn keeps_entry_stack_alias_when_used_after_prefix() {
        let mut func = HirFunction {
            name: "test".to_owned(),
            body: vec![
                HirStmt::Assign {
                    lhs: HirLValue::Var("xVar2".to_owned()),
                    rhs: HirExpr::PtrOffset {
                        base: Box::new(HirExpr::Var("sp".to_owned())),
                        offset: 16,
                    },
                },
                HirStmt::Assign {
                    lhs: HirLValue::Deref {
                        ptr: Box::new(HirExpr::Var("xVar2".to_owned())),
                        ty: u64_ty(),
                    },
                    rhs: HirExpr::Var("x20".to_owned()),
                },
                HirStmt::Expr(HirExpr::Var("xVar2".to_owned())),
            ],
            ..Default::default()
        };

        assert!(!remove_entry_stack_scaffold_stores(&mut func));
        assert_eq!(func.body.len(), 3);
    }

    #[test]
    fn removes_contiguous_entry_stack_slot_callee_saved_saves() {
        let mut func = HirFunction {
            name: "test".to_owned(),
            body: vec![
                HirStmt::Assign {
                    lhs: HirLValue::Var("home_0".to_owned()),
                    rhs: HirExpr::Var("r15".to_owned()),
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("home_0".to_owned()),
                    rhs: HirExpr::Var("param_1".to_owned()),
                },
                HirStmt::Return(None),
            ],
            ..Default::default()
        };

        assert!(remove_entry_stack_scaffold_stores(&mut func));
        assert_eq!(func.body, vec![HirStmt::Return(None)]);
    }

    #[test]
    fn removes_entry_stack_slot_callee_saved_saves_inside_entry_block() {
        let mut func = HirFunction {
            name: "test".to_owned(),
            body: vec![HirStmt::Block(vec![
                HirStmt::Assign {
                    lhs: HirLValue::Var("home_0".to_owned()),
                    rhs: HirExpr::Var("r15".to_owned()),
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("home_0".to_owned()),
                    rhs: HirExpr::Var("param_1".to_owned()),
                },
                HirStmt::Return(None),
            ])],
            ..Default::default()
        };

        assert!(remove_entry_stack_scaffold_stores(&mut func));
        assert_eq!(func.body, vec![HirStmt::Block(vec![HirStmt::Return(None)])]);
    }

    #[test]
    fn keeps_entry_stack_slot_initializers_without_callee_saved_evidence() {
        let mut func = HirFunction {
            name: "test".to_owned(),
            body: vec![
                HirStmt::Assign {
                    lhs: HirLValue::Var("local_8".to_owned()),
                    rhs: HirExpr::Var("param_1".to_owned()),
                },
                HirStmt::Return(None),
            ],
            ..Default::default()
        };

        assert!(!remove_entry_stack_scaffold_stores(&mut func));
        assert_eq!(func.body.len(), 2);
    }

    #[test]
    fn keeps_non_entry_and_non_scaffold_stores() {
        let mut func = HirFunction {
            name: "test".to_owned(),
            body: vec![
                HirStmt::Expr(HirExpr::Const(1, u64_ty())),
                scaffold_store("var_20", "var_38"),
                HirStmt::Assign {
                    lhs: HirLValue::Deref {
                        ptr: Box::new(HirExpr::Var("param_1".to_owned())),
                        ty: u64_ty(),
                    },
                    rhs: HirExpr::Var("param_2".to_owned()),
                },
            ],
            ..Default::default()
        };

        assert!(!remove_entry_stack_scaffold_stores(&mut func));
        assert_eq!(func.body.len(), 3);
    }
}
