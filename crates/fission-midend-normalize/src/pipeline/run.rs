use super::super::analysis::defuse::{
    apply_wide_dead_assignment_pass, constant_folding_pass, defuse_dead_assignment_pass,
    stabilize_repeated_pure_exprs,
};
use super::super::analysis::preservation::preserved_materialization_names;
use super::super::apply_rule_normalization;
use super::super::arith::{
    apply_conditional_move_pass, apply_double_precision_reconstruction_pass, apply_float_sign_pass,
    apply_ignore_nan_pass, apply_or_compare_pass, apply_subfloat_flow_pass,
    apply_three_way_compare_pass, canonicalize_condition_expr, canonicalize_flag_intrinsics,
    canonicalize_integer_expr, cleanup_arithmetic_wrappers, collapse_zero_offset_cast,
    merge_consecutive_shifts, normalize_boolean_logic, recognize_compiler_runtime_division,
    recognize_concat_zext_or, recognize_dumpty_hump_cast, recognize_dumpty_hump_late,
    recognize_hi_lo_extract, recognize_humpty_dumpty_or, recognize_magic_number_division,
    recognize_mod_div_power_of_two, recognize_wide_integer_recombine, simplify_collect_mul_terms,
    simplify_distribute_common_factor, simplify_double_add, simplify_factor_common_mul,
    simplify_negated_const, simplify_nested_adds_subs, simplify_subpiece_chain,
};
use super::super::cleanup::single_pred_label_inline;
use super::super::cleanup::{
    apply_condexe_folding_pass, apply_deindirect_pass, apply_expand_load_pass,
    apply_iblock_phi_elimination, apply_subvar_trim_pass, collapse_loop_exit_alias_returns,
    prune_unreachable_after_terminal, recover_guarded_loop_tail_accumulator_returns,
};
use super::super::cleanup::{
    apply_switch_norm_pass, canonicalize_minmax_conditional_returns, cast_elision_pass,
    cleanup_redundant_boundary_labels, collapse_adjacent_pure_copy_into_if,
    collapse_common_exit_guard_chain, collapse_redundant_conditional_returns,
    collapse_temp_self_square_assigns, collapse_trivial_assign_returns,
    collapse_trivial_pointer_alias_bindings, conditional_select_pass,
    elide_unused_popcount_assigns, eliminate_dead_local_clobber_assigns,
    eliminate_dead_temp_assigns, eliminate_redundant_var_assigns,
    fuse_single_predecessor_boundaries, inline_loop_condition_trailing_temps,
    inline_single_use_temps, normalize_dowhile_decrement_condition,
    promote_guarded_jump_target_tail, prune_unused_dead_local_bindings, prune_unused_temp_bindings,
    remove_unreferenced_leading_labels, rescue_undeclared_bindings,
    simplify_empty_and_constant_ifs, simplify_empty_and_constant_ifs_recursive,
    simplify_fallthrough_edges, strip_redundant_assign_casts,
};
use super::super::global_opt::{
    apply_bit_consume_dead_code_pass, apply_conditional_const_pass, apply_cse_pass,
    apply_dead_store_elimination, apply_gvn_join_hoist_pass, apply_licm_pass,
    apply_nz_mask_simplification_pass, apply_post_assign_value_representative_pass,
    apply_redundant_load_elimination, apply_sccp_pass,
};
use super::super::idioms::{
    apply_branch_prefix_hoist_pass, apply_split_flow_pass, apply_subflow_pruning,
    remove_callee_save_prologue_epilogue, remove_dead_callee_saved_param_loads,
    remove_entry_stack_scaffold_stores,
};
use super::super::memory::{
    apply_aggregate_alias_access_rewrite_pass, apply_aggregate_fields_pass,
    apply_constant_ptr_recovery_pass, apply_memory_heritage, apply_memory_slot_surfacing,
    apply_memory_slot_surfacing_cheap, apply_ptr_arith_recovery_pass, apply_split_datatype_pass,
    apply_union_resolve_pass, apply_zero_index_deref_pass, normalize_binding_initializers,
};
use super::super::recovery::{
    apply_break_continue_pass, apply_flag_recovery_pass, apply_for_loop_folding,
    apply_iv_recovery_pass, apply_variable_merge_pass, copy_propagation_pass, join_coalescing_pass,
};
use super::super::subvar_flow::apply_subvar_flow_pass;
use super::super::types::{
    apply_callsite_type_prop_pass, apply_entry_param_promotion_pass,
    apply_interproc_callsite_arity_pass, apply_type_constraint_propagation,
    apply_type_inference_pass, apply_use_driven_type_infer_pass, apply_variadic_stack_region_pass,
};
use crate::prelude::*;
use fission_midend_core::action_pipeline::{
    EARLY_CLEANUP_BLOCK_BLOCK_LIMIT, EARLY_CLEANUP_BLOCK_STMT_LIMIT, PassBudget,
    TYPE_SIGNATURE_FIXED_POINT_MAX_ROUNDS,
};
use fission_midend_core::vsa::{apply_jump_resolver_pass, jump_resolver_candidate_count};
use fission_midend_core::wave_stats;
use std::time::Instant;
use tracing::{debug, debug_span};

pub fn apply_type_signature_fixed_point(func: &mut HirFunction, diag: bool, perf: bool) {
    let mut interproc_signature_rounds = 0usize;
    for round in 0..TYPE_SIGNATURE_FIXED_POINT_MAX_ROUNDS {
        let (before_stmts, before_locals) = if perf { hir_shape(func) } else { (0, 0) };
        let round_start = if perf { Some(Instant::now()) } else { None };

        let def_changed = run_pass_logged(func, "type_inference", perf, apply_type_inference_pass);
        let callsite_changed = run_pass_logged(
            func,
            "callsite_type_prop",
            perf,
            apply_callsite_type_prop_pass,
        );
        let use_changed = run_pass_logged(
            func,
            "use_driven_type_infer",
            perf,
            apply_use_driven_type_infer_pass,
        );
        let constraint_changed = run_pass_logged(
            func,
            "type_constraint_prop",
            perf,
            apply_type_constraint_propagation,
        );
        let round_changed = def_changed || callsite_changed || use_changed || constraint_changed;

        if callsite_changed {
            interproc_signature_rounds += 1;
        }

        if diag {
            eprintln!(
                "[DIAG] normalize type-fp: {} round={} def_changed={} callsite_changed={} use_changed={} constraint_changed={}",
                func.name,
                round + 1,
                def_changed,
                callsite_changed,
                use_changed,
                constraint_changed,
            );
        }

        if let Some(start) = round_start {
            let (after_stmts, after_locals) = hir_shape(func);
            eprintln!(
                "[PERF] normalize type-fp-round: fn={} round={} changed={} elapsed_ms={:.3} stmts={}=>{} locals={}=>{}",
                func.name,
                round + 1,
                round_changed,
                start.elapsed().as_secs_f64() * 1000.0,
                before_stmts,
                after_stmts,
                before_locals,
                after_locals,
            );
        }

        if !round_changed {
            break;
        }
    }

    if interproc_signature_rounds > 0 {
        wave_stats::add_interproc_constraint_rounds(interproc_signature_rounds);
    }
}

pub fn normalize_function_body(body: &mut Vec<HirStmt>) {
    cleanup_stmt_list(body, "<body>", 0);
}

