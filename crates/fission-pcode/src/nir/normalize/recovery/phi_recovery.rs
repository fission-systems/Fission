use super::super::analysis::defuse::DefUseMap;
use super::super::analysis::preservation::{
    preserved_materialization_names, should_skip_copyprop_for_preserved_name,
};
/// HIR-level copy propagation and join-variable coalescing.
///
/// These passes improve the HIR after structuring by eliminating unnecessary
/// temporary variables and unifying variables that represent the same value
/// at control-flow join points.
///
/// ## Copy Propagation (`copy_propagation_pass`)
///
/// For every assignment `x = y` where `y` is a named variable and `x` is a
/// pure temporary with a single definition, substitutes `y` for every use of
/// `x` throughout the function and removes the assignment.
///
/// This is safe when:
/// - `x` has exactly one definition in the entire function body
/// - `y` is never re-assigned between the definition of `x` and any use of `x`
///   (conservatively approximated by requiring `y` to have no assignment at all
///   in the subtree between the definition and the last use — for the linear
///   case we simply require that `y` is not a pure temp that gets redefined)
///
/// ## Join Variable Coalescing (`join_coalescing_pass`)
///
/// Detects if-else structures where both branches end by assigning to the
/// *same* set of variables and renames join-point uses to the shared variable.
/// This models the classical SSA out-of-SSA transformation for 2-way joins.
use super::super::cleanup::{prune_unused_dead_local_bindings, prune_unused_temp_bindings};
use super::super::wave_stats;
use super::super::*;
use std::collections::{HashMap, HashSet};

// ── Copy Propagation ─────────────────────────────────────────────────────────

/// Propagate copies `x = y` (where both x and y are named variables and x is a
/// pure temporary with exactly one definition) by replacing every rvalue use of
/// `x` with `y` and removing the assignment.
///
/// Returns `true` if any substitution was made.
pub(crate) fn copy_propagation_pass(func: &mut HirFunction) -> bool {
    let mut changed = false;
    let loop_preservation_vars = collect_loop_preservation_vars(&func.body);

    // --- Phase 1: Standard Copy Propagation ---
    let preserved_temps = preserved_materialization_names(&func.locals);
    let temp_names: HashSet<&str> = func
        .locals
        .iter()
        .filter(|b| b.is_temp_like())
        .map(|b| b.name.as_str())
        .collect();

    if !temp_names.is_empty() {
        let def_count = count_definitions_in_stmts(&func.body, &temp_names);
        let mut copy_map: HashMap<String, String> = HashMap::new();
        collect_copies(&func.body, &temp_names, &def_count, &mut copy_map);

        if !copy_map.is_empty() {
            let mut predicate_vars = HashSet::new();
            collect_predicate_vars_in_stmts(&func.body, &mut predicate_vars);
            copy_map.retain(|name, _| !predicate_vars.contains(name.as_str()));
            let preserved_skip_count = copy_map
                .iter()
                .filter(|(name, source)| {
                    should_skip_copyprop_for_preserved_name(name, &preserved_temps)
                        || should_skip_copyprop_for_preserved_name(source, &preserved_temps)
                })
                .count();
            copy_map.retain(|name, source| {
                !should_skip_copyprop_for_preserved_name(name, &preserved_temps)
                    && !should_skip_copyprop_for_preserved_name(source, &preserved_temps)
                    && !loop_preservation_vars.contains(name.as_str())
                    && !loop_preservation_vars.contains(source.as_str())
            });
            wave_stats::add_preserved_temp_copyprop_skip(preserved_skip_count);

            if !copy_map.is_empty() {
                copy_map.retain(|_x, y| {
                    let y_def_count = def_count.get(y.as_str()).copied().unwrap_or(0);
                    y_def_count <= 1
                });

                if !copy_map.is_empty() {
                    remove_copy_assigns(&mut func.body, &copy_map, &mut changed);
                    substitute_copies_in_stmts(&mut func.body, &copy_map, &mut changed);
                }
            }
        }
    }

    // --- Phase 2: Constant Propagation for Primitive Variables ---
    let eligible_vars: HashSet<&str> = func
        .locals
        .iter()
        .filter(|b| {
            matches!(b.ty, NirType::Int { .. } | NirType::Float { .. } | NirType::Bool)
                && !should_skip_copyprop_for_preserved_name(&b.name, &preserved_temps)
                && !loop_preservation_vars.contains(b.name.as_str())
        })
        .map(|b| b.name.as_str())
        .collect();

    if !eligible_vars.is_empty() {
        let def_count = count_definitions_in_stmts(&func.body, &eligible_vars);
        let mut const_map = HashMap::new();
        collect_constants(&func.body, &eligible_vars, &def_count, &mut const_map);

        if !const_map.is_empty() {
            remove_constant_assigns(&mut func.body, &const_map, &mut changed);
            substitute_constants_in_stmts(&mut func.body, &const_map, &mut changed);
        }
    }

    if changed {
        prune_unused_temp_bindings(func);
        prune_unused_dead_local_bindings(func);
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nir::normalize::analysis::preservation::preserved_binding_origin;

    fn int(bits: u32) -> NirType {
        NirType::Int {
            bits,
            signed: false,
        }
    }

    #[test]
    fn copy_propagation_skips_preserved_temp_alias() {
        let mut func = HirFunction {
            name: "test_copy_prop_preserved".to_string(),
            params: vec![],
            locals: vec![NirBinding {
                name: "uVar0".to_string(),
                ty: int(32),
                surface_type_name: None,
                origin: Some(preserved_binding_origin()),
                initializer: None,
            }],
            return_type: int(32),
            surface_return_type_name: None,
            body: vec![
                HirStmt::Assign {
                    lhs: HirLValue::Var("uVar0".to_string()),
                    rhs: HirExpr::Var("eax".to_string()),
                },
                HirStmt::If {
                    cond: HirExpr::Binary {
                        op: HirBinaryOp::Eq,
                        lhs: Box::new(HirExpr::Var("uVar0".to_string())),
                        rhs: Box::new(HirExpr::Const(0, int(32))),
                        ty: NirType::Bool,
                    },
                    then_body: vec![HirStmt::Return(Some(HirExpr::Const(1, int(32))))],
                    else_body: vec![HirStmt::Return(Some(HirExpr::Const(0, int(32))))],
                },
            ],
            ..Default::default()
        };

        assert!(!copy_propagation_pass(&mut func));
        let HirStmt::If { cond, .. } = &func.body[1] else {
            panic!("expected preserved temp consumer to stay in the if condition");
        };
        assert!(print_expr(cond).contains("uVar0"));
    }

    #[test]
    fn copy_propagation_skips_single_use_alias_of_preserved_source() {
        let mut func = HirFunction {
            name: "test_copy_prop_preserved_source".to_string(),
            params: vec![],
            locals: vec![
                NirBinding {
                    name: "uVar0".to_string(),
                    ty: int(32),
                    surface_type_name: None,
                    origin: Some(preserved_binding_origin()),
                    initializer: None,
                },
                NirBinding {
                    name: "uVar1".to_string(),
                    ty: int(32),
                    surface_type_name: None,
                    origin: Some(NirBindingOrigin::Temp),
                    initializer: None,
                },
            ],
            return_type: int(32),
            surface_return_type_name: None,
            body: vec![
                HirStmt::Assign {
                    lhs: HirLValue::Var("uVar1".to_string()),
                    rhs: HirExpr::Var("uVar0".to_string()),
                },
                HirStmt::Return(Some(HirExpr::Var("uVar1".to_string()))),
            ],
            ..Default::default()
        };

        assert!(!copy_propagation_pass(&mut func));
        assert_eq!(func.body.len(), 2);
        assert!(matches!(
            &func.body[1],
            HirStmt::Return(Some(HirExpr::Var(name))) if name == "uVar1"
        ));
    }

    #[test]
    fn constant_propagation_eliminates_unused_local_constant() {
        let mut func = HirFunction {
            name: "test_const_prop".to_string(),
            params: vec![],
            locals: vec![NirBinding {
                name: "local_c".to_string(),
                ty: int(32),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::StackOffset(12)),
                initializer: None,
            }],
            return_type: int(32),
            surface_return_type_name: None,
            body: vec![
                HirStmt::Assign {
                    lhs: HirLValue::Var("local_c".to_string()),
                    rhs: HirExpr::Const(0, int(32)),
                },
                HirStmt::Return(Some(HirExpr::Var("local_c".to_string()))),
            ],
            ..Default::default()
        };

        assert!(copy_propagation_pass(&mut func));
        assert_eq!(func.body.len(), 1);
        assert!(matches!(
            &func.body[0],
            HirStmt::Return(Some(HirExpr::Const(0, _)))
        ));
        assert!(func.locals.is_empty());
    }
}

