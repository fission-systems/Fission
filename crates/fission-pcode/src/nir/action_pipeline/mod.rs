//! Ghidra-style action pipeline framework for the NIR decompiler.
//!
//! Provides typed pass registration, action groups with fixpoint repeat,
//! admission gates, and telemetry hooks aligned with [`GhidraActionConcept`].

mod budget;
mod concept;
mod gates;
mod group;
mod pass;
mod pipeline;
mod pool;

pub(crate) use budget::{
    count_hir_blocks, count_hir_stmts, hir_shape, is_large_hir_function, PassBudget,
    EARLY_CLEANUP_BLOCK_BLOCK_LIMIT, EARLY_CLEANUP_BLOCK_STMT_LIMIT,
    LARGE_FUNCTION_LOCAL_THRESHOLD, LARGE_FUNCTION_STMT_THRESHOLD,
    TYPE_SIGNATURE_FIXED_POINT_MAX_ROUNDS,
};
pub(crate) use gates::{
    body_exceeds_early_cleanup_budget, cleanup_round_limit_for, gate_not_large_function,
    CLEANUP_DEFAULT_ROUND_LIMIT, CLEANUP_LARGE_BODY_ROUND_LIMIT, CLEANUP_LARGE_BODY_STMT_THRESHOLD,
    INIT_CLEANUP_BLOCK_LIMIT, INIT_CLEANUP_ROUND_LIMIT, INIT_CLEANUP_STMT_LIMIT,
    JUMP_RESOLVER_CANDIDATE_LIMIT, MERGE_TYPE_MAX_ROUNDS, RULE_POOL_MAX_ROUNDS,
    STRUCTURING_TIME_CEILING_SECS, TRACE_DAG_FOLLOW_BLOCK_LIMIT,
};
pub(crate) use concept::{
    record_ghidra_action_stage, record_ghidra_clean_room_pipeline_complete,
    stage_boundary_violation, GhidraActionConcept, GHIDRA_CLEAN_ROOM_ACTION_SEQUENCE,
};
pub(crate) use group::{ActionGroup, Gate, Repeat};
pub(crate) use pass::{fn_pass, run_pass_logged, FnPass, Pass, PassCtx, PassOutcome};
pub(crate) use pipeline::{group, Pipeline};
pub(crate) use pool::{apply_rules_to_stmts, ActionPool, Rule};
