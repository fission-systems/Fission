//! Linear-body and terminator types shared by free-function structuring.
//!
//! These were historically `pub(crate)` inside `fission-pcode` builder support.
//! Owning them here lets residual `try_lower_*` free functions take
//! [`crate::host::StructuringHost`] without depending on PreviewBuilder.

use fission_midend_core::ir::{
    DispatcherProofUnit, MlilPreviewOptions, ResourceLimits, UnsupportedControlEvidence,
};
use fission_midend_prehir::PreHirExpr;
use std::cell::Cell;
use std::rc::Rc;
use std::time::Instant;

/// Soft subcall limit for condition-recovery lowering attempts. Wall-clock
/// (`CONDITION_RECOVERY_BUDGET_MS`) was dropped in favor of this deterministic
/// proxy: a wall-clock trip point made how far a single `try_lower_if`/
/// `try_lower_while` attempt got -- and therefore decompiled output --
/// depend on machine speed / load (see PROJECT.md).
pub const CONDITION_RECOVERY_SUBCALL_LIMIT: usize =
    ResourceLimits::DEFAULT.condition_recovery_subcalls;
/// Soft budget for the whole structuring attempt (all `IfLoweringBudget`
/// instances and the direct loop-lowering checkpoints in `loops.rs`
/// combined), in checkpoint calls since `CollapseDriver::run` began.
/// Deterministic replacement for the same 5000ms wall-clock ceiling this
/// budget used to share with the per-instance one above.
pub const STRUCTURING_TOTAL_WORK_BUDGET: u64 = ResourceLimits::DEFAULT.structuring_total_work;

/// How a linear region is expected to leave its body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LinearExit {
    Join(usize),
    Return,
    End,
}

/// Block terminator after host-side p-code lowering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoweredTerminator {
    Fallthrough(Option<u64>),
    Goto(u64),
    Cond {
        cond: PreHirExpr,
        true_target: u64,
        false_target: Option<u64>,
    },
    Switch {
        expr: PreHirExpr,
        targets: Vec<u64>,
        default_target: Option<u64>,
        /// Offset for ordinal case indices when the selector was adjusted
        /// (e.g. `sel = orig - min_val`). Zero when unknown/unrecovered.
        min_val: i64,
        proof: Option<DispatcherProofUnit>,
    },
    Return(Option<PreHirExpr>),
    Unsupported {
        evidence: UnsupportedControlEvidence,
        target_expr: Option<PreHirExpr>,
    },
}

/// Cache key for linear body lowering outcomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LinearBodyCacheKey {
    pub start_idx: usize,
    pub exit: LinearExit,
    pub region_recovery: bool,
}

/// Cache key for conditional-tail lowering outcomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConditionalTailKey {
    pub true_idx: usize,
    pub false_idx: usize,
    pub exit: LinearExit,
    pub region_recovery: bool,
}

/// Soft subcall / total-work budget for `try_lower_*` condition recovery.
#[derive(Debug)]
pub struct IfLoweringBudget {
    pub enabled: bool,
    pub start: Instant,
    pub subcalls: usize,
    pub tripped: bool,
    pub idx: usize,
    pub block_addr: u64,
    pub label: &'static str,
    pub structuring_total_work: Rc<Cell<u64>>,
    pub limits: ResourceLimits,
}

impl IfLoweringBudget {
    pub fn new(
        options: &MlilPreviewOptions,
        idx: usize,
        block_addr: u64,
        label: &'static str,
        structuring_total_work: Rc<Cell<u64>>,
    ) -> Self {
        Self {
            enabled: true,
            start: Instant::now(),
            subcalls: 0,
            tripped: false,
            idx,
            block_addr,
            label,
            structuring_total_work,
            limits: options.resource_limits,
        }
    }

    /// Returns `true` when the budget has tripped (caller should abort).
    pub fn checkpoint(&mut self, stage: &str) -> bool {
        if self.tripped || !self.enabled {
            return self.tripped;
        }
        self.subcalls += 1;

        let total_work = self.structuring_total_work.get() + 1;
        self.structuring_total_work.set(total_work);
        if total_work > self.limits.structuring_total_work {
            self.tripped = true;
            if structuring_diag_enabled() {
                eprintln!(
                    "[DIAG] TOTAL structuring budget exceeded: idx={} block=0x{:x} stage={} total_work={}",
                    self.idx, self.block_addr, stage, total_work
                );
            }
            return true;
        }

        if self.subcalls > self.limits.condition_recovery_subcalls {
            self.tripped = true;
            if structuring_diag_enabled() {
                eprintln!(
                    "[DIAG] {} budget_exceeded: idx={} block=0x{:x} stage={} elapsed={:.3}s subcalls={}",
                    self.label,
                    self.idx,
                    self.block_addr,
                    stage,
                    self.start.elapsed().as_secs_f64(),
                    self.subcalls
                );
            }
        }
        self.tripped
    }
}

