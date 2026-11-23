//! Compact instruction facts shared between the linear sweep and coverage walk.

use fission_sleigh::runtime::{DecodedFlowKind, DecodedInstruction, DecodedReferenceKind};
use rustc_hash::FxHashMap;

pub(super) const COVERAGE_WINDOW_BYTES: usize = 15;

#[derive(Clone, Copy)]
pub(super) enum CoverageFlow {
    Fallthrough,
    Stop,
    Call(u64),
    ConditionalJump(u64),
}

#[derive(Clone, Copy)]
pub(super) struct CoverageInstruction {
    pub(super) flow: CoverageFlow,
    pub(super) length: u64,
}

impl CoverageInstruction {
    pub(super) fn from_decoded(inst: &DecodedInstruction) -> Option<Self> {
        if inst.mnemonic.is_empty()
            || inst.mnemonic.eq_ignore_ascii_case("invalid")
            || !inst.mnemonic.is_ascii() && inst.mnemonic.to_lowercase() == "invalid"
        {
            return None;
        }
        let target = |kind| {
            inst.direct_target.or_else(|| {
                inst.references.iter().find_map(|reference| {
                    (reference.kind == kind
                        || reference.kind == DecodedReferenceKind::RipRelativeAddress)
                        .then_some(reference.target)
                })
            })
        };
        let flow = match inst.flow_kind {
            DecodedFlowKind::Return
            | DecodedFlowKind::Jump
            | DecodedFlowKind::Interrupt
            | DecodedFlowKind::Syscall => CoverageFlow::Stop,
            DecodedFlowKind::Call => target(DecodedReferenceKind::CallTarget)
                .map_or(CoverageFlow::Fallthrough, CoverageFlow::Call),
            DecodedFlowKind::ConditionalJump => target(DecodedReferenceKind::BranchTarget)
                .map_or(CoverageFlow::Fallthrough, CoverageFlow::ConditionalJump),
            DecodedFlowKind::None => CoverageFlow::Fallthrough,
        };
        Some(Self {
            flow,
            length: inst.length as u64,
        })
    }
}

/// Both scans read the same binary allocation with the same frontend. Key on
/// both the byte position and the virtual address so mapped section aliases
/// cannot inherit instructions decoded at a different PC or file offset.
pub(super) type CoverageDecodes = FxHashMap<(u64, usize), (usize, CoverageInstruction)>;
