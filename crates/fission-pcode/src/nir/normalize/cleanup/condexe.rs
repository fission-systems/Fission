use super::super::*;
use super::utils::stmt_assigns_var;
use std::collections::HashSet;

/// Simplifies a series of conditionally executed statements (Ghidra's ActionConditionalExe equivalent).
/// Merges sequential sibling Ifs with identical conditions, and uses path-sensitive propagation
/// to fold nested redundant If statement hierarchies.
pub(crate) fn apply_condexe_folding_pass(stmts: &mut Vec<HirStmt>) -> bool {
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

fn fold_sequential_siblings(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;
    let mut idx = 0;
    
    while idx + 1 < stmts.len() {
        let is_foldable = {
            if let (Some(HirStmt::If { cond: cond1, then_body: then1, else_body: else1 }),
                    Some(HirStmt::If { cond: cond2, then_body: then2, else_body: else2 })) = 
                (stmts.get(idx), stmts.get(idx + 1)) 
            {
                if cond1 == cond2 && else1.is_empty() && else2.is_empty() {
                    // Check if any variable in cond1 is modified inside then1
                    let mut cond_vars = HashSet::new();
                    get_variables_in_expr(cond1, &mut cond_vars);
                    let modifies_cond_var = cond_vars.iter().any(|var| {
                        then1.iter().any(|stmt| stmt_assigns_var(stmt, var))
                    });
                    !modifies_cond_var
                } else {
                    false
                }
            } else {
                false
            }
        };

        if is_foldable {
            if let HirStmt::If { then_body: mut then1, cond: cond1, .. } = stmts.remove(idx) {
                if let HirStmt::If { then_body: then2, .. } = stmts.remove(idx) {
                    then1.extend(then2);
                    let merged_if = HirStmt::If {
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
            HirStmt::Block(body)
            | HirStmt::While { body, .. }
            | HirStmt::DoWhile { body, .. }
            | HirStmt::For { body, .. } => {
                changed |= fold_sequential_siblings(body);
            }
            HirStmt::If { then_body, else_body, .. } => {
                changed |= fold_sequential_siblings(then_body);
                changed |= fold_sequential_siblings(else_body);
            }
            HirStmt::Switch { cases, default, .. } => {
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
    stmts: &mut Vec<HirStmt>,
    true_conds: &mut Vec<HirExpr>,
    false_conds: &mut Vec<HirExpr>,
) -> bool {
    let mut changed = false;
    let mut idx = 0;

    while idx < stmts.len() {
        let mut is_if = false;
        let mut cond_opt = None;
        if let HirStmt::If { cond, .. } = &stmts[idx] {
            is_if = true;
            cond_opt = Some(cond.clone());
        }

        if is_if {
            let cond = cond_opt.unwrap();
            
            // Case 1: Redundant If statement where condition is proven True
            if true_conds.contains(&cond) {
                if let HirStmt::If { then_body, .. } = stmts.remove(idx) {
                    for (i, s) in then_body.into_iter().enumerate() {
                        stmts.insert(idx + i, s);
                    }
                    changed = true;
                    continue;
                }
            }
            // Case 2: Redundant If statement where condition is proven False
            else if false_conds.contains(&cond) {
                if let HirStmt::If { else_body, .. } = stmts.remove(idx) {
                    for (i, s) in else_body.into_iter().enumerate() {
                        stmts.insert(idx + i, s);
                    }
                    changed = true;
                    continue;
                }
            }
            // Case 3: Condition not proven, recurse with path context
            else {
                if let HirStmt::If { cond, then_body, else_body } = &mut stmts[idx] {
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
                HirStmt::Block(body) => {
                    changed |= fold_conditions(body, true_conds, false_conds);
                }
                HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
                    let mut assigned_in_body = HashSet::new();
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
                HirStmt::For { init, update, body, .. } => {
                    let mut assigned = HashSet::new();
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
                HirStmt::Switch { cases, default, .. } => {
                    for case in cases {
                        let mut nested_true = true_conds.clone();
                        let mut nested_false = false_conds.clone();
                        changed |= fold_conditions(&mut case.body, &mut nested_true, &mut nested_false);
                    }
                    let mut nested_true = true_conds.clone();
                    let mut nested_false = false_conds.clone();
                    changed |= fold_conditions(default, &mut nested_true, &mut nested_false);
                }
                _ => {}
            }
        }

        // Invalidate any proven conditions referencing variables assigned by the statement at index
        let mut assigned_vars = HashSet::new();
        get_assigned_vars_in_stmt(&stmts[idx], &mut assigned_vars);
        for var in assigned_vars {
            invalidate_variable(&var, true_conds, false_conds);
        }

        idx += 1;
    }

    changed
}

fn get_variables_in_expr(expr: &HirExpr, vars: &mut HashSet<String>) {
    match expr {
        HirExpr::Var(name) => {
            vars.insert(name.clone());
        }
        HirExpr::Cast { expr, .. } => {
            get_variables_in_expr(expr, vars);
        }
        HirExpr::Unary { expr, .. } => {
            get_variables_in_expr(expr, vars);
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            get_variables_in_expr(lhs, vars);
            get_variables_in_expr(rhs, vars);
        }
        HirExpr::Select { cond, then_expr, else_expr, .. } => {
            get_variables_in_expr(cond, vars);
            get_variables_in_expr(then_expr, vars);
            get_variables_in_expr(else_expr, vars);
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                get_variables_in_expr(arg, vars);
            }
        }
        HirExpr::Load { ptr, .. } => {
            get_variables_in_expr(ptr, vars);
        }
        HirExpr::PtrOffset { base, .. } => {
            get_variables_in_expr(base, vars);
        }
        HirExpr::Index { base, index, .. } => {
            get_variables_in_expr(base, vars);
            get_variables_in_expr(index, vars);
        }
        HirExpr::AggregateCopy { src, .. } => {
            get_variables_in_expr(src, vars);
        }
        _ => {}
    }
}

fn get_assigned_vars_in_stmt(stmt: &HirStmt, vars: &mut HashSet<String>) {
    match stmt {
        HirStmt::Assign { lhs, .. } => {
            if let HirLValue::Var(name) = lhs {
                vars.insert(name.clone());
            }
        }
        HirStmt::Block(body)
        | HirStmt::While { body, .. }
        | HirStmt::DoWhile { body, .. }
        | HirStmt::For { body, .. } => {
            for s in body {
                get_assigned_vars_in_stmt(s, vars);
            }
        }
        HirStmt::If { then_body, else_body, .. } => {
            for s in then_body {
                get_assigned_vars_in_stmt(s, vars);
            }
            for s in else_body {
                get_assigned_vars_in_stmt(s, vars);
            }
        }
        HirStmt::Switch { cases, default, .. } => {
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
    true_conds: &mut Vec<HirExpr>,
    false_conds: &mut Vec<HirExpr>,
) {
    true_conds.retain(|cond| {
        let mut vars = HashSet::new();
        get_variables_in_expr(cond, &mut vars);
        !vars.contains(var_name)
    });
    false_conds.retain(|cond| {
        let mut vars = HashSet::new();
        get_variables_in_expr(cond, &mut vars);
        !vars.contains(var_name)
    });
}
