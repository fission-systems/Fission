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
use crate::prelude::*;
use crate::{HashMap, HashSet};

/// Callee-saved register names that can appear after register naming. This
/// covers x86-64, AArch64, and ARM32 preserved GPR sets. Frame/link registers
/// are included here because compiler prologues save and restore them as part
/// of the same ABI-preserving stack scaffold.
const CALLEE_SAVED_REGS: &[&str] = &[
    "rbx", "rbp", "rsi", "rdi", "r12", "r13", "r14", "r15", "x19", "x20", "x21", "x22", "x23",
    "x24", "x25", "x26", "x27", "x28", "x29", "x30", "r4", "r5", "r6", "r7", "r8", "r9", "r10",
    "r11", "lr", "ebx", "ebp", "esi", "edi",
];

fn is_callee_saved(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    CALLEE_SAVED_REGS.contains(&lower.as_str())
}

fn looks_like_stack_scaffold_name(name: &str) -> bool {
    name == "sp" || name.starts_with("var_") || name.starts_with("xVar") || name.starts_with("uVar")
}

fn stack_scaffold_ptr_expr(expr: &DirExpr) -> bool {
    match expr {
        DirExpr::Var(name) | DirExpr::AddressOfGlobal(name) => looks_like_stack_scaffold_name(name),
        DirExpr::PtrOffset { base, .. }
        | DirExpr::Cast { expr: base, .. }
        | DirExpr::Unary { expr: base, .. } => stack_scaffold_ptr_expr(base),
        DirExpr::Binary { lhs, rhs, .. } => {
            stack_scaffold_ptr_expr(lhs) || stack_scaffold_ptr_expr(rhs)
        }
        _ => false,
    }
}

fn is_entry_stack_scaffold_store(stmt: &DirStmt) -> bool {
    let DirStmt::Assign {
        lhs: DirLValue::Deref { ptr, .. },
        rhs: DirExpr::Var(_),
    } = stmt
    else {
        return false;
    };
    stack_scaffold_ptr_expr(ptr)
}

