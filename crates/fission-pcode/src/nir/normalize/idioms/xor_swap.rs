use super::super::*;
use std::collections::HashMap;

pub(crate) fn apply_xor_swap_pass(func: &mut HirFunction) -> bool {
    let mut type_map = HashMap::new();
    for binding in func.params.iter().chain(func.locals.iter()) {
        type_map.insert(binding.name.clone(), binding.ty.clone());
    }

    let mut changed = false;
    if process_statement_list(&mut func.body, &mut func.locals, &type_map) {
        changed = true;
    }
    changed
}

fn process_statement_list(
    stmts: &mut Vec<HirStmt>,
    locals: &mut Vec<NirBinding>,
    type_map: &HashMap<String, NirType>,
) -> bool {
    let mut changed = false;

    // Recurse into nested blocks first
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Block(body)
            | HirStmt::While { body, .. }
            | HirStmt::DoWhile { body, .. } => {
                changed |= process_statement_list(body, locals, type_map);
            }
            HirStmt::For { init, update, body, .. } => {
                if let Some(init_stmt) = init {
                    if let HirStmt::Block(init_body) = init_stmt.as_mut() {
                        changed |= process_statement_list(init_body, locals, type_map);
                    }
                }
                if let Some(update_stmt) = update {
                    if let HirStmt::Block(update_body) = update_stmt.as_mut() {
                        changed |= process_statement_list(update_body, locals, type_map);
                    }
                }
                changed |= process_statement_list(body, locals, type_map);
            }
            HirStmt::If { then_body, else_body, .. } => {
                changed |= process_statement_list(then_body, locals, type_map);
                changed |= process_statement_list(else_body, locals, type_map);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= process_statement_list(&mut case.body, locals, type_map);
                }
                changed |= process_statement_list(default, locals, type_map);
            }
            _ => {}
        }
    }

    // Process swaps at this level
    let mut i = 0;
    while i < stmts.len().saturating_sub(2) {
        if let Some((var_a, var_b)) = match_xor_swap_pattern(&stmts[i], &stmts[i + 1], &stmts[i + 2]) {
            let var_ty = type_map.get(&var_a).cloned().unwrap_or(NirType::Unknown);
            let temp_name = format!("tmp_swap_{}", locals.len());

            locals.push(NirBinding {
                name: temp_name.clone(),
                ty: var_ty.clone(),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::Temp),
                initializer: None,
            });

            // Replace the three statements with:
            // tmp = a;
            // a = b;
            // b = tmp;
            let swap_stmts = vec![
                HirStmt::Assign {
                    lhs: HirLValue::Var(temp_name.clone()),
                    rhs: HirExpr::Var(var_a.clone()),
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var(var_a),
                    rhs: HirExpr::Var(var_b.clone()),
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var(var_b),
                    rhs: HirExpr::Var(temp_name),
                },
            ];

            // Replace stmts[i..i+3] with swap_stmts
            stmts.splice(i..i + 3, swap_stmts);
            changed = true;
            i += 3; // Advance past the newly inserted statements
        } else {
            i += 1;
        }
    }

    changed
}

fn strip_casts(expr: &HirExpr) -> &HirExpr {
    match expr {
        HirExpr::Cast { expr: inner, .. } => strip_casts(inner),
        _ => expr,
    }
}

fn get_var_name(expr: &HirExpr) -> Option<String> {
    match strip_casts(expr) {
        HirExpr::Var(name) => Some(name.clone()),
        _ => None,
    }
}

fn match_xor_assign(stmt: &HirStmt) -> Option<(String, String)> {
    let HirStmt::Assign { lhs: HirLValue::Var(lhs_var), rhs } = stmt else {
        return None;
    };
    let inner_rhs = strip_casts(rhs);
    let HirExpr::Binary { op: HirBinaryOp::Xor, lhs, rhs: bin_rhs, .. } = inner_rhs else {
        return None;
    };
    let var_l = get_var_name(lhs)?;
    let var_r = get_var_name(bin_rhs)?;
    if var_l == *lhs_var {
        Some((var_l, var_r))
    } else if var_r == *lhs_var {
        Some((var_r, var_l))
    } else {
        None
    }
}

fn match_xor_swap_pattern(s1: &HirStmt, s2: &HirStmt, s3: &HirStmt) -> Option<(String, String)> {
    let (a1, b1) = match_xor_assign(s1)?;
    let (b2, a2) = match_xor_assign(s2)?;
    let (a3, b3) = match_xor_assign(s3)?;

    if a1 == a2 && a2 == a3 && b1 == b2 && b2 == b3 {
        Some((a1, b1))
    } else {
        None
    }
}