pub fn cleanup_func_stmt_list(func: &mut HirFunction) {
    let preserved_temps = preserved_materialization_names(&func.locals);
    // Scale round_limit by body size: large bodies (>500 stmts) converge in
    // fewer useful rounds; extra iterations mostly rescan unchanged trees.
    let stmt_count = count_hir_stmts(&func.body);
    let round_limit = if stmt_count > 500 { 6 } else { 16 };
    let global_refs = crate::cleanup::utils::collect_referenced_labels(&func.body);
    cleanup_stmt_list_with_options_and_preserved(
        &mut func.body,
        &func.name,
        0,
        CleanupStmtOptions {
            include_boundary_labels: true,
            round_limit,
        },
        &preserved_temps,
        Some(&global_refs),
    );
}

fn contains_call_expr(expr: &HirExpr) -> bool {
    match expr {
        HirExpr::Call { .. } => true,
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::Load { ptr: expr, .. }
        | HirExpr::PtrOffset { base: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. }
        | HirExpr::FieldAccess { base: expr, .. } => contains_call_expr(expr),
        HirExpr::Binary { lhs, rhs, .. } => contains_call_expr(lhs) || contains_call_expr(rhs),
        HirExpr::Index { base, index, .. } => contains_call_expr(base) || contains_call_expr(index),
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            contains_call_expr(cond)
                || contains_call_expr(then_expr)
                || contains_call_expr(else_expr)
        }
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => false,
    }
}

fn contains_call_lvalue(lhs: &HirLValue) -> bool {
    match lhs {
        HirLValue::Var(_) => false,
        HirLValue::Deref { ptr, .. } => contains_call_expr(ptr),
        HirLValue::Index { base, index, .. } => {
            contains_call_expr(base) || contains_call_expr(index)
        }
        HirLValue::FieldAccess { base, .. } => contains_call_expr(base),
    }
}

fn contains_call_stmt(stmt: &HirStmt) -> bool {
    match stmt {
        HirStmt::Assign { lhs, rhs } => contains_call_lvalue(lhs) || contains_call_expr(rhs),
        HirStmt::VaStart { va_list, .. } | HirStmt::Expr(va_list) => contains_call_expr(va_list),
        HirStmt::Block(stmts)
        | HirStmt::While { body: stmts, .. }
        | HirStmt::DoWhile { body: stmts, .. } => contains_call_stmts(stmts),
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            contains_call_expr(cond)
                || contains_call_stmts(then_body)
                || contains_call_stmts(else_body)
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            init.as_deref().is_some_and(contains_call_stmt)
                || cond.as_ref().is_some_and(contains_call_expr)
                || update.as_deref().is_some_and(contains_call_stmt)
                || contains_call_stmts(body)
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            contains_call_expr(expr)
                || cases.iter().any(|case| contains_call_stmts(&case.body))
                || contains_call_stmts(default)
        }
        HirStmt::Return(Some(expr)) => contains_call_expr(expr),
        HirStmt::Return(None)
        | HirStmt::Label(_)
        | HirStmt::Goto(_)
        | HirStmt::Break
        | HirStmt::Continue => false,
    }
}

pub fn body_contains_popcount_call(body: &[HirStmt]) -> bool {
    body.iter().any(stmt_contains_popcount_call)
}

fn stmt_contains_popcount_call(stmt: &HirStmt) -> bool {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            lvalue_contains_popcount_call(lhs) || expr_contains_popcount_call(rhs)
        }
        HirStmt::VaStart { va_list, .. } | HirStmt::Expr(va_list) => {
            expr_contains_popcount_call(va_list)
        }
        HirStmt::Block(stmts)
        | HirStmt::While { body: stmts, .. }
        | HirStmt::DoWhile { body: stmts, .. } => stmts.iter().any(stmt_contains_popcount_call),
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            expr_contains_popcount_call(cond)
                || then_body.iter().any(stmt_contains_popcount_call)
                || else_body.iter().any(stmt_contains_popcount_call)
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            init.as_deref().is_some_and(stmt_contains_popcount_call)
                || cond.as_ref().is_some_and(expr_contains_popcount_call)
                || update.as_deref().is_some_and(stmt_contains_popcount_call)
                || body.iter().any(stmt_contains_popcount_call)
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            expr_contains_popcount_call(expr)
                || cases
                    .iter()
                    .any(|case| case.body.iter().any(stmt_contains_popcount_call))
                || default.iter().any(stmt_contains_popcount_call)
        }
        HirStmt::Return(Some(expr)) => expr_contains_popcount_call(expr),
        HirStmt::Return(None)
        | HirStmt::Label(_)
        | HirStmt::Goto(_)
        | HirStmt::Break
        | HirStmt::Continue => false,
    }
}

fn lvalue_contains_popcount_call(lhs: &HirLValue) -> bool {
    match lhs {
        HirLValue::Var(_) => false,
        HirLValue::Deref { ptr, .. } => expr_contains_popcount_call(ptr),
        HirLValue::Index { base, index, .. } => {
            expr_contains_popcount_call(base) || expr_contains_popcount_call(index)
        }
        HirLValue::FieldAccess { base, .. } => expr_contains_popcount_call(base),
    }
}

fn expr_contains_popcount_call(expr: &HirExpr) -> bool {
    match expr {
        HirExpr::Call { target, args, .. } => {
            target == "__popcount" || args.iter().any(expr_contains_popcount_call)
        }
        HirExpr::Cast { expr: inner, .. }
        | HirExpr::Unary { expr: inner, .. }
        | HirExpr::Load { ptr: inner, .. }
        | HirExpr::PtrOffset { base: inner, .. }
        | HirExpr::AggregateCopy { src: inner, .. }
        | HirExpr::FieldAccess { base: inner, .. } => expr_contains_popcount_call(inner),
        HirExpr::Binary { lhs, rhs, .. } => {
            expr_contains_popcount_call(lhs) || expr_contains_popcount_call(rhs)
        }
        HirExpr::Index { base, index, .. } => {
            expr_contains_popcount_call(base) || expr_contains_popcount_call(index)
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            expr_contains_popcount_call(cond)
                || expr_contains_popcount_call(then_expr)
                || expr_contains_popcount_call(else_expr)
        }
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => false,
    }
}

pub fn contains_call_stmts(stmts: &[HirStmt]) -> bool {
    stmts.iter().any(contains_call_stmt)
}

use std::cell::RefCell;
use std::collections::HashMap;

thread_local! {
    pub static GLOBAL_SYMBOL_CONTEXT: RefCell<Option<GlobalSymbolContext>> = RefCell::new(None);
}

#[derive(Clone)]
pub struct GlobalSymbolContext {
    pub names: HashMap<u64, String>,
    pub sizes: HashMap<u64, u64>,
}

pub fn run_canonical_normalize_passes(func: &mut HirFunction, diag: bool, perf: bool) {
    use super::stages::{
        run_stage_block_structure_1, run_stage_cleanup, run_stage_deadcode_dynamic,
        run_stage_heritage_value_recovery, run_stage_memory_recovery, run_stage_merge,
        run_stage_proto_recovery, run_stage_stackstall, run_stage_type_early,
    };
    run_stage_proto_recovery(func, diag, perf);
    run_stage_deadcode_dynamic(func, diag, perf);
    run_stage_type_early(func, diag, perf);
    run_stage_stackstall(func, diag, perf);
    run_stage_heritage_value_recovery(func, diag, perf);
    run_stage_memory_recovery(func, diag, perf);
    run_stage_merge(func, diag, perf);
    run_stage_block_structure_1(func, diag, perf);
    run_stage_cleanup(func, diag, perf);
}

pub fn normalize_hir_function(func: &mut HirFunction) {
    super::groups::run_normalize_pipeline(func, normalize_diag_enabled(), normalize_perf_enabled());
}