fn is_entry_stack_scaffold_alias_binding(stmt: &DirStmt) -> Option<&str> {
    let DirStmt::Assign {
        lhs: DirLValue::Var(lhs),
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

fn var_name_through_cast(expr: &DirExpr) -> Option<&str> {
    match expr {
        DirExpr::Var(name) | DirExpr::AddressOfGlobal(name) => Some(name.as_str()),
        DirExpr::Cast { expr, .. } => var_name_through_cast(expr),
        _ => None,
    }
}

fn is_entry_stack_slot_scaffold_store(stmt: &DirStmt) -> bool {
    entry_stack_slot_scaffold_name(stmt).is_some()
}

fn entry_stack_slot_scaffold_name(stmt: &DirStmt) -> Option<&str> {
    match stmt {
        DirStmt::Assign {
            lhs: DirLValue::Var(lhs),
            rhs,
        } if looks_like_stack_slot_name(lhs) && var_name_through_cast(rhs).is_some() => {
            Some(lhs.as_str())
        }
        _ => None,
    }
}

fn is_entry_stack_slot_callee_saved_store(stmt: &DirStmt) -> bool {
    matches!(
        stmt,
        DirStmt::Assign {
            lhs: DirLValue::Var(lhs),
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
pub fn remove_entry_stack_scaffold_stores(func: &mut DirFunction) -> bool {
    remove_entry_stack_scaffold_stores_from_body(&mut func.body)
}

struct EntryStackScaffoldRemovalPlan {
    prefix_len: usize,
    remove_indices: HashSet<usize>,
}

impl EntryStackScaffoldRemovalPlan {
    fn prove(body: &[DirStmt]) -> Option<Self> {
        let prefix_len = body
            .iter()
            .take_while(|stmt| {
                is_entry_stack_scaffold_store(stmt)
                    || is_entry_stack_slot_scaffold_store(stmt)
                    || is_entry_stack_scaffold_alias_binding(stmt).is_some()
            })
            .count();
        if prefix_len == 0 {
            return None;
        }

        let prefix = &body[..prefix_len];
        let suffix = &body[prefix_len..];
        let has_scaffold_evidence = prefix.iter().any(is_entry_stack_scaffold_store)
            || prefix.iter().any(is_entry_stack_slot_callee_saved_store);
        if !has_scaffold_evidence {
            return None;
        }
        let alias_escapes_prefix = prefix
            .iter()
            .filter_map(is_entry_stack_scaffold_alias_binding)
            .any(|alias| count_ptr_var_rvalue_uses(suffix, alias) > 0);
        if alias_escapes_prefix {
            return None;
        }

        let remove_indices = prefix
            .iter()
            .enumerate()
            .filter_map(|(index, stmt)| {
                if let Some(slot) = entry_stack_slot_scaffold_name(stmt) {
                    // A stack-looking binding read by the function body is a
                    // semantic home/local initializer, not removable ABI noise.
                    return (count_ptr_var_rvalue_uses(suffix, slot) == 0).then_some(index);
                }
                Some(index)
            })
            .collect();

        Some(Self {
            prefix_len,
            remove_indices,
        })
    }

    fn apply(self, body: &mut Vec<DirStmt>) -> bool {
        if self.remove_indices.is_empty() {
            return false;
        }
        let mut index = 0;
        body.retain(|_| {
            let keep = index >= self.prefix_len || !self.remove_indices.contains(&index);
            index += 1;
            keep
        });
        true
    }
}

fn remove_entry_stack_scaffold_stores_from_body(body: &mut Vec<DirStmt>) -> bool {
    if let Some(plan) = EntryStackScaffoldRemovalPlan::prove(body)
        && plan.apply(body)
    {
        return true;
    }

    if let Some(DirStmt::Block(inner)) = body.first_mut() {
        return remove_entry_stack_scaffold_stores_from_body(inner);
    }

    false
}

// ── Pattern matching ──────────────────────────────────────────────────────────

/// Attempt to match a prologue SAVE statement:
/// `*<ptr_var> = <callee_saved_reg>`
///
/// Returns `(ptr_var_name, reg_name)` on success.
fn match_prologue_save(stmt: &DirStmt) -> Option<(String, String)> {
    let DirStmt::Assign { lhs, rhs } = stmt else {
        return None;
    };
    let ptr_var = match lhs {
        DirLValue::Deref { ptr, .. } => {
            if let DirExpr::Var(v) = ptr.as_ref() {
                v.as_str()
            } else {
                return None;
            }
        }
        _ => return None,
    };
    let reg = match rhs {
        DirExpr::Var(r) if is_callee_saved(r) => r.as_str(),
        _ => return None,
    };
    Some((ptr_var.to_string(), reg.to_string()))
}

/// Attempt to match an epilogue RESTORE statement:
/// `<callee_saved_reg> = *<ptr_var>` (or Cast-wrapped variant)
///
/// Returns `(ptr_var_name, reg_name)` on success.
fn match_epilogue_restore(stmt: &DirStmt) -> Option<(String, String)> {
    let DirStmt::Assign { lhs, rhs } = stmt else {
        return None;
    };
    let reg = match lhs {
        DirLValue::Var(r) if is_callee_saved(r) => r.as_str(),
        _ => return None,
    };
    // Match `Load { ptr: Var(v) }` or `Cast { Load { ptr: Var(v) } }`.
    let ptr_var = match rhs {
        DirExpr::Load { ptr, .. } => {
            if let DirExpr::Var(v) = ptr.as_ref() {
                v.as_str()
            } else {
                return None;
            }
        }
        DirExpr::Cast { expr: inner, .. } => {
            if let DirExpr::Load { ptr, .. } = inner.as_ref() {
                if let DirExpr::Var(v) = ptr.as_ref() {
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
fn count_ptr_var_rvalue_uses(stmts: &[DirStmt], ptr_var: &str) -> usize {
    stmts.iter().map(|s| count_ptr_in_stmt(s, ptr_var)).sum()
}

fn count_ptr_in_stmt(stmt: &DirStmt, name: &str) -> usize {
    let count = count_ptr_in_stmt_inner(stmt, name);
    if count > 0 && name == "rbx" {}
    count
}

fn count_ptr_in_stmt_inner(stmt: &DirStmt, name: &str) -> usize {
    match stmt {
        DirStmt::Assign { lhs, rhs } => {
            let lhs_uses = match lhs {
                // The write itself (`*p = ...`) does NOT count as an rvalue use
                // of `p` for our purposes — we only care whether `p` is READ
                // beyond the prologue/epilogue pair.  However, the pointer load
                // `*p` in `reg = *p` is an rvalue load, counted in `rhs`.
                DirLValue::Deref { ptr, .. } => count_ptr_in_expr(ptr, name),
                DirLValue::Index { base, index, .. } => {
                    count_ptr_in_expr(base, name) + count_ptr_in_expr(index, name)
                }
                DirLValue::Var(_) => 0,
                DirLValue::FieldAccess { base, .. } => count_ptr_in_expr(base, name),
            };
            lhs_uses + count_ptr_in_expr(rhs, name)
        }
        DirStmt::Expr(e) | DirStmt::Return(Some(e)) => count_ptr_in_expr(e, name),
        DirStmt::VaStart { va_list, .. } => count_ptr_in_expr(va_list, name),
        DirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            count_ptr_in_expr(cond, name)
                + count_ptr_var_rvalue_uses(then_body, name)
                + count_ptr_var_rvalue_uses(else_body, name)
        }
        DirStmt::While { cond, body } => {
            count_ptr_in_expr(cond, name) + count_ptr_var_rvalue_uses(body, name)
        }
        DirStmt::DoWhile { body, cond } => {
            count_ptr_var_rvalue_uses(body, name) + count_ptr_in_expr(cond, name)
        }
        DirStmt::For {
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
        DirStmt::Block(body) => count_ptr_var_rvalue_uses(body, name),
        DirStmt::Switch {
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
        DirStmt::Return(None)
        | DirStmt::Break
        | DirStmt::Continue
        | DirStmt::Label(_)
        | DirStmt::Goto(_) => 0,
    }
}

fn count_ptr_in_expr(expr: &DirExpr, name: &str) -> usize {
    match expr {
        DirExpr::Var(v) | DirExpr::AddressOfGlobal(v) => usize::from(v == name),
        DirExpr::Const(_, _) => 0,
        DirExpr::Cast { expr: inner, .. }
        | DirExpr::Unary { expr: inner, .. }
        | DirExpr::Load { ptr: inner, .. }
        | DirExpr::PtrOffset { base: inner, .. }
        | DirExpr::AggregateCopy { src: inner, .. }
        | DirExpr::FieldAccess { base: inner, .. } => count_ptr_in_expr(inner, name),
        DirExpr::Binary { lhs, rhs, .. } => {
            count_ptr_in_expr(lhs, name) + count_ptr_in_expr(rhs, name)
        }
        DirExpr::Call { args, .. } => args.iter().map(|a| count_ptr_in_expr(a, name)).sum(),
        DirExpr::Index { base, index, .. } => {
            count_ptr_in_expr(base, name) + count_ptr_in_expr(index, name)
        }
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            count_ptr_in_expr(cond, name)
                + count_ptr_in_expr(then_expr, name)
                + count_ptr_in_expr(else_expr, name)
        }
    }
}

// ── Statement removal ─────────────────────────────────────────────────────────

/// Remove all statements that match the given `(ptr_var, reg)` pairs from
/// `stmts` at any nesting level (epilogues can appear inside conditional arms).
fn remove_matching_saves_restores(
    stmts: &mut Vec<DirStmt>,
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

fn remove_nested(stmt: &mut DirStmt, pairs: &HashMap<String, String>, changed: &mut bool) {
    match stmt {
        DirStmt::Block(body) => remove_matching_saves_restores(body, pairs, changed),
        DirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            remove_matching_saves_restores(then_body, pairs, changed);
            remove_matching_saves_restores(else_body, pairs, changed);
        }
        DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
            remove_matching_saves_restores(body, pairs, changed)
        }
        DirStmt::For { body, .. } => remove_matching_saves_restores(body, pairs, changed),
        DirStmt::Switch { cases, default, .. } => {
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
pub fn remove_callee_save_prologue_epilogue(func: &mut DirFunction) -> bool {
    // ── Step 1: Discover prologue saves in the first few top-level statements.
    let max_prologue_scan = 16usize;
    let mut candidate_pairs: HashMap<String, String> = HashMap::default(); // ptr_var → reg

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
        let a = remove_orphaned_slot_epilogue_restores(func);
        let b = remove_dead_callee_saved_param_loads(func);
        return a | b;
    }

    // ── Step 2: Validate each candidate pair.
    // A pair (ptr, reg) is valid if:
    //   a. At least one epilogue restore for (ptr, reg) exists anywhere in the body.
    //   b. The ptr variable appears exactly ONCE as an rvalue in the body
    //      (the epilogue restore's Load expression).  Any additional use means
    //      the spill slot is aliased or used for something else.
    let mut confirmed: HashMap<String, String> = HashMap::default();

    // Collect all epilogue restores anywhere in the body.
    let mut restores: HashMap<String, String> = HashMap::default(); // ptr_var → reg
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
        let a = remove_orphaned_slot_epilogue_restores(func);
        let b = remove_dead_callee_saved_param_loads(func);
        return a | b;
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

    // ── Step 5: Also remove orphaned stack-slot epilogue restores that were
    // left behind by `remove_entry_stack_scaffold_stores`.
    changed |= remove_orphaned_slot_epilogue_restores(func);

    // ── Step 6: Remove dead callee-saved-register assignments whose uses were
    // all copy-propagated away, leaving an undeclared write with no reads.
    changed |= remove_dead_callee_saved_param_loads(func);

    changed
}

// ── Helper: collect all epilogue restores ────────────────────────────────────

fn collect_restores(stmts: &[DirStmt], restores: &mut HashMap<String, String>) {
    for stmt in stmts {
        if let Some((ptr, reg)) = match_epilogue_restore(stmt) {
            restores.entry(ptr).or_insert(reg);
        }
        match stmt {
            DirStmt::Block(body) => collect_restores(body, restores),
            DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_restores(then_body, restores);
                collect_restores(else_body, restores);
            }
            DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
                collect_restores(body, restores)
            }
            DirStmt::For { body, .. } => collect_restores(body, restores),
            DirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_restores(&case.body, restores);
                }
                collect_restores(default, restores);
            }
            _ => {}
        }
    }
}

fn count_restores_for_ptr(stmts: &[DirStmt], ptr: &str) -> usize {
    let mut count = 0;
    for stmt in stmts {
        if let Some((p, _)) = match_epilogue_restore(stmt) {
            if p == ptr {
                count += 1;
            }
        }
        match stmt {
            DirStmt::Block(body) => count += count_restores_for_ptr(body, ptr),
            DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                count += count_restores_for_ptr(then_body, ptr);
                count += count_restores_for_ptr(else_body, ptr);
            }
            DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
                count += count_restores_for_ptr(body, ptr)
            }
            DirStmt::For { body, .. } => count += count_restores_for_ptr(body, ptr),
            DirStmt::Switch { cases, default, .. } => {
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

// ── Orphaned stack-slot epilogue restore removal ──────────────────────────────
//
// When `remove_entry_stack_scaffold_stores` strips a prologue save of the form
// `home_X = callee_saved_reg`, it leaves the matching epilogue restore
// `callee_saved_reg = home_X` in place.  Because the definition of `home_X` is
// gone, that restore reads an uninitialized slot and is dead.  This sub-pass
// detects and removes such orphaned restores.

/// Match `callee_saved_reg = home_slot_var` (plain `Var` on RHS, no deref).
/// Returns `(slot_var_name, reg_name)` on success.
fn match_slot_epilogue_restore(stmt: &DirStmt) -> Option<(String, String)> {
    let DirStmt::Assign { lhs, rhs } = stmt else {
        return None;
    };
    let reg = match lhs {
        DirLValue::Var(r) if is_callee_saved(r) => r.as_str(),
        _ => return None,
    };
    let slot_var = match rhs {
        DirExpr::Var(v) if looks_like_stack_slot_name(v) => v.as_str(),
        DirExpr::Cast { expr: inner, .. } => match inner.as_ref() {
            DirExpr::Var(v) if looks_like_stack_slot_name(v) => v.as_str(),
            _ => return None,
        },
        _ => return None,
    };
    Some((slot_var.to_string(), reg.to_string()))
}

fn collect_slot_restores(stmts: &[DirStmt], out: &mut Vec<(String, String)>) {
    for stmt in stmts {
        if let Some(pair) = match_slot_epilogue_restore(stmt) {
            out.push(pair);
        }
        match stmt {
            DirStmt::Block(body) => collect_slot_restores(body, out),
            DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_slot_restores(then_body, out);
                collect_slot_restores(else_body, out);
            }
            DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
                collect_slot_restores(body, out)
            }
            DirStmt::For { body, .. } => collect_slot_restores(body, out),
            DirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_slot_restores(&case.body, out);
                }
                collect_slot_restores(default, out);
            }
            _ => {}
        }
    }
}

/// Count how many times `var` appears as the `Var` LHS of an assignment.
fn count_var_definitions(stmts: &[DirStmt], var: &str) -> usize {
    stmts.iter().map(|s| count_var_defs_in_stmt(s, var)).sum()
}

fn count_var_defs_in_stmt(stmt: &DirStmt, var: &str) -> usize {
    match stmt {
        DirStmt::Assign {
            lhs: DirLValue::Var(lhs),
            ..
        } if lhs == var => 1,
        DirStmt::Block(body) => count_var_definitions(body, var),
        DirStmt::If {
            then_body,
            else_body,
            ..
        } => count_var_definitions(then_body, var) + count_var_definitions(else_body, var),
        DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
            count_var_definitions(body, var)
        }
        DirStmt::For {
            init, body, update, ..
        } => {
            let i = init
                .as_ref()
                .map(|s| count_var_defs_in_stmt(s, var))
                .unwrap_or(0);
            let u = update
                .as_ref()
                .map(|s| count_var_defs_in_stmt(s, var))
                .unwrap_or(0);
            i + u + count_var_definitions(body, var)
        }
        DirStmt::Switch { cases, default, .. } => {
            let c: usize = cases
                .iter()
                .map(|c| count_var_definitions(&c.body, var))
                .sum();
            c + count_var_definitions(default, var)
        }
        _ => 0,
    }
}

fn remove_orphaned_slot_restores_from_stmts(
    stmts: &mut Vec<DirStmt>,
    slots: &HashSet<String>,
    changed: &mut bool,
) {
    for stmt in stmts.iter_mut() {
        remove_orphaned_slot_restore_nested(stmt, slots, changed);
    }
    stmts.retain(|stmt| {
        if let Some((slot, _)) = match_slot_epilogue_restore(stmt) {
            if slots.contains(&slot) {
                *changed = true;
                return false;
            }
        }
        true
    });
}

fn remove_orphaned_slot_restore_nested(
    stmt: &mut DirStmt,
    slots: &HashSet<String>,
    changed: &mut bool,
) {
    match stmt {
        DirStmt::Block(body) => remove_orphaned_slot_restores_from_stmts(body, slots, changed),
        DirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            remove_orphaned_slot_restores_from_stmts(then_body, slots, changed);
            remove_orphaned_slot_restores_from_stmts(else_body, slots, changed);
        }
        DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
            remove_orphaned_slot_restores_from_stmts(body, slots, changed)
        }
        DirStmt::For { body, .. } => remove_orphaned_slot_restores_from_stmts(body, slots, changed),
        DirStmt::Switch { cases, default, .. } => {
            for case in cases.iter_mut() {
                remove_orphaned_slot_restores_from_stmts(&mut case.body, slots, changed);
            }
            remove_orphaned_slot_restores_from_stmts(default, slots, changed);
        }
        _ => {}
    }
}

/// Remove epilogue restores of the form `callee_saved_reg = home_slot_var` where
/// `home_slot_var` has no remaining definition in the function body (its prologue
/// save was already stripped by `remove_entry_stack_scaffold_stores`).
fn remove_orphaned_slot_epilogue_restores(func: &mut DirFunction) -> bool {
    let mut candidates: Vec<(String, String)> = Vec::new();
    collect_slot_restores(&func.body, &mut candidates);

    let orphaned_slots: HashSet<String> = candidates
        .iter()
        .filter(|(slot, _)| count_var_definitions(&func.body, slot) == 0)
        .map(|(slot, _)| slot.clone())
        .collect();

    if orphaned_slots.is_empty() {
        return false;
    }

    let mut changed = false;
    remove_orphaned_slot_restores_from_stmts(&mut func.body, &orphaned_slots, &mut changed);

    if changed {
        func.locals.retain(|b| !orphaned_slots.contains(&b.name));
    }

    changed
}

/// Remove dead assignments `callee_saved_reg = expr` where:
/// 1. `callee_saved_reg` is a known callee-saved register name.
/// 2. `callee_saved_reg` has no `DirBinding` in `func.locals` (was never
///    materialized as a named local).
/// 3. `callee_saved_reg` has zero rvalue uses anywhere in the function body.
///
/// This arises when the compiler stores a parameter in a callee-saved register
/// (`rbx = param_3`) to keep it across calls, but a copy-propagation pass
/// later replaces every use of `rbx` with the original parameter, leaving the
/// initial assignment dead and the register name undeclared in the output.
pub fn remove_dead_callee_saved_param_loads(func: &mut DirFunction) -> bool {
    let mut candidates: HashSet<String> = HashSet::default();
    collect_callee_assign_targets_no_slot_rhs(&func.body, &mut candidates);

    for b in &func.locals {
        if is_callee_saved(&b.name) {
            candidates.insert(b.name.clone());
        }
    }

    if candidates.is_empty() {
        return false;
    }

    // Keep only those with zero rvalue uses in the entire body.
    candidates.retain(|name| {
        let uses = count_ptr_var_rvalue_uses(&func.body, name);
        uses == 0
    });

    if candidates.is_empty() {
        return false;
    }

    let mut changed = false;
    remove_dead_callee_assigns_from_stmts(&mut func.body, &candidates, &mut changed);

    // Also remove any corresponding DirBinding from locals (may have been
    // declared but later recognized as write-only by a prior pass).
    let before_locals = func.locals.len();
    func.locals.retain(|b| !candidates.contains(&b.name));
    if func.locals.len() != before_locals {
        changed = true;
    }

    changed
}

/// Collect all top-level `callee_reg = expr` assignments where the RHS is
/// NOT a stack-slot variable (to avoid touching epilogue-restore patterns).
fn collect_callee_assign_targets_no_slot_rhs(stmts: &[DirStmt], out: &mut HashSet<String>) {
    for stmt in stmts {
        match stmt {
            DirStmt::Assign {
                lhs: DirLValue::Var(name),
                rhs,
            } if is_callee_saved(name) => {
                let rhs_is_slot =
                    var_name_through_cast(rhs).is_some_and(looks_like_stack_slot_name);
                if !rhs_is_slot {
                    out.insert(name.clone());
                }
            }
            DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_callee_assign_targets_no_slot_rhs(then_body, out);
                collect_callee_assign_targets_no_slot_rhs(else_body, out);
            }
            DirStmt::Block(body)
            | DirStmt::While { body, .. }
            | DirStmt::DoWhile { body, .. }
            | DirStmt::For { body, .. } => {
                collect_callee_assign_targets_no_slot_rhs(body, out);
            }
            DirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_callee_assign_targets_no_slot_rhs(&case.body, out);
                }
                collect_callee_assign_targets_no_slot_rhs(default, out);
            }
            _ => {}
        }
    }
}

