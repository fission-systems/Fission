use super::super::utils::{
    expr_has_side_effects, is_dead_local_clobber_name, is_trivial_temp_name,
};
use crate::HashSet;
use crate::analysis::defuse::DefUseMap;
use crate::analysis::preservation::should_keep_unused_temp_binding;
use crate::prelude::*;

pub fn prune_unused_temp_bindings(func: &mut PreHirFunction) -> bool {
    // Built once for the whole function: this used to call
    // `count_uses_in_stmt_list` (a fresh full-body walk) per local binding,
    // making this O(locals * body size) on every one of the ~30 pipeline
    // stages that call this pass. A single whole-body use-count map makes
    // each binding's check O(1) instead.
    let use_map = DefUseMap::build(&func.body);
    // Same reason, one predicate lower: `stmt_list_assigns_var_from_side_
    // effecting_expr` walked the whole body again for every binding.
    let mut side_effect_assigned = std::collections::HashSet::new();
    collect_names_assigned_from_side_effecting_exprs(&func.body, &mut side_effect_assigned);
    let mut changed = false;
    func.locals.retain(|binding| {
        let used = use_map
            .use_count
            .get(binding.name.as_str())
            .copied()
            .unwrap_or(0)
            > 0;
        let written = use_map
            .def_count
            .get(binding.name.as_str())
            .copied()
            .unwrap_or(0)
            > 0;
        let initializer_has_side_effects = binding
            .initializer
            .as_ref()
            .is_some_and(expr_has_side_effects);
        // A binding the body neither reads nor writes has no effect the
        // emitted C can express, so its declaration is noise. The rule below
        // cannot reach these: it turns on `use_count` alone -- reads only --
        // and so must leave write-only stack homes declared for the rescue
        // pass to find. A home that is written appears in `def_count`, so
        // this cannot collide with them.
        //
        // **Stack-derived bindings are excluded, and that exclusion is the
        // whole subtlety.** This pass runs at some thirty pipeline stages,
        // including ones before slot addresses have been rewritten into slot
        // names. A stack local referenced only as `Load { ptr: <slot addr> }`
        // appears nowhere by name and is still live; pruning it there deletes
        // the binding a later stage was going to rewrite into. Nothing in the
        // emitted text breaks -- the name was absent either way -- so a
        // corpus sweep cannot see it. `preview_type_hints_apply_stack_local_
        // type_to_surfaced_slot_alias` can, and did.
        //
        // What is left reachable: temporaries and scaffolding, which are
        // name-referenced by construction. Measured on the 250-function
        // sample-set, 224 of 6,774 declared locals appear nowhere at all;
        // 123 of those are stack slots this deliberately does not touch.
        let address_referable = binding.origin.is_some_and(|origin| {
            matches!(
                origin,
                NirBindingOrigin::StackOffset(_)
                    | NirBindingOrigin::HomeSlot(_)
                    | NirBindingOrigin::OutgoingArgSlot(_)
                    | NirBindingOrigin::DerivedFromStackOffset(_)
                    | NirBindingOrigin::VaRegion
            )
        });
        if !used && !written && !initializer_has_side_effects && !address_referable {
            changed = true;
            return false;
        }
        let assigned_side_effect = side_effect_assigned.contains(binding.name.as_str());
        let keep = should_keep_unused_temp_binding(
            is_prunable_unused_temp_binding(binding),
            used || assigned_side_effect,
            initializer_has_side_effects,
        );
        changed |= !keep;
        keep
    });
    changed
}

fn is_prunable_unused_temp_binding(binding: &PreHirBinding) -> bool {
    is_trivial_temp_name(&binding.name) || binding.is_temp_like()
}

/// Every name assigned from a side-effecting expression anywhere in `stmts`.
///
/// Mirrors `stmt_assigns_var_from_side_effecting_expr`'s traversal exactly,
/// collecting instead of answering about one name. The predicate walked the
/// whole body once *per binding*, which is the same O(locals * body) shape the
/// `DefUseMap` in `prune_unused_temp_bindings` was introduced to remove -- it
/// was simply left behind one predicate lower.
fn collect_names_assigned_from_side_effecting_exprs<'a>(
    stmts: &'a [PreHirStmt],
    out: &mut std::collections::HashSet<&'a str>,
) {
    for stmt in stmts {
        match stmt {
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var(lhs_name),
                rhs,
            } => {
                if expr_has_side_effects(rhs) {
                    out.insert(lhs_name.as_str());
                }
            }
            PreHirStmt::Block(stmts)
            | PreHirStmt::While { body: stmts, .. }
            | PreHirStmt::DoWhile { body: stmts, .. } => {
                collect_names_assigned_from_side_effecting_exprs(stmts, out);
            }
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_names_assigned_from_side_effecting_exprs(then_body, out);
                collect_names_assigned_from_side_effecting_exprs(else_body, out);
            }
            PreHirStmt::For {
                init, update, body, ..
            } => {
                if let Some(init) = init.as_deref() {
                    collect_names_assigned_from_side_effecting_exprs(
                        std::slice::from_ref(init),
                        out,
                    );
                }
                if let Some(update) = update.as_deref() {
                    collect_names_assigned_from_side_effecting_exprs(
                        std::slice::from_ref(update),
                        out,
                    );
                }
                collect_names_assigned_from_side_effecting_exprs(body, out);
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_names_assigned_from_side_effecting_exprs(&case.body, out);
                }
                collect_names_assigned_from_side_effecting_exprs(default, out);
            }
            PreHirStmt::Assign { .. }
            | PreHirStmt::VaStart { .. }
            | PreHirStmt::Expr(_)
            | PreHirStmt::Label(_)
            | PreHirStmt::Goto(_)
            | PreHirStmt::Return(_)
            | PreHirStmt::Break
            | PreHirStmt::Continue => {}
        }
    }
}