/// Count definition sites (assignments to LHS Var(name)) for each name in
/// `temp_names` across the entire body.
fn count_definitions_in_stmts<'a>(
    stmts: &'a [HirStmt],
    temp_names: &HashSet<&str>,
) -> HashMap<&'a str, usize> {
    let mut counts: HashMap<&'a str, usize> = HashMap::new();
    for stmt in stmts {
        count_defs_stmt(stmt, temp_names, &mut counts);
    }
    counts
}

fn count_defs_stmt<'a>(
    stmt: &'a HirStmt,
    temps: &HashSet<&str>,
    counts: &mut HashMap<&'a str, usize>,
) {
    match stmt {
        HirStmt::Assign {
            lhs: HirLValue::Var(name),
            ..
        } => {
            // Count definitions for ALL variables (not just temps) so we can
            // validate the source variable y.
            *counts.entry(name.as_str()).or_default() += 1;
        }
        HirStmt::Block(stmts) => {
            for s in stmts {
                count_defs_stmt(s, temps, counts);
            }
        }
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            for s in then_body {
                count_defs_stmt(s, temps, counts);
            }
            for s in else_body {
                count_defs_stmt(s, temps, counts);
            }
        }
        HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            for s in body {
                count_defs_stmt(s, temps, counts);
            }
        }
        HirStmt::For {
            init, update, body, ..
        } => {
            if let Some(i) = init {
                count_defs_stmt(i, temps, counts);
            }
            if let Some(u) = update {
                count_defs_stmt(u, temps, counts);
            }
            for s in body {
                count_defs_stmt(s, temps, counts);
            }
        }
        HirStmt::Switch { cases, default, .. } => {
            for case in cases {
                for s in &case.body {
                    count_defs_stmt(s, temps, counts);
                }
            }
            for s in default {
                count_defs_stmt(s, temps, counts);
            }
        }
        _ => {}
    }
}

/// Collect copy assignments `x = Var(y)` where x is a pure temp with exactly
/// one definition.
fn collect_copies<'a>(
    stmts: &'a [HirStmt],
    temp_names: &HashSet<&str>,
    def_count: &HashMap<&'a str, usize>,
    copy_map: &mut HashMap<String, String>,
) {
    for stmt in stmts {
        collect_copies_stmt(stmt, temp_names, def_count, copy_map);
    }
}

