//! Whole-function pre-scans the printer runs before emitting text: assignment
//! counts, goto targets and Win32 return-enum groups.

use super::*;

/// Count statement-level assignments to each plain variable.
pub(super) fn count_var_assignments<'a>(stmts: &'a [HirStmt], out: &mut HashMap<&'a str, usize>) {
    for_each_stmt(stmts, &mut |stmt| {
        if let HirStmt::Assign {
            lhs: HirLValue::Var(name),
            ..
        } = stmt
        {
            *out.entry(name.as_str()).or_default() += 1;
        }
    });
}

/// Apply `visit` to every statement, including nested bodies.
pub(super) fn for_each_stmt<'a>(stmts: &'a [HirStmt], visit: &mut impl FnMut(&'a HirStmt)) {
    for stmt in stmts {
        visit(stmt);
        match stmt {
            HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
                for_each_stmt(body, visit)
            }
            HirStmt::For {
                init, update, body, ..
            } => {
                if let Some(init) = init {
                    for_each_stmt(std::slice::from_ref(&**init), visit);
                }
                if let Some(update) = update {
                    for_each_stmt(std::slice::from_ref(&**update), visit);
                }
                for_each_stmt(body, visit);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                for_each_stmt(then_body, visit);
                for_each_stmt(else_body, visit);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    for_each_stmt(&case.body, visit);
                }
                for_each_stmt(default, visit);
            }
            _ => {}
        }
    }
}

/// Variables holding a known Win32 API's return value, mapped to the enum group
/// win32metadata says that return carries.
///
/// `WaitForSingleObject`'s result reaches its comparison through a copy --
/// `rax = WaitForSingleObject(..); local_4 = rax; if (local_4 == 258)` -- so
/// copies are followed. Only a variable the body assigns exactly once is
/// tracked: a second assignment means the value at the comparison need not be
/// the one the call produced.
pub(super) fn collect_return_enum_groups(func: &HirFunction, out: &mut HashMap<String, String>) {
    let mut assignments = HashMap::new();
    count_var_assignments(&func.body, &mut assignments);
    let assigned_once = |name: &str| assignments.get(name).copied().unwrap_or(0) == 1;

    // Copies are followed to a fixed point: the chain can be longer than one
    // hop, and statement order does not have to agree with data flow.
    let mut changed = true;
    while changed {
        changed = false;
        for_each_stmt(&func.body, &mut |stmt| {
            let HirStmt::Assign {
                lhs: HirLValue::Var(dst),
                rhs,
            } = stmt
            else {
                return;
            };
            if !assigned_once(dst) || out.contains_key(dst.as_str()) {
                return;
            }
            let group = match rhs {
                HirExpr::Call { target, args, .. } => {
                    let callee = if target == "__fission_callind_opaque" {
                        args.first().and_then(callind_target_symbol)
                    } else {
                        Some(target.clone())
                    };
                    callee.and_then(|callee| {
                        let symbol =
                            fission_signatures::symbol_for_win_api_database_lookup(&callee)
                                .unwrap_or(&callee);
                        fission_signatures::WIN_CONSTANTS_DB
                            .group_for_parameter(symbol, "return")
                            .map(|group| group.name.clone())
                    })
                }
                HirExpr::Var(src) => out.get(src.as_str()).cloned(),
                _ => None,
            };
            if let Some(group) = group {
                out.insert(dst.clone(), group);
                changed = true;
            }
        });
    }
}

pub(super) fn collect_goto_targets<'a>(stmts: &'a [HirStmt], out: &mut HashSet<&'a str>) {
    for stmt in stmts {
        match stmt {
            HirStmt::Goto(label) => {
                if label != crate::render::SWITCH_FALLTHROUGH_SENTINEL {
                    out.insert(label.as_str());
                }
            }
            HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
                collect_goto_targets(body, out);
            }
            HirStmt::For {
                init, update, body, ..
            } => {
                if let Some(init) = init {
                    collect_goto_targets(std::slice::from_ref(&**init), out);
                }
                if let Some(update) = update {
                    collect_goto_targets(std::slice::from_ref(&**update), out);
                }
                collect_goto_targets(body, out);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_goto_targets(then_body, out);
                collect_goto_targets(else_body, out);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_goto_targets(&case.body, out);
                }
                collect_goto_targets(default, out);
            }
            _ => {}
        }
    }
}