pub fn is_large_hir_function(func: &HirFunction) -> bool {
    count_hir_stmts(&func.body) > 220 || func.locals.len() > 160
}

fn count_hir_stmts(stmts: &[HirStmt]) -> usize {
    fn count_stmt(stmt: &HirStmt) -> usize {
        match stmt {
            HirStmt::Block(stmts)
            | HirStmt::While { body: stmts, .. }
            | HirStmt::DoWhile { body: stmts, .. } => 1 + count_hir_stmts(stmts),
            HirStmt::Switch { cases, default, .. } => {
                1 + cases
                    .iter()
                    .map(|case| count_hir_stmts(&case.body))
                    .sum::<usize>()
                    + count_hir_stmts(default)
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => 1 + count_hir_stmts(then_body) + count_hir_stmts(else_body),
            _ => 1,
        }
    }

    stmts.iter().map(count_stmt).sum()
}

pub fn hir_shape(func: &HirFunction) -> (usize, usize) {
    (count_hir_stmts(&func.body), func.locals.len())
}

fn body_exceeds_early_cleanup_budget(body: &[HirStmt]) -> bool {
    count_hir_stmts(body) > EARLY_CLEANUP_BLOCK_STMT_LIMIT
        || count_hir_blocks(body) > EARLY_CLEANUP_BLOCK_BLOCK_LIMIT
}

#[derive(Debug, Clone, Copy)]
pub struct JumpResolverAdmission {
    pub eligible: bool,
    pub candidate_scoped: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct SccpAdmissionSummary {
    pub eligible: bool,
}

pub fn jump_resolver_admission(body: &[HirStmt]) -> JumpResolverAdmission {
    if !body_exceeds_early_cleanup_budget(body) {
        return JumpResolverAdmission {
            eligible: true,
            candidate_scoped: false,
        };
    }
    let candidate_count = jump_resolver_candidate_count(body);
    JumpResolverAdmission {
        eligible: candidate_count > 0 && candidate_count <= 16,
        candidate_scoped: candidate_count > 0 && candidate_count <= 16,
    }
}

pub fn sccp_admission_summary(body: &[HirStmt]) -> SccpAdmissionSummary {
    let has_control_seed = body_has_sccp_control_seed(body);
    let has_const_seed = body_has_sccp_const_seed(body);
    SccpAdmissionSummary {
        eligible: has_control_seed && has_const_seed,
    }
}

pub fn memory_fact_prefilter_allows_full(func: &HirFunction) -> bool {
    func.params
        .iter()
        .chain(func.locals.iter())
        .any(|binding| matches!(binding.ty, NirType::Ptr(_)))
        || body_has_memory_surface_interest(&func.body)
}

fn body_has_sccp_control_seed(body: &[HirStmt]) -> bool {
    body.iter().any(stmt_has_sccp_control_seed)
}

fn stmt_has_sccp_control_seed(stmt: &HirStmt) -> bool {
    match stmt {
        HirStmt::If { .. } | HirStmt::Switch { .. } => true,
        HirStmt::Block(body) => body_has_sccp_control_seed(body),
        HirStmt::While { cond, body } | HirStmt::DoWhile { body, cond } => {
            expr_contains_const(cond) || body_has_sccp_control_seed(body)
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            init.as_deref().is_some_and(stmt_has_sccp_const_seed)
                || cond.as_ref().is_some_and(expr_contains_const)
                || update.as_deref().is_some_and(stmt_has_sccp_const_seed)
                || body_has_sccp_control_seed(body)
        }
        _ => false,
    }
}

fn body_has_sccp_const_seed(body: &[HirStmt]) -> bool {
    body.iter().any(stmt_has_sccp_const_seed)
}

fn stmt_has_sccp_const_seed(stmt: &HirStmt) -> bool {
    match stmt {
        HirStmt::Assign { rhs, .. } | HirStmt::Expr(rhs) | HirStmt::Return(Some(rhs)) => {
            expr_contains_const(rhs)
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            expr_contains_const(cond)
                || body_has_sccp_const_seed(then_body)
                || body_has_sccp_const_seed(else_body)
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            expr_contains_const(expr)
                || cases
                    .iter()
                    .any(|case| body_has_sccp_const_seed(&case.body))
                || body_has_sccp_const_seed(default)
        }
        HirStmt::Block(body) => body_has_sccp_const_seed(body),
        HirStmt::While { cond, body } | HirStmt::DoWhile { body, cond } => {
            expr_contains_const(cond) || body_has_sccp_const_seed(body)
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            init.as_deref().is_some_and(stmt_has_sccp_const_seed)
                || cond.as_ref().is_some_and(expr_contains_const)
                || update.as_deref().is_some_and(stmt_has_sccp_const_seed)
                || body_has_sccp_const_seed(body)
        }
        HirStmt::VaStart { .. }
        | HirStmt::Return(None)
        | HirStmt::Break
        | HirStmt::Continue
        | HirStmt::Label(_)
        | HirStmt::Goto(_) => false,
    }
}

fn body_has_memory_surface_interest(body: &[HirStmt]) -> bool {
    body.iter().any(stmt_has_memory_surface_interest)
}

fn stmt_has_memory_surface_interest(stmt: &HirStmt) -> bool {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            lvalue_has_memory_surface_interest(lhs) || expr_has_memory_surface_interest(rhs)
        }
        HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => expr_has_memory_surface_interest(expr),
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            expr_has_memory_surface_interest(cond)
                || body_has_memory_surface_interest(then_body)
                || body_has_memory_surface_interest(else_body)
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            expr_has_memory_surface_interest(expr)
                || cases
                    .iter()
                    .any(|case| body_has_memory_surface_interest(&case.body))
                || body_has_memory_surface_interest(default)
        }
        HirStmt::Block(body) => body_has_memory_surface_interest(body),
        HirStmt::While { cond, body } | HirStmt::DoWhile { body, cond } => {
            expr_has_memory_surface_interest(cond) || body_has_memory_surface_interest(body)
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            init.as_deref()
                .is_some_and(stmt_has_memory_surface_interest)
                || cond.as_ref().is_some_and(expr_has_memory_surface_interest)
                || update
                    .as_deref()
                    .is_some_and(stmt_has_memory_surface_interest)
                || body_has_memory_surface_interest(body)
        }
        HirStmt::VaStart { .. }
        | HirStmt::Return(None)
        | HirStmt::Break
        | HirStmt::Continue
        | HirStmt::Label(_)
        | HirStmt::Goto(_) => false,
    }
}

fn lvalue_has_memory_surface_interest(lhs: &HirLValue) -> bool {
    match lhs {
        HirLValue::Var(_) => false,
        HirLValue::Deref { ptr, .. } => {
            let _ = ptr;
            true
        }
        HirLValue::Index { base, index, .. } => {
            let _ = (base, index);
            true
        }
        HirLValue::FieldAccess { base, .. } => {
            let _ = base;
            true
        }
    }
}

fn expr_has_memory_surface_interest(expr: &HirExpr) -> bool {
    match expr {
        HirExpr::Const(_, _) | HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) => false,
        HirExpr::Load { ptr, .. } => {
            let _ = ptr;
            true
        }
        HirExpr::PtrOffset { base, .. } | HirExpr::FieldAccess { base, .. } => {
            let _ = base;
            true
        }
        HirExpr::Index { base, index, .. } => {
            let _ = (base, index);
            true
        }
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. } => expr_has_memory_surface_interest(expr),
        HirExpr::Binary { lhs, rhs, .. } => {
            expr_has_memory_surface_interest(lhs) || expr_has_memory_surface_interest(rhs)
        }
        HirExpr::Call { args, .. } => args.iter().any(expr_has_memory_surface_interest),
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            expr_has_memory_surface_interest(cond)
                || expr_has_memory_surface_interest(then_expr)
                || expr_has_memory_surface_interest(else_expr)
        }
    }
}

