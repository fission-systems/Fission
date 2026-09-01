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

fn stack_scaffold_ptr_expr(expr: &PreHirExpr) -> bool {
    match expr {
        PreHirExpr::Var(name)
        | PreHirExpr::AddressOfGlobal(name)
        | PreHirExpr::AddressOfLocal(name) => looks_like_stack_scaffold_name(name),
        PreHirExpr::PtrOffset { base, .. }
        | PreHirExpr::Cast { expr: base, .. }
        | PreHirExpr::Unary { expr: base, .. } => stack_scaffold_ptr_expr(base),
        PreHirExpr::Binary { lhs, rhs, .. } => {
            stack_scaffold_ptr_expr(lhs) || stack_scaffold_ptr_expr(rhs)
        }
        _ => false,
    }
}

fn is_entry_stack_scaffold_store(stmt: &PreHirStmt) -> bool {
    let PreHirStmt::Assign {
        lhs: PreHirLValue::Deref { ptr, .. },
        rhs: PreHirExpr::Var(_),
    } = stmt
    else {
        return false;
    };
    stack_scaffold_ptr_expr(ptr)
}

fn is_entry_stack_scaffold_alias_binding(stmt: &PreHirStmt) -> Option<&str> {
    let PreHirStmt::Assign {
        lhs: PreHirLValue::Var(lhs),
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

/// Whether `expr` computes a stack address, given the names already proven to
/// hold one.
///
/// The name-based test above only knows the names this pass could anticipate.
/// ARM's `push {r4,r5,r6,lr}` produces one it cannot: the instruction lifts to
/// a single pointer copied out of `sp`, walked down four bytes per register,
/// stored through, and copied back -- and that pointer is a register the ARM
/// SLEIGH spec declares (`mult_addr`), so it survives naming as itself. What
/// makes it scaffold is not its name but where it came from.
fn expr_is_stack_alias(expr: &PreHirExpr, aliases: &HashSet<String>) -> bool {
    match expr {
        PreHirExpr::Var(name)
        | PreHirExpr::AddressOfGlobal(name)
        | PreHirExpr::AddressOfLocal(name) => {
            looks_like_stack_scaffold_name(name) || aliases.contains(name)
        }
        PreHirExpr::PtrOffset { base, .. }
        | PreHirExpr::Cast { expr: base, .. }
        | PreHirExpr::Unary { expr: base, .. } => expr_is_stack_alias(base, aliases),
        PreHirExpr::Binary { lhs, rhs, .. } => {
            expr_is_stack_alias(lhs, aliases) || expr_is_stack_alias(rhs, aliases)
        }
        _ => false,
    }
}

/// `<name> = <expression over a stack address>`, which makes `<name>` one too.
///
/// Deliberately permissive about the left-hand name: this only records that a
/// name holds a stack address. Nothing is removed on the strength of it --
/// [`is_entry_walking_scaffold_store`] still requires the stored value to be a
/// callee-saved register, and `prove` still refuses when the body reads the
/// name afterwards.
fn walking_scaffold_alias_binding<'a>(
    stmt: &'a PreHirStmt,
    aliases: &HashSet<String>,
) -> Option<&'a str> {
    let PreHirStmt::Assign {
        lhs: PreHirLValue::Var(lhs),
        rhs,
    } = stmt
    else {
        return None;
    };
    expr_is_stack_alias(rhs, aliases).then(|| lhs.as_str())
}

/// `*<stack alias> = <callee-saved register>`.
///
/// The callee-saved requirement is what separates a register save from an
/// ordinary early store through a stack-derived pointer: the first is ABI
/// scaffold whose only reader is the matching restore, the second is the
/// function writing to one of its own locals.
fn is_entry_walking_scaffold_store(stmt: &PreHirStmt, aliases: &HashSet<String>) -> bool {
    let PreHirStmt::Assign {
        lhs: PreHirLValue::Deref { ptr, .. },
        rhs,
    } = stmt
    else {
        return false;
    };
    expr_is_stack_alias(ptr, aliases) && var_name_through_cast(rhs).is_some_and(is_callee_saved)
}

fn looks_like_stack_slot_name(name: &str) -> bool {
    name.starts_with("home_") || name.starts_with("local_") || name.starts_with("ret_scaffold_")
}

fn var_name_through_cast(expr: &PreHirExpr) -> Option<&str> {
    match expr {
        PreHirExpr::Var(name)
        | PreHirExpr::AddressOfGlobal(name)
        | PreHirExpr::AddressOfLocal(name) => Some(name.as_str()),
        PreHirExpr::Cast { expr, .. } => var_name_through_cast(expr),
        _ => None,
    }
}

fn is_entry_stack_slot_scaffold_store(stmt: &PreHirStmt) -> bool {
    entry_stack_slot_scaffold_name(stmt).is_some()
}

fn entry_stack_slot_scaffold_name(stmt: &PreHirStmt) -> Option<&str> {
    match stmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(lhs),
            rhs,
        } if looks_like_stack_slot_name(lhs) && var_name_through_cast(rhs).is_some() => {
            Some(lhs.as_str())
        }
        _ => None,
    }
}

