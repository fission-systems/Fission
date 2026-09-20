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
/// pure temporary with a single definition, substitutes `y` for later uses of
/// `x` in the same straight-line statement run and removes the assignment when
/// all of its uses are covered by that run. Constant propagation follows the
/// same scope rule.
///
/// A whole-function definition count is only an admission guard. It is not a
/// dominance proof: a single definition can still live in one branch or one
/// loop iteration. The active replacement map is therefore cleared at every
/// control-flow boundary, label, goto, call, and nested statement list.
///
/// ## Join Variable Coalescing (`join_coalescing_pass`)
///
/// Detects if-else structures where both branches end by assigning to the
/// *same* set of variables and renames join-point uses to the shared variable.
/// This models the classical SSA out-of-SSA transformation for 2-way joins.
use super::super::cleanup::{prune_unused_dead_local_bindings, prune_unused_temp_bindings};
use crate::prelude::*;
use crate::{HashMap, HashSet};
use fission_midend_core::wave_stats;

// ── Copy Propagation ─────────────────────────────────────────────────────────

/// Propagate copies `x = y` (where both x and y are named variables and x is a
/// pure temporary with exactly one definition) by replacing every rvalue use of
/// `x` with `y` and removing the assignment.
///
/// Returns `true` if any substitution was made.
pub fn copy_propagation_pass(func: &mut PreHirFunction) -> bool {
    let mut changed = false;
    let loop_preservation_vars = collect_loop_preservation_vars(&func.body);
    // A local whose address is taken is reached by more than its name, and
    // this pass only sees names -- see `collect_address_taken_locals`.
    let address_taken = crate::analysis::defuse::collect_address_taken_locals(&func.body);

    // --- Phase 1: Standard Copy Propagation ---
    let preserved_temps = preserved_materialization_names(&func.locals);
    let temp_names: HashSet<String> = func
        .locals
        .iter()
        .filter(|b| b.is_temp_like() && !address_taken.contains(&b.name))
        .map(|b| b.name.clone())
        .collect();

    if !temp_names.is_empty() {
        let defuse = DefUseMap::build(&func.body);
        let def_count = defuse.def_count;
        let global_reads = defuse.use_count;
        let mut predicate_vars = HashSet::default();
        collect_predicate_vars_in_stmts(&func.body, &mut predicate_vars);
        let mut excluded_names: HashSet<String> =
            predicate_vars.into_iter().map(str::to_owned).collect();
        excluded_names.extend(preserved_temps.iter().map(|name| (*name).to_owned()));
        excluded_names.extend(loop_preservation_vars.iter().cloned());
        let preserved_skip_count = count_preserved_copyprop_candidates(
            &func.body,
            &temp_names,
            &preserved_temps,
            &def_count,
        );

        let kind = LinearReplacementKind::Copies {
            destinations: &temp_names,
            excluded: &excluded_names,
        };
        propagate_linear_replacements(
            &mut func.body,
            &kind,
            &def_count,
            &global_reads,
            &mut changed,
        );
        wave_stats::add_preserved_temp_copyprop_skip(preserved_skip_count);
    }

    // --- Phase 2: Constant Propagation for Primitive Variables ---
    let eligible_vars: HashSet<String> = func
        .locals
        .iter()
        .filter(|b| {
            matches!(
                b.ty,
                NirType::Int { .. } | NirType::Float { .. } | NirType::Bool
            ) && !should_skip_copyprop_for_preserved_name(&b.name, &preserved_temps)
                && !loop_preservation_vars.contains(b.name.as_str())
                && !address_taken.contains(&b.name)
        })
        .map(|b| b.name.clone())
        .collect();

    if !eligible_vars.is_empty() {
        let defuse = DefUseMap::build(&func.body);
        let def_count = defuse.def_count;
        let global_reads = defuse.use_count;
        let kind = LinearReplacementKind::Constants {
            destinations: &eligible_vars,
        };
        propagate_linear_replacements(
            &mut func.body,
            &kind,
            &def_count,
            &global_reads,
            &mut changed,
        );
    }

    if changed {
        prune_unused_temp_bindings(func);
        prune_unused_dead_local_bindings(func);
    }
    changed
}

#[cfg(test)]
mod tests {

    /// `edx = param_3` and `uVar8 = edx` can both be admitted at once. Every
    /// entry's defining copy is removed, but substitution runs once, so
    /// replacing `uVar8` with `edx` used to re-introduce a name whose own
    /// definition had just gone. `bounded_checksum` at gcc-m32 -O0 returned an
    /// undefined `edx` that way.
    #[test]
    fn a_copy_chain_resolves_to_its_root_before_substitution() {
        let mut m: HashMap<String, String> = HashMap::default();
        m.insert("edx".to_string(), "param_3".to_string());
        m.insert("uVar8".to_string(), "edx".to_string());
        resolve_copy_chains(&mut m);
        assert_eq!(m.get("edx").map(String::as_str), Some("param_3"));
        assert_eq!(
            m.get("uVar8").map(String::as_str),
            Some("param_3"),
            "uVar8 must skip the copy that is being removed with it"
        );
    }

    /// A cycle has no root; those entries are dropped rather than resolved to
    /// whichever element the iteration happened to stop on.
    #[test]
    fn a_copy_cycle_is_dropped_rather_than_resolved_arbitrarily() {
        let mut m: HashMap<String, String> = HashMap::default();
        m.insert("a".to_string(), "b".to_string());
        m.insert("b".to_string(), "a".to_string());
        resolve_copy_chains(&mut m);
        assert!(m.is_empty(), "cyclic copies must not be substituted: {m:?}");
    }
    use super::*;
    // prelude via parent
    use crate::analysis::preservation::preserved_binding_origin;

    fn int(bits: u32) -> NirType {
        NirType::Int {
            bits,
            signed: false,
        }
    }

