use crate::prelude::*;
use super::utils::expr_has_side_effects;
use super::utils::stmt_assigns_var;
use crate::HashSet;

/// Simplifies a series of conditionally executed statements (Ghidra's ActionConditionalExe equivalent).
/// Merges sequential sibling Ifs with identical conditions, and uses path-sensitive propagation
/// to fold nested redundant If statement hierarchies.
pub fn apply_condexe_folding_pass(stmts: &mut Vec<DirStmt>) -> bool {
    let mut changed = false;

    // Run fixed-point iteration of sequential and path-sensitive folding passes
    for _ in 0..10 {
        let mut pass_changed = false;

        // 1. Sibling sequential If folding
        pass_changed |= fold_sequential_siblings(stmts);

        // 2. Path-sensitive nested If folding
        let mut true_conds = Vec::new();
        let mut false_conds = Vec::new();
        pass_changed |= fold_conditions(stmts, &mut true_conds, &mut false_conds);

        if !pass_changed {
            break;
        }
        changed = true;
    }

    changed
}

fn fold_sequential_siblings(stmts: &mut Vec<DirStmt>) -> bool {
    let mut changed = false;
    let mut idx = 0;

    while idx + 1 < stmts.len() {
        let is_foldable = {
            if let (
                Some(DirStmt::If {
                    cond: cond1,
                    then_body: then1,
                    else_body: else1,
                }),
                Some(DirStmt::If {
                    cond: cond2,
                    then_body: then2,
                    else_body: else2,
                }),
            ) = (stmts.get(idx), stmts.get(idx + 1))
            {
                if cond1 == cond2 && else1.is_empty() && else2.is_empty() {
                    // Check if any variable in cond1 is modified inside then1
                    let mut cond_vars = HashSet::default();
                    get_variables_in_expr(cond1, &mut cond_vars);
                    let modifies_cond_var = cond_vars
                        .iter()
                        .any(|var| then1.iter().any(|stmt| stmt_assigns_var(stmt, var)));
                    !modifies_cond_var
                } else {
                    false
                }
            } else {
                false
            }
        };

        if is_foldable {
            if let DirStmt::If {
                then_body: mut then1,
                cond: cond1,
                ..
            } = stmts.remove(idx)
            {
                if let DirStmt::If {
                    then_body: then2, ..
                } = stmts.remove(idx)
                {
                    then1.extend(then2);
                    let merged_if = DirStmt::If {
                        cond: cond1,
                        then_body: then1,
                        else_body: Vec::new(),
                    };
                    stmts.insert(idx, merged_if);
                    changed = true;
                    // Do not increment idx to allow cascading sequential merges
                    continue;
                }
            }
        }
        idx += 1;
    }

    // Also recurse into all nested block/If structures
    for stmt in stmts.iter_mut() {
        match stmt {
            DirStmt::Block(body)
            | DirStmt::While { body, .. }
            | DirStmt::DoWhile { body, .. }
            | DirStmt::For { body, .. } => {
                changed |= fold_sequential_siblings(body);
            }
            DirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= fold_sequential_siblings(then_body);
                changed |= fold_sequential_siblings(else_body);
            }
            DirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= fold_sequential_siblings(&mut case.body);
                }
                changed |= fold_sequential_siblings(default);
            }
            _ => {}
        }
    }

    changed
}