/// Env gate for structuring diagnostic traces (`FISSION_PREVIEW_DIAG`).
///
/// Checked from ~20 call sites across the structuring subsystem, several on
/// hot per-region/per-block paths (`loops.rs`, `sese_driver.rs`,
/// `conditionals/*.rs`). Cached with `OnceLock` so it's one syscall per
/// process instead of one per call site visited.
pub fn structuring_diag_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("FISSION_PREVIEW_DIAG").is_some())
}

// ── Linear body recovery outcomes ──────────────────────────────────────────

pub const MAX_LINEAR_STRUCTURING_DEPTH: usize = 256;
pub const MAX_REGION_TARGET_CANONICALIZE_STEPS: usize = 4;
pub const MAX_REGION_JOIN_TRAMPOLINE_DISTANCE: usize = 4;
pub const MAX_REGION_SHARED_TAIL_STEPS: usize = 4;
pub const MAX_REGION_FOLLOW_DISCOVERY_STEPS: usize = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinearBodyRejectReason {
    ConditionalTailExitMismatch,
    SuccessorInlineRejected,
    RevisitCycle,
    UnsupportedTerminator,
    TargetIndexMissing,
    ExitMismatch,
    BudgetTripped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinearBodyLoweringOutcome {
    /// `Rc`, not an owned `Vec`: this body is cloned on every cache hit
    /// (`lower_linear_body_cached`) and every cache store -- an `Rc` clone
    /// there is a refcount bump instead of a full recursive `PreHirStmt`
    /// tree clone, which profiling showed was over half of total
    /// structuring CPU time on real, structuring-heavy functions.
    Lowered((Rc<Vec<fission_midend_prehir::PreHirStmt>>, usize)),
    Rejected(LinearBodyRejectReason),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinearBodyCachedOutcome {
    Lowered((Rc<Vec<fission_midend_prehir::PreHirStmt>>, usize)),
    Rejected(LinearBodyRejectReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConditionalTailMismatchSubtype {
    NoCommonFollowInWindow,
    FollowBeyondWindow,
    SideEntryOrExit,
    ComplexArmShape,
    DepthOrBudgetExceeded,
    OneArmBodyLoweringFailed,
    BothArmsBodyLoweringFailed,
    FollowTailLoweringFailed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConditionalTailLoweringResult {
    Lowered((fission_midend_prehir::PreHirStmt, usize)),
    Mismatch(ConditionalTailMismatchSubtype),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NormalizedConditionalTailArm {
    pub canonical_idx: usize,
    pub effective_start_idx: usize,
    pub reaches_join_trivially: bool,
}

#[cfg(test)]
mod resource_limit_tests {
    use super::*;

    fn budget_with(limits: ResourceLimits, total: Rc<Cell<u64>>) -> IfLoweringBudget {
        let mut options = MlilPreviewOptions::default();
        options.resource_limits = limits;
        IfLoweringBudget::new(&options, 0, 0x1000, "test", total)
    }

    #[test]
    fn subcall_limit_comes_from_the_options() {
        let limits = ResourceLimits {
            condition_recovery_subcalls: 3,
            ..ResourceLimits::DEFAULT
        };
        let mut budget = budget_with(limits, Rc::new(Cell::new(0)));
        assert!(!budget.checkpoint("a"));
        assert!(!budget.checkpoint("b"));
        assert!(!budget.checkpoint("c"));
        assert!(
            budget.checkpoint("d"),
            "the fourth call exceeds a limit of 3"
        );
        assert!(budget.checkpoint("e"), "a tripped budget stays tripped");
    }

    #[test]
    fn total_work_limit_is_shared_through_the_counter() {
        let limits = ResourceLimits {
            structuring_total_work: 4,
            ..ResourceLimits::DEFAULT
        };
        let total = Rc::new(Cell::new(0));
        let mut first = budget_with(limits, total.clone());
        let mut second = budget_with(limits, total.clone());
        assert!(!first.checkpoint("a"));
        assert!(!second.checkpoint("b"));
        assert!(!first.checkpoint("c"));
        assert!(!second.checkpoint("d"));
        assert!(
            first.checkpoint("e"),
            "the fifth shared call exceeds a total of 4"
        );
        assert_eq!(total.get(), 5);
    }

    #[test]
    fn default_limits_do_not_trip_on_small_work() {
        let mut budget = budget_with(ResourceLimits::DEFAULT, Rc::new(Cell::new(0)));
        for _ in 0..CONDITION_RECOVERY_SUBCALL_LIMIT {
            assert!(!budget.checkpoint("work"));
        }
        assert!(budget.checkpoint("over"));
    }
}