    #[test]
    fn copy_propagation_skips_preserved_temp_alias() {
        let mut func = PreHirFunction {
            name: "test_copy_prop_preserved".to_string(),
            int_param_offsets: Vec::new(),
            params: vec![],
            locals: vec![PreHirBinding {
                name: "uVar0".to_string(),
                ty: int(32),
                surface_type_name: None,
                origin: Some(preserved_binding_origin()),
                initializer: None,
            }],
            return_type: int(32),
            surface_return_type_name: None,
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("uVar0".to_string()),
                    rhs: PreHirExpr::Var("eax".to_string()),
                },
                PreHirStmt::If {
                    cond: PreHirExpr::Binary {
                        op: PreHirBinaryOp::Eq,
                        lhs: Box::new(PreHirExpr::Var("uVar0".to_string())),
                        rhs: Box::new(PreHirExpr::Const(0, int(32))),
                        ty: NirType::Bool,
                    },
                    then_body: vec![PreHirStmt::Return(Some(PreHirExpr::Const(1, int(32))))].into(),
                    else_body: vec![PreHirStmt::Return(Some(PreHirExpr::Const(0, int(32))))].into(),
                },
            ],
            ..Default::default()
        };

        assert!(!copy_propagation_pass(&mut func));
        let PreHirStmt::If { cond, .. } = &func.body[1] else {
            panic!("expected preserved temp consumer to stay in the if condition");
        };
        assert!(format_expr_key(cond).contains("uVar0"));
    }

    #[test]
    fn copy_propagation_skips_single_use_alias_of_preserved_source() {
        let mut func = PreHirFunction {
            name: "test_copy_prop_preserved_source".to_string(),
            int_param_offsets: Vec::new(),
            params: vec![],
            locals: vec![
                PreHirBinding {
                    name: "uVar0".to_string(),
                    ty: int(32),
                    surface_type_name: None,
                    origin: Some(preserved_binding_origin()),
                    initializer: None,
                },
                PreHirBinding {
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
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("uVar1".to_string()),
                    rhs: PreHirExpr::Var("uVar0".to_string()),
                },
                PreHirStmt::Return(Some(PreHirExpr::Var("uVar1".to_string()))),
            ],
            ..Default::default()
        };

        assert!(!copy_propagation_pass(&mut func));
        assert_eq!(func.body.len(), 2);
        assert!(matches!(
            &func.body[1],
            PreHirStmt::Return(Some(PreHirExpr::Var(name))) if name == "uVar1"
        ));
    }

    #[test]
    fn constant_propagation_eliminates_unused_local_constant() {
        let mut func = PreHirFunction {
            name: "test_const_prop".to_string(),
            int_param_offsets: Vec::new(),
            params: vec![],
            locals: vec![PreHirBinding {
                name: "local_c".to_string(),
                ty: int(32),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::StackOffset(12)),
                initializer: None,
            }],
            return_type: int(32),
            surface_return_type_name: None,
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("local_c".to_string()),
                    rhs: PreHirExpr::Const(0, int(32)),
                },
                PreHirStmt::Return(Some(PreHirExpr::Var("local_c".to_string()))),
            ],
            ..Default::default()
        };

        assert!(copy_propagation_pass(&mut func));
        assert_eq!(func.body.len(), 1);
        assert!(matches!(
            &func.body[0],
            PreHirStmt::Return(Some(PreHirExpr::Const(0, _)))
        ));
        assert!(func.locals.is_empty());
    }

    #[test]
    fn constant_propagation_does_not_cross_conditional_definition() {
        let mut func = PreHirFunction {
            name: "test_conditional_constant_definition".to_string(),
            locals: vec![PreHirBinding {
                name: "result".to_string(),
                ty: int(32),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::StackOffset(4)),
                initializer: None,
            }],
            return_type: int(32),
            body: vec![
                PreHirStmt::If {
                    cond: PreHirExpr::Var("condition".to_string()),
                    then_body: vec![PreHirStmt::Assign {
                        lhs: PreHirLValue::Var("result".to_string()),
                        rhs: PreHirExpr::Const(42, int(32)),
                    }]
                    .into(),
                    else_body: Vec::new().into(),
                },
                PreHirStmt::Return(Some(PreHirExpr::Var("result".to_string()))),
            ],
            ..Default::default()
        };
        let before = func.clone();

        assert!(!copy_propagation_pass(&mut func));
        assert_eq!(func, before);
    }

    #[test]
    fn copy_propagation_does_not_cross_conditional_definition() {
        let mut func = PreHirFunction {
            name: "test_conditional_copy_definition".to_string(),
            locals: vec![PreHirBinding {
                name: "uVar0".to_string(),
                ty: int(32),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::Temp),
                initializer: None,
            }],
            return_type: int(32),
            body: vec![
                PreHirStmt::If {
                    cond: PreHirExpr::Var("condition".to_string()),
                    then_body: vec![PreHirStmt::Assign {
                        lhs: PreHirLValue::Var("uVar0".to_string()),
                        rhs: PreHirExpr::Var("source".to_string()),
                    }]
                    .into(),
                    else_body: Vec::new().into(),
                },
                PreHirStmt::Return(Some(PreHirExpr::Var("uVar0".to_string()))),
            ],
            ..Default::default()
        };
        let before = func.clone();

        assert!(!copy_propagation_pass(&mut func));
        assert_eq!(func, before);
    }

    #[test]
    fn constant_propagation_does_not_cross_loop_definition() {
        let mut func = PreHirFunction {
            name: "test_loop_constant_definition".to_string(),
            locals: vec![PreHirBinding {
                name: "result".to_string(),
                ty: int(32),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::StackOffset(4)),
                initializer: None,
            }],
            return_type: int(32),
            body: vec![
                PreHirStmt::While {
                    cond: PreHirExpr::Var("condition".to_string()),
                    body: vec![PreHirStmt::Assign {
                        lhs: PreHirLValue::Var("result".to_string()),
                        rhs: PreHirExpr::Const(42, int(32)),
                    }]
                    .into(),
                },
                PreHirStmt::Return(Some(PreHirExpr::Var("result".to_string()))),
            ],
            ..Default::default()
        };
        let before = func.clone();

        assert!(!copy_propagation_pass(&mut func));
        assert_eq!(func, before);
    }

    #[test]
    fn constant_propagation_does_not_cross_switch_definition() {
        let mut func = PreHirFunction {
            name: "test_switch_constant_definition".to_string(),
            locals: vec![PreHirBinding {
                name: "result".to_string(),
                ty: int(32),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::StackOffset(4)),
                initializer: None,
            }],
            return_type: int(32),
            body: vec![
                PreHirStmt::Switch {
                    expr: PreHirExpr::Var("selector".to_string()),
                    cases: vec![PreHirSwitchCase {
                        values: vec![1],
                        body: vec![PreHirStmt::Assign {
                            lhs: PreHirLValue::Var("result".to_string()),
                            rhs: PreHirExpr::Const(42, int(32)),
                        }]
                        .into(),
                    }],
                    default: Vec::new().into(),
                },
                PreHirStmt::Return(Some(PreHirExpr::Var("result".to_string()))),
            ],
            ..Default::default()
        };
        let before = func.clone();

        assert!(!copy_propagation_pass(&mut func));
        assert_eq!(func, before);
    }
}

fn count_preserved_copyprop_candidates(
    stmts: &[PreHirStmt],
    destinations: &HashSet<String>,
    preserved: &HashSet<&str>,
    def_count: &HashMap<String, usize>,
) -> usize {
    fn visit(
        stmt: &PreHirStmt,
        destinations: &HashSet<String>,
        preserved: &HashSet<&str>,
        def_count: &HashMap<String, usize>,
    ) -> usize {
        match stmt {
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var(name),
                rhs: PreHirExpr::Var(source),
            } if destinations.contains(name)
                && def_count.get(name).copied().unwrap_or(0) == 1
                && name != source
                && (preserved.contains(name.as_str()) || preserved.contains(source.as_str())) =>
            {
                1
            }
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. } => body
                .iter()
                .map(|stmt| visit(stmt, destinations, preserved, def_count))
                .sum(),
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => then_body
                .iter()
                .chain(else_body.iter())
                .map(|stmt| visit(stmt, destinations, preserved, def_count))
                .sum(),
            PreHirStmt::For {
                init, update, body, ..
            } => init
                .iter()
                .chain(update.iter())
                .map(|stmt| visit(stmt, destinations, preserved, def_count))
                .chain(
                    body.iter()
                        .map(|stmt| visit(stmt, destinations, preserved, def_count)),
                )
                .sum(),
            PreHirStmt::Switch { cases, default, .. } => cases
                .iter()
                .flat_map(|case| case.body.iter())
                .chain(default.iter())
                .map(|stmt| visit(stmt, destinations, preserved, def_count))
                .sum(),
            _ => 0,
        }
    }

    stmts
        .iter()
        .map(|stmt| visit(stmt, destinations, preserved, def_count))
        .sum()
}