fn fold_conditions(
    stmts: &mut Vec<DirStmt>,
    true_conds: &mut Vec<DirExpr>,
    false_conds: &mut Vec<DirExpr>,
) -> bool {
    let mut changed = false;
    let mut idx = 0;

    while idx < stmts.len() {
        let mut is_if = false;
        let mut cond_opt = None;
        if let DirStmt::If { cond, .. } = &stmts[idx] {
            is_if = true;
            cond_opt = Some(cond.clone());
        }

        if is_if {
            let cond = cond_opt.unwrap();

            // Case 1: Redundant If statement where condition is proven True
            if true_conds.contains(&cond) {
                if let DirStmt::If { then_body, .. } = stmts.remove(idx) {
                    for (i, s) in then_body.into_iter().enumerate() {
                        stmts.insert(idx + i, s);
                    }
                    changed = true;
                    continue;
                }
            }
            // Case 2: Redundant If statement where condition is proven False
            else if false_conds.contains(&cond) {
                if let DirStmt::If { else_body, .. } = stmts.remove(idx) {
                    for (i, s) in else_body.into_iter().enumerate() {
                        stmts.insert(idx + i, s);
                    }
                    changed = true;
                    continue;
                }
            }
            // Case 3: Condition not proven, recurse with path context
            else {
                if let DirStmt::If {
                    cond,
                    then_body,
                    else_body,
                } = &mut stmts[idx]
                {
                    // Inside then_body: cond is True
                    let mut nested_true = true_conds.clone();
                    let mut nested_false = false_conds.clone();
                    nested_true.push(cond.clone());
                    changed |= fold_conditions(then_body, &mut nested_true, &mut nested_false);

                    // Inside else_body: cond is False
                    let mut nested_true = true_conds.clone();
                    let mut nested_false = false_conds.clone();
                    nested_false.push(cond.clone());
                    changed |= fold_conditions(else_body, &mut nested_true, &mut nested_false);
                }
            }
        } else {
            // For other control-flow statements, recursively fold with safety invalidations
            match &mut stmts[idx] {
                DirStmt::Block(body) => {
                    changed |= fold_conditions(body, true_conds, false_conds);
                }
                DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
                    let mut assigned_in_body = HashSet::default();
                    for s in body.iter() {
                        get_assigned_vars_in_stmt(s, &mut assigned_in_body);
                    }
                    let mut nested_true = true_conds.clone();
                    let mut nested_false = false_conds.clone();
                    for var in assigned_in_body {
                        invalidate_variable(&var, &mut nested_true, &mut nested_false);
                    }
                    changed |= fold_conditions(body, &mut nested_true, &mut nested_false);
                }
                DirStmt::For {
                    init, update, body, ..
                } => {
                    let mut assigned = HashSet::default();
                    if let Some(i) = init {
                        get_assigned_vars_in_stmt(i, &mut assigned);
                    }
                    if let Some(u) = update {
                        get_assigned_vars_in_stmt(u, &mut assigned);
                    }
                    for s in body.iter() {
                        get_assigned_vars_in_stmt(s, &mut assigned);
                    }
                    let mut nested_true = true_conds.clone();
                    let mut nested_false = false_conds.clone();
                    for var in assigned {
                        invalidate_variable(&var, &mut nested_true, &mut nested_false);
                    }
                    changed |= fold_conditions(body, &mut nested_true, &mut nested_false);
                }
                DirStmt::Switch { cases, default, .. } => {
                    for case in cases {
                        let mut nested_true = true_conds.clone();
                        let mut nested_false = false_conds.clone();
                        changed |=
                            fold_conditions(&mut case.body, &mut nested_true, &mut nested_false);
                    }
                    let mut nested_true = true_conds.clone();
                    let mut nested_false = false_conds.clone();
                    changed |= fold_conditions(default, &mut nested_true, &mut nested_false);
                }
                _ => {}
            }
        }

        // Invalidate any proven conditions referencing variables assigned by the statement at index
        let mut assigned_vars = HashSet::default();
        get_assigned_vars_in_stmt(&stmts[idx], &mut assigned_vars);
        for var in assigned_vars {
            invalidate_variable(&var, true_conds, false_conds);
        }

        idx += 1;
    }

    changed
}

fn get_variables_in_expr(expr: &DirExpr, vars: &mut HashSet<String>) {
    match expr {
        DirExpr::Var(name) => {
            vars.insert(name.clone());
        }
        DirExpr::Cast { expr, .. } => {
            get_variables_in_expr(expr, vars);
        }
        DirExpr::Unary { expr, .. } => {
            get_variables_in_expr(expr, vars);
        }
        DirExpr::Binary { lhs, rhs, .. } => {
            get_variables_in_expr(lhs, vars);
            get_variables_in_expr(rhs, vars);
        }
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            get_variables_in_expr(cond, vars);
            get_variables_in_expr(then_expr, vars);
            get_variables_in_expr(else_expr, vars);
        }
        DirExpr::Call { args, .. } => {
            for arg in args {
                get_variables_in_expr(arg, vars);
            }
        }
        DirExpr::Load { ptr, .. } => {
            get_variables_in_expr(ptr, vars);
        }
        DirExpr::PtrOffset { base, .. } => {
            get_variables_in_expr(base, vars);
        }
        DirExpr::Index { base, index, .. } => {
            get_variables_in_expr(base, vars);
            get_variables_in_expr(index, vars);
        }
        DirExpr::AggregateCopy { src, .. } => {
            get_variables_in_expr(src, vars);
        }
        _ => {}
    }
}

