//! Ghidra-style action pipeline framework for the NIR decompiler.
//!
//! Provides typed pass registration, action groups with fixpoint repeat,
//! admission gates, and telemetry hooks aligned with [`GhidraActionConcept`].

mod budget;
mod cleanup_pass;
mod concept;
mod gates;
mod group;
mod pass;
mod pipeline;
mod pool;

pub use cleanup_pass::{CleanupPass, GatedFollowupPass, cleanup_pass, gated_followup};
pub use budget::{
    EARLY_CLEANUP_BLOCK_BLOCK_LIMIT, EARLY_CLEANUP_BLOCK_STMT_LIMIT,
    LARGE_FUNCTION_LOCAL_THRESHOLD, LARGE_FUNCTION_STMT_THRESHOLD, PassBudget,
    TYPE_SIGNATURE_FIXED_POINT_MAX_ROUNDS, count_hir_blocks, count_hir_stmts, hir_shape,
    is_large_hir_function,
};
pub use concept::{
    GHIDRA_CLEAN_ROOM_ACTION_SEQUENCE, GhidraActionConcept, record_ghidra_action_stage,
    record_ghidra_clean_room_pipeline_complete, stage_boundary_violation,
};
pub use gates::{
    CLEANUP_DEFAULT_ROUND_LIMIT, CLEANUP_LARGE_BODY_ROUND_LIMIT, CLEANUP_LARGE_BODY_STMT_THRESHOLD,
    INIT_CLEANUP_BLOCK_LIMIT, INIT_CLEANUP_ROUND_LIMIT, INIT_CLEANUP_STMT_LIMIT,
    JUMP_RESOLVER_CANDIDATE_LIMIT, MERGE_TYPE_MAX_ROUNDS, RULE_POOL_MAX_ROUNDS,
    STRUCTURING_TIME_CEILING_SECS, TRACE_DAG_FOLLOW_BLOCK_LIMIT, body_exceeds_early_cleanup_budget,
    cleanup_round_limit_for, gate_not_large_function,
};
pub use group::{ActionGroup, Gate, Repeat};
pub use pass::{FnPass, Pass, PassCtx, PassOutcome, fn_pass, run_pass_logged};
pub use pipeline::{Pipeline, group};
pub use pool::{ActionPool, Rule, apply_rules_to_stmts};