fn expr_contains_const(expr: &HirExpr) -> bool {
    match expr {
        HirExpr::Const(_, _) => true,
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) => false,
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::Load { ptr: expr, .. }
        | HirExpr::PtrOffset { base: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. }
        | HirExpr::FieldAccess { base: expr, .. } => expr_contains_const(expr),
        HirExpr::Binary { lhs, rhs, .. } => expr_contains_const(lhs) || expr_contains_const(rhs),
        HirExpr::Call { args, .. } => args.iter().any(expr_contains_const),
        HirExpr::Index { base, index, .. } => {
            expr_contains_const(base) || expr_contains_const(index)
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            expr_contains_const(cond)
                || expr_contains_const(then_expr)
                || expr_contains_const(else_expr)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // prelude via parent

    fn empty_func() -> HirFunction {
        HirFunction {
            name: "admission".to_string(),
            params: Vec::new(),
            locals: Vec::new(),
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: Vec::new(),
            calling_convention: Default::default(),
            int_param_offsets: Vec::new(),
            is_64bit: true,
            suppress_entry_register_params: false,
            callee_observed_max_arity: Default::default(),
            callee_summaries: Default::default(),
        }
    }

    #[test]
    fn sccp_admission_rejects_linear_body_without_control_seed() {
        let body = vec![HirStmt::Assign {
            lhs: HirLValue::Var("xVar0".to_string()),
            rhs: HirExpr::Const(
                1,
                NirType::Int {
                    bits: 32,
                    signed: false,
                },
            ),
        }];
        assert!(!sccp_admission_summary(&body).eligible);
    }

    #[test]
    fn sccp_admission_accepts_if_with_const_guard() {
        let body = vec![HirStmt::If {
            cond: HirExpr::Binary {
                op: HirBinaryOp::Eq,
                lhs: Box::new(HirExpr::Var("uVar0".to_string())),
                rhs: Box::new(HirExpr::Const(
                    1,
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                )),
                ty: NirType::Bool,
            },
            then_body: vec![HirStmt::Return(None)],
            else_body: Vec::new(),
        }];
        assert!(sccp_admission_summary(&body).eligible);
    }

    #[test]
    fn memory_fact_prefilter_rejects_non_pointer_function() {
        let mut func = empty_func();
        func.body.push(HirStmt::Return(None));
        assert!(!memory_fact_prefilter_allows_full(&func));
    }

    #[test]
    fn memory_fact_prefilter_accepts_pointer_param() {
        let mut func = empty_func();
        func.params.push(NirBinding {
            name: "param_1".to_string(),
            ty: NirType::Ptr(Box::new(NirType::Unknown)),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::ParamIndex(0)),
            initializer: None,
        });
        assert!(memory_fact_prefilter_allows_full(&func));
    }

    #[test]
    fn type_fixed_point_keeps_pointer_add_offset_parameter_scalar() {
        let u8_ty = NirType::Int {
            bits: 8,
            signed: false,
        };
        let u64_ty = NirType::Int {
            bits: 64,
            signed: false,
        };
        let ptr_ty = NirType::Ptr(Box::new(u8_ty.clone()));
        let binding = |name: &str, ty: NirType, origin| NirBinding {
            name: name.to_string(),
            ty,
            surface_type_name: None,
            origin: Some(origin),
            initializer: None,
        };
        let mut func = empty_func();
        func.params = vec![
            binding("base", ptr_ty.clone(), NirBindingOrigin::ParamIndex(0)),
            binding("offset", ptr_ty.clone(), NirBindingOrigin::ParamIndex(1)),
        ];
        func.locals = vec![
            binding("end", ptr_ty.clone(), NirBindingOrigin::Temp),
            binding("value", u8_ty.clone(), NirBindingOrigin::Temp),
        ];
        func.body = vec![
            HirStmt::Assign {
                lhs: HirLValue::Var("end".into()),
                rhs: HirExpr::Binary {
                    op: HirBinaryOp::Add,
                    lhs: Box::new(HirExpr::Var("offset".into())),
                    rhs: Box::new(HirExpr::Cast {
                        ty: u64_ty.clone(),
                        expr: Box::new(HirExpr::Var("base".into())),
                    }),
                    ty: u64_ty,
                },
            },
            HirStmt::Assign {
                lhs: HirLValue::Var("value".into()),
                rhs: HirExpr::Load {
                    ptr: Box::new(HirExpr::Var("base".into())),
                    ty: u8_ty,
                },
            },
            HirStmt::If {
                cond: HirExpr::Binary {
                    op: HirBinaryOp::Ne,
                    lhs: Box::new(HirExpr::Var("end".into())),
                    rhs: Box::new(HirExpr::Var("base".into())),
                    ty: NirType::Bool,
                },
                then_body: Vec::new(),
                else_body: Vec::new(),
            },
        ];

        apply_type_signature_fixed_point(&mut func, false, false);

        assert!(matches!(func.params[0].ty, NirType::Ptr(_)));
        assert_eq!(
            func.params[1].ty,
            NirType::Int {
                bits: 64,
                signed: false,
            }
        );
    }

    #[test]
    fn type_fixed_point_keeps_aliased_pointer_add_offset_parameter_scalar() {
        let u8_ty = NirType::Int {
            bits: 8,
            signed: false,
        };
        let u64_ty = NirType::Int {
            bits: 64,
            signed: false,
        };
        let ptr_ty = NirType::Ptr(Box::new(u8_ty.clone()));
        let binding = |name: &str, ty: NirType, origin| NirBinding {
            name: name.to_string(),
            ty,
            surface_type_name: None,
            origin: Some(origin),
            initializer: None,
        };
        let mut func = empty_func();
        func.params = vec![
            binding("base", ptr_ty.clone(), NirBindingOrigin::ParamIndex(0)),
            binding("offset", ptr_ty.clone(), NirBindingOrigin::ParamIndex(1)),
        ];
        func.locals = vec![
            binding("cursor", ptr_ty.clone(), NirBindingOrigin::Temp),
            binding("end", ptr_ty.clone(), NirBindingOrigin::Temp),
            binding("value", u8_ty.clone(), NirBindingOrigin::Temp),
        ];
        func.body = vec![
            HirStmt::Assign {
                lhs: HirLValue::Var("end".into()),
                rhs: HirExpr::Var("offset".into()),
            },
            HirStmt::If {
                cond: HirExpr::Var("offset".into()),
                then_body: vec![
                    HirStmt::Assign {
                        lhs: HirLValue::Var("cursor".into()),
                        rhs: HirExpr::Var("base".into()),
                    },
                    HirStmt::Assign {
                        lhs: HirLValue::Var("end".into()),
                        rhs: HirExpr::Binary {
                            op: HirBinaryOp::Add,
                            lhs: Box::new(HirExpr::Var("offset".into())),
                            rhs: Box::new(HirExpr::Cast {
                                ty: u64_ty.clone(),
                                expr: Box::new(HirExpr::Var("cursor".into())),
                            }),
                            ty: u64_ty,
                        },
                    },
                    HirStmt::Assign {
                        lhs: HirLValue::Var("value".into()),
                        rhs: HirExpr::Load {
                            ptr: Box::new(HirExpr::Var("cursor".into())),
                            ty: u8_ty,
                        },
                    },
                ],
                else_body: Vec::new(),
            },
            HirStmt::If {
                cond: HirExpr::Binary {
                    op: HirBinaryOp::Ne,
                    lhs: Box::new(HirExpr::Var("end".into())),
                    rhs: Box::new(HirExpr::Var("cursor".into())),
                    ty: NirType::Bool,
                },
                then_body: Vec::new(),
                else_body: Vec::new(),
            },
        ];

        apply_type_signature_fixed_point(&mut func, false, false);

        assert!(matches!(func.params[0].ty, NirType::Ptr(_)));
        assert_eq!(
            func.params[1].ty,
            NirType::Int {
                bits: 64,
                signed: false,
            }
        );
    }
}