fn get_assigned_vars_in_stmt(stmt: &DirStmt, vars: &mut HashSet<String>) {
    match stmt {
        DirStmt::Assign { lhs, .. } => {
            if let DirLValue::Var(name) = lhs {
                vars.insert(name.clone());
            }
        }
        DirStmt::Block(body)
        | DirStmt::While { body, .. }
        | DirStmt::DoWhile { body, .. }
        | DirStmt::For { body, .. } => {
            for s in body {
                get_assigned_vars_in_stmt(s, vars);
            }
        }
        DirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            for s in then_body {
                get_assigned_vars_in_stmt(s, vars);
            }
            for s in else_body {
                get_assigned_vars_in_stmt(s, vars);
            }
        }
        DirStmt::Switch { cases, default, .. } => {
            for case in cases {
                for s in &case.body {
                    get_assigned_vars_in_stmt(s, vars);
                }
            }
            for s in default {
                get_assigned_vars_in_stmt(s, vars);
            }
        }
        _ => {}
    }
}

fn invalidate_variable(
    var_name: &str,
    true_conds: &mut Vec<DirExpr>,
    false_conds: &mut Vec<DirExpr>,
) {
    true_conds.retain(|cond| {
        let mut vars = HashSet::default();
        get_variables_in_expr(cond, &mut vars);
        !vars.contains(var_name)
    });
    false_conds.retain(|cond| {
        let mut vars = HashSet::default();
        get_variables_in_expr(cond, &mut vars);
        !vars.contains(var_name)
    });
}

// ---------------------------------------------------------------------------
// Ghidra ConditionalExecution::execute() equivalent — iblock phi elimination
// ---------------------------------------------------------------------------

/// Eliminates diamond-shaped iblock merge variables from HIR.
///
/// This mirrors Ghidra's `ConditionalExecution::execute()`: when a pair of
/// sequential if-else statements assign the same `lhs` variable in both arms,
/// and those two statements share the same condition, the merge variable pattern
/// is:
///
/// ```text
/// // block A (then): lhs = val_true;
/// // block B (else): lhs = val_false;
/// // merge block:    use(lhs);   <- iblock MULTIEQUAL in Ghidra
/// ```
///
/// At the HIR level, this appears as:
///
/// ```text
/// if cond { lhs = val_true; } else { lhs = val_false; }
/// ... use(lhs) ...
/// ```
///
/// The optimization replaces uses of `lhs` that follow the if-else block with
/// `if cond { val_true } else { val_false }` (a ternary select), then removes
/// the now-dead assignment. This allows downstream passes to further simplify
/// or inline the ternary.
///
/// Returns `true` if any transformation was applied.
pub fn apply_iblock_phi_elimination(stmts: &mut Vec<DirStmt>) -> bool {
    let mut changed = false;

    for _ in 0..8 {
        let pass_changed = iblock_phi_pass(stmts);
        if !pass_changed {
            break;
        }
        changed = true;
    }

    changed
}

/// Single-pass iblock phi elimination. Scans for:
/// ```text
/// if cond { lhs = val_t; } else { lhs = val_f; }
/// ```
/// where `lhs` is a simple variable (no memory write), and replaces subsequent
/// uses of `lhs` with `(cond ? val_t : val_f)`, then removes the dead if-else.
fn iblock_phi_pass(stmts: &mut Vec<DirStmt>) -> bool {
    let mut changed = false;
    let mut idx = 0;

    while idx < stmts.len() {
        // Recurse into nested blocks first
        match &mut stmts[idx] {
            DirStmt::Block(body) => {
                changed |= iblock_phi_pass(body);
            }
            DirStmt::While { body, .. }
            | DirStmt::DoWhile { body, .. }
            | DirStmt::For { body, .. } => {
                changed |= iblock_phi_pass(body);
            }
            DirStmt::Switch { cases, default, .. } => {
                for case in cases.iter_mut() {
                    changed |= iblock_phi_pass(&mut case.body);
                }
                changed |= iblock_phi_pass(default);
            }
            _ => {}
        }

        // Try to match diamond pattern at current index
        let diamond = extract_diamond_phi(&stmts[idx]);
        if let Some((cond, lhs_var, val_true, val_false)) = diamond {
            // Check if lhs_var is used in any subsequent statement and can be replaced
            let lhs_used_below = stmts[idx + 1..].iter().any(|s| stmt_uses_var(s, &lhs_var));

            if lhs_used_below {
                let select_expr = DirExpr::Select {
                    cond: Box::new(cond),
                    then_expr: Box::new(val_true),
                    else_expr: Box::new(val_false),
                    ty: NirType::Unknown, // type resolved by downstream normalization
                };

                // Replace all uses of lhs_var below with the select expression
                for s in stmts[idx + 1..].iter_mut() {
                    replace_var_in_stmt(s, &lhs_var, &select_expr);
                }

                // Remove the diamond if-else (it's now dead for lhs)
                stmts.remove(idx);
                changed = true;
                // Don't increment idx — new stmt at this position needs to be checked
                continue;
            }
        }

        idx += 1;
    }

    changed
}