/// The old propagation phases used a function-wide replacement map.  That map
/// was populated by walking into branches and loops, even though a
/// whole-function definition count does not prove that the definition
/// dominates its uses.  Keep the two replacement kinds separate at the
/// admission point, but share the control-flow-scoped walk below.
enum LinearReplacementKind<'a> {
    Copies {
        destinations: &'a HashSet<String>,
        excluded: &'a HashSet<String>,
    },
    Constants {
        destinations: &'a HashSet<String>,
    },
}

impl LinearReplacementKind<'_> {
    fn candidate(
        &self,
        stmt: &PreHirStmt,
        def_count: &HashMap<String, usize>,
    ) -> Option<(String, PreHirExpr)> {
        match self {
            Self::Copies {
                destinations,
                excluded,
            } => {
                let PreHirStmt::Assign {
                    lhs: PreHirLValue::Var(name),
                    rhs: PreHirExpr::Var(source),
                } = stmt
                else {
                    return None;
                };
                if !destinations.contains(name)
                    || excluded.contains(name)
                    || excluded.contains(source)
                    || name == source
                    || def_count.get(name).copied().unwrap_or(0) != 1
                    || def_count.get(source).copied().unwrap_or(0) > 1
                {
                    return None;
                }
                Some((name.clone(), PreHirExpr::Var(source.clone())))
            }
            Self::Constants { destinations } => {
                let PreHirStmt::Assign {
                    lhs: PreHirLValue::Var(name),
                    rhs: rhs @ PreHirExpr::Const(_, _),
                } = stmt
                else {
                    return None;
                };
                if !destinations.contains(name) || def_count.get(name).copied().unwrap_or(0) != 1 {
                    return None;
                }
                Some((name.clone(), rhs.clone()))
            }
        }
    }
}

/// Apply replacements only inside straight-line statement runs.
///
/// A replacement assignment is removed only when every read of its destination
/// is in the same run, after the definition, and the active replacement is not
/// invalidated before the run ends.  This makes the transform conservative in
/// the presence of residual labels/gotos and structured control flow while
/// retaining the useful straight-line copy/constant cleanup.
fn propagate_linear_replacements(
    stmts: &mut Vec<PreHirStmt>,
    kind: &LinearReplacementKind<'_>,
    def_count: &HashMap<String, usize>,
    global_reads: &HashMap<String, usize>,
    changed: &mut bool,
) {
    let direct_reads = count_direct_linear_reads(stmts);
    let mut seen_names: HashSet<String> = HashSet::default();
    let mut active: HashMap<String, PreHirExpr> = HashMap::default();
    let mut active_indices: HashMap<String, usize> = HashMap::default();
    let mut removable: HashMap<usize, bool> = HashMap::default();

    for index in 0..stmts.len() {
        let names_here = statement_names(&stmts[index]);

        match &mut stmts[index] {
            PreHirStmt::Assign { .. }
            | PreHirStmt::Expr(_)
            | PreHirStmt::VaStart { .. }
            | PreHirStmt::Return(_) => {
                substitute_constants_in_stmt(&mut stmts[index], &active, changed);
                let has_call = stmt_contains_call(&stmts[index]);
                let candidate = kind.candidate(&stmts[index], def_count);

                if let PreHirStmt::Assign { lhs, .. } = &stmts[index] {
                    if let PreHirLValue::Var(destination) = lhs {
                        let destination = destination.clone();
                        let mut writes = HashSet::default();
                        writes.insert(destination.clone());
                        invalidate_active_replacements(
                            &mut active,
                            &mut active_indices,
                            &mut removable,
                            &writes,
                        );

                        if let Some((name, replacement)) = candidate {
                            let can_remove = !seen_names.contains(name.as_str())
                                && global_reads.get(&name).copied().unwrap_or(0)
                                    == direct_reads.get(&name).copied().unwrap_or(0);
                            active_indices.insert(name.clone(), index);
                            active.insert(name, replacement);
                            removable.insert(index, can_remove);
                        }
                    } else {
                        clear_active_replacements(&mut active, &mut active_indices, &mut removable);
                    }
                }

                if has_call {
                    clear_active_replacements(&mut active, &mut active_indices, &mut removable);
                }
                // A return is terminal. Keep the active map long enough for
                // `x = 1; return x` to retire the pure defining assignment;
                // a later label/goto still clears it before any re-entry path.
            }
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                substitute_constants_expr(cond, &active, changed);
                propagate_linear_replacements(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                    kind,
                    def_count,
                    global_reads,
                    changed,
                );
                propagate_linear_replacements(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                    kind,
                    def_count,
                    global_reads,
                    changed,
                );
                clear_active_replacements(&mut active, &mut active_indices, &mut removable);
            }
            PreHirStmt::While { cond, body } => {
                substitute_constants_expr(cond, &active, changed);
                propagate_linear_replacements(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    kind,
                    def_count,
                    global_reads,
                    changed,
                );
                clear_active_replacements(&mut active, &mut active_indices, &mut removable);
            }
            PreHirStmt::DoWhile { body, .. } => {
                propagate_linear_replacements(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    kind,
                    def_count,
                    global_reads,
                    changed,
                );
                clear_active_replacements(&mut active, &mut active_indices, &mut removable);
            }
            PreHirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                if let Some(init) = init {
                    substitute_constants_in_stmt(init, &active, changed);
                    propagate_linear_replacement_slot(init, kind, def_count, global_reads, changed);
                }
                if let Some(cond) = cond {
                    substitute_constants_expr(cond, &active, changed);
                }
                if let Some(update) = update {
                    propagate_linear_replacement_slot(
                        update,
                        kind,
                        def_count,
                        global_reads,
                        changed,
                    );
                }
                propagate_linear_replacements(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    kind,
                    def_count,
                    global_reads,
                    changed,
                );
                clear_active_replacements(&mut active, &mut active_indices, &mut removable);
            }
            PreHirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                substitute_constants_expr(expr, &active, changed);
                for case in cases {
                    propagate_linear_replacements(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                        kind,
                        def_count,
                        global_reads,
                        changed,
                    );
                }
                propagate_linear_replacements(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                    kind,
                    def_count,
                    global_reads,
                    changed,
                );
                clear_active_replacements(&mut active, &mut active_indices, &mut removable);
            }
            PreHirStmt::Block(body) => {
                propagate_linear_replacements(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    kind,
                    def_count,
                    global_reads,
                    changed,
                );
                clear_active_replacements(&mut active, &mut active_indices, &mut removable);
            }
            PreHirStmt::Label(_)
            | PreHirStmt::Goto(_)
            | PreHirStmt::Break
            | PreHirStmt::Continue => {
                clear_active_replacements(&mut active, &mut active_indices, &mut removable);
            }
        }

        // Names observed before the next definition make that later
        // definition non-removable, even if the use is in a nested construct.
        seen_names.extend(names_here);
    }

    let mut remove_indices = HashSet::default();
    for (index, can_remove) in removable {
        if can_remove {
            remove_indices.insert(index);
        }
    }
    if !remove_indices.is_empty() {
        let old = std::mem::take(stmts);
        *stmts = old
            .into_iter()
            .enumerate()
            .filter_map(|(index, stmt)| {
                if remove_indices.contains(&index) {
                    *changed = true;
                    None
                } else {
                    Some(stmt)
                }
            })
            .collect();
    }
}

