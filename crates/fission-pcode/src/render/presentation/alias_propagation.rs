//! Presentation-only propagation of stable formal aliases.
//!
//! This module removes compiler-generated single-definition copies rooted at
//! formal parameters. It relies on the presentation owner's generic HIR
//! traversal helpers but owns the alias proof and rewrite policy.

use super::*;

/// Substitute single-def pure `x = y` aliases when `y` is a **stable** name.
///
/// Only formals (and chains eventually rooted at formals) are eligible sources.
/// Copying a mutable register surface (`rbx = rax` after a call) must not
/// rewrite later uses of `rbx` after `rax` is reassigned — that turns
/// `rax = f(); rbx = rax; rax = g(); rax += rbx` into `rax += rax`.
pub(super) fn propagate_pure_var_aliases(func: &mut HirFunction) -> bool {
    let formal: HashSet<&str> = func.params.iter().map(|b| b.name.as_str()).collect();
    let mut def_counts = HashMap::new();
    count_defs_in_stmts(&func.body, &mut def_counts);

    // Collect x → y for single-def pure var copies. Resolve short chains.
    let mut copy_map: HashMap<String, String> = HashMap::new();
    collect_pure_var_copies(&func.body, &formal, &def_counts, &mut copy_map);
    if copy_map.is_empty() {
        return false;
    }

    // Resolve chains x → y → z to x → z (bounded).
    let keys: Vec<String> = copy_map.keys().cloned().collect();
    for k in keys {
        let mut seen = HashSet::new();
        let mut cur = k.clone();
        while let Some(next) = copy_map.get(&cur).cloned() {
            if !seen.insert(cur.clone()) {
                break;
            }
            cur = next;
        }
        if let Some(src) = copy_map.get(&k) {
            if src != &cur {
                copy_map.insert(k, cur);
            }
        }
    }

    let mut changed = false;
    for (name, source) in &copy_map {
        let replacement = HirExpr::Var(source.clone());
        for stmt in &mut func.body {
            replace_var_in_stmt(stmt, name, &replacement);
        }
        changed = true;
    }
    changed |= remove_copy_assigns(&mut func.body, &copy_map);
    changed
}

fn collect_pure_var_copies(
    stmts: &[HirStmt],
    formal: &HashSet<&str>,
    def_counts: &HashMap<String, usize>,
    out: &mut HashMap<String, String>,
) {
    for stmt in stmts {
        match stmt {
            HirStmt::Assign {
                lhs: HirLValue::Var(name),
                rhs: HirExpr::Var(source),
            } if name != source
                && !formal.contains(name.as_str())
                && formal.contains(source.as_str())
                && def_counts.get(name.as_str()).copied().unwrap_or(0) == 1 =>
            {
                // Source must be a formal: non-formal sources (eax/rax/temps)
                // may be redefined after the copy.
                out.insert(name.clone(), source.clone());
            }
            HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
                collect_pure_var_copies(body, formal, def_counts, out)
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_pure_var_copies(then_body, formal, def_counts, out);
                collect_pure_var_copies(else_body, formal, def_counts, out);
            }
            HirStmt::For {
                init, update, body, ..
            } => {
                if let Some(i) = init {
                    collect_pure_var_copies(
                        std::slice::from_ref(i.as_ref()),
                        formal,
                        def_counts,
                        out,
                    );
                }
                if let Some(u) = update {
                    collect_pure_var_copies(
                        std::slice::from_ref(u.as_ref()),
                        formal,
                        def_counts,
                        out,
                    );
                }
                collect_pure_var_copies(body, formal, def_counts, out);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_pure_var_copies(&case.body, formal, def_counts, out);
                }
                collect_pure_var_copies(default, formal, def_counts, out);
            }
            _ => {}
        }
    }
}

fn remove_copy_assigns(stmts: &mut Vec<HirStmt>, copy_map: &HashMap<String, String>) -> bool {
    let mut changed = false;
    let before = stmts.len();
    stmts.retain(|stmt| {
        !matches!(
            stmt,
            HirStmt::Assign {
                lhs: HirLValue::Var(name),
                rhs: HirExpr::Var(_),
            } if copy_map.contains_key(name)
        )
    });
    if stmts.len() != before {
        changed = true;
    }
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
                changed |= remove_copy_assigns(body, copy_map);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= remove_copy_assigns(then_body, copy_map);
                changed |= remove_copy_assigns(else_body, copy_map);
            }
            HirStmt::For {
                init, update, body, ..
            } => {
                if let Some(i) = init {
                    if let HirStmt::Block(b) = i.as_mut() {
                        changed |= remove_copy_assigns(b, copy_map);
                    }
                }
                if let Some(u) = update {
                    if let HirStmt::Block(b) = u.as_mut() {
                        changed |= remove_copy_assigns(b, copy_map);
                    }
                }
                changed |= remove_copy_assigns(body, copy_map);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= remove_copy_assigns(&mut case.body, copy_map);
                }
                changed |= remove_copy_assigns(default, copy_map);
            }
            _ => {}
        }
    }
    changed
}
