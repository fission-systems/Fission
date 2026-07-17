//! Type definitions for linear body lowering and conditional-tail recovery.

use super::*;

pub const MAX_LINEAR_STRUCTURING_DEPTH: usize = 256;
pub const MAX_REGION_TARGET_CANONICALIZE_STEPS: usize = 4;
pub const MAX_REGION_JOIN_TRAMPOLINE_DISTANCE: usize = 4;
pub const MAX_REGION_SHARED_TAIL_STEPS: usize = 4;
pub const MAX_REGION_FOLLOW_DISCOVERY_STEPS: usize = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LinearBodyRejectReason {
    ConditionalTailExitMismatch,
    SuccessorInlineRejected,
    RevisitCycle,
    UnsupportedTerminator,
    TargetIndexMissing,
    ExitMismatch,
    BudgetTripped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LinearBodyLoweringOutcome {
    Lowered((Vec<HirStmt>, usize)),
    Rejected(LinearBodyRejectReason),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LinearBodyCachedOutcome {
    Lowered((Vec<HirStmt>, usize)),
    Rejected(LinearBodyRejectReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConditionalTailMismatchSubtype {
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
pub(crate) enum ConditionalTailLoweringResult {
    Lowered((HirStmt, usize)),
    Mismatch(ConditionalTailMismatchSubtype),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NormalizedConditionalTailArm {
    pub canonical_idx: usize,
    pub effective_start_idx: usize,
    pub reaches_join_trivially: bool,
}
