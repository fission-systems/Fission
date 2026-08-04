//! Diagnostic-only cross-check between `self.locals`' flat, size-blind
//! `BTreeMap<i64, StackSlot>` stack-slot naming and the SSA memory-promotion
//! data (`NirScalarSsa.memory_values`, landed
//! `docs/proposals/2026-07-26-heritage-memory-promotion-and-cover-coalescing.md`,
//! still unconsumed by any production stack-slot decision as of this
//! writing). `self.locals` keys purely by byte offset (`resolve_stack_address`'s
//! `rbp_frame_bias`-normalized, RSP-at-entry-relative coordinate -- the same
//! origin `SsaMemoryStorageKey`'s Stack-region offset uses, so the two are
//! directly comparable without further bridging) with no size field at all,
//! so two genuinely different stack variables that a compiler happens to
//! place at the same offset at different points in the function (classic
//! stack-slot coloring/reuse) would silently share one display name.
//!
//! This module changes no materialize output. It exists to measure, on real
//! corpus functions, whether that theoretical gap has real instances: does
//! any one `self.locals` offset cover 2+ distinct SSA memory value sizes.
use super::super::*;
use fission_midend_core::ir::{SsaMemoryRegion, SsaMemoryStorageKey};

/// One `self.locals` offset whose bound SSA memory accesses span more than
/// one distinct size -- a `self.locals` name might be silently covering two
/// different logical variables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StackSlotSizeAmbiguity {
    pub(crate) name: String,
    pub(crate) offset: i64,
    pub(crate) sizes: Vec<u32>,
}

impl<'a> PreviewBuilder<'a> {
    /// Diagnostic-only: see module docs. Pure/read-only, `FISSION_PREVIEW_DIAG`-gated caller.
    pub(crate) fn scan_stack_slot_size_ambiguities(&self) -> Vec<StackSlotSizeAmbiguity> {
        let mut sizes_by_offset: std::collections::BTreeMap<i64, Vec<u32>> =
            std::collections::BTreeMap::new();
        for value in &self.scalar_ssa.memory_values {
            let SsaMemoryStorageKey {
                region,
                offset,
                size,
                ..
            } = value.storage;
            if region != SsaMemoryRegion::Stack {
                continue;
            }
            let Ok(offset) = i64::try_from(offset) else {
                continue;
            };
            sizes_by_offset.entry(offset).or_default().push(size);
        }
        let mut violations = Vec::new();
        for (offset, slot) in &self.locals {
            let Some(sizes) = sizes_by_offset.get(offset) else {
                continue;
            };
            let mut distinct: Vec<u32> = sizes.clone();
            distinct.sort_unstable();
            distinct.dedup();
            if distinct.len() > 1 {
                violations.push(StackSlotSizeAmbiguity {
                    name: slot.name.clone(),
                    offset: *offset,
                    sizes: distinct,
                });
            }
        }
        violations
    }
}