fn collect_copies_stmt<'a>(
    stmt: &'a HirStmt,
    temp_names: &HashSet<&str>,
    def_count: &HashMap<&'a str, usize>,
    copy_map: &mut HashMap<String, String>,
) {
    match stmt {
        HirStmt::Assign {
            lhs: HirLValue::Var(name),
            rhs: HirExpr::Var(src),
        } if temp_names.contains(name.as_str())
            && def_count.get(name.as_str()).copied().unwrap_or(0) == 1
            && name != src =>
        {
            copy_map.insert(name.clone(), src.clone());
        }
        HirStmt::Block(stmts) => collect_copies(stmts, temp_names, def_count, copy_map),
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            collect_copies(then_body, temp_names, def_count, copy_map);
            collect_copies(else_body, temp_names, def_count, copy_map);
        }
        HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            collect_copies(body, temp_names, def_count, copy_map);
        }
        HirStmt::For {
            init, update, body, ..
        } => {
            if let Some(i) = init {
                collect_copies_stmt(i, temp_names, def_count, copy_map);
            }
            if let Some(u) = update {
                collect_copies_stmt(u, temp_names, def_count, copy_map);
            }
            collect_copies(body, temp_names, def_count, copy_map);
        }
        HirStmt::Switch { cases, default, .. } => {
            for case in cases {
                collect_copies(&case.body, temp_names, def_count, copy_map);
            }
            collect_copies(default, temp_names, def_count, copy_map);
        }
        _ => {}
    }
}

fn collect_predicate_vars_in_stmts<'a>(stmts: &'a [HirStmt], out: &mut HashSet<&'a str>) {
    for stmt in stmts {
        collect_predicate_vars_in_stmt(stmt, out);
    }
}

fn collect_predicate_vars_in_stmt<'a>(stmt: &'a HirStmt, out: &mut HashSet<&'a str>) {
    match stmt {
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            collect_vars_in_expr(cond, out);
            collect_predicate_vars_in_stmts(then_body, out);
            collect_predicate_vars_in_stmts(else_body, out);
        }
        HirStmt::While { cond, body } => {
            collect_vars_in_expr(cond, out);
            collect_predicate_vars_in_stmts(body, out);
        }
        HirStmt::DoWhile { body, cond } => {
            collect_predicate_vars_in_stmts(body, out);
            collect_vars_in_expr(cond, out);
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(init) = init {
                collect_predicate_vars_in_stmt(init, out);
            }
            if let Some(cond) = cond {
                collect_vars_in_expr(cond, out);
            }
            if let Some(update) = update {
                collect_predicate_vars_in_stmt(update, out);
            }
            collect_predicate_vars_in_stmts(body, out);
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            collect_vars_in_expr(expr, out);
            for case in cases {
                collect_predicate_vars_in_stmts(&case.body, out);
            }
            collect_predicate_vars_in_stmts(default, out);
        }
        HirStmt::Block(stmts) => collect_predicate_vars_in_stmts(stmts, out),
        HirStmt::Assign { .. }
        | HirStmt::VaStart { .. }
        | HirStmt::Expr(_)
        | HirStmt::Return(_)
        | HirStmt::Break
        | HirStmt::Continue
        | HirStmt::Label(_)
        | HirStmt::Goto(_) => {}
    }
}

fn collect_vars_in_expr<'a>(expr: &'a HirExpr, out: &mut HashSet<&'a str>) {
    match expr {
        HirExpr::Var(name) | HirExpr::AddressOfGlobal(name) => {
            out.insert(name.as_str());
        }
        HirExpr::Const(_, _) => {}
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::Load { ptr: expr, .. }
        | HirExpr::PtrOffset { base: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. }
        | HirExpr::FieldAccess { base: expr, .. } => collect_vars_in_expr(expr, out),
        HirExpr::Binary { lhs, rhs, .. } => {
            collect_vars_in_expr(lhs, out);
            collect_vars_in_expr(rhs, out);
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                collect_vars_in_expr(arg, out);
            }
        }
        HirExpr::Index { base, index, .. } => {
            collect_vars_in_expr(base, out);
            collect_vars_in_expr(index, out);
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_vars_in_expr(cond, out);
            collect_vars_in_expr(then_expr, out);
            collect_vars_in_expr(else_expr, out);
        }
    }
}

/// Remove copy assignments `x = y` from the body where x is in copy_map.
fn remove_copy_assigns(
    stmts: &mut Vec<HirStmt>,
    copy_map: &HashMap<String, String>,
    changed: &mut bool,
) {
    for stmt in stmts.iter_mut() {
        remove_copy_assigns_nested(stmt, copy_map, changed);
    }
    stmts.retain(|stmt| {
        if let HirStmt::Assign {
            lhs: HirLValue::Var(name),
            rhs: HirExpr::Var(_),
        } = stmt
        {
            if copy_map.contains_key(name.as_str()) {
                *changed = true;
                return false;
            }
        }
        true
    });
}

fn remove_copy_assigns_nested(
    stmt: &mut HirStmt,
    copy_map: &HashMap<String, String>,
    changed: &mut bool,
) {
    match stmt {
        HirStmt::Block(stmts) => remove_copy_assigns(stmts, copy_map, changed),
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            remove_copy_assigns(then_body, copy_map, changed);
            remove_copy_assigns(else_body, copy_map, changed);
        }
        HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            remove_copy_assigns(body, copy_map, changed);
        }
        HirStmt::For {
            init, update, body, ..
        } => {
            if let Some(i) = init {
                remove_copy_assigns_nested(i, copy_map, changed);
            }
            if let Some(u) = update {
                remove_copy_assigns_nested(u, copy_map, changed);
            }
            remove_copy_assigns(body, copy_map, changed);
        }
        HirStmt::Switch { cases, default, .. } => {
            for case in cases.iter_mut() {
                remove_copy_assigns(&mut case.body, copy_map, changed);
            }
            remove_copy_assigns(default, copy_map, changed);
        }
        _ => {}
    }
}

/// Substitute every rvalue occurrence of `x` (keys of copy_map) with its
/// source `y` (values of copy_map) throughout all expressions.
fn substitute_copies_in_stmts(
    stmts: &mut Vec<HirStmt>,
    copy_map: &HashMap<String, String>,
    changed: &mut bool,
) {
    for stmt in stmts.iter_mut() {
        substitute_copies_in_stmt(stmt, copy_map, changed);
    }
}