/// Attempts to extract a diamond phi assignment from an if-else statement.
/// Returns `(cond, lhs_var, val_in_then, val_in_else)` if the pattern matches:
/// - Both `then_body` and `else_body` have exactly one statement.
/// - Both are assignments to the same simple variable `lhs_var`.
/// - The assigned values are pure expressions (no side effects).
fn extract_diamond_phi(stmt: &DirStmt) -> Option<(DirExpr, String, DirExpr, DirExpr)> {
    let DirStmt::If {
        cond,
        then_body,
        else_body,
    } = stmt
    else {
        return None;
    };

    // Both arms must be exactly a single assign
    if then_body.len() != 1 || else_body.len() != 1 {
        return None;
    }

    let (then_lhs, then_rhs) = extract_simple_assign(&then_body[0])?;
    let (else_lhs, else_rhs) = extract_simple_assign(&else_body[0])?;

    // Must assign to the same variable
    if then_lhs != else_lhs {
        return None;
    }

    // Both RHS must be side-effect-free
    if expr_has_side_effects(then_rhs) || expr_has_side_effects(else_rhs) {
        return None;
    }

    Some((cond.clone(), then_lhs, then_rhs.clone(), else_rhs.clone()))
}

/// Extracts `(var_name, rhs_expr)` from a simple `var = expr` assignment.
/// Returns `None` if the LHS is not a simple variable or if the stmt is not an assign.
fn extract_simple_assign(stmt: &DirStmt) -> Option<(String, &DirExpr)> {
    let DirStmt::Assign { lhs, rhs } = stmt else {
        return None;
    };
    let DirLValue::Var(name) = lhs else {
        return None;
    };
    Some((name.clone(), rhs))
}

/// Returns `true` if any expression in `stmt` reads from `var_name`.
fn stmt_uses_var(stmt: &DirStmt, var_name: &str) -> bool {
    match stmt {
        DirStmt::Assign { rhs, .. } => expr_uses_var(rhs, var_name),
        DirStmt::Return(Some(expr)) => expr_uses_var(expr, var_name),
        DirStmt::Return(None) => false,
        DirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            expr_uses_var(cond, var_name)
                || then_body.iter().any(|s| stmt_uses_var(s, var_name))
                || else_body.iter().any(|s| stmt_uses_var(s, var_name))
        }
        DirStmt::While { cond, body } | DirStmt::DoWhile { cond, body } => {
            expr_uses_var(cond, var_name) || body.iter().any(|s| stmt_uses_var(s, var_name))
        }
        DirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            init.as_ref().map_or(false, |s| stmt_uses_var(s, var_name))
                || cond.as_ref().map_or(false, |e| expr_uses_var(e, var_name))
                || update
                    .as_ref()
                    .map_or(false, |s| stmt_uses_var(s, var_name))
                || body.iter().any(|s| stmt_uses_var(s, var_name))
        }
        DirStmt::Block(body) => body.iter().any(|s| stmt_uses_var(s, var_name)),
        DirStmt::Expr(expr) => expr_uses_var(expr, var_name),
        DirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            expr_uses_var(expr, var_name)
                || cases
                    .iter()
                    .any(|c| c.body.iter().any(|s| stmt_uses_var(s, var_name)))
                || default.iter().any(|s| stmt_uses_var(s, var_name))
        }
        _ => false,
    }
}