pub fn run_cleanup_block<F>(
    func: &mut HirFunction,
    pass_name: &str,
    perf: bool,
    mut block: F,
) -> bool
where
    F: FnMut(&mut HirFunction),
{
    if body_exceeds_early_cleanup_budget(&func.body) {
        wave_stats::add_cleanup_budget_skips(1);
        return false;
    }

    run_pass_logged(func, pass_name, perf, |f| {
        let (before_stmts, before_locals) = hir_shape(f);
        block(f);
        let (after_stmts, after_locals) = hir_shape(f);
        before_stmts != after_stmts || before_locals != after_locals
    })
}

pub fn run_cleanup_family_passes(
    func: &mut HirFunction,
    stage: &str,
    perf: bool,
    budget: PassBudget,
) -> bool {
    let mut changed = false;
    let body_stmt_count = count_hir_stmts(&func.body);
    let body_block_count = count_hir_blocks(&func.body);
    let within_body_budget = budget.allows_body_cleanup(body_stmt_count, body_block_count);

    if has_binding_initializers(&func.locals) {
        wave_stats::add_cleanup_family_binding_init(1);
        changed |= run_pass_logged(func, &format!("cleanup_binding_init_{stage}"), perf, |f| {
            let before = collect_initializer_fingerprints(&f.locals);
            normalize_binding_initializers(&mut f.locals);
            before != collect_initializer_fingerprints(&f.locals)
        });
        if within_body_budget {
            changed |= run_pass_logged(
                func,
                &format!("cleanup_pointer_alias_binding_{stage}"),
                perf,
                collapse_trivial_pointer_alias_bindings,
            );
        }
    } else {
        wave_stats::add_cleanup_budget_skips(1);
    }

    if !func.body.is_empty() {
        wave_stats::add_cleanup_family_stmt_canonical(1);
        if body_has_conditional_return_shapes(&func.body) {
            changed |= run_pass_logged(
                func,
                &format!("cleanup_stmt_fold_conditional_return_{stage}"),
                perf,
                |f| collapse_redundant_conditional_returns_recursive(&mut f.body),
            );
        }
        if within_body_budget && body_needs_stmt_fold_cleanup(&func.body) {
            wave_stats::add_cleanup_stmt_fold(1);
            changed |= run_pass_logged(func, &format!("cleanup_stmt_fold_{stage}"), perf, |f| {
                let before = hir_shape(f);
                cleanup_stmt_list_with_options(
                    &mut f.body,
                    &f.name,
                    0,
                    CleanupStmtOptions {
                        include_boundary_labels: false,
                        round_limit: budget.round_limit,
                    },
                );
                before != hir_shape(f)
            });
        } else if body_needs_stmt_fold_cleanup(&func.body) {
            wave_stats::add_cleanup_budget_skips(1);
        }

        if within_body_budget {
            changed |= run_pass_logged(
                func,
                &format!("strip_redundant_assign_casts_{stage}"),
                perf,
                strip_redundant_assign_casts,
            );
        }

        if body_has_boundary_label_shapes(&func.body) {
            wave_stats::add_cleanup_boundary_label(1);
            changed |= run_pass_logged(
                func,
                &format!("cleanup_boundary_label_{stage}"),
                perf,
                |f| {
                    let global_refs = crate::cleanup::utils::collect_referenced_labels(&f.body);
                    cleanup_boundary_labels_recursive(&mut f.body, &global_refs)
                },
            );
        } else {
            wave_stats::add_cleanup_budget_skips(1);
        }

        if within_body_budget && body_has_loopish_shapes(&func.body) {
            wave_stats::add_cleanup_loopish_rewrite(1);
            changed |= run_pass_logged(
                func,
                &format!("cleanup_loopish_rewrite_{stage}"),
                perf,
                |f| apply_for_loop_folding(&mut f.body),
            );
        } else if body_has_loopish_shapes(&func.body) {
            wave_stats::add_cleanup_budget_skips(1);
        }
    } else {
        wave_stats::add_cleanup_budget_skips(1);
    }

    if !func.locals.is_empty() && within_body_budget {
        wave_stats::add_cleanup_family_dead_binding(1);
        changed |= run_pass_logged(func, &format!("cleanup_dead_binding_{stage}"), perf, |f| {
            let before = hir_shape(f);
            eliminate_dead_local_clobber_assigns(f);
            apply_wide_dead_assignment_pass(f);
            prune_unused_temp_bindings(f);
            prune_unused_dead_local_bindings(f);
            before != hir_shape(f)
        });
    } else if !func.locals.is_empty() {
        wave_stats::add_cleanup_budget_skips(1);
    }

    changed
}

pub fn run_pass_logged<F>(func: &mut HirFunction, pass_name: &str, perf: bool, pass_fn: F) -> bool
where
    F: FnOnce(&mut HirFunction) -> bool,
{
    let _span = debug_span!("normalize_pass", fn_name = %func.name, pass = pass_name).entered();

    let (before_stmts, before_locals) = hir_shape(func);
    let start = Instant::now();
    let changed = pass_fn(func);
    let (after_stmts, after_locals) = hir_shape(func);
    let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;

    fission_midend_core::wave_stats::add_pass_metric(
        pass_name,
        elapsed_ms,
        changed,
        before_stmts,
        after_stmts,
        before_locals,
        after_locals,
    );

    debug!(
        changed,
        elapsed_ms,
        stmts_reduced = (before_stmts as isize - after_stmts as isize),
        locals_reduced = (before_locals as isize - after_locals as isize),
        "pass completed"
    );

    if perf {
        eprintln!(
            "[PERF] normalize pass: fn={} pass={} changed={} elapsed_ms={:.3} stmts={}=>{} locals={}=>{}",
            func.name,
            pass_name,
            changed,
            elapsed_ms,
            before_stmts,
            after_stmts,
            before_locals,
            after_locals,
        );
    }
    changed
}

pub fn run_pass_logged_fn<F>(
    func: &mut HirFunction,
    pass_name: &str,
    perf: bool,
    pass_fn: F,
) -> bool
where
    F: FnOnce(&mut HirFunction) -> bool,
{
    run_pass_logged(func, pass_name, perf, pass_fn)
}

fn has_binding_initializers(bindings: &[NirBinding]) -> bool {
    bindings.iter().any(|binding| binding.initializer.is_some())
}

fn collect_initializer_fingerprints(bindings: &[NirBinding]) -> Vec<(String, Option<String>)> {
    bindings
        .iter()
        .map(|binding| {
            (
                binding.name.clone(),
                binding.initializer.as_ref().map(format_expr_key),
            )
        })
        .collect()
}