fn substitute_copies_in_stmt(
    stmt: &mut HirStmt,
    copy_map: &HashMap<String, String>,
    changed: &mut bool,
) {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            substitute_copies_lvalue(lhs, copy_map, changed);
            substitute_copies_expr(rhs, copy_map, changed);
        }
        HirStmt::VaStart { va_list, .. } => {
            substitute_copies_expr(va_list, copy_map, changed);
        }
        HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => {
            substitute_copies_expr(expr, copy_map, changed);
        }
        HirStmt::Return(None)
        | HirStmt::Break
        | HirStmt::Continue
        | HirStmt::Label(_)
        | HirStmt::Goto(_) => {}
        HirStmt::Block(stmts) => substitute_copies_in_stmts(stmts, copy_map, changed),
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            substitute_copies_expr(cond, copy_map, changed);
            substitute_copies_in_stmts(then_body, copy_map, changed);
            substitute_copies_in_stmts(else_body, copy_map, changed);
        }
        HirStmt::While { cond, body } => {
            substitute_copies_expr(cond, copy_map, changed);
            substitute_copies_in_stmts(body, copy_map, changed);
        }
        HirStmt::DoWhile { body, cond } => {
            substitute_copies_in_stmts(body, copy_map, changed);
            substitute_copies_expr(cond, copy_map, changed);
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(i) = init {
                substitute_copies_in_stmt(i, copy_map, changed);
            }
            if let Some(c) = cond {
                substitute_copies_expr(c, copy_map, changed);
            }
            if let Some(u) = update {
                substitute_copies_in_stmt(u, copy_map, changed);
            }
            substitute_copies_in_stmts(body, copy_map, changed);
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            substitute_copies_expr(expr, copy_map, changed);
            for case in cases.iter_mut() {
                substitute_copies_in_stmts(&mut case.body, copy_map, changed);
            }
            substitute_copies_in_stmts(default, copy_map, changed);
        }
    }
}

fn substitute_copies_lvalue(
    lhs: &mut HirLValue,
    copy_map: &HashMap<String, String>,
    changed: &mut bool,
) {
    match lhs {
        HirLValue::Var(_) => {}
        HirLValue::Deref { ptr, .. } => substitute_copies_expr(ptr, copy_map, changed),
        HirLValue::Index { base, index, .. } => {
            substitute_copies_expr(base, copy_map, changed);
            substitute_copies_expr(index, copy_map, changed);
        }
        HirLValue::FieldAccess { base, .. } => {
            substitute_copies_expr(base, copy_map, changed);
        }
    }
}

fn substitute_copies_expr(
    expr: &mut HirExpr,
    copy_map: &HashMap<String, String>,
    changed: &mut bool,
) {
    match expr {
        HirExpr::Var(name) | HirExpr::AddressOfGlobal(name) => {
            if let Some(src) = copy_map.get(name.as_str()) {
                *name = src.clone();
                *changed = true;
            }
        }
        HirExpr::Const(_, _) => {}
        HirExpr::Cast { expr: inner, .. }
        | HirExpr::Unary { expr: inner, .. }
        | HirExpr::Load { ptr: inner, .. }
        | HirExpr::PtrOffset { base: inner, .. }
        | HirExpr::AggregateCopy { src: inner, .. }
        | HirExpr::FieldAccess { base: inner, .. } => {
            substitute_copies_expr(inner, copy_map, changed);
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            substitute_copies_expr(lhs, copy_map, changed);
            substitute_copies_expr(rhs, copy_map, changed);
        }
        HirExpr::Call { args, .. } => {
            for a in args.iter_mut() {
                substitute_copies_expr(a, copy_map, changed);
            }
        }
        HirExpr::Index { base, index, .. } => {
            substitute_copies_expr(base, copy_map, changed);
            substitute_copies_expr(index, copy_map, changed);
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            substitute_copies_expr(cond, copy_map, changed);
            substitute_copies_expr(then_expr, copy_map, changed);
            substitute_copies_expr(else_expr, copy_map, changed);
        }
    }
}

// ── Join Variable Coalescing ──────────────────────────────────────────────────

/// Detect 2-way join patterns: an if-else structure where both the then-branch
/// and the else-branch end with an assignment to the same set of variables.
/// Rename subsequent uses of those variables to a single canonical name
/// (the one from the then-branch), eliminating redundant parallel assignments.
///
/// Classic pattern:
/// ```text
/// if (cond) { v_then = expr_a; } else { v_else = expr_b; }
/// use(v_then);    ← v_then is the canonical name
/// ```
///
/// After coalescing, the else-branch assignment is renamed so that code after
/// the if-else consistently uses the same variable name whether it came from
/// the then-branch or the else-branch.
///
/// Returns `true` if any renaming was made.
pub(crate) fn join_coalescing_pass(func: &mut HirFunction) -> bool {
    let temp_names: HashSet<String> = func
        .locals
        .iter()
        .filter(|b| b.is_temp_like())
        .map(|b| b.name.clone())
        .collect();

    if temp_names.is_empty() {
        return false;
    }

    let map = DefUseMap::build(&func.body);
    let mut rename_map: HashMap<String, String> = HashMap::new();
    collect_join_renames(&func.body, &temp_names, &map, &mut rename_map);

    if rename_map.is_empty() {
        return false;
    }

    let mut changed = false;
    // Apply renames: wherever we see an assignment `else_var = rhs`, rename
    // `else_var` to `then_var` in the LHS (inside the else-branch).
    // Also substitute rvalue uses of `else_var` with `then_var` everywhere.
    apply_join_renames(&mut func.body, &rename_map, &mut changed);

    if changed {
        prune_unused_temp_bindings(func);
    }
    changed
}

