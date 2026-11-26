//! Compact instruction facts the linear sweep shares with the passes that
//! decode the same code again: candidate validation and the coverage walk.

use fission_sleigh::runtime::{
    DecodedFlowKind, DecodedInstruction, DecodedReferenceKind, PackedContextOverride,
};
use rustc_hash::FxHashMap;

pub(super) const COVERAGE_WINDOW_BYTES: usize = 15;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CoverageFlow {
    Fallthrough,
    Stop,
    Call(u64),
    ConditionalJump(u64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CoverageInstruction {
    pub(super) flow: CoverageFlow,
    pub(super) length: u64,
}

impl CoverageInstruction {
    pub(super) fn from_decoded(inst: &DecodedInstruction) -> Option<Self> {
        if mnemonic_rejected(&inst.mnemonic) {
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

/// The mnemonic test shared by candidate validation and the coverage walk:
/// empty, or `invalid` in any case. No allocation for an ASCII mnemonic.
fn mnemonic_rejected(mnemonic: &str) -> bool {
    mnemonic.is_empty()
        || mnemonic.eq_ignore_ascii_case("invalid")
        || !mnemonic.is_ascii() && mnemonic.to_lowercase() == "invalid"
}

/// What candidate validation and the coverage walk read from one decoded
/// instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SweptInstruction {
    pub(super) length: u32,
    pub(super) mnemonic_rejected: bool,
    pub(super) flow_kind: DecodedFlowKind,
    pub(super) direct_target: Option<u64>,
    /// For a call, the first call-target or RIP-relative reference; for a
    /// conditional jump, the first branch-target or RIP-relative reference.
    reference_target: Option<u64>,
}

impl SweptInstruction {
    pub(super) fn from_decoded(inst: &DecodedInstruction) -> Option<Self> {
        let reference = |kind| {
            inst.references.iter().find_map(|reference| {
                (reference.kind == kind
                    || reference.kind == DecodedReferenceKind::RipRelativeAddress)
                    .then_some(reference.target)
            })
        };
        Some(Self {
            length: u32::try_from(inst.length).ok()?,
            mnemonic_rejected: mnemonic_rejected(&inst.mnemonic),
            flow_kind: inst.flow_kind,
            direct_target: inst.direct_target,
            reference_target: match inst.flow_kind {
                DecodedFlowKind::Call => reference(DecodedReferenceKind::CallTarget),
                DecodedFlowKind::ConditionalJump => reference(DecodedReferenceKind::BranchTarget),
                _ => None,
            },
        })
    }

    /// A call's target: the direct one, else its first call-target or
    /// RIP-relative reference.
    pub(super) fn call_target(&self) -> Option<u64> {
        match self.flow_kind {
            DecodedFlowKind::Call => self.direct_target.or(self.reference_target),
            _ => None,
        }
    }

    /// The coverage walk's view, as [`CoverageInstruction::from_decoded`].
    pub(super) fn coverage(&self) -> Option<CoverageInstruction> {
        if self.mnemonic_rejected {
            return None;
        }
        let target = || self.direct_target.or(self.reference_target);
        let flow = match self.flow_kind {
            DecodedFlowKind::Return
            | DecodedFlowKind::Jump
            | DecodedFlowKind::Interrupt
            | DecodedFlowKind::Syscall => CoverageFlow::Stop,
            DecodedFlowKind::Call => target().map_or(CoverageFlow::Fallthrough, CoverageFlow::Call),
            DecodedFlowKind::ConditionalJump => {
                target().map_or(CoverageFlow::Fallthrough, CoverageFlow::ConditionalJump)
            }
            DecodedFlowKind::None => CoverageFlow::Fallthrough,
        };
        Some(CoverageInstruction {
            flow,
            length: u64::from(self.length),
        })
    }
}

/// Instructions the linear sweep decoded, offered to later passes over the
/// same image.
///
/// All of them read the same binary allocation with the same frontend. A row
/// is keyed on both the virtual address and the byte position, so mapped
/// section aliases cannot inherit instructions decoded at a different PC or
/// file offset, and it holds how many bytes its decode read. It is served only
/// to a decode in the context the sweep decoded in.
pub(super) struct CoverageDecodes {
    context: Option<PackedContextOverride>,
    rows: FxHashMap<(u64, usize), (u8, SweptInstruction)>,
}

impl CoverageDecodes {
    pub(super) fn new(
        context: Option<PackedContextOverride>,
        rows: impl IntoIterator<Item = ((u64, usize), (u8, SweptInstruction))>,
    ) -> Self {
        Self {
            context,
            rows: rows.into_iter().collect(),
        }
    }

    pub(super) fn len(&self) -> usize {
        self.rows.len()
    }

    /// The instruction the sweep decoded at `address` from these very bytes
    /// (`window`, the image from `address` on, read in place), when that
    /// decode started in `context` and read no further than `window` reaches.
    pub(super) fn get(
        &self,
        address: u64,
        window: &[u8],
        context: Option<PackedContextOverride>,
    ) -> Option<&SweptInstruction> {
        if context != self.context {
            return None;
        }
        self.rows
            .get(&(address, window.as_ptr() as usize))
            .filter(|(prefix, _)| usize::from(*prefix) <= window.len())
            .map(|(_, instruction)| instruction)
    }
}