fn propagate_linear_replacement_slot(
    slot: &mut Box<PreHirStmt>,
    kind: &LinearReplacementKind<'_>,
    def_count: &HashMap<String, usize>,
    global_reads: &HashMap<String, usize>,
    changed: &mut bool,
) {
    let owned = std::mem::replace(slot, Box::new(PreHirStmt::Return(None)));
    let original = (*owned).clone();
    let mut stmts = vec![*owned];
    propagate_linear_replacements(&mut stmts, kind, def_count, global_reads, changed);
    if let Some(stmt) = stmts.pop() {
        *slot = Box::new(stmt);
    } else {
        // A `For` header is part of the loop contract. Keep an elided pure
        // assignment when the generic run walker proved it dead rather than
        // changing the header's shape here.
        *slot = Box::new(original);
    }
}

fn count_direct_linear_reads(stmts: &[PreHirStmt]) -> HashMap<String, usize> {
    let mut counts = HashMap::default();
    for stmt in stmts {
        if is_linear_propagation_stmt(stmt) {
            for (name, count) in DefUseMap::build(std::slice::from_ref(stmt)).use_count {
                *counts.entry(name).or_default() += count;
            }
        }
    }
    counts
}

fn is_linear_propagation_stmt(stmt: &PreHirStmt) -> bool {
    matches!(
        stmt,
        PreHirStmt::Assign { .. }
            | PreHirStmt::Expr(_)
            | PreHirStmt::VaStart { .. }
            | PreHirStmt::Return(_)
    )
}

fn statement_names(stmt: &PreHirStmt) -> HashSet<String> {
    let mut names = HashSet::default();
    let mut borrowed = HashSet::default();
    collect_all_vars_in_stmt(stmt, &mut borrowed);
    names.extend(borrowed.into_iter().map(str::to_owned));
    names
}

fn invalidate_active_replacements(
    active: &mut HashMap<String, PreHirExpr>,
    active_indices: &mut HashMap<String, usize>,
    removable: &mut HashMap<usize, bool>,
    writes: &HashSet<String>,
) {
    let invalidated: Vec<String> = active
        .iter()
        .filter_map(|(destination, replacement)| {
            let mut replacement_names = HashSet::default();
            collect_vars_in_expr(replacement, &mut replacement_names);
            if writes.contains(destination)
                || replacement_names.iter().any(|name| writes.contains(*name))
            {
                Some(destination.clone())
            } else {
                None
            }
        })
        .collect();
    for destination in invalidated {
        if let Some(index) = active_indices.remove(&destination) {
            removable.insert(index, false);
        }
        active.remove(&destination);
    }
}

fn clear_active_replacements(
    active: &mut HashMap<String, PreHirExpr>,
    active_indices: &mut HashMap<String, usize>,
    removable: &mut HashMap<usize, bool>,
) {
    for index in active_indices.values().copied() {
        removable.insert(index, false);
    }
    active.clear();
    active_indices.clear();
}

fn stmt_contains_call(stmt: &PreHirStmt) -> bool {
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => lvalue_contains_call(lhs) || expr_contains_call(rhs),
        PreHirStmt::VaStart { va_list, .. }
        | PreHirStmt::Expr(va_list)
        | PreHirStmt::Return(Some(va_list)) => expr_contains_call(va_list),
        PreHirStmt::Return(None)
        | PreHirStmt::Block(_)
        | PreHirStmt::Switch { .. }
        | PreHirStmt::If { .. }
        | PreHirStmt::While { .. }
        | PreHirStmt::DoWhile { .. }
        | PreHirStmt::For { .. }
        | PreHirStmt::Label(_)
        | PreHirStmt::Goto(_)
        | PreHirStmt::Break
        | PreHirStmt::Continue => false,
    }
}

fn lvalue_contains_call(lhs: &PreHirLValue) -> bool {
    match lhs {
        PreHirLValue::Var(_) => false,
        PreHirLValue::Deref { ptr, .. } => expr_contains_call(ptr),
        PreHirLValue::Index { base, index, .. } => {
            expr_contains_call(base) || expr_contains_call(index)
        }
        PreHirLValue::FieldAccess { base, .. } => expr_contains_call(base),
    }
}

fn expr_contains_call(expr: &PreHirExpr) -> bool {
    match expr {
        PreHirExpr::Call { .. } => true,
        PreHirExpr::Var(_)
        | PreHirExpr::AddressOfGlobal(_)
        | PreHirExpr::AddressOfLocal(_)
        | PreHirExpr::Const(_, _) => false,
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. } => expr_contains_call(expr),
        PreHirExpr::Binary { lhs, rhs, .. } => expr_contains_call(lhs) || expr_contains_call(rhs),
        PreHirExpr::Index { base, index, .. } => {
            expr_contains_call(base) || expr_contains_call(index)
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            expr_contains_call(cond)
                || expr_contains_call(then_expr)
                || expr_contains_call(else_expr)
        }
    }
}

/// Count definition sites (assignments to LHS Var(name)) for each name in
/// `temp_names` across the entire body.
fn count_definitions_in_stmts<'a>(
    stmts: &'a [PreHirStmt],
    temp_names: &HashSet<&str>,
) -> HashMap<&'a str, usize> {
    let mut counts: HashMap<&'a str, usize> = HashMap::default();
    for stmt in stmts {
        count_defs_stmt(stmt, temp_names, &mut counts);
    }
    counts
}

fn count_defs_stmt<'a>(
    stmt: &'a PreHirStmt,
    temps: &HashSet<&str>,
    counts: &mut HashMap<&'a str, usize>,
) {
    match stmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name),
            ..
        } => {
            // Count definitions for ALL variables (not just temps) so we can
            // validate the source variable y.
            *counts.entry(name.as_str()).or_default() += 1;
        }
        PreHirStmt::Block(stmts) => {
            for s in stmts.iter() {
                count_defs_stmt(s, temps, counts);
            }
        }
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            for s in then_body.iter() {
                count_defs_stmt(s, temps, counts);
            }
            for s in else_body.iter() {
                count_defs_stmt(s, temps, counts);
            }
        }
        PreHirStmt::While { body, .. } | PreHirStmt::DoWhile { body, .. } => {
            for s in body.iter() {
                count_defs_stmt(s, temps, counts);
            }
        }
        PreHirStmt::For {
            init, update, body, ..
        } => {
            if let Some(i) = init {
                count_defs_stmt(i, temps, counts);
            }
            if let Some(u) = update {
                count_defs_stmt(u, temps, counts);
            }
            for s in body.iter() {
                count_defs_stmt(s, temps, counts);
            }
        }
        PreHirStmt::Switch { cases, default, .. } => {
            for case in cases {
                for s in case.body.iter() {
                    count_defs_stmt(s, temps, counts);
                }
            }
            for s in default.iter() {
                count_defs_stmt(s, temps, counts);
            }
        }
        _ => {}
    }
}

/// Collect copy assignments `x = Var(y)` where x is a pure temp with exactly
/// one definition.
fn collect_copies<'a>(
    stmts: &'a [PreHirStmt],
    temp_names: &HashSet<&str>,
    def_count: &HashMap<&'a str, usize>,
    copy_map: &mut HashMap<String, String>,
) {
    for stmt in stmts {
        collect_copies_stmt(stmt, temp_names, def_count, copy_map);
    }
}