pub fn body_has_loopish_shapes(stmts: &[HirStmt]) -> bool {
    for stmt in stmts {
        match stmt {
            HirStmt::While { .. } | HirStmt::DoWhile { .. } | HirStmt::For { .. } => return true,
            HirStmt::Block(body) => {
                if body_has_loopish_shapes(body) {
                    return true;
                }
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                if body_has_loopish_shapes(then_body) || body_has_loopish_shapes(else_body) {
                    return true;
                }
            }
            HirStmt::Switch { cases, default, .. } => {
                if cases.iter().any(|case| body_has_loopish_shapes(&case.body))
                    || body_has_loopish_shapes(default)
                {
                    return true;
                }
            }
            HirStmt::Assign { .. }
            | HirStmt::VaStart { .. }
            | HirStmt::Expr(_)
            | HirStmt::Label(_)
            | HirStmt::Goto(_)
            | HirStmt::Return(_)
            | HirStmt::Break
            | HirStmt::Continue => {}
        }
    }
    false
}

fn body_has_boundary_label_shapes(stmts: &[HirStmt]) -> bool {
    for stmt in stmts {
        match stmt {
            HirStmt::Label(_) | HirStmt::Goto(_) => return true,
            HirStmt::Block(body)
            | HirStmt::While { body, .. }
            | HirStmt::DoWhile { body, .. }
            | HirStmt::For { body, .. } => {
                if body_has_boundary_label_shapes(body) {
                    return true;
                }
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                if body_has_boundary_label_shapes(then_body)
                    || body_has_boundary_label_shapes(else_body)
                {
                    return true;
                }
            }
            HirStmt::Switch { cases, default, .. } => {
                if cases
                    .iter()
                    .any(|case| body_has_boundary_label_shapes(&case.body))
                    || body_has_boundary_label_shapes(default)
                {
                    return true;
                }
            }
            HirStmt::Assign { .. }
            | HirStmt::VaStart { .. }
            | HirStmt::Expr(_)
            | HirStmt::Return(_)
            | HirStmt::Break
            | HirStmt::Continue => {}
        }
    }
    false
}

fn body_has_conditional_return_shapes(stmts: &[HirStmt]) -> bool {
    for stmt in stmts {
        match stmt {
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                let then_ret = then_body
                    .last()
                    .is_some_and(|stmt| matches!(stmt, HirStmt::Return(_)));
                let else_ret = else_body
                    .last()
                    .is_some_and(|stmt| matches!(stmt, HirStmt::Return(_)));
                if (then_ret && else_ret)
                    || body_has_conditional_return_shapes(then_body)
                    || body_has_conditional_return_shapes(else_body)
                {
                    return true;
                }
            }
            HirStmt::Block(body)
            | HirStmt::While { body, .. }
            | HirStmt::DoWhile { body, .. }
            | HirStmt::For { body, .. } => {
                if body_has_conditional_return_shapes(body) {
                    return true;
                }
            }
            HirStmt::Switch { cases, default, .. } => {
                if cases
                    .iter()
                    .any(|case| body_has_conditional_return_shapes(&case.body))
                    || body_has_conditional_return_shapes(default)
                {
                    return true;
                }
            }
            HirStmt::Assign { .. }
            | HirStmt::VaStart { .. }
            | HirStmt::Expr(_)
            | HirStmt::Label(_)
            | HirStmt::Goto(_)
            | HirStmt::Return(_)
            | HirStmt::Break
            | HirStmt::Continue => {}
        }
    }
    false
}

fn body_needs_stmt_fold_cleanup(stmts: &[HirStmt]) -> bool {
    for stmt in stmts {
        match stmt {
            HirStmt::Assign {
                lhs: HirLValue::Var(name),
                ..
            } if looks_like_trivial_temp_name(name) => return true,
            HirStmt::Return(Some(HirExpr::Var(name))) if looks_like_trivial_temp_name(name) => {
                return true;
            }
            HirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                if matches!(cond, HirExpr::Const(_, _))
                    || then_body.is_empty()
                    || else_body.is_empty()
                    || body_needs_stmt_fold_cleanup(then_body)
                    || body_needs_stmt_fold_cleanup(else_body)
                {
                    return true;
                }
            }
            HirStmt::Block(body)
            | HirStmt::While { body, .. }
            | HirStmt::DoWhile { body, .. }
            | HirStmt::For { body, .. } => {
                if body_needs_stmt_fold_cleanup(body) {
                    return true;
                }
            }
            HirStmt::Switch { cases, default, .. } => {
                if cases
                    .iter()
                    .any(|case| body_needs_stmt_fold_cleanup(&case.body))
                    || body_needs_stmt_fold_cleanup(default)
                {
                    return true;
                }
            }
            HirStmt::Assign { .. }
            | HirStmt::VaStart { .. }
            | HirStmt::Expr(_)
            | HirStmt::Label(_)
            | HirStmt::Goto(_)
            | HirStmt::Return(_)
            | HirStmt::Break
            | HirStmt::Continue => {}
        }
    }
    false
}

fn looks_like_trivial_temp_name(name: &str) -> bool {
    name == "result"
        || name == "retval"
        || name.starts_with("uVar")
        || name.starts_with("iVar")
        || name.starts_with("xVar")
        || name.starts_with("bVar")
}

fn count_hir_blocks(stmts: &[HirStmt]) -> usize {
    fn count_stmt(stmt: &HirStmt) -> usize {
        match stmt {
            HirStmt::Block(body)
            | HirStmt::While { body, .. }
            | HirStmt::DoWhile { body, .. }
            | HirStmt::For { body, .. } => 1 + count_hir_blocks(body),
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => 1 + count_hir_blocks(then_body) + count_hir_blocks(else_body),
            HirStmt::Switch { cases, default, .. } => {
                1 + cases
                    .iter()
                    .map(|case| count_hir_blocks(&case.body))
                    .sum::<usize>()
                    + count_hir_blocks(default)
            }
            HirStmt::Assign { .. }
            | HirStmt::VaStart { .. }
            | HirStmt::Expr(_)
            | HirStmt::Label(_)
            | HirStmt::Goto(_)
            | HirStmt::Return(_)
            | HirStmt::Break
            | HirStmt::Continue => 0,
        }
    }

    stmts.iter().map(count_stmt).sum()
}

pub fn normalize_stmt(stmt: &mut HirStmt) {
    match stmt {
        HirStmt::Assign { rhs, .. } => normalize_expr(rhs),
        HirStmt::VaStart { va_list, .. } => normalize_expr(va_list),
        HirStmt::Expr(expr) => normalize_expr(expr),
        HirStmt::Block(stmts) => {
            for stmt in stmts {
                normalize_stmt(stmt);
            }
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            normalize_expr(expr);
            for case in cases {
                for stmt in &mut case.body {
                    normalize_stmt(stmt);
                }
            }
            for stmt in default {
                normalize_stmt(stmt);
            }
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            normalize_condition_expr(cond);
            for stmt in then_body {
                normalize_stmt(stmt);
            }
            for stmt in else_body {
                normalize_stmt(stmt);
            }
        }
        HirStmt::While { cond, body } => {
            normalize_condition_expr(cond);
            for stmt in body {
                normalize_stmt(stmt);
            }
        }
        HirStmt::DoWhile { body, cond } => {
            for stmt in body {
                normalize_stmt(stmt);
            }
            normalize_condition_expr(cond);
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(i) = init {
                normalize_stmt(i);
            }
            if let Some(c) = cond {
                normalize_condition_expr(c);
            }
            if let Some(u) = update {
                normalize_stmt(u);
            }
            for stmt in body {
                normalize_stmt(stmt);
            }
        }
        HirStmt::Label(_) | HirStmt::Goto(_) => {}
        HirStmt::Return(Some(expr)) => normalize_expr(expr),
        HirStmt::Return(None) | HirStmt::Break | HirStmt::Continue => {}
    }
}

