//! Linear-body and terminator types shared by free-function structuring.
//!
//! These were historically `pub(crate)` inside `fission-pcode` builder support.
//! Owning them here lets residual `try_lower_*` free functions take
//! [`crate::host::StructuringHost`] without depending on PreviewBuilder.

use fission_midend_core::ir::{
    DispatcherProofUnit, HirExpr, MlilPreviewOptions, UnsupportedControlEvidence,
};
use std::time::Instant;

/// Soft budget for condition-recovery lowering attempts.
pub const CONDITION_RECOVERY_BUDGET_MS: f64 = 10.0;
/// Soft subcall limit for condition-recovery lowering attempts.
pub const CONDITION_RECOVERY_SUBCALL_LIMIT: usize = 512;

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
        cond: HirExpr,
        true_target: u64,
        false_target: Option<u64>,
    },
    Switch {
        expr: HirExpr,
        targets: Vec<u64>,
        default_target: Option<u64>,
        /// Offset for ordinal case indices when the selector was adjusted
        /// (e.g. `sel = orig - min_val`). Zero when unknown/unrecovered.
        min_val: i64,
        proof: Option<DispatcherProofUnit>,
    },
    Return(Option<HirExpr>),
    Unsupported {
        evidence: UnsupportedControlEvidence,
        target_expr: Option<HirExpr>,
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

/// Soft wall-clock / subcall budget for `try_lower_*` condition recovery.
#[derive(Debug)]
pub struct IfLoweringBudget {
    pub enabled: bool,
    pub start: Instant,
    pub subcalls: usize,
    pub tripped: bool,
    pub idx: usize,
    pub block_addr: u64,
    pub label: &'static str,
    pub structuring_start: Option<Instant>,
}

impl IfLoweringBudget {
    pub fn new(
        _options: &MlilPreviewOptions,
        idx: usize,
        block_addr: u64,
        label: &'static str,
        structuring_start: Option<Instant>,
    ) -> Self {
        Self {
            enabled: true,
            start: Instant::now(),
            subcalls: 0,
            tripped: false,
            idx,
            block_addr,
            label,
            structuring_start,
        }
    }

    /// Returns `true` when the budget has tripped (caller should abort).
    pub fn checkpoint(&mut self, stage: &str) -> bool {
        if self.tripped || !self.enabled {
            return self.tripped;
        }
        self.subcalls += 1;

        if let Some(total_start) = self.structuring_start {
            let total_elapsed_ms = total_start.elapsed().as_secs_f64() * 1000.0;
            if total_elapsed_ms > 5000.0 {
                self.tripped = true;
                if structuring_diag_enabled() {
                    eprintln!(
                        "[DIAG] TOTAL structuring budget exceeded: idx={} block=0x{:x} stage={} total_elapsed={:.3}s",
                        self.idx,
                        self.block_addr,
                        stage,
                        total_start.elapsed().as_secs_f64()
                    );
                }
                return true;
            }
        }

        let elapsed_ms = self.start.elapsed().as_secs_f64() * 1000.0;
        if self.subcalls > CONDITION_RECOVERY_SUBCALL_LIMIT
            || elapsed_ms > CONDITION_RECOVERY_BUDGET_MS
        {
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
pub fn structuring_diag_enabled() -> bool {
    std::env::var_os("FISSION_PREVIEW_DIAG").is_some()
}
