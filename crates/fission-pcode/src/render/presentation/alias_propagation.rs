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
pub(super) fn propagate_pure_var_aliases(
    func: &mut HirFunction,
    globals: &HashSet<String>,
) -> bool {
    let formal: HashSet<&str> = func.params.iter().map(|b| b.name.as_str()).collect();
    let mut def_counts = HashMap::new();
    count_defs_in_stmts(&func.body, &mut def_counts);
    // A formal is only a stable alias source when the body never assigns to
    // it.  A loop-carried register can initially be represented as
    // `carrier = param_2` and later write the remainder back to `param_2`.
    // Replacing every carrier use with the formal would then move the use
    // across that write and change the loop's value flow.
    let stable_formal: HashSet<&str> = formal
        .iter()
        .copied()
        .filter(|name| def_counts.get(*name).copied().unwrap_or(0) == 0)
        .collect();

    // Collect x → y for single-def pure var copies. Resolve short chains.
    let mut copy_map: HashMap<String, String> = HashMap::new();
    let mut used_before: HashSet<String> = HashSet::new();
    collect_pure_var_copies(
        &func.body,
        &stable_formal,
        &def_counts,
        globals,
        &mut used_before,
        &mut copy_map,
    );
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

#[cfg(test)]
mod tests {
    use super::*;

    fn int32() -> NirType {
        NirType::Int {
            bits: 32,
            signed: false,
        }
    }

    fn binding(name: &str) -> NirBinding {
        NirBinding {
            name: name.to_string(),
            ty: int32(),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        }
    }

    #[test]
    fn does_not_fold_alias_from_formal_reassigned_in_loop() {
        let mut func = HirFunction {
            params: vec![binding("param_2")],
            locals: vec![binding("carrier")],
            body: vec![
                HirStmt::Assign {
                    lhs: HirLValue::Var("carrier".to_string()),
                    rhs: HirExpr::Var("param_2".to_string()),
                },
                HirStmt::DoWhile {
                    body: vec![HirStmt::Assign {
                        lhs: HirLValue::Var("param_2".to_string()),
                        rhs: HirExpr::Const(0, int32()),
                    }],
                    cond: HirExpr::Var("param_2".to_string()),
                },
                HirStmt::Return(Some(HirExpr::Var("carrier".to_string()))),
            ],
            ..Default::default()
        };

        assert!(!propagate_pure_var_aliases(&mut func, &HashSet::new()));
        assert!(matches!(
            &func.body[0],
            HirStmt::Assign {
                lhs: HirLValue::Var(name),
                rhs: HirExpr::Var(source),
            } if name == "carrier" && source == "param_2"
        ));
        assert!(matches!(
            &func.body[2],
            HirStmt::Return(Some(HirExpr::Var(name))) if name == "carrier"
        ));
    }

    #[test]
    fn folds_alias_from_unchanged_formal() {
        let mut func = HirFunction {
            params: vec![binding("param_1")],
            locals: vec![binding("carrier")],
            body: vec![
                HirStmt::Assign {
                    lhs: HirLValue::Var("carrier".to_string()),
                    rhs: HirExpr::Var("param_1".to_string()),
                },
                HirStmt::Return(Some(HirExpr::Var("carrier".to_string()))),
            ],
            ..Default::default()
        };

        assert!(propagate_pure_var_aliases(&mut func, &HashSet::new()));
        assert!(matches!(
            &func.body[0],
            HirStmt::Return(Some(HirExpr::Var(name))) if name == "param_1"
        ));
    }

    #[test]
    fn does_not_propagate_conditional_copy_inside_if() {
        let mut func = HirFunction {
            params: vec![binding("param_1"), binding("cond")],
            locals: vec![binding("x")],
            body: vec![
                HirStmt::If {
                    cond: HirExpr::Var("cond".to_string()),
                    then_body: vec![HirStmt::Assign {
                        lhs: HirLValue::Var("x".to_string()),
                        rhs: HirExpr::Var("param_1".to_string()),
                    }],
                    else_body: Vec::new(),
                },
                HirStmt::Return(Some(HirExpr::Var("x".to_string()))),
            ],
            ..Default::default()
        };

        assert!(!propagate_pure_var_aliases(&mut func, &HashSet::new()));
        assert!(matches!(
            &func.body[1],
            HirStmt::Return(Some(HirExpr::Var(name))) if name == "x"
        ));
    }

    #[test]
    fn does_not_propagate_copy_when_var_used_before_definition() {
        let mut func = HirFunction {
            params: vec![binding("param_1")],
            locals: vec![binding("x"), binding("y")],
            body: vec![
                HirStmt::Assign {
                    lhs: HirLValue::Var("y".to_string()),
                    rhs: HirExpr::Var("x".to_string()),
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("x".to_string()),
                    rhs: HirExpr::Var("param_1".to_string()),
                },
                HirStmt::Return(Some(HirExpr::Var("y".to_string()))),
            ],
            ..Default::default()
        };

        assert!(!propagate_pure_var_aliases(&mut func, &HashSet::new()));
        assert!(matches!(
            &func.body[0],
            HirStmt::Assign {
                lhs: HirLValue::Var(lhs),
                rhs: HirExpr::Var(rhs),
            } if lhs == "y" && rhs == "x"
        ));
    }
}

/// Collect single-def pure variable copies strictly from the top-level
/// unconditional prefix of `stmts` before any branching, looping, or labels.
/// Returns `true` if traversal may continue, or `false` once a control-flow
/// construct is encountered.
fn collect_pure_var_copies(
    stmts: &[HirStmt],
    formal: &HashSet<&str>,
    def_counts: &HashMap<String, usize>,
    globals: &HashSet<String>,
    used_before: &mut HashSet<String>,
    out: &mut HashMap<String, String>,
) -> bool {
    for stmt in stmts {
        match stmt {
            HirStmt::Assign {
                lhs: HirLValue::Var(name),
                rhs: HirExpr::Var(source),
            } if name != source
                && !formal.contains(name.as_str())
                // A global store is an observable effect, not a temporary
                // copy that presentation may erase after substituting uses.
                && !globals.contains(name.as_str())
                && !used_before.contains(name.as_str())
                && (formal.contains(source.as_str()) || out.contains_key(source.as_str()))
                && def_counts.get(name.as_str()).copied().unwrap_or(0) == 1 =>
            {
                // Source must be a stable formal (or a single-def chain rooted
                // at a stable formal): non-formal sources (eax/rax/temps) may
                // be redefined after the copy.
                out.insert(name.clone(), source.clone());
                used_before.insert(source.clone());
            }
            HirStmt::Assign { lhs, rhs } => {
                collect_used_vars_in_lvalue(lhs, used_before);
                collect_free_vars_in_expr(rhs, used_before);
            }
            HirStmt::Expr(expr) => {
                collect_free_vars_in_expr(expr, used_before);
            }
            HirStmt::VaStart {
                va_list,
                last_named_param,
            } => {
                collect_free_vars_in_expr(va_list, used_before);
                used_before.insert(last_named_param.clone());
            }
            HirStmt::Block(body) => {
                if !collect_pure_var_copies(body, formal, def_counts, globals, used_before, out) {
                    return false;
                }
            }
            HirStmt::If { .. }
            | HirStmt::While { .. }
            | HirStmt::DoWhile { .. }
            | HirStmt::For { .. }
            | HirStmt::Switch { .. }
            | HirStmt::Goto(_)
            | HirStmt::Label(_)
            | HirStmt::Return(_)
            | HirStmt::Break
            | HirStmt::Continue => {
                return false;
            }
        }
    }
    true
}

fn collect_used_vars_in_lvalue(lhs: &HirLValue, out: &mut HashSet<String>) {
    match lhs {
        HirLValue::Var(_) => {}
        HirLValue::Deref { ptr, .. } | HirLValue::FieldAccess { base: ptr, .. } => {
            collect_free_vars_in_expr(ptr, out);
        }
        HirLValue::Index { base, index, .. } => {
            collect_free_vars_in_expr(base, out);
            collect_free_vars_in_expr(index, out);
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
