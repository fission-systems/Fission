//! Runtime telemetry for emulator quality and coverage.

use fission_pcode::ir::PcodeOpcode;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Counters collected during a sandbox / emulation run.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct EmulatorMetrics {
    /// Guest instructions retired (same as `Emulator::inst_count` at end of run).
    pub instructions: u64,
    /// Translation blocks compiled.
    pub tbs_compiled: u64,
    /// JIT cache hits (entry into already-compiled TB).
    pub tbs_cache_hits: u64,
    /// Hard-chain transfers (when chain table had a host target).
    pub hard_chains: u64,
    /// Soft-chain transfers.
    pub soft_chains: u64,
    /// CallOther dispatches by userop name.
    pub userops: BTreeMap<String, u64>,
    /// Linux syscalls by number.
    pub syscalls: BTreeMap<u64, u64>,
    /// Unimplemented P-Code opcodes observed at compile time (no-op lowered).
    pub unimplemented_opcodes: BTreeMap<String, u64>,
    /// Missing libc / Win32 HLE procedures (name → count). Fake-success returns 0.
    #[serde(default)]
    pub hle_misses: BTreeMap<String, u64>,
    /// Unimplemented Linux syscall numbers (fake RAX=0 path).
    #[serde(default)]
    pub unknown_syscalls: BTreeMap<u64, u64>,
    /// Decode / fetch failures.
    pub decode_errors: u64,
    /// PageFault / memory errors.
    pub memory_faults: u64,
    /// Exit reason (human string).
    pub exit_reason: Option<String>,
    /// Guest PC when the run loop last stopped (budget / halt / gate).
    #[serde(default)]
    pub stop_pc: u64,
    /// Persistent register-cache hits (MachineState::reg_cache).
    #[serde(default)]
    pub reg_cache_hits: u64,
    /// Persistent register-cache misses.
    #[serde(default)]
    pub reg_cache_misses: u64,
}

impl EmulatorMetrics {
    pub fn note_unimplemented(&mut self, op: PcodeOpcode) {
        *self
            .unimplemented_opcodes
            .entry(format!("{op:?}"))
            .or_insert(0) += 1;
    }

    pub fn note_userop(&mut self, name: &str) {
        *self.userops.entry(name.to_string()).or_insert(0) += 1;
    }

    pub fn note_syscall(&mut self, num: u64) {
        *self.syscalls.entry(num).or_insert(0) += 1;
    }

    pub fn note_hle_miss(&mut self, name: &str) {
        *self.hle_misses.entry(name.to_string()).or_insert(0) += 1;
    }

    pub fn note_unknown_syscall(&mut self, num: u64) {
        *self.unknown_syscalls.entry(num).or_insert(0) += 1;
    }

    /// Top-N unimplemented opcodes by count (descending).
    pub fn top_unimplemented(&self, n: usize) -> Vec<(String, u64)> {
        let mut v: Vec<_> = self
            .unimplemented_opcodes
            .iter()
            .map(|(k, c)| (k.clone(), *c))
            .collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        v.truncate(n);
        v
    }

    pub fn hle_miss_total(&self) -> u64 {
        self.hle_misses.values().sum()
    }

    pub fn unknown_syscall_total(&self) -> u64 {
        self.unknown_syscalls.values().sum()
    }

    /// One-line summary for logs.
    pub fn summary_line(&self) -> String {
        let top = self
            .top_unimplemented(5)
            .into_iter()
            .map(|(k, c)| format!("{k}={c}"))
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "inst={} tbs_compiled={} cache_hits={} hard_chain={} soft_chain={} decode_err={} mem_fault={} unimplemented=[{}] hle_miss={} unk_sys={} exit={:?}",
            self.instructions,
            self.tbs_compiled,
            self.tbs_cache_hits,
            self.hard_chains,
            self.soft_chains,
            self.decode_errors,
            self.memory_faults,
            top,
            self.hle_miss_total(),
            self.unknown_syscall_total(),
            self.exit_reason
        )
    }

    /// Total count of unimplemented-opcode events observed at compile time.
    pub fn unimplemented_total(&self) -> u64 {
        self.unimplemented_opcodes.values().sum()
    }

    /// Distinct unimplemented opcode kinds.
    pub fn unimplemented_kinds(&self) -> usize {
        self.unimplemented_opcodes.len()
    }

    /// Budget gate: fail if too many unimplemented events or kinds.
    ///
    /// Returns `Ok(())` when within budget, else an error with a short report.
    pub fn check_unimplemented_budget(
        &self,
        max_events: u64,
        max_kinds: usize,
    ) -> Result<(), String> {
        let events = self.unimplemented_total();
        let kinds = self.unimplemented_kinds();
        if events <= max_events && kinds <= max_kinds {
            return Ok(());
        }
        let top = self
            .top_unimplemented(8)
            .into_iter()
            .map(|(k, c)| format!("{k}={c}"))
            .collect::<Vec<_>>()
            .join(", ");
        Err(format!(
            "unimplemented opcode budget exceeded: events={events} (max {max_events}), kinds={kinds} (max {max_kinds}); top=[{top}]"
        ))
    }

    /// Budget gate for fake-success HLE / unknown syscalls.
    pub fn check_hle_budget(
        &self,
        max_hle_misses: u64,
        max_unknown_syscalls: u64,
    ) -> Result<(), String> {
        let hle = self.hle_miss_total();
        let unk = self.unknown_syscall_total();
        if hle <= max_hle_misses && unk <= max_unknown_syscalls {
            return Ok(());
        }
        let top_hle: Vec<_> = self
            .hle_misses
            .iter()
            .map(|(k, c)| format!("{k}={c}"))
            .take(8)
            .collect();
        Err(format!(
            "HLE quality budget exceeded: hle_misses={hle} (max {max_hle_misses}), unknown_syscalls={unk} (max {max_unknown_syscalls}); hle_top=[{}]",
            top_hle.join(", ")
        ))
    }

    /// Combined opcode + HLE budget (all must pass).
    pub fn check_quality_budget(
        &self,
        max_unimpl_events: u64,
        max_unimpl_kinds: usize,
        max_hle_misses: u64,
        max_unknown_syscalls: u64,
    ) -> Result<(), String> {
        self.check_unimplemented_budget(max_unimpl_events, max_unimpl_kinds)?;
        self.check_hle_budget(max_hle_misses, max_unknown_syscalls)?;
        Ok(())
    }
}