/// Walk the statement list looking for If statements that have matching
/// last-assignments in both branches.
fn collect_join_renames(
    stmts: &[HirStmt],
    temp_names: &HashSet<String>,
    map: &DefUseMap,
    rename_map: &mut HashMap<String, String>,
) {
    for (idx, stmt) in stmts.iter().enumerate() {
        match stmt {
            HirStmt::If {
                then_body,
                else_body,
                ..
            } if !then_body.is_empty() && !else_body.is_empty() => {
                // Find the last assignments in each branch.
                let then_assigns = last_assigns(then_body, temp_names);
                let else_assigns = last_assigns(else_body, temp_names);

                // For each (then_var, else_var) pair where both are pure temps
                // with the same type and else_var only ever appears in the
                // else-branch after this point (not used independently
                // elsewhere), we can coalesce: rename else_var → then_var.
                for (then_var, then_ty) in &then_assigns {
                    for (else_var, else_ty) in &else_assigns {
                        if then_var == else_var {
                            continue;
                        }
                        if then_ty != else_ty {
                            continue;
                        }
                        // Only coalesce if else_var is NOT used outside the
                        // else-branch (other than in subsequent stmts where
                        // then_var is also used — this is an approximation).
                        let else_uses_total =
                            map.use_count.get(else_var.as_str()).copied().unwrap_or(0);
                        let else_uses_after = count_uses_after(stmts, idx + 1, else_var);
                        // If all remaining uses of else_var are in the
                        // statements that follow this If (not inside the
                        // else-branch itself), they can be replaced by
                        // then_var.  The else-branch use is the definition
                        // site (assignment), which is not counted in use_count.
                        let else_uses_in_branch = else_uses_total.saturating_sub(else_uses_after);
                        if else_uses_in_branch == 0 {
                            rename_map.insert(else_var.clone(), then_var.clone());
                        }
                    }
                }

                // Recurse into branches.
                collect_join_renames(then_body, temp_names, map, rename_map);
                collect_join_renames(else_body, temp_names, map, rename_map);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_join_renames(then_body, temp_names, map, rename_map);
                collect_join_renames(else_body, temp_names, map, rename_map);
            }
            HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
                collect_join_renames(body, temp_names, map, rename_map);
            }
            HirStmt::For { init: _, body, .. } => {
                collect_join_renames(body, temp_names, map, rename_map);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_join_renames(&case.body, temp_names, map, rename_map);
                }
                collect_join_renames(default, temp_names, map, rename_map);
            }
            _ => {}
        }
    }
}

/// Return (name, type_repr) pairs for the LAST assignment to each pure temp
/// within a flat statement list.
fn last_assigns(stmts: &[HirStmt], temp_names: &HashSet<String>) -> Vec<(String, String)> {
    let mut seen: HashMap<String, String> = HashMap::new();
    for stmt in stmts {
        if let HirStmt::Assign {
            lhs: HirLValue::Var(name),
            rhs,
        } = stmt
        {
            if temp_names.contains(name.as_str()) {
                seen.insert(name.clone(), type_repr(rhs));
            }
        }
    }
    seen.into_iter().collect()
}

/// Quick structural type fingerprint for an expression (used to gate
/// coalescing by compatible assignment shapes).
fn type_repr(expr: &HirExpr) -> String {
    match expr {
        HirExpr::Const(_, ty) | HirExpr::Cast { ty, .. } => format!("{ty:?}"),
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) => "var".to_string(),
        HirExpr::Binary { ty, .. } | HirExpr::Unary { ty, .. } => format!("{ty:?}"),
        HirExpr::Load { ty, .. } => format!("load_{ty:?}"),
        _ => "other".to_string(),
    }
}

/// Count uses of `name` in stmts[start_idx..].
fn count_uses_after(stmts: &[HirStmt], start_idx: usize, name: &str) -> usize {
    stmts[start_idx.min(stmts.len())..]
        .iter()
        .map(|s| count_uses_in_stmt_flat(s, name))
        .sum()
}

fn count_uses_in_stmt_flat(stmt: &HirStmt, name: &str) -> usize {
    match stmt {
        HirStmt::Assign { lhs: _, rhs } => count_var_in_expr(rhs, name),
        HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => count_var_in_expr(expr, name),
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            count_var_in_expr(cond, name)
                + then_body
                    .iter()
                    .map(|s| count_uses_in_stmt_flat(s, name))
                    .sum::<usize>()
                + else_body
                    .iter()
                    .map(|s| count_uses_in_stmt_flat(s, name))
                    .sum::<usize>()
        }
        HirStmt::While { cond, body } | HirStmt::DoWhile { body, cond } => {
            count_var_in_expr(cond, name)
                + body
                    .iter()
                    .map(|s| count_uses_in_stmt_flat(s, name))
                    .sum::<usize>()
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            init.as_deref()
                .map_or(0, |s| count_uses_in_stmt_flat(s, name))
                + cond.as_ref().map_or(0, |e| count_var_in_expr(e, name))
                + update
                    .as_deref()
                    .map_or(0, |s| count_uses_in_stmt_flat(s, name))
                + body
                    .iter()
                    .map(|s| count_uses_in_stmt_flat(s, name))
                    .sum::<usize>()
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            count_var_in_expr(expr, name)
                + cases
                    .iter()
                    .flat_map(|c| &c.body)
                    .map(|s| count_uses_in_stmt_flat(s, name))
                    .sum::<usize>()
                + default
                    .iter()
                    .map(|s| count_uses_in_stmt_flat(s, name))
                    .sum::<usize>()
        }
        _ => 0,
    }
}

fn count_var_in_expr(expr: &HirExpr, name: &str) -> usize {
    match expr {
        HirExpr::Var(n) | HirExpr::AddressOfGlobal(n) => usize::from(n.as_str() == name),
        HirExpr::Const(_, _) => 0,
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::Load { ptr: expr, .. }
        | HirExpr::PtrOffset { base: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. }
        | HirExpr::FieldAccess { base: expr, .. } => count_var_in_expr(expr, name),
        HirExpr::Binary { lhs, rhs, .. } => {
            count_var_in_expr(lhs, name) + count_var_in_expr(rhs, name)
        }
        HirExpr::Call { args, .. } => args.iter().map(|a| count_var_in_expr(a, name)).sum(),
        HirExpr::Index { base, index, .. } => {
            count_var_in_expr(base, name) + count_var_in_expr(index, name)
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            count_var_in_expr(cond, name)
                + count_var_in_expr(then_expr, name)
                + count_var_in_expr(else_expr, name)
        }
    }
}