fn stmt_list_assigns_var_from_side_effecting_expr(stmts: &[PreHirStmt], name: &str) -> bool {
    stmts
        .iter()
        .any(|stmt| stmt_assigns_var_from_side_effecting_expr(stmt, name))
}

fn stmt_assigns_var_from_side_effecting_expr(stmt: &PreHirStmt, name: &str) -> bool {
    match stmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(lhs_name),
            rhs,
        } => lhs_name == name && expr_has_side_effects(rhs),
        PreHirStmt::Block(stmts)
        | PreHirStmt::While { body: stmts, .. }
        | PreHirStmt::DoWhile { body: stmts, .. } => {
            stmt_list_assigns_var_from_side_effecting_expr(stmts, name)
        }
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            stmt_list_assigns_var_from_side_effecting_expr(then_body, name)
                || stmt_list_assigns_var_from_side_effecting_expr(else_body, name)
        }
        PreHirStmt::For {
            init, update, body, ..
        } => {
            init.as_deref()
                .is_some_and(|stmt| stmt_assigns_var_from_side_effecting_expr(stmt, name))
                || update
                    .as_deref()
                    .is_some_and(|stmt| stmt_assigns_var_from_side_effecting_expr(stmt, name))
                || stmt_list_assigns_var_from_side_effecting_expr(body, name)
        }
        PreHirStmt::Switch { cases, default, .. } => {
            cases
                .iter()
                .any(|case| stmt_list_assigns_var_from_side_effecting_expr(&case.body, name))
                || stmt_list_assigns_var_from_side_effecting_expr(default, name)
        }
        PreHirStmt::Assign { .. }
        | PreHirStmt::VaStart { .. }
        | PreHirStmt::Expr(_)
        | PreHirStmt::Label(_)
        | PreHirStmt::Goto(_)
        | PreHirStmt::Return(_)
        | PreHirStmt::Break
        | PreHirStmt::Continue => false,
    }
}

pub fn prune_unused_dead_local_bindings(func: &mut PreHirFunction) -> bool {
    let param_names = func
        .params
        .iter()
        .map(|binding| binding.name.as_str())
        .collect::<HashSet<_>>();
    // Built once for the whole function rather than per-binding: same
    // rationale as `prune_unused_temp_bindings` above.
    let use_map = DefUseMap::build(&func.body);
    let mut changed = false;
    func.locals.retain(|binding| {
        // LHS assigns do not count as "uses" in `use_map`, so a
        // write-only stack home (`local_18 = param_3`) would previously drop its
        // binding while leaving the assign — undeclared identifier / compile_error
        // (`matrix_multiply`-class). Keep the binding whenever the name is still
        // defined in the body.
        let keep = !is_dead_local_clobber_name(&binding.name)
            || param_names.contains(binding.name.as_str())
            || binding.name.starts_with("slot_")
            || matches!(binding.ty, NirType::Aggregate { .. })
            || use_map
                .use_count
                .get(binding.name.as_str())
                .copied()
                .unwrap_or(0)
                > 0
            || binding
                .initializer
                .as_ref()
                .is_some_and(expr_has_side_effects);
        changed |= !keep;
        keep
    });
    changed
}

/// True when `name` appears as a plain variable assignment target anywhere in
/// `stmts` (definition site, not a use).
fn stmt_list_defines_var(stmts: &[PreHirStmt], name: &str) -> bool {
    stmts.iter().any(|stmt| stmt_defines_var(stmt, name))
}

pub(super) fn stmt_defines_var(stmt: &PreHirStmt, name: &str) -> bool {
    match stmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(lhs_name),
            ..
        } => lhs_name == name,
        PreHirStmt::Block(body)
        | PreHirStmt::While { body, .. }
        | PreHirStmt::DoWhile { body, .. } => stmt_list_defines_var(body, name),
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => stmt_list_defines_var(then_body, name) || stmt_list_defines_var(else_body, name),
        PreHirStmt::For {
            init, update, body, ..
        } => {
            init.as_ref().is_some_and(|s| stmt_defines_var(s, name))
                || update.as_ref().is_some_and(|s| stmt_defines_var(s, name))
                || stmt_list_defines_var(body, name)
        }
        PreHirStmt::Switch { cases, default, .. } => {
            cases.iter().any(|c| stmt_list_defines_var(&c.body, name))
                || stmt_list_defines_var(default, name)
        }
        _ => false,
    }
}