fn is_entry_stack_slot_callee_saved_store(stmt: &PreHirStmt) -> bool {
    matches!(
        stmt,
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(lhs),
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
pub fn remove_entry_stack_scaffold_stores(func: &mut PreHirFunction) -> bool {
    remove_entry_stack_scaffold_stores_from_body(&mut func.body)
}

struct EntryStackScaffoldRemovalPlan {
    prefix_len: usize,
    remove_indices: HashSet<usize>,
}

impl EntryStackScaffoldRemovalPlan {
    fn prove(body: &[PreHirStmt]) -> Option<Self> {
        // The prefix has to be walked with state, not filtered statement by
        // statement: a pointer only counts as scaffold because an earlier
        // statement in this same prefix bound it to a stack address. ARM's
        // multi-register push is the whole reason -- one pointer out of `sp`,
        // decremented and stored through once per saved register.
        let mut aliases: HashSet<String> = HashSet::default();
        let mut walking_evidence = false;
        let mut prefix_len = 0;
        for stmt in body {
            if is_entry_stack_scaffold_store(stmt) || is_entry_stack_slot_scaffold_store(stmt) {
                prefix_len += 1;
                continue;
            }
            if is_entry_walking_scaffold_store(stmt, &aliases) {
                walking_evidence = true;
                prefix_len += 1;
                continue;
            }
            if let Some(alias) = is_entry_stack_scaffold_alias_binding(stmt) {
                aliases.insert(alias.to_string());
                prefix_len += 1;
                continue;
            }
            if let Some(alias) = walking_scaffold_alias_binding(stmt, &aliases) {
                aliases.insert(alias.to_string());
                prefix_len += 1;
                continue;
            }
            break;
        }
        if prefix_len == 0 {
            return None;
        }

        let prefix = &body[..prefix_len];
        let suffix = &body[prefix_len..];
        let has_scaffold_evidence = walking_evidence
            || prefix.iter().any(is_entry_stack_scaffold_store)
            || prefix.iter().any(is_entry_stack_slot_callee_saved_store);
        if !has_scaffold_evidence {
            return None;
        }
        // Every name the prefix bound to a stack address, not just the ones
        // the name-based rule recognised: a walking pointer the body reads
        // afterwards is a local the function uses, not scaffold to drop.
        let alias_escapes_prefix = aliases
            .iter()
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

    fn apply(self, body: &mut Vec<PreHirStmt>) -> bool {
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

fn remove_entry_stack_scaffold_stores_from_body(body: &mut Vec<PreHirStmt>) -> bool {
    if let Some(plan) = EntryStackScaffoldRemovalPlan::prove(body)
        && plan.apply(body)
    {
        return true;
    }

    if let Some(PreHirStmt::Block(inner)) = body.first_mut() {
        return remove_entry_stack_scaffold_stores_from_body(
            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(inner),
        );
    }

    false
}

// ── Pattern matching ──────────────────────────────────────────────────────────

/// Attempt to match a prologue SAVE statement:
/// `*<ptr_var> = <callee_saved_reg>`
///
/// Returns `(ptr_var_name, reg_name)` on success.
fn match_prologue_save(stmt: &PreHirStmt) -> Option<(String, String)> {
    let PreHirStmt::Assign { lhs, rhs } = stmt else {
        return None;
    };
    let ptr_var = match lhs {
        PreHirLValue::Deref { ptr, .. } => {
            if let PreHirExpr::Var(v) = ptr.as_ref() {
                v.as_str()
            } else {
                return None;
            }
        }
        _ => return None,
    };
    let reg = match rhs {
        PreHirExpr::Var(r) if is_callee_saved(r) => r.as_str(),
        _ => return None,
    };
    Some((ptr_var.to_string(), reg.to_string()))
}

/// Attempt to match an epilogue RESTORE statement:
/// `<callee_saved_reg> = *<ptr_var>` (or Cast-wrapped variant)
///
/// Returns `(ptr_var_name, reg_name)` on success.
fn match_epilogue_restore(stmt: &PreHirStmt) -> Option<(String, String)> {
    let PreHirStmt::Assign { lhs, rhs } = stmt else {
        return None;
    };
    let reg = match lhs {
        PreHirLValue::Var(r) if is_callee_saved(r) => r.as_str(),
        _ => return None,
    };
    // Match `Load { ptr: Var(v) }` or `Cast { Load { ptr: Var(v) } }`.
    let ptr_var = match rhs {
        PreHirExpr::Load { ptr, .. } => {
            if let PreHirExpr::Var(v) = ptr.as_ref() {
                v.as_str()
            } else {
                return None;
            }
        }
        PreHirExpr::Cast { expr: inner, .. } => {
            if let PreHirExpr::Load { ptr, .. } = inner.as_ref() {
                if let PreHirExpr::Var(v) = ptr.as_ref() {
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
fn count_ptr_var_rvalue_uses(stmts: &[PreHirStmt], ptr_var: &str) -> usize {
    stmts.iter().map(|s| count_ptr_in_stmt(s, ptr_var)).sum()
}

fn count_ptr_in_stmt(stmt: &PreHirStmt, name: &str) -> usize {
    let count = count_ptr_in_stmt_inner(stmt, name);
    if count > 0 && name == "rbx" {}
    count
}

fn count_ptr_in_stmt_inner(stmt: &PreHirStmt, name: &str) -> usize {
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            let lhs_uses = match lhs {
                // The write itself (`*p = ...`) does NOT count as an rvalue use
                // of `p` for our purposes — we only care whether `p` is READ
                // beyond the prologue/epilogue pair.  However, the pointer load
                // `*p` in `reg = *p` is an rvalue load, counted in `rhs`.
                PreHirLValue::Deref { ptr, .. } => count_ptr_in_expr(ptr, name),
                PreHirLValue::Index { base, index, .. } => {
                    count_ptr_in_expr(base, name) + count_ptr_in_expr(index, name)
                }
                PreHirLValue::Var(_) => 0,
                PreHirLValue::FieldAccess { base, .. } => count_ptr_in_expr(base, name),
            };
            lhs_uses + count_ptr_in_expr(rhs, name)
        }
        PreHirStmt::Expr(e) | PreHirStmt::Return(Some(e)) => count_ptr_in_expr(e, name),
        PreHirStmt::VaStart { va_list, .. } => count_ptr_in_expr(va_list, name),
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            count_ptr_in_expr(cond, name)
                + count_ptr_var_rvalue_uses(then_body, name)
                + count_ptr_var_rvalue_uses(else_body, name)
        }
        PreHirStmt::While { cond, body } => {
            count_ptr_in_expr(cond, name) + count_ptr_var_rvalue_uses(body, name)
        }
        PreHirStmt::DoWhile { body, cond } => {
            count_ptr_var_rvalue_uses(body, name) + count_ptr_in_expr(cond, name)
        }
        PreHirStmt::For {
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
        PreHirStmt::Block(body) => count_ptr_var_rvalue_uses(body, name),
        PreHirStmt::Switch {
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
        PreHirStmt::Return(None)
        | PreHirStmt::Break
        | PreHirStmt::Continue
        | PreHirStmt::Label(_)
        | PreHirStmt::Goto(_) => 0,
    }
}

fn count_ptr_in_expr(expr: &PreHirExpr, name: &str) -> usize {
    match expr {
        PreHirExpr::Var(v) | PreHirExpr::AddressOfGlobal(v) | PreHirExpr::AddressOfLocal(v) => {
            usize::from(v == name)
        }
        PreHirExpr::Const(_, _) => 0,
        PreHirExpr::Cast { expr: inner, .. }
        | PreHirExpr::Unary { expr: inner, .. }
        | PreHirExpr::Load { ptr: inner, .. }
        | PreHirExpr::PtrOffset { base: inner, .. }
        | PreHirExpr::AggregateCopy { src: inner, .. }
        | PreHirExpr::FieldAccess { base: inner, .. } => count_ptr_in_expr(inner, name),
        PreHirExpr::Binary { lhs, rhs, .. } => {
            count_ptr_in_expr(lhs, name) + count_ptr_in_expr(rhs, name)
        }
        PreHirExpr::Call { args, .. } => args.iter().map(|a| count_ptr_in_expr(a, name)).sum(),
        PreHirExpr::Index { base, index, .. } => {
            count_ptr_in_expr(base, name) + count_ptr_in_expr(index, name)
        }
        PreHirExpr::Select {
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
    stmts: &mut Vec<PreHirStmt>,
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

fn remove_nested(stmt: &mut PreHirStmt, pairs: &HashMap<String, String>, changed: &mut bool) {
    match stmt {
        PreHirStmt::Block(body) => remove_matching_saves_restores(
            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
            pairs,
            changed,
        ),
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            remove_matching_saves_restores(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                pairs,
                changed,
            );
            remove_matching_saves_restores(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                pairs,
                changed,
            );
        }
        PreHirStmt::While { body, .. } | PreHirStmt::DoWhile { body, .. } => {
            remove_matching_saves_restores(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                pairs,
                changed,
            )
        }
        PreHirStmt::For { body, .. } => remove_matching_saves_restores(
            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
            pairs,
            changed,
        ),
        PreHirStmt::Switch { cases, default, .. } => {
            for case in cases.iter_mut() {
                remove_matching_saves_restores(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                    pairs,
                    changed,
                );
            }
            remove_matching_saves_restores(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                pairs,
                changed,
            );
        }
        _ => {}
    }
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Remove callee-saved register prologue/epilogue save-restore pairs from
/// `func`.  Returns `true` if any statements were removed.
pub fn remove_callee_save_prologue_epilogue(func: &mut PreHirFunction) -> bool {
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

fn collect_restores(stmts: &[PreHirStmt], restores: &mut HashMap<String, String>) {
    for stmt in stmts {
        if let Some((ptr, reg)) = match_epilogue_restore(stmt) {
            restores.entry(ptr).or_insert(reg);
        }
        match stmt {
            PreHirStmt::Block(body) => collect_restores(body, restores),
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_restores(then_body, restores);
                collect_restores(else_body, restores);
            }
            PreHirStmt::While { body, .. } | PreHirStmt::DoWhile { body, .. } => {
                collect_restores(body, restores)
            }
            PreHirStmt::For { body, .. } => collect_restores(body, restores),
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_restores(&case.body, restores);
                }
                collect_restores(default, restores);
            }
            _ => {}
        }
    }
}

fn count_restores_for_ptr(stmts: &[PreHirStmt], ptr: &str) -> usize {
    let mut count = 0;
    for stmt in stmts {
        if let Some((p, _)) = match_epilogue_restore(stmt) {
            if p == ptr {
                count += 1;
            }
        }
        match stmt {
            PreHirStmt::Block(body) => count += count_restores_for_ptr(body, ptr),
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                count += count_restores_for_ptr(then_body, ptr);
                count += count_restores_for_ptr(else_body, ptr);
            }
            PreHirStmt::While { body, .. } | PreHirStmt::DoWhile { body, .. } => {
                count += count_restores_for_ptr(body, ptr)
            }
            PreHirStmt::For { body, .. } => count += count_restores_for_ptr(body, ptr),
            PreHirStmt::Switch { cases, default, .. } => {
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
fn match_slot_epilogue_restore(stmt: &PreHirStmt) -> Option<(String, String)> {
    let PreHirStmt::Assign { lhs, rhs } = stmt else {
        return None;
    };
    let reg = match lhs {
        PreHirLValue::Var(r) if is_callee_saved(r) => r.as_str(),
        _ => return None,
    };
    let slot_var = match rhs {
        PreHirExpr::Var(v) if looks_like_stack_slot_name(v) => v.as_str(),
        PreHirExpr::Cast { expr: inner, .. } => match inner.as_ref() {
            PreHirExpr::Var(v) if looks_like_stack_slot_name(v) => v.as_str(),
            _ => return None,
        },
        _ => return None,
    };
    Some((slot_var.to_string(), reg.to_string()))
}

fn collect_slot_restores(stmts: &[PreHirStmt], out: &mut Vec<(String, String)>) {
    for stmt in stmts {
        if let Some(pair) = match_slot_epilogue_restore(stmt) {
            out.push(pair);
        }
        match stmt {
            PreHirStmt::Block(body) => collect_slot_restores(body, out),
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_slot_restores(then_body, out);
                collect_slot_restores(else_body, out);
            }
            PreHirStmt::While { body, .. } | PreHirStmt::DoWhile { body, .. } => {
                collect_slot_restores(body, out)
            }
            PreHirStmt::For { body, .. } => collect_slot_restores(body, out),
            PreHirStmt::Switch { cases, default, .. } => {
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
fn count_var_definitions(stmts: &[PreHirStmt], var: &str) -> usize {
    stmts.iter().map(|s| count_var_defs_in_stmt(s, var)).sum()
}

fn count_var_defs_in_stmt(stmt: &PreHirStmt, var: &str) -> usize {
    match stmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(lhs),
            ..
        } if lhs == var => 1,
        PreHirStmt::Block(body) => count_var_definitions(body, var),
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => count_var_definitions(then_body, var) + count_var_definitions(else_body, var),
        PreHirStmt::While { body, .. } | PreHirStmt::DoWhile { body, .. } => {
            count_var_definitions(body, var)
        }
        PreHirStmt::For {
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
        PreHirStmt::Switch { cases, default, .. } => {
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
    stmts: &mut Vec<PreHirStmt>,
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
    stmt: &mut PreHirStmt,
    slots: &HashSet<String>,
    changed: &mut bool,
) {
    match stmt {
        PreHirStmt::Block(body) => remove_orphaned_slot_restores_from_stmts(
            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
            slots,
            changed,
        ),
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            remove_orphaned_slot_restores_from_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                slots,
                changed,
            );
            remove_orphaned_slot_restores_from_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                slots,
                changed,
            );
        }
        PreHirStmt::While { body, .. } | PreHirStmt::DoWhile { body, .. } => {
            remove_orphaned_slot_restores_from_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                slots,
                changed,
            )
        }
        PreHirStmt::For { body, .. } => remove_orphaned_slot_restores_from_stmts(
            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
            slots,
            changed,
        ),
        PreHirStmt::Switch { cases, default, .. } => {
            for case in cases.iter_mut() {
                remove_orphaned_slot_restores_from_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                    slots,
                    changed,
                );
            }
            remove_orphaned_slot_restores_from_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                slots,
                changed,
            );
        }
        _ => {}
    }
}

/// Remove epilogue restores of the form `callee_saved_reg = home_slot_var` where
/// `home_slot_var` has no remaining definition in the function body (its prologue
/// save was already stripped by `remove_entry_stack_scaffold_stores`).
fn remove_orphaned_slot_epilogue_restores(func: &mut PreHirFunction) -> bool {
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
/// 2. `callee_saved_reg` has no `PreHirBinding` in `func.locals` (was never
///    materialized as a named local).
/// 3. `callee_saved_reg` has zero rvalue uses anywhere in the function body.
///
/// This arises when the compiler stores a parameter in a callee-saved register
/// (`rbx = param_3`) to keep it across calls, but a copy-propagation pass
/// later replaces every use of `rbx` with the original parameter, leaving the
/// initial assignment dead and the register name undeclared in the output.
pub fn remove_dead_callee_saved_param_loads(func: &mut PreHirFunction) -> bool {
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

    // Also remove any corresponding PreHirBinding from locals (may have been
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
fn collect_callee_assign_targets_no_slot_rhs(stmts: &[PreHirStmt], out: &mut HashSet<String>) {
    for stmt in stmts {
        match stmt {
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var(name),
                rhs,
            } if is_callee_saved(name) => {
                let rhs_is_slot =
                    var_name_through_cast(rhs).is_some_and(looks_like_stack_slot_name);
                if !rhs_is_slot {
                    out.insert(name.clone());
                }
            }
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_callee_assign_targets_no_slot_rhs(then_body, out);
                collect_callee_assign_targets_no_slot_rhs(else_body, out);
            }
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. }
            | PreHirStmt::For { body, .. } => {
                collect_callee_assign_targets_no_slot_rhs(body, out);
            }
            PreHirStmt::Switch { cases, default, .. } => {
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
    stmts: &mut Vec<PreHirStmt>,
    dead: &HashSet<String>,
    changed: &mut bool,
) {
    for stmt in stmts.iter_mut() {
        match stmt {
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                remove_dead_callee_assigns_from_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                    dead,
                    changed,
                );
                remove_dead_callee_assigns_from_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                    dead,
                    changed,
                );
            }
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. }
            | PreHirStmt::For { body, .. } => {
                remove_dead_callee_assigns_from_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    dead,
                    changed,
                );
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases.iter_mut() {
                    remove_dead_callee_assigns_from_stmts(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                        dead,
                        changed,
                    );
                }
                remove_dead_callee_assigns_from_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                    dead,
                    changed,
                );
            }
            _ => {}
        }
    }
    let before = stmts.len();
    stmts.retain(|stmt| {
        !matches!(stmt, PreHirStmt::Assign { lhs: PreHirLValue::Var(name), .. } if dead.contains(name))
    });
    if stmts.len() < before {
        *changed = true;
    }
}

/// Remove the entry spill of a callee-saved register into a local that is
/// overwritten before it is ever read:
///
/// ```text
/// local_8 = rbp;      // rbp is never assigned anywhere -- this is its
///                     // entry value, i.e. the caller's frame pointer
/// local_8 = 0;        // overwrites it before any read
/// ```
///
/// `clang -O0` and `gcc -m32 -O0` write the frame-pointer save this way: into
/// a slot that normalization later names as an ordinary local rather than as
/// the stack scaffold `remove_callee_save_prologue_epilogue` recognizes. That
/// pass needs a matching `reg = *p` restore to fire; there is none here,
/// because the value is dead rather than restored, so the store survives into
/// the output and reads a register the function never defines.
///
/// Removal is sound because both halves are unobservable: the value read is
/// the register's entry value (undefined within this function's own
/// semantics), and the local it lands in is reassigned before anything reads
/// it. `defuse_dead_assignment_pass` does not cover this because it only
/// considers temp-like names, and these destinations are named locals.
pub fn remove_entry_register_spills(func: &mut PreHirFunction) -> bool {
    // Callee-saved registers with no definition anywhere: their only value is
    // the one they held on entry.
    let mut undefined_regs: HashSet<String> = HashSet::default();
    for b in &func.locals {
        if is_callee_saved(&b.name) && count_var_definitions(&func.body, &b.name) == 0 {
            undefined_regs.insert(b.name.clone());
        }
    }
    if undefined_regs.is_empty() {
        return false;
    }
    // A parameter is a real value even if it shares a register's name.
    for p in &func.params {
        undefined_regs.remove(&p.name);
    }
    if undefined_regs.is_empty() {
        return false;
    }

    let mut changed = false;
    remove_entry_register_spills_in_stmts(&mut func.body, &undefined_regs, &mut changed);

    if changed {
        // Drop register bindings that no longer appear anywhere.
        func.locals.retain(|b| {
            !undefined_regs.contains(&b.name) || count_ptr_var_rvalue_uses(&func.body, &b.name) > 0
        });
    }
    changed
}

/// Does `stmt` assign `name` before reading it, reading it, or neither?
enum DstFate {
    /// Overwritten here without being read first -- the earlier value is dead.
    Overwritten,
    /// Read here -- the earlier value is live.
    Read,
    /// Untouched.
    Untouched,
}

fn dst_fate_in_stmt(stmt: &PreHirStmt, dst: &str) -> DstFate {
    match stmt {
        // `dst = expr`: the RHS is evaluated first, so a use there wins.
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name),
            rhs,
        } if name == dst => {
            if count_ptr_in_expr(rhs, dst) > 0 {
                DstFate::Read
            } else {
                DstFate::Overwritten
            }
        }
        // `for (dst = ...; ...)`: the initializer runs before the condition,
        // the update, and the body, so it kills whatever came before.
        PreHirStmt::For {
            init: Some(init), ..
        } => {
            if matches!(dst_fate_in_stmt(init, dst), DstFate::Overwritten) {
                DstFate::Overwritten
            } else if count_ptr_in_stmt(stmt, dst) > 0 {
                DstFate::Read
            } else {
                DstFate::Untouched
            }
        }
        _ => {
            if count_ptr_in_stmt(stmt, dst) > 0 || count_var_defs_in_stmt(stmt, dst) > 0 {
                // Anything else that touches `dst` is treated as a read; this
                // pass never removes a store whose value might be observed.
                DstFate::Read
            } else {
                DstFate::Untouched
            }
        }
    }
}

fn remove_entry_register_spills_in_stmts(
    stmts: &mut Vec<PreHirStmt>,
    undefined_regs: &HashSet<String>,
    changed: &mut bool,
) {
    let mut dead_idx: Vec<usize> = Vec::new();
    for (i, stmt) in stmts.iter().enumerate() {
        let PreHirStmt::Assign {
            lhs: PreHirLValue::Var(dst),
            rhs,
        } = stmt
        else {
            continue;
        };
        // The RHS must be nothing but a read of an undefined callee-saved
        // register (possibly through a cast) -- never an address computation
        // such as `rbp - 24`, which is a real stack address.
        let Some(reg) = var_name_through_cast(rhs) else {
            continue;
        };
        if dst == reg || !undefined_regs.contains(reg) {
            continue;
        }
        // `dst` must not be observed before it is overwritten.
        let mut dead = true;
        for later in &stmts[i + 1..] {
            match dst_fate_in_stmt(later, dst) {
                DstFate::Overwritten => break,
                DstFate::Read => {
                    dead = false;
                    break;
                }
                DstFate::Untouched => {}
            }
        }
        if dead {
            dead_idx.push(i);
        }
    }

    if !dead_idx.is_empty() {
        let mut i = 0usize;
        stmts.retain(|_| {
            let keep = !dead_idx.contains(&i);
            i += 1;
            keep
        });
        *changed = true;
    }

    for stmt in stmts.iter_mut() {
        match stmt {
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                remove_entry_register_spills_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                    undefined_regs,
                    changed,
                );
                remove_entry_register_spills_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                    undefined_regs,
                    changed,
                );
            }
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. }
            | PreHirStmt::For { body, .. } => {
                remove_entry_register_spills_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    undefined_regs,
                    changed,
                );
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases.iter_mut() {
                    remove_entry_register_spills_in_stmts(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                        undefined_regs,
                        changed,
                    );
                }
                remove_entry_register_spills_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                    undefined_regs,
                    changed,
                );
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // prelude via parent
    use fission_midend_prehir::PreHirBinding;

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

    fn scaffold_store(ptr: &str, rhs: &str) -> PreHirStmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Deref {
                ptr: Box::new(PreHirExpr::PtrOffset {
                    base: Box::new(PreHirExpr::Var(ptr.to_owned())),
                    offset: -8,
                }),
                ty: u64_ty(),
            },
            rhs: PreHirExpr::Var(rhs.to_owned()),
        }
    }

    fn spill_assign(dst: &str, rhs: PreHirExpr) -> PreHirStmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(dst.to_owned()),
            rhs,
        }
    }

    fn reg_binding(name: &str) -> PreHirBinding {
        PreHirBinding {
            name: name.to_owned(),
            ty: u64_ty(),
            surface_type_name: None,
            origin: None,
            initializer: None,
        }
    }

    /// `local_8 = rbp; local_8 = 0;` -- rbp is never defined, and the value it
    /// ARM's `push {r4,r5,r6,lr}`: one pointer copied out of `sp`, walked
    /// down four bytes per register, stored through, and copied back. The
    /// pointer is a register the ARM SLEIGH spec declares, so it survives
    /// naming as `mult_addr` -- a name this pass could never have listed,
    /// which is why it matches on where the pointer came from instead.
    fn arm_multi_push(saved: &[&str]) -> Vec<PreHirStmt> {
        let mut body = vec![spill_assign("mult_addr", PreHirExpr::Var("sp".to_owned()))];
        for reg in saved {
            body.push(spill_assign(
                "mult_addr",
                PreHirExpr::Binary {
                    op: PreHirBinaryOp::Sub,
                    lhs: Box::new(PreHirExpr::Var("mult_addr".to_owned())),
                    rhs: Box::new(PreHirExpr::Const(4, u32_ty())),
                    ty: u32_ty(),
                },
            ));
            body.push(PreHirStmt::Assign {
                lhs: PreHirLValue::Deref {
                    ptr: Box::new(PreHirExpr::Var("mult_addr".to_owned())),
                    ty: u32_ty(),
                },
                rhs: PreHirExpr::Var((*reg).to_owned()),
            });
        }
        body
    }

    #[test]
    fn removes_an_arm_multi_register_push() {
        let mut body = arm_multi_push(&["lr", "r6", "r5", "r4"]);
        body.push(PreHirStmt::Return(Some(PreHirExpr::Var("r0".to_owned()))));
        let mut func = PreHirFunction {
            name: "memset".to_owned(),
            int_param_offsets: Vec::new(),
            locals: vec![reg_binding("mult_addr"), reg_binding("sp")],
            body,
            ..Default::default()
        };
        assert!(remove_entry_stack_scaffold_stores(&mut func));
        assert_eq!(
            func.body.len(),
            1,
            "the whole push should be gone, leaving the return: {:?}",
            func.body
        );
    }

    /// `push {r0,r1,r2,r3}` is not a register save -- it is a varargs spill,
    /// and the function reads those slots back as its `va_list`. Removing it
    /// would delete the arguments, so a non-callee-saved register stops the
    /// prefix rather than widening it.
    #[test]
    fn keeps_an_arm_push_of_argument_registers() {
        let mut body = arm_multi_push(&["r3", "r2", "r1", "r0"]);
        body.push(PreHirStmt::Return(Some(PreHirExpr::Var("r0".to_owned()))));
        let before = body.len();
        let mut func = PreHirFunction {
            name: "vprintf".to_owned(),
            int_param_offsets: Vec::new(),
            locals: vec![reg_binding("mult_addr"), reg_binding("sp")],
            body,
            ..Default::default()
        };
        assert!(!remove_entry_stack_scaffold_stores(&mut func));
        assert_eq!(func.body.len(), before);
    }

    /// A walking pointer the body reads afterwards is a local the function
    /// uses, not scaffold: the stores through it have a reader.
    #[test]
    fn keeps_a_walking_pointer_the_body_reads() {
        let mut body = arm_multi_push(&["r4"]);
        body.push(PreHirStmt::Return(Some(PreHirExpr::Var(
            "mult_addr".to_owned(),
        ))));
        let before = body.len();
        let mut func = PreHirFunction {
            name: "escapes".to_owned(),
            int_param_offsets: Vec::new(),
            locals: vec![reg_binding("mult_addr"), reg_binding("sp")],
            body,
            ..Default::default()
        };
        assert!(!remove_entry_stack_scaffold_stores(&mut func));
        assert_eq!(func.body.len(), before);
    }

    /// lands in is overwritten before any read. This is the clang -O0 frame
    /// pointer save that reached the corpus output as an undefined read.
    #[test]
    fn removes_entry_frame_register_spill_overwritten_before_read() {
        let mut func = PreHirFunction {
            name: "count_bits".to_owned(),
            int_param_offsets: Vec::new(),
            locals: vec![reg_binding("rbp"), reg_binding("local_8")],
            body: vec![
                spill_assign("local_8", PreHirExpr::Var("rbp".to_owned())),
                spill_assign("local_8", PreHirExpr::Const(0, u32_ty())),
                PreHirStmt::Return(Some(PreHirExpr::Var("local_8".to_owned()))),
            ],
            ..Default::default()
        };
        assert!(remove_entry_register_spills(&mut func));
        assert_eq!(
            func.body.len(),
            2,
            "spill statement should be gone: {:?}",
            func.body
        );
        assert!(
            !func.locals.iter().any(|b| b.name == "rbp"),
            "the undefined register binding should be dropped too"
        );
    }

    /// The same spill, but the destination is read before it is reassigned.
    /// Removing it would change what the output says, so it must stay.
    #[test]
    fn keeps_entry_register_spill_whose_destination_is_read() {
        let mut func = PreHirFunction {
            name: "keeps".to_owned(),
            int_param_offsets: Vec::new(),
            locals: vec![reg_binding("rbp"), reg_binding("local_8")],
            body: vec![
                spill_assign("local_8", PreHirExpr::Var("rbp".to_owned())),
                PreHirStmt::Return(Some(PreHirExpr::Var("local_8".to_owned()))),
            ],
            ..Default::default()
        };
        assert!(!remove_entry_register_spills(&mut func));
        assert_eq!(func.body.len(), 2);
    }

    /// `xVar10 = rbp - 24` is a real frame-relative address, not a spill of the
    /// register's own value, and must survive even when its destination is dead.
    #[test]
    fn keeps_frame_relative_address_computation() {
        let mut func = PreHirFunction {
            name: "addr".to_owned(),
            int_param_offsets: Vec::new(),
            locals: vec![reg_binding("rbp"), reg_binding("xVar10")],
            body: vec![
                spill_assign(
                    "xVar10",
                    PreHirExpr::Binary {
                        op: PreHirBinaryOp::Sub,
                        lhs: Box::new(PreHirExpr::Var("rbp".to_owned())),
                        rhs: Box::new(PreHirExpr::Const(24, u64_ty())),
                        ty: u64_ty(),
                    },
                ),
                spill_assign("xVar10", PreHirExpr::Const(0, u32_ty())),
                PreHirStmt::Return(Some(PreHirExpr::Var("xVar10".to_owned()))),
            ],
            ..Default::default()
        };
        assert!(!remove_entry_register_spills(&mut func));
        assert_eq!(func.body.len(), 3);
    }

    /// A register the function actually writes is a normal value, not an entry
    /// spill, so its store is left alone.
    #[test]
    fn keeps_spill_of_a_register_the_function_defines() {
        let mut func = PreHirFunction {
            name: "defined".to_owned(),
            int_param_offsets: Vec::new(),
            locals: vec![reg_binding("rbx"), reg_binding("local_8")],
            body: vec![
                spill_assign("rbx", PreHirExpr::Const(7, u32_ty())),
                spill_assign("local_8", PreHirExpr::Var("rbx".to_owned())),
                spill_assign("local_8", PreHirExpr::Const(0, u32_ty())),
                PreHirStmt::Return(None),
            ],
            ..Default::default()
        };
        assert!(!remove_entry_register_spills(&mut func));
        assert_eq!(func.body.len(), 4);
    }

    /// `for (local_4 = 0; ...)` kills the earlier value the same way a plain
    /// assignment does -- the initializer runs before condition, body, update.
    #[test]
    fn treats_a_for_initializer_as_an_overwrite() {
        let mut func = PreHirFunction {
            name: "loop_init".to_owned(),
            int_param_offsets: Vec::new(),
            locals: vec![reg_binding("ebp"), reg_binding("local_4")],
            body: vec![
                spill_assign("local_4", PreHirExpr::Var("ebp".to_owned())),
                PreHirStmt::For {
                    init: Some(Box::new(spill_assign(
                        "local_4",
                        PreHirExpr::Const(0, u32_ty()),
                    ))),
                    cond: Some(PreHirExpr::Var("local_4".to_owned())),
                    update: None,
                    body: std::rc::Rc::new(Vec::new()),
                },
                PreHirStmt::Return(None),
            ],
            ..Default::default()
        };
        assert!(remove_entry_register_spills(&mut func));
        assert_eq!(func.body.len(), 2, "{:?}", func.body);
    }

    #[test]
    fn removes_contiguous_entry_stack_scaffold_stores() {
        let mut func = PreHirFunction {
            name: "test".to_owned(),
            int_param_offsets: Vec::new(),
            body: vec![
                scaffold_store("var_20", "var_38"),
                scaffold_store("xVar0", "param_2"),
                PreHirStmt::Return(None),
            ],
            ..Default::default()
        };

        assert!(remove_entry_stack_scaffold_stores(&mut func));
        assert_eq!(func.body, vec![PreHirStmt::Return(None)]);
    }

    #[test]
    fn removes_aarch64_sp_based_entry_callee_saved_scaffold() {
        let mut func = PreHirFunction {
            name: "test".to_owned(),
            int_param_offsets: Vec::new(),
            body: vec![
                scaffold_store("sp", "x29"),
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Deref {
                        ptr: Box::new(PreHirExpr::PtrOffset {
                            base: Box::new(PreHirExpr::Var("sp".to_owned())),
                            offset: 8,
                        }),
                        ty: u64_ty(),
                    },
                    rhs: PreHirExpr::Var("x30".to_owned()),
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Deref {
                        ptr: Box::new(PreHirExpr::PtrOffset {
                            base: Box::new(PreHirExpr::Var("sp".to_owned())),
                            offset: 16,
                        }),
                        ty: u64_ty(),
                    },
                    rhs: PreHirExpr::Var("x20".to_owned()),
                },
                PreHirStmt::Return(Some(PreHirExpr::Var("param_1".to_owned()))),
            ],
            ..Default::default()
        };

        assert!(remove_entry_stack_scaffold_stores(&mut func));
        assert_eq!(
            func.body,
            vec![PreHirStmt::Return(Some(PreHirExpr::Var(
                "param_1".to_owned()
            )))]
        );
    }

    #[test]
    fn removes_aarch64_entry_stack_alias_callee_saved_scaffold() {
        let mut func = PreHirFunction {
            name: "test".to_owned(),
            int_param_offsets: Vec::new(),
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("xVar2".to_owned()),
                    rhs: PreHirExpr::PtrOffset {
                        base: Box::new(PreHirExpr::Var("sp".to_owned())),
                        offset: 16,
                    },
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Deref {
                        ptr: Box::new(PreHirExpr::Var("xVar2".to_owned())),
                        ty: u64_ty(),
                    },
                    rhs: PreHirExpr::Var("x20".to_owned()),
                },
                PreHirStmt::Return(Some(PreHirExpr::Var("param_1".to_owned()))),
            ],
            ..Default::default()
        };

        assert!(remove_entry_stack_scaffold_stores(&mut func));
        assert_eq!(
            func.body,
            vec![PreHirStmt::Return(Some(PreHirExpr::Var(
                "param_1".to_owned()
            )))]
        );
    }

    #[test]
    fn removes_arm32_uvar_stack_alias_callee_saved_scaffold() {
        let mut func = PreHirFunction {
            name: "test".to_owned(),
            int_param_offsets: Vec::new(),
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("uVar0".to_owned()),
                    rhs: PreHirExpr::Binary {
                        op: PreHirBinaryOp::Sub,
                        lhs: Box::new(PreHirExpr::Var("sp".to_owned())),
                        rhs: Box::new(PreHirExpr::Const(4, u32_ty())),
                        ty: u32_ty(),
                    },
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Deref {
                        ptr: Box::new(PreHirExpr::Var("uVar0".to_owned())),
                        ty: u32_ty(),
                    },
                    rhs: PreHirExpr::Var("lr".to_owned()),
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("uVar1".to_owned()),
                    rhs: PreHirExpr::Binary {
                        op: PreHirBinaryOp::Sub,
                        lhs: Box::new(PreHirExpr::Var("uVar0".to_owned())),
                        rhs: Box::new(PreHirExpr::Const(1, u32_ty())),
                        ty: u32_ty(),
                    },
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Deref {
                        ptr: Box::new(PreHirExpr::Var("uVar1".to_owned())),
                        ty: u32_ty(),
                    },
                    rhs: PreHirExpr::Var("r11".to_owned()),
                },
                PreHirStmt::Return(Some(PreHirExpr::Var("param_1".to_owned()))),
            ],
            ..Default::default()
        };

        assert!(remove_entry_stack_scaffold_stores(&mut func));
        assert_eq!(
            func.body,
            vec![PreHirStmt::Return(Some(PreHirExpr::Var(
                "param_1".to_owned()
            )))]
        );
    }

    #[test]
    fn keeps_entry_stack_alias_when_used_after_prefix() {
        let mut func = PreHirFunction {
            name: "test".to_owned(),
            int_param_offsets: Vec::new(),
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("xVar2".to_owned()),
                    rhs: PreHirExpr::PtrOffset {
                        base: Box::new(PreHirExpr::Var("sp".to_owned())),
                        offset: 16,
                    },
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Deref {
                        ptr: Box::new(PreHirExpr::Var("xVar2".to_owned())),
                        ty: u64_ty(),
                    },
                    rhs: PreHirExpr::Var("x20".to_owned()),
                },
                PreHirStmt::Expr(PreHirExpr::Var("xVar2".to_owned())),
            ],
            ..Default::default()
        };

        assert!(!remove_entry_stack_scaffold_stores(&mut func));
        assert_eq!(func.body.len(), 3);
    }

    #[test]
    fn removes_contiguous_entry_stack_slot_callee_saved_saves() {
        let mut func = PreHirFunction {
            name: "test".to_owned(),
            int_param_offsets: Vec::new(),
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("home_0".to_owned()),
                    rhs: PreHirExpr::Var("r15".to_owned()),
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("home_0".to_owned()),
                    rhs: PreHirExpr::Var("param_1".to_owned()),
                },
                PreHirStmt::Return(None),
            ],
            ..Default::default()
        };

        assert!(remove_entry_stack_scaffold_stores(&mut func));
        assert_eq!(func.body, vec![PreHirStmt::Return(None)]);
    }

    #[test]
    fn keeps_live_stack_slot_initializers_after_callee_saved_prefix() {
        let mut func = PreHirFunction {
            name: "test".to_owned(),
            int_param_offsets: Vec::new(),
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("home_0".to_owned()),
                    rhs: PreHirExpr::Var("r15".to_owned()),
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("local_8".to_owned()),
                    rhs: PreHirExpr::Var("param_1".to_owned()),
                },
                PreHirStmt::Return(Some(PreHirExpr::Var("local_8".to_owned()))),
            ],
            ..Default::default()
        };

        assert!(remove_entry_stack_scaffold_stores(&mut func));
        assert_eq!(func.body.len(), 2);
        assert!(matches!(
            &func.body[0],
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var(lhs),
                rhs: PreHirExpr::Var(rhs),
            } if lhs == "local_8" && rhs == "param_1"
        ));
    }

    #[test]
    fn removes_entry_stack_slot_callee_saved_saves_inside_entry_block() {
        let mut func = PreHirFunction {
            name: "test".to_owned(),
            int_param_offsets: Vec::new(),
            body: vec![PreHirStmt::Block(
                vec![
                    PreHirStmt::Assign {
                        lhs: PreHirLValue::Var("home_0".to_owned()),
                        rhs: PreHirExpr::Var("r15".to_owned()),
                    },
                    PreHirStmt::Assign {
                        lhs: PreHirLValue::Var("home_0".to_owned()),
                        rhs: PreHirExpr::Var("param_1".to_owned()),
                    },
                    PreHirStmt::Return(None),
                ]
                .into(),
            )],
            ..Default::default()
        };

        assert!(remove_entry_stack_scaffold_stores(&mut func));
        assert_eq!(
            func.body,
            vec![PreHirStmt::Block(vec![PreHirStmt::Return(None)].into())]
        );
    }

    #[test]
    fn keeps_entry_stack_slot_initializers_without_callee_saved_evidence() {
        let mut func = PreHirFunction {
            name: "test".to_owned(),
            int_param_offsets: Vec::new(),
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("local_8".to_owned()),
                    rhs: PreHirExpr::Var("param_1".to_owned()),
                },
                PreHirStmt::Return(None),
            ],
            ..Default::default()
        };

        assert!(!remove_entry_stack_scaffold_stores(&mut func));
        assert_eq!(func.body.len(), 2);
    }

    #[test]
    fn keeps_non_entry_and_non_scaffold_stores() {
        let mut func = PreHirFunction {
            name: "test".to_owned(),
            int_param_offsets: Vec::new(),
            body: vec![
                PreHirStmt::Expr(PreHirExpr::Const(1, u64_ty())),
                scaffold_store("var_20", "var_38"),
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Deref {
                        ptr: Box::new(PreHirExpr::Var("param_1".to_owned())),
                        ty: u64_ty(),
                    },
                    rhs: PreHirExpr::Var("param_2".to_owned()),
                },
            ],
            ..Default::default()
        };

        assert!(!remove_entry_stack_scaffold_stores(&mut func));
        assert_eq!(func.body.len(), 3);
    }

    // ── Orphaned stack-slot epilogue restore tests ─────────────────────────────

    fn slot_restore(reg: &str, slot: &str) -> PreHirStmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(reg.to_owned()),
            rhs: PreHirExpr::Var(slot.to_owned()),
        }
    }

    #[test]
    fn removes_orphaned_slot_epilogue_restore_with_uppercase_register() {
        let mut func = PreHirFunction {
            name: "fill_matrix".to_owned(),
            int_param_offsets: Vec::new(),
            body: vec![
                PreHirStmt::Expr(PreHirExpr::Const(42, u64_ty())),
                slot_restore("RDI", "home_0"),
                PreHirStmt::Return(None),
            ],
            locals: vec![PreHirBinding {
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
        let mut func = PreHirFunction {
            name: "test".to_owned(),
            int_param_offsets: Vec::new(),
            body: vec![
                PreHirStmt::Expr(PreHirExpr::Const(42, u64_ty())),
                slot_restore("rbx", "home_0"),
                PreHirStmt::Return(None),
            ],
            locals: vec![PreHirBinding {
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
        let mut func = PreHirFunction {
            name: "test".to_owned(),
            int_param_offsets: Vec::new(),
            body: vec![
                PreHirStmt::Expr(PreHirExpr::Const(1, u64_ty())),
                slot_restore("rbx", "home_0"),
                slot_restore("rsi", "home_8"),
                PreHirStmt::Return(None),
            ],
            locals: vec![
                PreHirBinding {
                    name: "home_0".to_owned(),
                    ty: u64_ty(),
                    surface_type_name: None,
                    origin: None,
                    initializer: None,
                },
                PreHirBinding {
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
        let mut func = PreHirFunction {
            name: "test".to_owned(),
            int_param_offsets: Vec::new(),
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("home_0".to_owned()),
                    rhs: PreHirExpr::Var("param_1".to_owned()),
                },
                slot_restore("rbx", "home_0"),
                PreHirStmt::Return(None),
            ],
            locals: vec![PreHirBinding {
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
        let mut func = PreHirFunction {
            name: "test".to_owned(),
            int_param_offsets: Vec::new(),
            body: vec![
                PreHirStmt::If {
                    cond: PreHirExpr::Const(1, u64_ty()),
                    then_body: vec![slot_restore("rsi", "home_0")].into(),
                    else_body: vec![].into(),
                },
                PreHirStmt::Return(None),
            ],
            locals: vec![PreHirBinding {
                name: "home_0".to_owned(),
                ty: u64_ty(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            }],
            ..Default::default()
        };

        assert!(remove_callee_save_prologue_epilogue(&mut func));
        if let PreHirStmt::If { then_body, .. } = &func.body[0] {
            assert!(
                then_body.is_empty(),
                "orphaned restore inside if-branch should be removed"
            );
        }
    }

    // ── remove_dead_callee_saved_param_loads ──────────────────────────────────

    fn assign_var(lhs: &str, rhs: PreHirExpr) -> PreHirStmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(lhs.to_owned()),
            rhs,
        }
    }

    fn var(name: &str) -> PreHirExpr {
        PreHirExpr::Var(name.to_owned())
    }

    #[test]
    fn removes_dead_undeclared_callee_saved_assignment() {
        // rbx = param_3  but rbx has no binding and is never read → remove.
        let mut func = PreHirFunction {
            name: "test".to_owned(),
            int_param_offsets: Vec::new(),
            body: vec![assign_var("rbx", var("param_3")), PreHirStmt::Return(None)],
            locals: vec![], // rbx has no PreHirBinding
            ..Default::default()
        };

        assert!(remove_callee_save_prologue_epilogue(&mut func));
        assert_eq!(func.body, vec![PreHirStmt::Return(None)]);
    }

    #[test]
    fn keeps_live_callee_saved_assignment_that_is_read() {
        // rsi = param_2, but rsi IS read in the condition → keep.
        let mut func = PreHirFunction {
            name: "test".to_owned(),
            int_param_offsets: Vec::new(),
            body: vec![
                assign_var("rsi", var("param_2")),
                PreHirStmt::If {
                    cond: var("rsi"),
                    then_body: vec![PreHirStmt::Return(None)].into(),
                    else_body: vec![].into(),
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
        let mut func = PreHirFunction {
            name: "test".to_owned(),
            int_param_offsets: Vec::new(),
            body: vec![assign_var("rbx", var("param_3")), PreHirStmt::Return(None)],
            locals: vec![PreHirBinding {
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
            vec![PreHirStmt::Return(None)],
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
        let mut func = PreHirFunction {
            name: "fill_matrix".to_owned(),
            int_param_offsets: Vec::new(),
            body: vec![PreHirStmt::Return(None)],
            locals: vec![PreHirBinding {
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