/// Apply renames: in the else-branch of each If, rename LHS `else_var` to
/// `then_var`. Also rename rvalue uses of `else_var` to `then_var` everywhere
/// outside the else-branch (and inside other branches).
fn apply_join_renames(
    stmts: &mut Vec<HirStmt>,
    rename_map: &HashMap<String, String>,
    changed: &mut bool,
) {
    for stmt in stmts.iter_mut() {
        apply_join_renames_stmt(stmt, rename_map, changed);
    }
}

fn apply_join_renames_stmt(
    stmt: &mut HirStmt,
    rename_map: &HashMap<String, String>,
    changed: &mut bool,
) {
    match stmt {
        HirStmt::Assign {
            lhs: HirLValue::Var(name),
            rhs,
        } => {
            if let Some(canonical) = rename_map.get(name.as_str()) {
                *name = canonical.clone();
                *changed = true;
            }
            apply_join_renames_expr(rhs, rename_map, changed);
        }
        HirStmt::Assign { lhs, rhs } => {
            apply_join_renames_expr(rhs, rename_map, changed);
            // Also rename inside index/deref lvalues.
            apply_join_renames_lvalue(lhs, rename_map, changed);
        }
        HirStmt::VaStart { va_list, .. } => {
            apply_join_renames_expr(va_list, rename_map, changed);
        }
        HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => {
            apply_join_renames_expr(expr, rename_map, changed);
        }
        HirStmt::Return(None)
        | HirStmt::Break
        | HirStmt::Continue
        | HirStmt::Label(_)
        | HirStmt::Goto(_) => {}
        HirStmt::Block(stmts) => apply_join_renames(stmts, rename_map, changed),
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            apply_join_renames_expr(cond, rename_map, changed);
            apply_join_renames(then_body, rename_map, changed);
            apply_join_renames(else_body, rename_map, changed);
        }
        HirStmt::While { cond, body } => {
            apply_join_renames_expr(cond, rename_map, changed);
            apply_join_renames(body, rename_map, changed);
        }
        HirStmt::DoWhile { body, cond } => {
            apply_join_renames(body, rename_map, changed);
            apply_join_renames_expr(cond, rename_map, changed);
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(i) = init {
                apply_join_renames_stmt(i, rename_map, changed);
            }
            if let Some(c) = cond {
                apply_join_renames_expr(c, rename_map, changed);
            }
            if let Some(u) = update {
                apply_join_renames_stmt(u, rename_map, changed);
            }
            apply_join_renames(body, rename_map, changed);
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            apply_join_renames_expr(expr, rename_map, changed);
            for case in cases.iter_mut() {
                apply_join_renames(&mut case.body, rename_map, changed);
            }
            apply_join_renames(default, rename_map, changed);
        }
    }
}

fn apply_join_renames_lvalue(
    lhs: &mut HirLValue,
    rename_map: &HashMap<String, String>,
    changed: &mut bool,
) {
    match lhs {
        HirLValue::Var(_) => {}
        HirLValue::Deref { ptr, .. } => apply_join_renames_expr(ptr, rename_map, changed),
        HirLValue::Index { base, index, .. } => {
            apply_join_renames_expr(base, rename_map, changed);
            apply_join_renames_expr(index, rename_map, changed);
        }
        HirLValue::FieldAccess { base, .. } => {
            apply_join_renames_expr(base, rename_map, changed);
        }
    }
}

fn apply_join_renames_expr(
    expr: &mut HirExpr,
    rename_map: &HashMap<String, String>,
    changed: &mut bool,
) {
    match expr {
        HirExpr::Var(name) | HirExpr::AddressOfGlobal(name) => {
            if let Some(canonical) = rename_map.get(name.as_str()) {
                *name = canonical.clone();
                *changed = true;
            }
        }
        HirExpr::Const(_, _) => {}
        HirExpr::Cast { expr: inner, .. }
        | HirExpr::Unary { expr: inner, .. }
        | HirExpr::Load { ptr: inner, .. }
        | HirExpr::PtrOffset { base: inner, .. }
        | HirExpr::AggregateCopy { src: inner, .. }
        | HirExpr::FieldAccess { base: inner, .. } => {
            apply_join_renames_expr(inner, rename_map, changed);
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            apply_join_renames_expr(lhs, rename_map, changed);
            apply_join_renames_expr(rhs, rename_map, changed);
        }
        HirExpr::Call { args, .. } => {
            for a in args.iter_mut() {
                apply_join_renames_expr(a, rename_map, changed);
            }
        }
        HirExpr::Index { base, index, .. } => {
            apply_join_renames_expr(base, rename_map, changed);
            apply_join_renames_expr(index, rename_map, changed);
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            apply_join_renames_expr(cond, rename_map, changed);
            apply_join_renames_expr(then_expr, rename_map, changed);
            apply_join_renames_expr(else_expr, rename_map, changed);
        }
    }
}

// ── Constant Propagation Helpers ──────────────────────────────────────────────

fn collect_constants<'a>(
    stmts: &'a [HirStmt],
    eligible_vars: &HashSet<&str>,
    def_count: &HashMap<&'a str, usize>,
    const_map: &mut HashMap<String, HirExpr>,
) {
    for stmt in stmts {
        collect_constants_stmt(stmt, eligible_vars, def_count, const_map);
    }
}