fn normalize_condition_expr(expr: &mut HirExpr) {
    normalize_expr(expr);
    let mut current = expr.clone();
    loop {
        let next = canonicalize_condition_expr(&current);
        match next {
            Some(next_expr) if next_expr != current => {
                current = next_expr;
                normalize_expr(&mut current);
            }
            _ => break,
        }
    }
    *expr = current;
}

#[derive(Debug, Clone, Copy)]
struct CleanupStmtOptions {
    include_boundary_labels: bool,
    round_limit: usize,
}

fn cleanup_stmt_list(stmts: &mut Vec<HirStmt>, func_name: &str, depth: usize) {
    let preserved_temps = HashSet::new();
    let global_refs = if depth == 0 {
        Some(crate::cleanup::utils::collect_referenced_labels(stmts))
    } else {
        None
    };
    cleanup_stmt_list_with_options_and_preserved(
        stmts,
        func_name,
        depth,
        CleanupStmtOptions {
            include_boundary_labels: true,
            round_limit: 16,
        },
        &preserved_temps,
        global_refs.as_ref(),
    );
}

fn cleanup_stmt_list_with_options(
    stmts: &mut Vec<HirStmt>,
    func_name: &str,
    depth: usize,
    options: CleanupStmtOptions,
) {
    let preserved_temps = HashSet::new();
    let global_refs = if depth == 0 {
        Some(crate::cleanup::utils::collect_referenced_labels(stmts))
    } else {
        None
    };
    cleanup_stmt_list_with_options_and_preserved(
        stmts,
        func_name,
        depth,
        options,
        &preserved_temps,
        global_refs.as_ref(),
    );
}

fn cleanup_stmt_list_with_options_and_preserved(
    stmts: &mut Vec<HirStmt>,
    func_name: &str,
    depth: usize,
    options: CleanupStmtOptions,
    preserved_temps: &HashSet<&str>,
    global_refs: Option<&HashSet<String>>,
) {
    for stmt in stmts.iter_mut() {
        normalize_stmt(stmt);
        match stmt {
            HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
                cleanup_stmt_list_with_options_and_preserved(
                    body,
                    func_name,
                    depth + 1,
                    options,
                    preserved_temps,
                    global_refs,
                )
            }
            HirStmt::For {
                init, update, body, ..
            } => {
                if let Some(i) = init {
                    if let HirStmt::Block(b) = &mut **i {
                        cleanup_stmt_list_with_options_and_preserved(
                            b,
                            func_name,
                            depth + 1,
                            options,
                            preserved_temps,
                            global_refs,
                        );
                    }
                }
                if let Some(u) = update {
                    if let HirStmt::Block(b) = &mut **u {
                        cleanup_stmt_list_with_options_and_preserved(
                            b,
                            func_name,
                            depth + 1,
                            options,
                            preserved_temps,
                            global_refs,
                        );
                    }
                }
                cleanup_stmt_list_with_options_and_preserved(
                    body,
                    func_name,
                    depth + 1,
                    options,
                    preserved_temps,
                    global_refs,
                )
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                cleanup_stmt_list_with_options_and_preserved(
                    then_body,
                    func_name,
                    depth + 1,
                    options,
                    preserved_temps,
                    global_refs,
                );
                cleanup_stmt_list_with_options_and_preserved(
                    else_body,
                    func_name,
                    depth + 1,
                    options,
                    preserved_temps,
                    global_refs,
                );
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    cleanup_stmt_list_with_options_and_preserved(
                        &mut case.body,
                        func_name,
                        depth + 1,
                        options,
                        preserved_temps,
                        global_refs,
                    );
                }
                cleanup_stmt_list_with_options_and_preserved(
                    default,
                    func_name,
                    depth + 1,
                    options,
                    preserved_temps,
                    global_refs,
                );
            }
            HirStmt::Assign { .. }
            | HirStmt::VaStart { .. }
            | HirStmt::Expr(_)
            | HirStmt::Label(_)
            | HirStmt::Goto(_)
            | HirStmt::Return(_)
            | HirStmt::Break
            | HirStmt::Continue => {}
        }
    }

    let diag = normalize_diag_enabled();
    let loop_start = Instant::now();
    let mut iterations = 0usize;
    let mut last_stmt_count = count_hir_stmts(stmts);
    loop {
        iterations += 1;
        let mut changed = false;
        let mut last_changed_pass = None;
        // Run at every nesting depth so Block-wrapped tails and if-arms also
        // fold `reg = C; return reg` (ABI return regs included).
        if collapse_trivial_assign_returns(stmts, preserved_temps) {
            changed = true;
            last_changed_pass = Some("collapse_trivial_assign_returns");
        }
        if collapse_loop_exit_alias_returns(stmts) {
            changed = true;
            last_changed_pass = Some("collapse_loop_exit_alias_returns");
        }
        if recover_guarded_loop_tail_accumulator_returns(stmts) {
            changed = true;
            last_changed_pass = Some("recover_guarded_loop_tail_accumulator_returns");
        }
        if depth == 0 && inline_single_use_temps(stmts, preserved_temps) {
            changed = true;
            last_changed_pass = Some("inline_single_use_temps");
        }
        if collapse_temp_self_square_assigns(stmts) {
            changed = true;
            last_changed_pass = Some("collapse_temp_self_square_assigns");
        }
        if collapse_adjacent_pure_copy_into_if(stmts) {
            changed = true;
            last_changed_pass = Some("collapse_adjacent_pure_copy_into_if");
        }
        if prune_unreachable_after_terminal(stmts) {
            changed = true;
            last_changed_pass = Some("prune_unreachable_after_terminal");
        }
        if depth == 0 && eliminate_dead_temp_assigns(stmts, preserved_temps) {
            changed = true;
            last_changed_pass = Some("eliminate_dead_temp_assigns");
        }
        if eliminate_redundant_var_assigns(stmts) {
            changed = true;
            last_changed_pass = Some("eliminate_redundant_var_assigns");
        }
        if simplify_empty_and_constant_ifs(stmts) {
            changed = true;
            last_changed_pass = Some("simplify_empty_and_constant_ifs");
        }
        if canonicalize_minmax_conditional_returns(stmts) {
            changed = true;
            last_changed_pass = Some("canonicalize_minmax_conditional_returns");
        }
        if conditional_select_pass(stmts) {
            changed = true;
            last_changed_pass = Some("conditional_select_pass");
        }
        if apply_condexe_folding_pass(stmts) {
            changed = true;
            last_changed_pass = Some("apply_condexe_folding_pass");
        }
        if apply_iblock_phi_elimination(stmts) {
            changed = true;
            last_changed_pass = Some("apply_iblock_phi_elimination");
        }
        if collapse_redundant_conditional_returns(stmts) {
            changed = true;
            last_changed_pass = Some("collapse_redundant_conditional_returns");
        }
        if simplify_fallthrough_edges(stmts) {
            changed = true;
            last_changed_pass = Some("simplify_fallthrough_edges");
        }
        if fuse_single_predecessor_boundaries(stmts) {
            changed = true;
            last_changed_pass = Some("fuse_single_predecessor_boundaries");
        }
        if collapse_common_exit_guard_chain(stmts) {
            changed = true;
            last_changed_pass = Some("collapse_common_exit_guard_chain");
        }
        if promote_guarded_jump_target_tail(stmts) {
            changed = true;
            last_changed_pass = Some("promote_guarded_jump_target_tail");
        }
        let current_refs = if depth == 0 {
            Some(crate::cleanup::utils::collect_referenced_labels(stmts))
        } else {
            None
        };
        let active_refs = if depth == 0 {
            current_refs.as_ref()
        } else {
            global_refs
        };

        if options.include_boundary_labels && cleanup_redundant_boundary_labels(stmts, active_refs)
        {
            changed = true;
            last_changed_pass = Some("cleanup_redundant_boundary_labels");
        }
        if remove_unreferenced_leading_labels(stmts, active_refs) {
            changed = true;
            last_changed_pass = Some("remove_unreferenced_leading_labels");
        }
        if !changed {
            break;
        }
        if iterations >= options.round_limit {
            break;
        }
        let current_count = count_hir_stmts(stmts);
        // Diminishing-returns early exit: after 3+ rounds, if the stmt-count
        // reduction in this round is less than 1% of the starting count,
        // further rounds are unlikely to produce meaningful simplifications.
        if iterations >= 3 {
            if last_stmt_count > 100 {
                let diff = if last_stmt_count >= current_count {
                    last_stmt_count - current_count
                } else {
                    current_count - last_stmt_count
                };
                if diff * 100 < last_stmt_count {
                    if diag {
                        eprintln!(
                            "[DIAG] normalize loop early exit: {} depth={} iterations={} diff={} (< 1%)",
                            func_name, depth, iterations, diff
                        );
                    }
                    break;
                }
            }
            last_stmt_count = current_count;
        }
        if diag {
            eprintln!(
                "[DIAG] normalize loop: {} depth={} iterations={} elapsed={:.3}s stmt_count={} last_changed_pass={}",
                func_name,
                depth,
                iterations,
                loop_start.elapsed().as_secs_f64(),
                current_count,
                last_changed_pass.unwrap_or("<none>")
            );
        }
        for stmt in stmts.iter_mut() {
            normalize_stmt(stmt);
        }
        if iterations < 3 {
            last_stmt_count = count_hir_stmts(stmts);
        }
    }
    if diag && (iterations > 1 || loop_start.elapsed().as_millis() > 100) {
        eprintln!(
            "[DIAG] normalize loop done: {} depth={} iterations={} elapsed={:.3}s",
            func_name,
            depth,
            iterations,
            loop_start.elapsed().as_secs_f64()
        );
    }
}