fn expr_uses_var(expr: &DirExpr, var_name: &str) -> bool {
    match expr {
        DirExpr::Var(name) => name == var_name,
        DirExpr::Cast { expr, .. } => expr_uses_var(expr, var_name),
        DirExpr::Unary { expr, .. } => expr_uses_var(expr, var_name),
        DirExpr::Binary { lhs, rhs, .. } => {
            expr_uses_var(lhs, var_name) || expr_uses_var(rhs, var_name)
        }
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            expr_uses_var(cond, var_name)
                || expr_uses_var(then_expr, var_name)
                || expr_uses_var(else_expr, var_name)
        }
        DirExpr::Call { args, .. } => args.iter().any(|a| expr_uses_var(a, var_name)),
        DirExpr::Load { ptr, .. } => expr_uses_var(ptr, var_name),
        DirExpr::PtrOffset { base, .. } => expr_uses_var(base, var_name),
        DirExpr::Index { base, index, .. } => {
            expr_uses_var(base, var_name) || expr_uses_var(index, var_name)
        }
        DirExpr::AggregateCopy { src, .. } => expr_uses_var(src, var_name),
        _ => false,
    }
}

/// Replaces all reads of `var_name` in `stmt` with `replacement`.
fn replace_var_in_stmt(stmt: &mut DirStmt, var_name: &str, replacement: &DirExpr) {
    match stmt {
        DirStmt::Assign { rhs, .. } => {
            replace_var_in_expr(rhs, var_name, replacement);
        }
        DirStmt::Return(Some(expr)) => {
            replace_var_in_expr(expr, var_name, replacement);
        }
        DirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            replace_var_in_expr(cond, var_name, replacement);
            for s in then_body.iter_mut() {
                replace_var_in_stmt(s, var_name, replacement);
            }
            for s in else_body.iter_mut() {
                replace_var_in_stmt(s, var_name, replacement);
            }
        }
        DirStmt::While { cond, body } | DirStmt::DoWhile { cond, body } => {
            replace_var_in_expr(cond, var_name, replacement);
            for s in body.iter_mut() {
                replace_var_in_stmt(s, var_name, replacement);
            }
        }
        DirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(s) = init {
                replace_var_in_stmt(s, var_name, replacement);
            }
            if let Some(e) = cond {
                replace_var_in_expr(e, var_name, replacement);
            }
            if let Some(s) = update {
                replace_var_in_stmt(s, var_name, replacement);
            }
            for s in body.iter_mut() {
                replace_var_in_stmt(s, var_name, replacement);
            }
        }
        DirStmt::Block(body) => {
            for s in body.iter_mut() {
                replace_var_in_stmt(s, var_name, replacement);
            }
        }
        DirStmt::Expr(expr) => {
            replace_var_in_expr(expr, var_name, replacement);
        }
        DirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            replace_var_in_expr(expr, var_name, replacement);
            for case in cases.iter_mut() {
                for s in case.body.iter_mut() {
                    replace_var_in_stmt(s, var_name, replacement);
                }
            }
            for s in default.iter_mut() {
                replace_var_in_stmt(s, var_name, replacement);
            }
        }
        _ => {}
    }
}

fn replace_var_in_expr(expr: &mut DirExpr, var_name: &str, replacement: &DirExpr) {
    match expr {
        DirExpr::Var(name) if name == var_name => {
            *expr = replacement.clone();
        }
        DirExpr::Cast { expr: inner, .. } => {
            replace_var_in_expr(inner, var_name, replacement);
        }
        DirExpr::Unary { expr: inner, .. } => {
            replace_var_in_expr(inner, var_name, replacement);
        }
        DirExpr::Binary { lhs, rhs, .. } => {
            replace_var_in_expr(lhs, var_name, replacement);
            replace_var_in_expr(rhs, var_name, replacement);
        }
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            replace_var_in_expr(cond, var_name, replacement);
            replace_var_in_expr(then_expr, var_name, replacement);
            replace_var_in_expr(else_expr, var_name, replacement);
        }
        DirExpr::Call { args, .. } => {
            for a in args.iter_mut() {
                replace_var_in_expr(a, var_name, replacement);
            }
        }
        DirExpr::Load { ptr, .. } => {
            replace_var_in_expr(ptr, var_name, replacement);
        }
        DirExpr::PtrOffset { base, .. } => {
            replace_var_in_expr(base, var_name, replacement);
        }
        DirExpr::Index { base, index, .. } => {
            replace_var_in_expr(base, var_name, replacement);
            replace_var_in_expr(index, var_name, replacement);
        }
        DirExpr::AggregateCopy { src, .. } => {
            replace_var_in_expr(src, var_name, replacement);
        }
        _ => {}
    }
}