fn collect_constants_stmt<'a>(
    stmt: &'a HirStmt,
    eligible_vars: &HashSet<&str>,
    def_count: &HashMap<&'a str, usize>,
    const_map: &mut HashMap<String, HirExpr>,
) {
    match stmt {
        HirStmt::Assign {
            lhs: HirLValue::Var(name),
            rhs: const_expr @ HirExpr::Const(_, _),
        } if eligible_vars.contains(name.as_str())
            && def_count.get(name.as_str()).copied().unwrap_or(0) == 1 =>
        {
            const_map.insert(name.clone(), const_expr.clone());
        }
        HirStmt::Block(stmts) => collect_constants(stmts, eligible_vars, def_count, const_map),
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            collect_constants(then_body, eligible_vars, def_count, const_map);
            collect_constants(else_body, eligible_vars, def_count, const_map);
        }
        HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            collect_constants(body, eligible_vars, def_count, const_map);
        }
        HirStmt::For {
            init, update, body, ..
        } => {
            if let Some(i) = init {
                collect_constants_stmt(i, eligible_vars, def_count, const_map);
            }
            if let Some(u) = update {
                collect_constants_stmt(u, eligible_vars, def_count, const_map);
            }
            collect_constants(body, eligible_vars, def_count, const_map);
        }
        HirStmt::Switch { cases, default, .. } => {
            for case in cases {
                collect_constants(&case.body, eligible_vars, def_count, const_map);
            }
            collect_constants(default, eligible_vars, def_count, const_map);
        }
        _ => {}
    }
}

fn remove_constant_assigns(
    stmts: &mut Vec<HirStmt>,
    const_map: &HashMap<String, HirExpr>,
    changed: &mut bool,
) {
    for stmt in stmts.iter_mut() {
        remove_constant_assigns_nested(stmt, const_map, changed);
    }
    stmts.retain(|stmt| {
        if let HirStmt::Assign {
            lhs: HirLValue::Var(name),
            rhs: HirExpr::Const(_, _),
        } = stmt
        {
            if const_map.contains_key(name.as_str()) {
                *changed = true;
                return false;
            }
        }
        true
    });
}

fn remove_constant_assigns_nested(
    stmt: &mut HirStmt,
    const_map: &HashMap<String, HirExpr>,
    changed: &mut bool,
) {
    match stmt {
        HirStmt::Block(stmts) => remove_constant_assigns(stmts, const_map, changed),
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            remove_constant_assigns(then_body, const_map, changed);
            remove_constant_assigns(else_body, const_map, changed);
        }
        HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            remove_constant_assigns(body, const_map, changed);
        }
        HirStmt::For {
            init, update, body, ..
        } => {
            if let Some(i) = init {
                remove_constant_assigns_nested(i, const_map, changed);
            }
            if let Some(u) = update {
                remove_constant_assigns_nested(u, const_map, changed);
            }
            remove_constant_assigns(body, const_map, changed);
        }
        HirStmt::Switch { cases, default, .. } => {
            for case in cases.iter_mut() {
                remove_constant_assigns(&mut case.body, const_map, changed);
            }
            remove_constant_assigns(default, const_map, changed);
        }
        _ => {}
    }
}

fn substitute_constants_in_stmts(
    stmts: &mut Vec<HirStmt>,
    const_map: &HashMap<String, HirExpr>,
    changed: &mut bool,
) {
    for stmt in stmts.iter_mut() {
        substitute_constants_in_stmt(stmt, const_map, changed);
    }
}

fn substitute_constants_in_stmt(
    stmt: &mut HirStmt,
    const_map: &HashMap<String, HirExpr>,
    changed: &mut bool,
) {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            substitute_constants_lvalue(lhs, const_map, changed);
            substitute_constants_expr(rhs, const_map, changed);
        }
        HirStmt::VaStart { va_list, .. } => {
            substitute_constants_expr(va_list, const_map, changed);
        }
        HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => {
            substitute_constants_expr(expr, const_map, changed);
        }
        _ => {}
    }
    match stmt {
        HirStmt::Block(stmts) => substitute_constants_in_stmts(stmts, const_map, changed),
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            substitute_constants_expr(cond, const_map, changed);
            substitute_constants_in_stmts(then_body, const_map, changed);
            substitute_constants_in_stmts(else_body, const_map, changed);
        }
        HirStmt::While { cond, body } => {
            substitute_constants_expr(cond, const_map, changed);
            substitute_constants_in_stmts(body, const_map, changed);
        }
        HirStmt::DoWhile { body, cond } => {
            substitute_constants_in_stmts(body, const_map, changed);
            substitute_constants_expr(cond, const_map, changed);
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(i) = init {
                substitute_constants_in_stmt(i, const_map, changed);
            }
            if let Some(c) = cond {
                substitute_constants_expr(c, const_map, changed);
            }
            if let Some(u) = update {
                substitute_constants_in_stmt(u, const_map, changed);
            }
            substitute_constants_in_stmts(body, const_map, changed);
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            substitute_constants_expr(expr, const_map, changed);
            for case in cases.iter_mut() {
                substitute_constants_in_stmts(&mut case.body, const_map, changed);
            }
            substitute_constants_in_stmts(default, const_map, changed);
        }
        _ => {}
    }
}

fn substitute_constants_lvalue(
    lhs: &mut HirLValue,
    const_map: &HashMap<String, HirExpr>,
    changed: &mut bool,
) {
    match lhs {
        HirLValue::Var(_) => {}
        HirLValue::Deref { ptr, .. } => substitute_constants_expr(ptr, const_map, changed),
        HirLValue::Index { base, index, .. } => {
            substitute_constants_expr(base, const_map, changed);
            substitute_constants_expr(index, const_map, changed);
        }
        HirLValue::FieldAccess { base, .. } => {
            substitute_constants_expr(base, const_map, changed);
        }
    }
}