fn cleanup_boundary_labels_recursive(
    stmts: &mut Vec<HirStmt>,
    global_refs: &HashSet<String>,
) -> bool {
    let mut changed = cleanup_redundant_boundary_labels(stmts, Some(global_refs))
        || remove_unreferenced_leading_labels(stmts, Some(global_refs));
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Block(body)
            | HirStmt::While { body, .. }
            | HirStmt::DoWhile { body, .. }
            | HirStmt::For { body, .. } => {
                changed |= cleanup_boundary_labels_recursive(body, global_refs);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= cleanup_boundary_labels_recursive(then_body, global_refs);
                changed |= cleanup_boundary_labels_recursive(else_body, global_refs);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= cleanup_boundary_labels_recursive(&mut case.body, global_refs);
                }
                changed |= cleanup_boundary_labels_recursive(default, global_refs);
            }
            HirStmt::Assign { .. }
            | HirStmt::VaStart { .. }
            | HirStmt::Expr(_)
            | HirStmt::Label(_)
            | HirStmt::Goto(_)
            | HirStmt::Return(_)
            | HirStmt::Break
            | HirStmt::Continue => {}
        }
    }
    changed
}

fn collapse_redundant_conditional_returns_recursive(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = collapse_redundant_conditional_returns(stmts);
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Block(body)
            | HirStmt::While { body, .. }
            | HirStmt::DoWhile { body, .. }
            | HirStmt::For { body, .. } => {
                changed |= collapse_redundant_conditional_returns_recursive(body);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= collapse_redundant_conditional_returns_recursive(then_body);
                changed |= collapse_redundant_conditional_returns_recursive(else_body);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= collapse_redundant_conditional_returns_recursive(&mut case.body);
                }
                changed |= collapse_redundant_conditional_returns_recursive(default);
            }
            HirStmt::Assign { .. }
            | HirStmt::VaStart { .. }
            | HirStmt::Expr(_)
            | HirStmt::Label(_)
            | HirStmt::Goto(_)
            | HirStmt::Return(_)
            | HirStmt::Break
            | HirStmt::Continue => {}
        }
    }
    changed
}

pub fn normalize_expr(expr: &mut HirExpr) {
    // Pre-pass: merge consecutive shifts top-down before child recursion so that
    // Shr(Shr(x, K1), K2) → Shr(x, K1+K2) is visible before any child Shr gets
    // converted to a division by recognize_mod_div_power_of_two.
    if let Some(merged) = merge_consecutive_shifts(expr) {
        *expr = merged;
    }
    match expr {
        HirExpr::Cast { expr: inner, .. } => normalize_expr(inner),
        HirExpr::Unary { expr: inner, .. } => normalize_expr(inner),
        HirExpr::Binary { lhs, rhs, .. } => {
            normalize_expr(lhs);
            normalize_expr(rhs);
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                normalize_expr(arg);
            }
        }
        HirExpr::Load { ptr, .. }
        | HirExpr::PtrOffset { base: ptr, .. }
        | HirExpr::FieldAccess { base: ptr, .. } => normalize_expr(ptr),
        HirExpr::Index { base, index, .. } => {
            normalize_expr(base);
            normalize_expr(index);
        }
        HirExpr::AggregateCopy { src, .. } => normalize_expr(src),
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            normalize_expr(cond);
            normalize_expr(then_expr);
            normalize_expr(else_expr);
        }
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => {}
    }

    let mut current = expr.clone();
    loop {
        let next = canonicalize_integer_expr(&current)
            .or_else(|| recognize_dumpty_hump_cast(&current))
            .or_else(|| recognize_humpty_dumpty_or(&current))
            .or_else(|| recognize_concat_zext_or(&current))
            .or_else(|| recognize_dumpty_hump_late(&current))
            .or_else(|| simplify_negated_const(&current))
            .or_else(|| simplify_double_add(&current))
            .or_else(|| simplify_factor_common_mul(&current))
            .or_else(|| simplify_distribute_common_factor(&current))
            .or_else(|| simplify_nested_adds_subs(&current))
            .or_else(|| simplify_collect_mul_terms(&current))
            .or_else(|| recognize_compiler_runtime_division(&current))
            .or_else(|| recognize_mod_div_power_of_two(&current))
            .or_else(|| recognize_magic_number_division(&current))
            .or_else(|| recognize_hi_lo_extract(&current))
            .or_else(|| simplify_subpiece_chain(&current))
            .or_else(|| merge_consecutive_shifts(&current))
            .or_else(|| recognize_wide_integer_recombine(&current))
            .or_else(|| canonicalize_flag_intrinsics(&current))
            .or_else(|| normalize_boolean_logic(&current))
            .or_else(|| cleanup_arithmetic_wrappers(&current))
            .or_else(|| collapse_zero_offset_cast(&current));
        match next {
            Some(next_expr) if next_expr != current => current = next_expr,
            _ => break,
        }
    }
    *expr = current;
}

fn normalize_diag_enabled() -> bool {
    std::env::var_os("FISSION_PREVIEW_DIAG").is_some()
}

fn normalize_perf_enabled() -> bool {
    std::env::var_os("FISSION_PREVIEW_PERF").is_some()
}