fn remove_dead_callee_assigns_from_stmts(
    stmts: &mut Vec<DirStmt>,
    dead: &HashSet<String>,
    changed: &mut bool,
) {
    for stmt in stmts.iter_mut() {
        match stmt {
            DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                remove_dead_callee_assigns_from_stmts(then_body, dead, changed);
                remove_dead_callee_assigns_from_stmts(else_body, dead, changed);
            }
            DirStmt::Block(body)
            | DirStmt::While { body, .. }
            | DirStmt::DoWhile { body, .. }
            | DirStmt::For { body, .. } => {
                remove_dead_callee_assigns_from_stmts(body, dead, changed);
            }
            DirStmt::Switch { cases, default, .. } => {
                for case in cases.iter_mut() {
                    remove_dead_callee_assigns_from_stmts(&mut case.body, dead, changed);
                }
                remove_dead_callee_assigns_from_stmts(default, dead, changed);
            }
            _ => {}
        }
    }
    let before = stmts.len();
    stmts.retain(|stmt| {
        !matches!(stmt, DirStmt::Assign { lhs: DirLValue::Var(name), .. } if dead.contains(name))
    });
    if stmts.len() < before {
        *changed = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
// prelude via parent
    use fission_midend_dir::DirBinding;

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

    fn scaffold_store(ptr: &str, rhs: &str) -> DirStmt {
        DirStmt::Assign {
            lhs: DirLValue::Deref {
                ptr: Box::new(DirExpr::PtrOffset {
                    base: Box::new(DirExpr::Var(ptr.to_owned())),
                    offset: -8,
                }),
                ty: u64_ty(),
            },
            rhs: DirExpr::Var(rhs.to_owned()),
        }
    }

    #[test]
    fn removes_contiguous_entry_stack_scaffold_stores() {
        let mut func = DirFunction {
            name: "test".to_owned(),
            int_param_offsets: Vec::new(),
            body: vec![
                scaffold_store("var_20", "var_38"),
                scaffold_store("xVar0", "param_2"),
                DirStmt::Return(None),
            ],
            ..Default::default()
        };

        assert!(remove_entry_stack_scaffold_stores(&mut func));
        assert_eq!(func.body, vec![DirStmt::Return(None)]);
    }

    #[test]
    fn removes_aarch64_sp_based_entry_callee_saved_scaffold() {
        let mut func = DirFunction {
            name: "test".to_owned(),
            int_param_offsets: Vec::new(),
            body: vec![
                scaffold_store("sp", "x29"),
                DirStmt::Assign {
                    lhs: DirLValue::Deref {
                        ptr: Box::new(DirExpr::PtrOffset {
                            base: Box::new(DirExpr::Var("sp".to_owned())),
                            offset: 8,
                        }),
                        ty: u64_ty(),
                    },
                    rhs: DirExpr::Var("x30".to_owned()),
                },
                DirStmt::Assign {
                    lhs: DirLValue::Deref {
                        ptr: Box::new(DirExpr::PtrOffset {
                            base: Box::new(DirExpr::Var("sp".to_owned())),
                            offset: 16,
                        }),
                        ty: u64_ty(),
                    },
                    rhs: DirExpr::Var("x20".to_owned()),
                },
                DirStmt::Return(Some(DirExpr::Var("param_1".to_owned()))),
            ],
            ..Default::default()
        };

        assert!(remove_entry_stack_scaffold_stores(&mut func));
        assert_eq!(
            func.body,
            vec![DirStmt::Return(Some(DirExpr::Var("param_1".to_owned())))]
        );
    }

    #[test]
    fn removes_aarch64_entry_stack_alias_callee_saved_scaffold() {
        let mut func = DirFunction {
            name: "test".to_owned(),
            int_param_offsets: Vec::new(),
            body: vec![
                DirStmt::Assign {
                    lhs: DirLValue::Var("xVar2".to_owned()),
                    rhs: DirExpr::PtrOffset {
                        base: Box::new(DirExpr::Var("sp".to_owned())),
                        offset: 16,
                    },
                },
                DirStmt::Assign {
                    lhs: DirLValue::Deref {
                        ptr: Box::new(DirExpr::Var("xVar2".to_owned())),
                        ty: u64_ty(),
                    },
                    rhs: DirExpr::Var("x20".to_owned()),
                },
                DirStmt::Return(Some(DirExpr::Var("param_1".to_owned()))),
            ],
            ..Default::default()
        };

        assert!(remove_entry_stack_scaffold_stores(&mut func));
        assert_eq!(
            func.body,
            vec![DirStmt::Return(Some(DirExpr::Var("param_1".to_owned())))]
        );
    }

    #[test]
    fn removes_arm32_uvar_stack_alias_callee_saved_scaffold() {
        let mut func = DirFunction {
            name: "test".to_owned(),
            int_param_offsets: Vec::new(),
            body: vec![
                DirStmt::Assign {
                    lhs: DirLValue::Var("uVar0".to_owned()),
                    rhs: DirExpr::Binary {
                        op: DirBinaryOp::Sub,
                        lhs: Box::new(DirExpr::Var("sp".to_owned())),
                        rhs: Box::new(DirExpr::Const(4, u32_ty())),
                        ty: u32_ty(),
                    },
                },
                DirStmt::Assign {
                    lhs: DirLValue::Deref {
                        ptr: Box::new(DirExpr::Var("uVar0".to_owned())),
                        ty: u32_ty(),
                    },
                    rhs: DirExpr::Var("lr".to_owned()),
                },
                DirStmt::Assign {
                    lhs: DirLValue::Var("uVar1".to_owned()),
                    rhs: DirExpr::Binary {
                        op: DirBinaryOp::Sub,
                        lhs: Box::new(DirExpr::Var("uVar0".to_owned())),
                        rhs: Box::new(DirExpr::Const(1, u32_ty())),
                        ty: u32_ty(),
                    },
                },
                DirStmt::Assign {
                    lhs: DirLValue::Deref {
                        ptr: Box::new(DirExpr::Var("uVar1".to_owned())),
                        ty: u32_ty(),
                    },
                    rhs: DirExpr::Var("r11".to_owned()),
                },
                DirStmt::Return(Some(DirExpr::Var("param_1".to_owned()))),
            ],
            ..Default::default()
        };

        assert!(remove_entry_stack_scaffold_stores(&mut func));
        assert_eq!(
            func.body,
            vec![DirStmt::Return(Some(DirExpr::Var("param_1".to_owned())))]
        );
    }

    #[test]
    fn keeps_entry_stack_alias_when_used_after_prefix() {
        let mut func = DirFunction {
            name: "test".to_owned(),
            int_param_offsets: Vec::new(),
            body: vec![
                DirStmt::Assign {
                    lhs: DirLValue::Var("xVar2".to_owned()),
                    rhs: DirExpr::PtrOffset {
                        base: Box::new(DirExpr::Var("sp".to_owned())),
                        offset: 16,
                    },
                },
                DirStmt::Assign {
                    lhs: DirLValue::Deref {
                        ptr: Box::new(DirExpr::Var("xVar2".to_owned())),
                        ty: u64_ty(),
                    },
                    rhs: DirExpr::Var("x20".to_owned()),
                },
                DirStmt::Expr(DirExpr::Var("xVar2".to_owned())),
            ],
            ..Default::default()
        };

        assert!(!remove_entry_stack_scaffold_stores(&mut func));
        assert_eq!(func.body.len(), 3);
    }

    #[test]
    fn removes_contiguous_entry_stack_slot_callee_saved_saves() {
        let mut func = DirFunction {
            name: "test".to_owned(),
            int_param_offsets: Vec::new(),
            body: vec![
                DirStmt::Assign {
                    lhs: DirLValue::Var("home_0".to_owned()),
                    rhs: DirExpr::Var("r15".to_owned()),
                },
                DirStmt::Assign {
                    lhs: DirLValue::Var("home_0".to_owned()),
                    rhs: DirExpr::Var("param_1".to_owned()),
                },
                DirStmt::Return(None),
            ],
            ..Default::default()
        };

        assert!(remove_entry_stack_scaffold_stores(&mut func));
        assert_eq!(func.body, vec![DirStmt::Return(None)]);
    }

    #[test]
    fn keeps_live_stack_slot_initializers_after_callee_saved_prefix() {
        let mut func = DirFunction {
            name: "test".to_owned(),
            int_param_offsets: Vec::new(),
            body: vec![
                DirStmt::Assign {
                    lhs: DirLValue::Var("home_0".to_owned()),
                    rhs: DirExpr::Var("r15".to_owned()),
                },
                DirStmt::Assign {
                    lhs: DirLValue::Var("local_8".to_owned()),
                    rhs: DirExpr::Var("param_1".to_owned()),
                },
                DirStmt::Return(Some(DirExpr::Var("local_8".to_owned()))),
            ],
            ..Default::default()
        };

        assert!(remove_entry_stack_scaffold_stores(&mut func));
        assert_eq!(func.body.len(), 2);
        assert!(matches!(
            &func.body[0],
            DirStmt::Assign {
                lhs: DirLValue::Var(lhs),
                rhs: DirExpr::Var(rhs),
            } if lhs == "local_8" && rhs == "param_1"
        ));
    }

    #[test]
    fn removes_entry_stack_slot_callee_saved_saves_inside_entry_block() {
        let mut func = DirFunction {
            name: "test".to_owned(),
            int_param_offsets: Vec::new(),
            body: vec![DirStmt::Block(vec![
                DirStmt::Assign {
                    lhs: DirLValue::Var("home_0".to_owned()),
                    rhs: DirExpr::Var("r15".to_owned()),
                },
                DirStmt::Assign {
                    lhs: DirLValue::Var("home_0".to_owned()),
                    rhs: DirExpr::Var("param_1".to_owned()),
                },
                DirStmt::Return(None),
            ])],
            ..Default::default()
        };

        assert!(remove_entry_stack_scaffold_stores(&mut func));
        assert_eq!(func.body, vec![DirStmt::Block(vec![DirStmt::Return(None)])]);
    }

    #[test]
    fn keeps_entry_stack_slot_initializers_without_callee_saved_evidence() {
        let mut func = DirFunction {
            name: "test".to_owned(),
            int_param_offsets: Vec::new(),
            body: vec![
                DirStmt::Assign {
                    lhs: DirLValue::Var("local_8".to_owned()),
                    rhs: DirExpr::Var("param_1".to_owned()),
                },
                DirStmt::Return(None),
            ],
            ..Default::default()
        };

        assert!(!remove_entry_stack_scaffold_stores(&mut func));
        assert_eq!(func.body.len(), 2);
    }

    #[test]
    fn keeps_non_entry_and_non_scaffold_stores() {
        let mut func = DirFunction {
            name: "test".to_owned(),
            int_param_offsets: Vec::new(),
            body: vec![
                DirStmt::Expr(DirExpr::Const(1, u64_ty())),
                scaffold_store("var_20", "var_38"),
                DirStmt::Assign {
                    lhs: DirLValue::Deref {
                        ptr: Box::new(DirExpr::Var("param_1".to_owned())),
                        ty: u64_ty(),
                    },
                    rhs: DirExpr::Var("param_2".to_owned()),
                },
            ],
            ..Default::default()
        };

        assert!(!remove_entry_stack_scaffold_stores(&mut func));
        assert_eq!(func.body.len(), 3);
    }

    // ── Orphaned stack-slot epilogue restore tests ─────────────────────────────

    fn slot_restore(reg: &str, slot: &str) -> DirStmt {
        DirStmt::Assign {
            lhs: DirLValue::Var(reg.to_owned()),
            rhs: DirExpr::Var(slot.to_owned()),
        }
    }

    #[test]
    fn removes_orphaned_slot_epilogue_restore_with_uppercase_register() {
        let mut func = DirFunction {
            name: "fill_matrix".to_owned(),
            int_param_offsets: Vec::new(),
            body: vec![
                DirStmt::Expr(DirExpr::Const(42, u64_ty())),
                slot_restore("RDI", "home_0"),
                DirStmt::Return(None),
            ],
            locals: vec![DirBinding {
                name: "home_0".to_owned(),
                ty: u64_ty(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            }],
            ..Default::default()
        };

        assert!(remove_callee_save_prologue_epilogue(&mut func));
        assert_eq!(
            func.body.len(),
            2,
            "uppercase register restore should be removed"
        );
        assert!(!func.locals.iter().any(|b| b.name == "home_0"));
    }

    #[test]
    fn removes_orphaned_slot_epilogue_restore_when_no_definition() {
        // home_0 has no definition — its prologue save was already stripped.
        // `rbx = home_0` is an orphaned epilogue restore and should be removed.
        let mut func = DirFunction {
            name: "test".to_owned(),
            int_param_offsets: Vec::new(),
            body: vec![
                DirStmt::Expr(DirExpr::Const(42, u64_ty())),
                slot_restore("rbx", "home_0"),
                DirStmt::Return(None),
            ],
            locals: vec![DirBinding {
                name: "home_0".to_owned(),
                ty: u64_ty(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            }],
            ..Default::default()
        };

        assert!(remove_callee_save_prologue_epilogue(&mut func));
        assert_eq!(func.body.len(), 2, "orphaned restore should be removed");
        assert!(
            !func.locals.iter().any(|b| b.name == "home_0"),
            "home_0 local should be removed"
        );
    }

    #[test]
    fn removes_multiple_orphaned_slot_restores() {
        // Both home_0 and home_8 have no definitions (prologue saves stripped).
        let mut func = DirFunction {
            name: "test".to_owned(),
            int_param_offsets: Vec::new(),
            body: vec![
                DirStmt::Expr(DirExpr::Const(1, u64_ty())),
                slot_restore("rbx", "home_0"),
                slot_restore("rsi", "home_8"),
                DirStmt::Return(None),
            ],
            locals: vec![
                DirBinding {
                    name: "home_0".to_owned(),
                    ty: u64_ty(),
                    surface_type_name: None,
                    origin: None,
                    initializer: None,
                },
                DirBinding {
                    name: "home_8".to_owned(),
                    ty: u64_ty(),
                    surface_type_name: None,
                    origin: None,
                    initializer: None,
                },
            ],
            ..Default::default()
        };

        assert!(remove_callee_save_prologue_epilogue(&mut func));
        assert_eq!(
            func.body.len(),
            2,
            "both orphaned restores should be removed"
        );
        assert!(func.locals.is_empty(), "home locals should be removed");
    }

    #[test]
    fn keeps_slot_restore_when_slot_has_definition() {
        // home_0 IS defined in the body — not orphaned, must NOT be removed.
        let mut func = DirFunction {
            name: "test".to_owned(),
            int_param_offsets: Vec::new(),
            body: vec![
                DirStmt::Assign {
                    lhs: DirLValue::Var("home_0".to_owned()),
                    rhs: DirExpr::Var("param_1".to_owned()),
                },
                slot_restore("rbx", "home_0"),
                DirStmt::Return(None),
            ],
            locals: vec![DirBinding {
                name: "home_0".to_owned(),
                ty: u64_ty(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            }],
            ..Default::default()
        };

        assert!(!remove_callee_save_prologue_epilogue(&mut func));
        assert_eq!(
            func.body.len(),
            3,
            "slot restore with live definition must be kept"
        );
    }

    #[test]
    fn removes_orphaned_slot_restore_inside_nested_block() {
        // Orphaned restores inside nested blocks are also removed.
        let mut func = DirFunction {
            name: "test".to_owned(),
            int_param_offsets: Vec::new(),
            body: vec![
                DirStmt::If {
                    cond: DirExpr::Const(1, u64_ty()),
                    then_body: vec![slot_restore("rsi", "home_0")],
                    else_body: vec![],
                },
                DirStmt::Return(None),
            ],
            locals: vec![DirBinding {
                name: "home_0".to_owned(),
                ty: u64_ty(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            }],
            ..Default::default()
        };

        assert!(remove_callee_save_prologue_epilogue(&mut func));
        if let DirStmt::If { then_body, .. } = &func.body[0] {
            assert!(
                then_body.is_empty(),
                "orphaned restore inside if-branch should be removed"
            );
        }
    }

    // ── remove_dead_callee_saved_param_loads ──────────────────────────────────

    fn assign_var(lhs: &str, rhs: DirExpr) -> DirStmt {
        DirStmt::Assign {
            lhs: DirLValue::Var(lhs.to_owned()),
            rhs,
        }
    }

    fn var(name: &str) -> DirExpr {
        DirExpr::Var(name.to_owned())
    }

    #[test]
    fn removes_dead_undeclared_callee_saved_assignment() {
        // rbx = param_3  but rbx has no binding and is never read → remove.
        let mut func = DirFunction {
            name: "test".to_owned(),
            int_param_offsets: Vec::new(),
            body: vec![assign_var("rbx", var("param_3")), DirStmt::Return(None)],
            locals: vec![], // rbx has no DirBinding
            ..Default::default()
        };

        assert!(remove_callee_save_prologue_epilogue(&mut func));
        assert_eq!(func.body, vec![DirStmt::Return(None)]);
    }

    #[test]
    fn keeps_live_callee_saved_assignment_that_is_read() {
        // rsi = param_2, but rsi IS read in the condition → keep.
        let mut func = DirFunction {
            name: "test".to_owned(),
            int_param_offsets: Vec::new(),
            body: vec![
                assign_var("rsi", var("param_2")),
                DirStmt::If {
                    cond: var("rsi"),
                    then_body: vec![DirStmt::Return(None)],
                    else_body: vec![],
                },
            ],
            locals: vec![], // undeclared but has reads
            ..Default::default()
        };

        assert!(!remove_callee_save_prologue_epilogue(&mut func));
        assert_eq!(func.body.len(), 2, "live assignment must not be removed");
    }

    #[test]
    fn removes_declared_callee_saved_assignment_when_dead() {
        // rbx = param_3, rbx IS declared in locals but is never read.
        // The new strategy: 0 rvalue uses → remove assignment AND binding.
        let mut func = DirFunction {
            name: "test".to_owned(),
            int_param_offsets: Vec::new(),
            body: vec![assign_var("rbx", var("param_3")), DirStmt::Return(None)],
            locals: vec![DirBinding {
                name: "rbx".to_owned(),
                ty: u64_ty(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            }],
            ..Default::default()
        };

        assert!(remove_callee_save_prologue_epilogue(&mut func));
        assert_eq!(
            func.body,
            vec![DirStmt::Return(None)],
            "dead assignment removed"
        );
        assert!(
            func.locals.is_empty(),
            "dead binding also removed from locals"
        );
    }

    #[test]
    fn removes_declared_dead_callee_saved_assignment_already_deleted_by_prior_pass() {
        // rbx has already been deleted from body, but remains in locals.
        // It has 0 rvalue uses and should be pruned.
        let mut func = DirFunction {
            name: "fill_matrix".to_owned(),
            int_param_offsets: Vec::new(),
            body: vec![DirStmt::Return(None)],
            locals: vec![DirBinding {
                name: "rbx".to_owned(),
                ty: u64_ty(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            }],
            ..Default::default()
        };

        assert!(remove_dead_callee_saved_param_loads(&mut func));
        assert!(
            func.locals.is_empty(),
            "rbx local should be removed even if assignment was already deleted"
        );
    }
}