fn collect_copies_stmt<'a>(
    stmt: &'a PreHirStmt,
    temp_names: &HashSet<&str>,
    def_count: &HashMap<&'a str, usize>,
    copy_map: &mut HashMap<String, String>,
) {
    match stmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name),
            rhs: PreHirExpr::Var(src),
        } if temp_names.contains(name.as_str())
            && def_count.get(name.as_str()).copied().unwrap_or(0) == 1
            && name != src =>
        {
            copy_map.insert(name.clone(), src.clone());
        }
        PreHirStmt::Block(stmts) => collect_copies(stmts, temp_names, def_count, copy_map),
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            collect_copies(then_body, temp_names, def_count, copy_map);
            collect_copies(else_body, temp_names, def_count, copy_map);
        }
        PreHirStmt::While { body, .. } | PreHirStmt::DoWhile { body, .. } => {
            collect_copies(body, temp_names, def_count, copy_map);
        }
        PreHirStmt::For {
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
        PreHirStmt::Switch { cases, default, .. } => {
            for case in cases {
                collect_copies(&case.body, temp_names, def_count, copy_map);
            }
            collect_copies(default, temp_names, def_count, copy_map);
        }
        _ => {}
    }
}

fn collect_predicate_vars_in_stmts<'a>(stmts: &'a [PreHirStmt], out: &mut HashSet<&'a str>) {
    for stmt in stmts {
        collect_predicate_vars_in_stmt(stmt, out);
    }
}

fn collect_predicate_vars_in_stmt<'a>(stmt: &'a PreHirStmt, out: &mut HashSet<&'a str>) {
    match stmt {
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            collect_vars_in_expr(cond, out);
            collect_predicate_vars_in_stmts(then_body, out);
            collect_predicate_vars_in_stmts(else_body, out);
        }
        PreHirStmt::While { cond, body } => {
            collect_vars_in_expr(cond, out);
            collect_predicate_vars_in_stmts(body, out);
        }
        PreHirStmt::DoWhile { body, cond } => {
            collect_predicate_vars_in_stmts(body, out);
            collect_vars_in_expr(cond, out);
        }
        PreHirStmt::For {
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
        PreHirStmt::Switch {
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
        PreHirStmt::Block(stmts) => collect_predicate_vars_in_stmts(stmts, out),
        PreHirStmt::Assign { .. }
        | PreHirStmt::VaStart { .. }
        | PreHirStmt::Expr(_)
        | PreHirStmt::Return(_)
        | PreHirStmt::Break
        | PreHirStmt::Continue
        | PreHirStmt::Label(_)
        | PreHirStmt::Goto(_) => {}
    }
}

fn collect_vars_in_expr<'a>(expr: &'a PreHirExpr, out: &mut HashSet<&'a str>) {
    match expr {
        PreHirExpr::Var(name)
        | PreHirExpr::AddressOfGlobal(name)
        | PreHirExpr::AddressOfLocal(name) => {
            out.insert(name.as_str());
        }
        PreHirExpr::Const(_, _) => {}
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. } => collect_vars_in_expr(expr, out),
        PreHirExpr::Binary { lhs, rhs, .. } => {
            collect_vars_in_expr(lhs, out);
            collect_vars_in_expr(rhs, out);
        }
        PreHirExpr::Call { args, .. } => {
            for arg in args {
                collect_vars_in_expr(arg, out);
            }
        }
        PreHirExpr::Index { base, index, .. } => {
            collect_vars_in_expr(base, out);
            collect_vars_in_expr(index, out);
        }
        PreHirExpr::Select {
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
    stmts: &mut Vec<PreHirStmt>,
    copy_map: &HashMap<String, String>,
    changed: &mut bool,
) {
    for stmt in stmts.iter_mut() {
        remove_copy_assigns_nested(stmt, copy_map, changed);
    }
    stmts.retain(|stmt| {
        if let PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name),
            rhs: PreHirExpr::Var(_),
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
    stmt: &mut PreHirStmt,
    copy_map: &HashMap<String, String>,
    changed: &mut bool,
) {
    match stmt {
        PreHirStmt::Block(stmts) => remove_copy_assigns(
            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(stmts),
            copy_map,
            changed,
        ),
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            remove_copy_assigns(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                copy_map,
                changed,
            );
            remove_copy_assigns(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                copy_map,
                changed,
            );
        }
        PreHirStmt::While { body, .. } | PreHirStmt::DoWhile { body, .. } => {
            remove_copy_assigns(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                copy_map,
                changed,
            );
        }
        PreHirStmt::For {
            init, update, body, ..
        } => {
            if let Some(i) = init {
                remove_copy_assigns_nested(i, copy_map, changed);
            }
            if let Some(u) = update {
                remove_copy_assigns_nested(u, copy_map, changed);
            }
            remove_copy_assigns(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                copy_map,
                changed,
            );
        }
        PreHirStmt::Switch { cases, default, .. } => {
            for case in cases.iter_mut() {
                remove_copy_assigns(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                    copy_map,
                    changed,
                );
            }
            remove_copy_assigns(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                copy_map,
                changed,
            );
        }
        _ => {}
    }
}

/// Substitute every rvalue occurrence of `x` (keys of copy_map) with its
/// source `y` (values of copy_map) throughout all expressions.
/// Follow `x <- y <- z` to `x <- z`, so one substitution pass cannot leave a
/// name behind whose own defining copy is being removed in the same step.
///
/// A cycle (`a <- b`, `b <- a`) has no root; those entries are dropped rather
/// than iterated forever or resolved arbitrarily.
fn resolve_copy_chains(copy_map: &mut HashMap<String, String>) {
    let snapshot: HashMap<String, String> = copy_map.clone();
    let mut cyclic: Vec<String> = Vec::new();
    for (dst, src) in copy_map.iter_mut() {
        let mut seen: HashSet<String> = HashSet::default();
        seen.insert(dst.clone());
        let mut cursor = src.clone();
        loop {
            if !seen.insert(cursor.clone()) {
                cyclic.push(dst.clone());
                break;
            }
            match snapshot.get(&cursor) {
                Some(next) => cursor = next.clone(),
                None => break,
            }
        }
        *src = cursor;
    }
    for dst in cyclic {
        copy_map.remove(&dst);
    }
}

fn substitute_copies_in_stmts(
    stmts: &mut Vec<PreHirStmt>,
    copy_map: &HashMap<String, String>,
    changed: &mut bool,
) {
    for stmt in stmts.iter_mut() {
        substitute_copies_in_stmt(stmt, copy_map, changed);
    }
}

fn substitute_copies_in_stmt(
    stmt: &mut PreHirStmt,
    copy_map: &HashMap<String, String>,
    changed: &mut bool,
) {
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            substitute_copies_lvalue(lhs, copy_map, changed);
            substitute_copies_expr(rhs, copy_map, changed);
        }
        PreHirStmt::VaStart { va_list, .. } => {
            substitute_copies_expr(va_list, copy_map, changed);
        }
        PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => {
            substitute_copies_expr(expr, copy_map, changed);
        }
        PreHirStmt::Return(None)
        | PreHirStmt::Break
        | PreHirStmt::Continue
        | PreHirStmt::Label(_)
        | PreHirStmt::Goto(_) => {}
        PreHirStmt::Block(stmts) => substitute_copies_in_stmts(
            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(stmts),
            copy_map,
            changed,
        ),
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            substitute_copies_expr(cond, copy_map, changed);
            substitute_copies_in_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                copy_map,
                changed,
            );
            substitute_copies_in_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                copy_map,
                changed,
            );
        }
        PreHirStmt::While { cond, body } => {
            substitute_copies_expr(cond, copy_map, changed);
            substitute_copies_in_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                copy_map,
                changed,
            );
        }
        PreHirStmt::DoWhile { body, cond } => {
            substitute_copies_in_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                copy_map,
                changed,
            );
            substitute_copies_expr(cond, copy_map, changed);
        }
        PreHirStmt::For {
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
            substitute_copies_in_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                copy_map,
                changed,
            );
        }
        PreHirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            substitute_copies_expr(expr, copy_map, changed);
            for case in cases.iter_mut() {
                substitute_copies_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                    copy_map,
                    changed,
                );
            }
            substitute_copies_in_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                copy_map,
                changed,
            );
        }
    }
}

