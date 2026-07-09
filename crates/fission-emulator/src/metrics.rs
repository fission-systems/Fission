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
    /// Decode / fetch failures.
    pub decode_errors: u64,
    /// PageFault / memory errors.
    pub memory_faults: u64,
    /// Exit reason (human string).
    pub exit_reason: Option<String>,
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

    /// One-line summary for logs.
    pub fn summary_line(&self) -> String {
        let top = self
            .top_unimplemented(5)
            .into_iter()
            .map(|(k, c)| format!("{k}={c}"))
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "inst={} tbs_compiled={} cache_hits={} hard_chain={} soft_chain={} decode_err={} mem_fault={} unimplemented=[{}] exit={:?}",
            self.instructions,
            self.tbs_compiled,
            self.tbs_cache_hits,
            self.hard_chains,
            self.soft_chains,
            self.decode_errors,
            self.memory_faults,
            top,
            self.exit_reason
        )
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
