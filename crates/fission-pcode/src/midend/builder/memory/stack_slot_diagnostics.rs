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
use fission_midend_core::ir::{
    SsaMemoryHighVariableId, SsaMemoryRegion, SsaMemoryStorageKey,
};

/// One `self.locals` offset whose bound SSA memory accesses belong to two or
/// more distinct `SsaMemoryHighVariable`s (memory-SSA "variables" -- congruent
/// only via `memory_phis`, see `build_memory_out_of_ssa_facts`) with
/// interfering covers: real evidence the compiler reused this stack offset
/// for two logically different variables with disjoint lifetimes, not just
/// many writes to the same one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StackSlotCoverViolation {
    pub(crate) name: String,
    pub(crate) offset: i64,
    pub(crate) high_a: SsaMemoryHighVariableId,
    pub(crate) high_b: SsaMemoryHighVariableId,
}

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

    /// Diagnostic-only: see module docs. Pure/read-only, `FISSION_PREVIEW_DIAG`-gated caller.
    /// Unlike `scan_stack_slot_size_ambiguities` (a crude same-offset/
    /// different-size check), this uses the memory Cover/HighVariable data
    /// to distinguish "one variable, many writes" (common: 40+ SSA versions
    /// of one local is normal in a single O0 function) from "two different
    /// variables reusing one offset with disjoint lifetimes" -- the latter
    /// is what `self.locals`' flat, identity-blind naming cannot detect.
    ///
    /// KNOWN IMPRECISE as of this writing: real-corpus tracing (a single
    /// `stack_slot_cover_violation` case, `matrix_multiply`'s innermost loop
    /// counter) found the flagged pair was a false positive -- the rendered
    /// pseudocode's loop counter is unambiguous, but one short-lived memory
    /// value at the same offset wasn't transitively unioned into the loop
    /// variable's phi-chain group (`build_memory_out_of_ssa_facts`'s
    /// forced-union-only approach under-connects some real phi relationships
    /// for deeply nested loops -- root cause not yet isolated). Do **not**
    /// treat this scan's raw count as a bug count, and do not gate any
    /// materialize decision on it, until that gap is understood and fixed.
    pub(crate) fn scan_stack_slot_cover_violations(&self) -> Vec<StackSlotCoverViolation> {
        let mut highs_by_offset: std::collections::BTreeMap<i64, Vec<SsaMemoryHighVariableId>> =
            std::collections::BTreeMap::new();
        for value in &self.scalar_ssa.memory_values {
            if value.storage.region != SsaMemoryRegion::Stack {
                continue;
            }
            let Ok(offset) = i64::try_from(value.storage.offset) else {
                continue;
            };
            let Some(&high) = self
                .scalar_ssa
                .value_memory_high_variables
                .get(value.id.0 as usize)
            else {
                continue;
            };
            let entries = highs_by_offset.entry(offset).or_default();
            if !entries.contains(&high) {
                entries.push(high);
            }
        }
        let mut violations = Vec::new();
        for (offset, slot) in &self.locals {
            let Some(highs) = highs_by_offset.get(offset) else {
                continue;
            };
            for i in 0..highs.len() {
                for j in (i + 1)..highs.len() {
                    if memory_high_variables_interfere(&self.scalar_ssa, highs[i], highs[j]) {
                        violations.push(StackSlotCoverViolation {
                            name: slot.name.clone(),
                            offset: *offset,
                            high_a: highs[i],
                            high_b: highs[j],
                        });
                    }
                }
            }
        }
        violations
    }
}

fn memory_high_variables_interfere(
    scalar_ssa: &NirScalarSsa,
    a: SsaMemoryHighVariableId,
    b: SsaMemoryHighVariableId,
) -> bool {
    let Some(high_a) = scalar_ssa.memory_high_variables.get(a.0 as usize) else {
        return false;
    };
    let Some(high_b) = scalar_ssa.memory_high_variables.get(b.0 as usize) else {
        return false;
    };
    high_a.cover.iter().any(|left| {
        high_b.cover.iter().any(|right| {
            left.block == right.block
                && left.start < right.end_exclusive
                && right.start < left.end_exclusive
        })
    })
}