fn substitute_constants_expr(
    expr: &mut HirExpr,
    const_map: &HashMap<String, HirExpr>,
    changed: &mut bool,
) {
    if let HirExpr::Var(name) = expr {
        if let Some(c) = const_map.get(name.as_str()) {
            *expr = c.clone();
            *changed = true;
            return;
        }
    }
    match expr {
        HirExpr::Cast { expr: inner, .. }
        | HirExpr::Unary { expr: inner, .. }
        | HirExpr::Load { ptr: inner, .. }
        | HirExpr::PtrOffset { base: inner, .. }
        | HirExpr::AggregateCopy { src: inner, .. }
        | HirExpr::FieldAccess { base: inner, .. } => {
            substitute_constants_expr(inner, const_map, changed);
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            substitute_constants_expr(lhs, const_map, changed);
            substitute_constants_expr(rhs, const_map, changed);
        }
        HirExpr::Call { args, .. } => {
            for a in args.iter_mut() {
                substitute_constants_expr(a, const_map, changed);
            }
        }
        HirExpr::Index { base, index, .. } => {
            substitute_constants_expr(base, const_map, changed);
            substitute_constants_expr(index, const_map, changed);
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            substitute_constants_expr(cond, const_map, changed);
            substitute_constants_expr(then_expr, const_map, changed);
            substitute_constants_expr(else_expr, const_map, changed);
        }
        _ => {}
    }
}

// ── Loop-carried/Preheader Preservation Helpers ──────────────────────────────

fn collect_loop_preservation_vars(stmts: &[HirStmt]) -> HashSet<String> {
    let mut defined_outside: HashSet<String> = HashSet::new();
    let mut used_inside: HashSet<String> = HashSet::new();
    collect_defs_outside_loops(stmts, &mut defined_outside);
    collect_uses_inside_loops(stmts, &mut used_inside);
    defined_outside.intersection(&used_inside).cloned().collect()
}

fn collect_defs_outside_loops(stmts: &[HirStmt], out: &mut HashSet<String>) {
    for stmt in stmts {
        match stmt {
            HirStmt::Assign { lhs, .. } => {
                if let HirLValue::Var(name) = lhs {
                    out.insert(name.clone());
                }
            }
            HirStmt::Block(body) => collect_defs_outside_loops(body, out),
            HirStmt::If { then_body, else_body, .. } => {
                collect_defs_outside_loops(then_body, out);
                collect_defs_outside_loops(else_body, out);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_defs_outside_loops(&case.body, out);
                }
                collect_defs_outside_loops(default, out);
            }
            _ => {}
        }
    }
}

fn collect_uses_inside_loops(stmts: &[HirStmt], out: &mut HashSet<String>) {
    // Collect via a local &str set, then promote to String at the boundary so
    // we can reuse the shared helpers (collect_vars_in_expr, etc.) without
    // changing their signatures.
    fn inner<'a>(stmts: &'a [HirStmt], inner_out: &mut HashSet<&'a str>) {
        for stmt in stmts {
            match stmt {
                HirStmt::While { cond, body } => {
                    collect_vars_in_expr(cond, inner_out);
                    collect_all_vars_in_stmts(body, inner_out);
                }
                HirStmt::DoWhile { body, cond } => {
                    collect_all_vars_in_stmts(body, inner_out);
                    collect_vars_in_expr(cond, inner_out);
                }
                HirStmt::For { init, cond, update, body } => {
                    if let Some(i) = init {
                        collect_all_vars_in_stmt(i, inner_out);
                    }
                    if let Some(c) = cond {
                        collect_vars_in_expr(c, inner_out);
                    }
                    if let Some(u) = update {
                        collect_all_vars_in_stmt(u, inner_out);
                    }
                    collect_all_vars_in_stmts(body, inner_out);
                }
                HirStmt::Block(body) => inner(body, inner_out),
                HirStmt::If { then_body, else_body, .. } => {
                    inner(then_body, inner_out);
                    inner(else_body, inner_out);
                }
                HirStmt::Switch { cases, default, .. } => {
                    for case in cases {
                        inner(&case.body, inner_out);
                    }
                    inner(default, inner_out);
                }
                _ => {}
            }
        }
    }
    let mut local: HashSet<&str> = HashSet::new();
    inner(stmts, &mut local);
    out.extend(local.into_iter().map(str::to_owned));
}

fn collect_all_vars_in_stmts<'a>(stmts: &'a [HirStmt], out: &mut HashSet<&'a str>) {
    for stmt in stmts {
        collect_all_vars_in_stmt(stmt, out);
    }
}

fn collect_all_vars_in_stmt<'a>(stmt: &'a HirStmt, out: &mut HashSet<&'a str>) {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            collect_vars_in_lvalue(lhs, out);
            collect_vars_in_expr(rhs, out);
        }
        HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) | HirStmt::VaStart { va_list: expr, .. } => {
            collect_vars_in_expr(expr, out);
        }
        HirStmt::Block(body)
        | HirStmt::While { body, .. }
        | HirStmt::DoWhile { body, .. } => collect_all_vars_in_stmts(body, out),
        HirStmt::If { then_body, else_body, cond } => {
            collect_vars_in_expr(cond, out);
            collect_all_vars_in_stmts(then_body, out);
            collect_all_vars_in_stmts(else_body, out);
        }
        HirStmt::For { init, cond, update, body } => {
            if let Some(i) = init {
                collect_all_vars_in_stmt(i, out);
            }
            if let Some(c) = cond {
                collect_vars_in_expr(c, out);
            }
            if let Some(u) = update {
                collect_all_vars_in_stmt(u, out);
            }
            collect_all_vars_in_stmts(body, out);
        }
        HirStmt::Switch { cases, default, expr } => {
            collect_vars_in_expr(expr, out);
            for case in cases {
                collect_all_vars_in_stmts(&case.body, out);
            }
            collect_all_vars_in_stmts(default, out);
        }
        _ => {}
    }
}

fn collect_vars_in_lvalue<'a>(lhs: &'a HirLValue, out: &mut HashSet<&'a str>) {
    match lhs {
        HirLValue::Var(name) => {
            out.insert(name.as_str());
        }
        HirLValue::Deref { ptr, .. } => collect_vars_in_expr(ptr, out),
        HirLValue::Index { base, index, .. } => {
            collect_vars_in_expr(base, out);
            collect_vars_in_expr(index, out);
        }
        HirLValue::FieldAccess { base, .. } => collect_vars_in_expr(base, out),
    }
}