fn substitute_copies_lvalue(
    lhs: &mut PreHirLValue,
    copy_map: &HashMap<String, String>,
    changed: &mut bool,
) {
    match lhs {
        PreHirLValue::Var(_) => {}
        PreHirLValue::Deref { ptr, .. } => substitute_copies_expr(ptr, copy_map, changed),
        PreHirLValue::Index { base, index, .. } => {
            substitute_copies_expr(base, copy_map, changed);
            substitute_copies_expr(index, copy_map, changed);
        }
        PreHirLValue::FieldAccess { base, .. } => {
            substitute_copies_expr(base, copy_map, changed);
        }
    }
}

fn substitute_copies_expr(
    expr: &mut PreHirExpr,
    copy_map: &HashMap<String, String>,
    changed: &mut bool,
) {
    match expr {
        PreHirExpr::Var(name)
        | PreHirExpr::AddressOfGlobal(name)
        | PreHirExpr::AddressOfLocal(name) => {
            if let Some(src) = copy_map.get(name.as_str()) {
                *name = src.clone();
                *changed = true;
            }
        }
        PreHirExpr::Const(_, _) => {}
        PreHirExpr::Cast { expr: inner, .. }
        | PreHirExpr::Unary { expr: inner, .. }
        | PreHirExpr::Load { ptr: inner, .. }
        | PreHirExpr::PtrOffset { base: inner, .. }
        | PreHirExpr::AggregateCopy { src: inner, .. }
        | PreHirExpr::FieldAccess { base: inner, .. } => {
            substitute_copies_expr(inner, copy_map, changed);
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            substitute_copies_expr(lhs, copy_map, changed);
            substitute_copies_expr(rhs, copy_map, changed);
        }
        PreHirExpr::Call { args, .. } => {
            for a in args.iter_mut() {
                substitute_copies_expr(a, copy_map, changed);
            }
        }
        PreHirExpr::Index { base, index, .. } => {
            substitute_copies_expr(base, copy_map, changed);
            substitute_copies_expr(index, copy_map, changed);
        }
        PreHirExpr::Select {
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
pub fn join_coalescing_pass(func: &mut PreHirFunction) -> bool {
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
    let mut rename_map: HashMap<String, String> = HashMap::default();
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
    stmts: &[PreHirStmt],
    temp_names: &HashSet<String>,
    map: &DefUseMap,
    rename_map: &mut HashMap<String, String>,
) {
    for (idx, stmt) in stmts.iter().enumerate() {
        match stmt {
            PreHirStmt::If {
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
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_join_renames(then_body, temp_names, map, rename_map);
                collect_join_renames(else_body, temp_names, map, rename_map);
            }
            PreHirStmt::While { body, .. } | PreHirStmt::DoWhile { body, .. } => {
                collect_join_renames(body, temp_names, map, rename_map);
            }
            PreHirStmt::For { init: _, body, .. } => {
                collect_join_renames(body, temp_names, map, rename_map);
            }
            PreHirStmt::Switch { cases, default, .. } => {
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
fn last_assigns(stmts: &[PreHirStmt], temp_names: &HashSet<String>) -> Vec<(String, String)> {
    let mut seen: HashMap<String, String> = HashMap::default();
    for stmt in stmts {
        if let PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name),
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
fn type_repr(expr: &PreHirExpr) -> String {
    match expr {
        PreHirExpr::Const(_, ty) | PreHirExpr::Cast { ty, .. } => format!("{ty:?}"),
        PreHirExpr::Var(_) | PreHirExpr::AddressOfGlobal(_) | PreHirExpr::AddressOfLocal(_) => {
            "var".to_string()
        }
        PreHirExpr::Binary { ty, .. } | PreHirExpr::Unary { ty, .. } => format!("{ty:?}"),
        PreHirExpr::Load { ty, .. } => format!("load_{ty:?}"),
        _ => "other".to_string(),
    }
}

/// Count uses of `name` in stmts[start_idx..].
fn count_uses_after(stmts: &[PreHirStmt], start_idx: usize, name: &str) -> usize {
    stmts[start_idx.min(stmts.len())..]
        .iter()
        .map(|s| count_uses_in_stmt_flat(s, name))
        .sum()
}

fn count_uses_in_stmt_flat(stmt: &PreHirStmt, name: &str) -> usize {
    match stmt {
        PreHirStmt::Assign { lhs: _, rhs } => count_var_in_expr(rhs, name),
        PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => count_var_in_expr(expr, name),
        PreHirStmt::If {
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
        PreHirStmt::While { cond, body } | PreHirStmt::DoWhile { body, cond } => {
            count_var_in_expr(cond, name)
                + body
                    .iter()
                    .map(|s| count_uses_in_stmt_flat(s, name))
                    .sum::<usize>()
        }
        PreHirStmt::For {
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
        PreHirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            count_var_in_expr(expr, name)
                + cases
                    .iter()
                    .flat_map(|c| c.body.iter())
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

fn count_var_in_expr(expr: &PreHirExpr, name: &str) -> usize {
    match expr {
        PreHirExpr::Var(n) | PreHirExpr::AddressOfGlobal(n) | PreHirExpr::AddressOfLocal(n) => {
            usize::from(n.as_str() == name)
        }
        PreHirExpr::Const(_, _) => 0,
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. } => count_var_in_expr(expr, name),
        PreHirExpr::Binary { lhs, rhs, .. } => {
            count_var_in_expr(lhs, name) + count_var_in_expr(rhs, name)
        }
        PreHirExpr::Call { args, .. } => args.iter().map(|a| count_var_in_expr(a, name)).sum(),
        PreHirExpr::Index { base, index, .. } => {
            count_var_in_expr(base, name) + count_var_in_expr(index, name)
        }
        PreHirExpr::Select {
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
    stmts: &mut Vec<PreHirStmt>,
    rename_map: &HashMap<String, String>,
    changed: &mut bool,
) {
    for stmt in stmts.iter_mut() {
        apply_join_renames_stmt(stmt, rename_map, changed);
    }
}

fn apply_join_renames_stmt(
    stmt: &mut PreHirStmt,
    rename_map: &HashMap<String, String>,
    changed: &mut bool,
) {
    match stmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name),
            rhs,
        } => {
            if let Some(canonical) = rename_map.get(name.as_str()) {
                *name = canonical.clone();
                *changed = true;
            }
            apply_join_renames_expr(rhs, rename_map, changed);
        }
        PreHirStmt::Assign { lhs, rhs } => {
            apply_join_renames_expr(rhs, rename_map, changed);
            // Also rename inside index/deref lvalues.
            apply_join_renames_lvalue(lhs, rename_map, changed);
        }
        PreHirStmt::VaStart { va_list, .. } => {
            apply_join_renames_expr(va_list, rename_map, changed);
        }
        PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => {
            apply_join_renames_expr(expr, rename_map, changed);
        }
        PreHirStmt::Return(None)
        | PreHirStmt::Break
        | PreHirStmt::Continue
        | PreHirStmt::Label(_)
        | PreHirStmt::Goto(_) => {}
        PreHirStmt::Block(stmts) => apply_join_renames(
            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(stmts),
            rename_map,
            changed,
        ),
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            apply_join_renames_expr(cond, rename_map, changed);
            apply_join_renames(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                rename_map,
                changed,
            );
            apply_join_renames(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                rename_map,
                changed,
            );
        }
        PreHirStmt::While { cond, body } => {
            apply_join_renames_expr(cond, rename_map, changed);
            apply_join_renames(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                rename_map,
                changed,
            );
        }
        PreHirStmt::DoWhile { body, cond } => {
            apply_join_renames(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                rename_map,
                changed,
            );
            apply_join_renames_expr(cond, rename_map, changed);
        }
        PreHirStmt::For {
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
            apply_join_renames(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                rename_map,
                changed,
            );
        }
        PreHirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            apply_join_renames_expr(expr, rename_map, changed);
            for case in cases.iter_mut() {
                apply_join_renames(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                    rename_map,
                    changed,
                );
            }
            apply_join_renames(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                rename_map,
                changed,
            );
        }
    }
}

fn apply_join_renames_lvalue(
    lhs: &mut PreHirLValue,
    rename_map: &HashMap<String, String>,
    changed: &mut bool,
) {
    match lhs {
        PreHirLValue::Var(_) => {}
        PreHirLValue::Deref { ptr, .. } => apply_join_renames_expr(ptr, rename_map, changed),
        PreHirLValue::Index { base, index, .. } => {
            apply_join_renames_expr(base, rename_map, changed);
            apply_join_renames_expr(index, rename_map, changed);
        }
        PreHirLValue::FieldAccess { base, .. } => {
            apply_join_renames_expr(base, rename_map, changed);
        }
    }
}

fn apply_join_renames_expr(
    expr: &mut PreHirExpr,
    rename_map: &HashMap<String, String>,
    changed: &mut bool,
) {
    match expr {
        PreHirExpr::Var(name)
        | PreHirExpr::AddressOfGlobal(name)
        | PreHirExpr::AddressOfLocal(name) => {
            if let Some(canonical) = rename_map.get(name.as_str()) {
                *name = canonical.clone();
                *changed = true;
            }
        }
        PreHirExpr::Const(_, _) => {}
        PreHirExpr::Cast { expr: inner, .. }
        | PreHirExpr::Unary { expr: inner, .. }
        | PreHirExpr::Load { ptr: inner, .. }
        | PreHirExpr::PtrOffset { base: inner, .. }
        | PreHirExpr::AggregateCopy { src: inner, .. }
        | PreHirExpr::FieldAccess { base: inner, .. } => {
            apply_join_renames_expr(inner, rename_map, changed);
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            apply_join_renames_expr(lhs, rename_map, changed);
            apply_join_renames_expr(rhs, rename_map, changed);
        }
        PreHirExpr::Call { args, .. } => {
            for a in args.iter_mut() {
                apply_join_renames_expr(a, rename_map, changed);
            }
        }
        PreHirExpr::Index { base, index, .. } => {
            apply_join_renames_expr(base, rename_map, changed);
            apply_join_renames_expr(index, rename_map, changed);
        }
        PreHirExpr::Select {
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
    stmts: &'a [PreHirStmt],
    eligible_vars: &HashSet<&str>,
    def_count: &HashMap<&'a str, usize>,
    const_map: &mut HashMap<String, PreHirExpr>,
) {
    for stmt in stmts {
        collect_constants_stmt(stmt, eligible_vars, def_count, const_map);
    }
}

fn collect_constants_stmt<'a>(
    stmt: &'a PreHirStmt,
    eligible_vars: &HashSet<&str>,
    def_count: &HashMap<&'a str, usize>,
    const_map: &mut HashMap<String, PreHirExpr>,
) {
    match stmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name),
            rhs: const_expr @ PreHirExpr::Const(_, _),
        } if eligible_vars.contains(name.as_str())
            && def_count.get(name.as_str()).copied().unwrap_or(0) == 1 =>
        {
            const_map.insert(name.clone(), const_expr.clone());
        }
        PreHirStmt::Block(stmts) => collect_constants(stmts, eligible_vars, def_count, const_map),
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            collect_constants(then_body, eligible_vars, def_count, const_map);
            collect_constants(else_body, eligible_vars, def_count, const_map);
        }
        PreHirStmt::While { body, .. } | PreHirStmt::DoWhile { body, .. } => {
            collect_constants(body, eligible_vars, def_count, const_map);
        }
        PreHirStmt::For {
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
        PreHirStmt::Switch { cases, default, .. } => {
            for case in cases {
                collect_constants(&case.body, eligible_vars, def_count, const_map);
            }
            collect_constants(default, eligible_vars, def_count, const_map);
        }
        _ => {}
    }
}

fn remove_constant_assigns(
    stmts: &mut Vec<PreHirStmt>,
    const_map: &HashMap<String, PreHirExpr>,
    changed: &mut bool,
) {
    for stmt in stmts.iter_mut() {
        remove_constant_assigns_nested(stmt, const_map, changed);
    }
    stmts.retain(|stmt| {
        if let PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name),
            rhs: PreHirExpr::Const(_, _),
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
    stmt: &mut PreHirStmt,
    const_map: &HashMap<String, PreHirExpr>,
    changed: &mut bool,
) {
    match stmt {
        PreHirStmt::Block(stmts) => remove_constant_assigns(
            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(stmts),
            const_map,
            changed,
        ),
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            remove_constant_assigns(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                const_map,
                changed,
            );
            remove_constant_assigns(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                const_map,
                changed,
            );
        }
        PreHirStmt::While { body, .. } | PreHirStmt::DoWhile { body, .. } => {
            remove_constant_assigns(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                const_map,
                changed,
            );
        }
        PreHirStmt::For {
            init, update, body, ..
        } => {
            if let Some(i) = init {
                remove_constant_assigns_nested(i, const_map, changed);
            }
            if let Some(u) = update {
                remove_constant_assigns_nested(u, const_map, changed);
            }
            remove_constant_assigns(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                const_map,
                changed,
            );
        }
        PreHirStmt::Switch { cases, default, .. } => {
            for case in cases.iter_mut() {
                remove_constant_assigns(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                    const_map,
                    changed,
                );
            }
            remove_constant_assigns(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                const_map,
                changed,
            );
        }
        _ => {}
    }
}

fn substitute_constants_in_stmts(
    stmts: &mut Vec<PreHirStmt>,
    const_map: &HashMap<String, PreHirExpr>,
    changed: &mut bool,
) {
    for stmt in stmts.iter_mut() {
        substitute_constants_in_stmt(stmt, const_map, changed);
    }
}

fn substitute_constants_in_stmt(
    stmt: &mut PreHirStmt,
    const_map: &HashMap<String, PreHirExpr>,
    changed: &mut bool,
) {
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            substitute_constants_lvalue(lhs, const_map, changed);
            substitute_constants_expr(rhs, const_map, changed);
        }
        PreHirStmt::VaStart { va_list, .. } => {
            substitute_constants_expr(va_list, const_map, changed);
        }
        PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => {
            substitute_constants_expr(expr, const_map, changed);
        }
        _ => {}
    }
    match stmt {
        PreHirStmt::Block(stmts) => substitute_constants_in_stmts(
            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(stmts),
            const_map,
            changed,
        ),
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            substitute_constants_expr(cond, const_map, changed);
            substitute_constants_in_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                const_map,
                changed,
            );
            substitute_constants_in_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                const_map,
                changed,
            );
        }
        PreHirStmt::While { cond, body } => {
            substitute_constants_expr(cond, const_map, changed);
            substitute_constants_in_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                const_map,
                changed,
            );
        }
        PreHirStmt::DoWhile { body, cond } => {
            substitute_constants_in_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                const_map,
                changed,
            );
            substitute_constants_expr(cond, const_map, changed);
        }
        PreHirStmt::For {
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
            substitute_constants_in_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                const_map,
                changed,
            );
        }
        PreHirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            substitute_constants_expr(expr, const_map, changed);
            for case in cases.iter_mut() {
                substitute_constants_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                    const_map,
                    changed,
                );
            }
            substitute_constants_in_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                const_map,
                changed,
            );
        }
        _ => {}
    }
}