/// Opcodes the JIT currently lowers (for coverage reports).
pub fn jit_supported_opcodes() -> &'static [PcodeOpcode] {
    use PcodeOpcode::*;
    &[
        Copy, Load, Store, Branch, CBranch, BranchInd, Call, CallInd, CallOther, Return,
        IntEqual, IntNotEqual, IntSLess, IntSLessEqual, IntLess, IntLessEqual, IntZExt, IntSExt,
        IntAdd, IntSub, IntCarry, IntSCarry, IntSBorrow, Int2Comp, IntNegate, IntXor, IntAnd,
        IntOr, IntLeft, IntRight, IntSRight, IntMult, IntDiv, IntSDiv, IntRem, IntSRem, BoolNegate,
        BoolXor, BoolAnd, BoolOr, FloatEqual, FloatNotEqual, FloatLess, FloatLessEqual, FloatNan,
        FloatAdd, FloatDiv, FloatMult, FloatSub, FloatNeg, FloatAbs, FloatSqrt, FloatInt2Float,
        FloatFloat2Float, FloatTrunc, FloatCeil, FloatFloor, FloatRound, MultiEqual, Indirect,
        Piece, SubPiece, Cast, PtrAdd, PtrSub, SegmentOp, PopCount, LzCount, Extract, Insert,
    ]
}

pub fn is_jit_supported(op: PcodeOpcode) -> bool {
    jit_supported_opcodes().contains(&op)
}

/// Budget evaluation embedded in a sandbox metrics JSON report.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BudgetReport {
    pub max_events: u64,
    pub max_kinds: usize,
    pub events: u64,
    pub kinds: usize,
    #[serde(default)]
    pub max_hle_misses: u64,
    #[serde(default)]
    pub hle_misses: u64,
    #[serde(default)]
    pub max_unknown_syscalls: u64,
    #[serde(default)]
    pub unknown_syscalls: u64,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Optional quality budgets for sandbox runs.
///
/// `(max_unimpl_events, max_unimpl_kinds, max_hle_misses, max_unknown_syscalls)`.
/// Use `u64::MAX` / large kinds to disable a sub-gate.
pub type QualityBudget = (u64, usize, u64, u64);

/// Serializable sandbox run summary for CLI `--json` / automation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SandboxMetricsReport {
    pub binary: String,
    pub format: String,
    pub halt_requested: bool,
    pub pc: u64,
    pub metrics: EmulatorMetrics,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub budget: Option<BudgetReport>,
}

impl SandboxMetricsReport {
    pub fn from_run(
        binary: impl Into<String>,
        format: impl Into<String>,
        halt_requested: bool,
        pc: u64,
        metrics: EmulatorMetrics,
        budget: Option<(u64, usize)>,
    ) -> Self {
        // Backward-compatible: opcode-only budget disables HLE gates (MAX).
        let quality = budget.map(|(e, k)| (e, k, u64::MAX, u64::MAX));
        Self::from_run_quality(binary, format, halt_requested, pc, metrics, quality)
    }

    pub fn from_run_quality(
        binary: impl Into<String>,
        format: impl Into<String>,
        halt_requested: bool,
        pc: u64,
        metrics: EmulatorMetrics,
        budget: Option<QualityBudget>,
    ) -> Self {
        let budget = budget.map(|(max_events, max_kinds, max_hle, max_unk)| {
            let events = metrics.unimplemented_total();
            let kinds = metrics.unimplemented_kinds();
            let hle_misses = metrics.hle_miss_total();
            let unknown_syscalls = metrics.unknown_syscall_total();
            match metrics.check_quality_budget(max_events, max_kinds, max_hle, max_unk) {
                Ok(()) => BudgetReport {
                    max_events,
                    max_kinds,
                    events,
                    kinds,
                    max_hle_misses: max_hle,
                    hle_misses,
                    max_unknown_syscalls: max_unk,
                    unknown_syscalls,
                    ok: true,
                    error: None,
                },
                Err(error) => BudgetReport {
                    max_events,
                    max_kinds,
                    events,
                    kinds,
                    max_hle_misses: max_hle,
                    hle_misses,
                    max_unknown_syscalls: max_unk,
                    unknown_syscalls,
                    ok: false,
                    error: Some(error),
                },
            }
        });
        Self {
            binary: binary.into(),
            format: format.into(),
            halt_requested,
            pc,
            metrics,
            budget,
        }
    }

    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn budget_ok(&self) -> bool {
        self.budget.as_ref().map(|b| b.ok).unwrap_or(true)
    }
}
