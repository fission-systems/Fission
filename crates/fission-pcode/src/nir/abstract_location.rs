//! Abstract storage locations for stack slots (architecture-neutral keys).
//!
//! Lifting attaches stack variables to integer offsets in the builder’s frame model.
//! [`AbstractStackSlot`] is the canonical key for “same stack region” decisions used by
//! alias checks and type hints—without encoding a specific CPU register as the frame base
//! (that remains in the lifter / [`crate::nir::types::NirBindingOrigin`]).
//!
//! Parameter slots use [`ParamSlotIndex`] keyed by [`fission_core::CallingConvention`]
//! ordering via [`CallingConvention::param_offsets`](fission_core::CallingConvention::param_offsets).

use crate::nir::types::NirBindingOrigin;

/// Byte-identified region of the stack frame at a fixed offset from the builder’s model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AbstractStackSlot(pub i64);

impl AbstractStackSlot {
    /// Stack-backed binding: direct stack offset or value derived from a stack address.
    pub fn from_binding_origin(origin: Option<NirBindingOrigin>) -> Option<Self> {
        match origin {
            Some(NirBindingOrigin::StackOffset(o))
            | Some(NirBindingOrigin::HomeSlot(o))
            | Some(NirBindingOrigin::OutgoingArgSlot(o))
            | Some(NirBindingOrigin::DerivedFromStackOffset(o)) => Some(AbstractStackSlot(o)),
            _ => None,
        }
    }

    /// Half-open interval `[self, self + size)` overlaps `[other, other + other_size)`.
    #[inline]
    pub fn intervals_overlap(self, size: u64, other: AbstractStackSlot, other_size: u64) -> bool {
        let s0 = self.0;
        let e0 = s0.saturating_add_unsigned(size);
        let s1 = other.0;
        let e1 = s1.saturating_add_unsigned(other_size);
        s0 < e1 && s1 < e0
    }
}

/// Ordinal parameter slot (0-based), aligned with [`CallingConvention::param_offsets`](fission_core::CallingConvention::param_offsets).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ParamSlotIndex(pub usize);

impl ParamSlotIndex {
    pub fn from_binding_origin(origin: Option<NirBindingOrigin>) -> Option<Self> {
        match origin {
            Some(NirBindingOrigin::ParamIndex(i)) => Some(ParamSlotIndex(i)),
            _ => None,
        }
    }
}