fn substitute_constants_lvalue(
    lhs: &mut PreHirLValue,
    const_map: &HashMap<String, PreHirExpr>,
    changed: &mut bool,
) {
    match lhs {
        PreHirLValue::Var(_) => {}
        PreHirLValue::Deref { ptr, .. } => substitute_constants_expr(ptr, const_map, changed),
        PreHirLValue::Index { base, index, .. } => {
            substitute_constants_expr(base, const_map, changed);
            substitute_constants_expr(index, const_map, changed);
        }
        PreHirLValue::FieldAccess { base, .. } => {
            substitute_constants_expr(base, const_map, changed);
        }
    }
}

fn substitute_constants_expr(
    expr: &mut PreHirExpr,
    const_map: &HashMap<String, PreHirExpr>,
    changed: &mut bool,
) {
    if let PreHirExpr::Var(name) = expr {
        if let Some(c) = const_map.get(name.as_str()) {
            *expr = c.clone();
            *changed = true;
            return;
        }
    }
    match expr {
        PreHirExpr::Cast { expr: inner, .. }
        | PreHirExpr::Unary { expr: inner, .. }
        | PreHirExpr::Load { ptr: inner, .. }
        | PreHirExpr::PtrOffset { base: inner, .. }
        | PreHirExpr::AggregateCopy { src: inner, .. }
        | PreHirExpr::FieldAccess { base: inner, .. } => {
            substitute_constants_expr(inner, const_map, changed);
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            substitute_constants_expr(lhs, const_map, changed);
            substitute_constants_expr(rhs, const_map, changed);
        }
        PreHirExpr::Call { args, .. } => {
            for a in args.iter_mut() {
                substitute_constants_expr(a, const_map, changed);
            }
        }
        PreHirExpr::Index { base, index, .. } => {
            substitute_constants_expr(base, const_map, changed);
            substitute_constants_expr(index, const_map, changed);
        }
        PreHirExpr::Select {
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

fn collect_loop_preservation_vars(stmts: &[PreHirStmt]) -> HashSet<String> {
    let mut defined_outside: HashSet<String> = HashSet::default();
    let mut used_inside: HashSet<String> = HashSet::default();
    collect_defs_outside_loops(stmts, &mut defined_outside);
    collect_uses_inside_loops(stmts, &mut used_inside);
    defined_outside
        .intersection(&used_inside)
        .cloned()
        .collect()
}

fn collect_defs_outside_loops(stmts: &[PreHirStmt], out: &mut HashSet<String>) {
    for stmt in stmts {
        match stmt {
            PreHirStmt::Assign { lhs, .. } => {
                if let PreHirLValue::Var(name) = lhs {
                    out.insert(name.clone());
                }
            }
            PreHirStmt::Block(body) => collect_defs_outside_loops(body, out),
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_defs_outside_loops(then_body, out);
                collect_defs_outside_loops(else_body, out);
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_defs_outside_loops(&case.body, out);
                }
                collect_defs_outside_loops(default, out);
            }
            _ => {}
        }
    }
}

fn collect_uses_inside_loops(stmts: &[PreHirStmt], out: &mut HashSet<String>) {
    // Collect via a local &str set, then promote to String at the boundary so
    // we can reuse the shared helpers (collect_vars_in_expr, etc.) without
    // changing their signatures.
    fn inner<'a>(stmts: &'a [PreHirStmt], inner_out: &mut HashSet<&'a str>) {
        for stmt in stmts {
            match stmt {
                PreHirStmt::While { cond, body } => {
                    collect_vars_in_expr(cond, inner_out);
                    collect_all_vars_in_stmts(body, inner_out);
                }
                PreHirStmt::DoWhile { body, cond } => {
                    collect_all_vars_in_stmts(body, inner_out);
                    collect_vars_in_expr(cond, inner_out);
                }
                PreHirStmt::For {
                    init,
                    cond,
                    update,
                    body,
                } => {
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
                PreHirStmt::Block(body) => inner(body, inner_out),
                PreHirStmt::If {
                    then_body,
                    else_body,
                    ..
                } => {
                    inner(then_body, inner_out);
                    inner(else_body, inner_out);
                }
                PreHirStmt::Switch { cases, default, .. } => {
                    for case in cases {
                        inner(&case.body, inner_out);
                    }
                    inner(default, inner_out);
                }
                _ => {}
            }
        }
    }
    let mut local: HashSet<&str> = HashSet::default();
    inner(stmts, &mut local);
    out.extend(local.into_iter().map(str::to_owned));
}

fn collect_all_vars_in_stmts<'a>(stmts: &'a [PreHirStmt], out: &mut HashSet<&'a str>) {
    for stmt in stmts {
        collect_all_vars_in_stmt(stmt, out);
    }
}

fn collect_all_vars_in_stmt<'a>(stmt: &'a PreHirStmt, out: &mut HashSet<&'a str>) {
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            collect_vars_in_lvalue(lhs, out);
            collect_vars_in_expr(rhs, out);
        }
        PreHirStmt::Expr(expr)
        | PreHirStmt::Return(Some(expr))
        | PreHirStmt::VaStart { va_list: expr, .. } => {
            collect_vars_in_expr(expr, out);
        }
        PreHirStmt::Block(body)
        | PreHirStmt::While { body, .. }
        | PreHirStmt::DoWhile { body, .. } => collect_all_vars_in_stmts(body, out),
        PreHirStmt::If {
            then_body,
            else_body,
            cond,
        } => {
            collect_vars_in_expr(cond, out);
            collect_all_vars_in_stmts(then_body, out);
            collect_all_vars_in_stmts(else_body, out);
        }
        PreHirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
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
        PreHirStmt::Switch {
            cases,
            default,
            expr,
        } => {
            collect_vars_in_expr(expr, out);
            for case in cases {
                collect_all_vars_in_stmts(&case.body, out);
            }
            collect_all_vars_in_stmts(default, out);
        }
        _ => {}
    }
}

fn collect_vars_in_lvalue<'a>(lhs: &'a PreHirLValue, out: &mut HashSet<&'a str>) {
    match lhs {
        PreHirLValue::Var(name) => {
            out.insert(name.as_str());
        }
        PreHirLValue::Deref { ptr, .. } => collect_vars_in_expr(ptr, out),
        PreHirLValue::Index { base, index, .. } => {
            collect_vars_in_expr(base, out);
            collect_vars_in_expr(index, out);
        }
        PreHirLValue::FieldAccess { base, .. } => collect_vars_in_expr(base, out),
    }
}
