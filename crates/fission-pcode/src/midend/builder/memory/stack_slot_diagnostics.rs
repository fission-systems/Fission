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
    SsaCoverBlock, SsaMemoryHighVariableId, SsaMemoryRegion, SsaMemoryStorageKey, SsaOpSite,
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
    /// Two real-corpus false-positive sources were found and fixed:
    /// 1. `MemoryLayout::write_effect`'s unresolved-region fallback was
    ///    conservatively including Stack partitions -- an unrelated write
    ///    through a parameter/heap pointer that couldn't be resolved to any
    ///    region fabricated a phantom "definition" of every promoted stack
    ///    slot. (908 -> 225 real-corpus violations.)
    /// 2. `write_effect`'s `Call` case unconditionally treated every
    ///    function call as writing to every promoted stack slot. Real escape
    ///    analysis (`compute_escaping_stack_storages`) now narrows this to
    ///    slots whose address is provably taken (stored elsewhere, returned,
    ///    or placed in an argument-passing register). (225 -> 12.)
    ///
    /// STILL KNOWN IMPRECISE: the remaining ~12 real-corpus instances traced
    /// to a different cause -- not a false union-find/escape gap, but the
    /// inherent precision limit of a single merged per-block `Cover` range
    /// per `SsaMemoryHighVariable`. A loop-carried phi-chain group's merged
    /// cover can span an entire block even though no *individual* member's
    /// own live range actually overlaps a third, genuinely disjoint write
    /// that happens to sit temporally between two of the group's own
    /// sub-ranges (traced case: `_power`'s accumulator, `math_gcc-m32_O2.exe`
    /// -- a real Store at block-local position 88..100 flagged against a
    /// 5-member phi-chain group whose *merged* cover for that block is
    /// 0..131, even though no single member is live across 88..100).
    /// Ghidra's own block-granular `Cover`/`Merge` has the same class of
    /// approximation; resolving it here would need per-value point-in-time
    /// liveness rather than per-block ranges -- a larger, separate
    /// undertaking. Do **not** treat this scan's raw count as a bug count,
    /// and do not gate any materialize decision on it, until that gap (or a
    /// documented acceptance of its residual imprecision) is addressed.
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

    /// Resolve the `SsaMemoryValueId` the *current* access (per
    /// `self.current_lowering_site`), if it can be resolved with confidence.
    /// The access's size comes from the op itself (the `Load`'s output
    /// varnode, or the `Store`'s value-operand varnode), not from the
    /// caller, so this can never disagree with what the SSA side actually
    /// modeled. Conservative: returns `None` (not a guess) whenever the
    /// current op isn't a plain `Load`/`Store`, the SSA side has no
    /// matching piece, or the resolved value's own storage offset doesn't
    /// exactly match `offset` -- callers must treat `None` as "can't prove
    /// anything," never as a stand-in for a specific answer.
    fn resolve_current_stack_memory_value(
        &self,
        offset: i64,
    ) -> Option<fission_midend_core::ir::SsaMemoryValueId> {
        let site = self.current_lowering_site?;
        let op = self.pcode.blocks.get(site.block_idx)?.ops.get(site.op_idx)?;
        let ssa_site = SsaOpSite {
            block: u32::try_from(site.block_idx).ok()?,
            op: u32::try_from(site.op_idx).ok()?,
        };
        let pieces = match op.opcode {
            PcodeOpcode::Load => self.scalar_ssa.memory_operation_inputs.get(&ssa_site),
            PcodeOpcode::Store => self.scalar_ssa.memory_operation_outputs.get(&ssa_site),
            _ => None,
        }?;
        let piece = pieces.iter().find(|piece| piece.byte_offset == 0)?;
        let value = self.scalar_ssa.memory_value(piece.value)?;
        if value.storage.region != SsaMemoryRegion::Stack || value.storage.offset != i128::from(offset)
        {
            return None;
        }
        Some(piece.value)
    }

    /// Live counterpart of `scan_stack_slot_cover_violations`'s core test,
    /// gating a `self.locals` name-reuse decision before it happens. Uses
    /// per-value covers (`memory_value_covers`), not a group's merged
    /// cover, specifically to avoid the over-approximation traced in
    /// `_power` (Section 8/9 of the memory-SSA proposal doc) -- a
    /// loop-carried group's *merged* cover can span a whole block even
    /// though no individual member is live across a given sub-range within
    /// it. `true` only when the current access's value provably belongs to
    /// a different `SsaMemoryHighVariable` from *every* group already
    /// recorded as owning this offset's name, AND that value's own cover
    /// interferes with at least one owning group's member's own cover.
    /// `false` whenever the current access can't be resolved, or hasn't
    /// been claimed by anyone yet, or already matches a recorded owner --
    /// this only ever blocks a reuse the SSA model can positively disprove.
    pub(crate) fn cover_proves_stack_slot_reuse_unsafe(&self, offset: i64) -> bool {
        let Some(value_id) = self.resolve_current_stack_memory_value(offset) else {
            return false;
        };
        let Some(&new_group) = self
            .scalar_ssa
            .value_memory_high_variables
            .get(value_id.0 as usize)
        else {
            return false;
        };
        let Some(owners) = self.stack_slot_memory_owners.get(&offset) else {
            return false;
        };
        if owners.contains(&new_group) {
            return false;
        }
        let Some(new_cover) = self.scalar_ssa.memory_value_covers.get(value_id.0 as usize) else {
            return false;
        };
        owners.iter().any(|owner_group| {
            let Some(owner_hv) = self
                .scalar_ssa
                .memory_high_variables
                .get(owner_group.0 as usize)
            else {
                return false;
            };
            owner_hv.members.iter().any(|&member| {
                self.scalar_ssa
                    .memory_value_covers
                    .get(member.0 as usize)
                    .is_some_and(|owner_cover| covers_interfere(new_cover, owner_cover))
            })
        })
    }

    /// Record that the current access's `SsaMemoryHighVariable` group has
    /// (successfully) claimed `offset`'s canonical name -- called after a
    /// `self.locals` bind that `cover_proves_stack_slot_reuse_unsafe`
    /// (called beforehand) did not reject. A no-op when the current access
    /// can't be resolved, matching this module's permissive-on-uncertainty
    /// posture: an unresolved access neither claims nor blocks anything.
    pub(crate) fn record_stack_slot_memory_owner(&mut self, offset: i64) {
        let Some(value_id) = self.resolve_current_stack_memory_value(offset) else {
            return;
        };
        let Some(&group) = self
            .scalar_ssa
            .value_memory_high_variables
            .get(value_id.0 as usize)
        else {
            return;
        };
        let owners = self.stack_slot_memory_owners.entry(offset).or_default();
        if !owners.contains(&group) {
            owners.push(group);
        }
    }
}

fn covers_interfere(left: &[SsaCoverBlock], right: &[SsaCoverBlock]) -> bool {
    left.iter().any(|left| {
        right.iter().any(|right| {
            left.block == right.block
                && left.start < right.end_exclusive
                && right.start < left.end_exclusive
        })
    })
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pcode::PcodeBasicBlock;
    use fission_midend_core::ir::{
        SsaMemoryHighVariable, SsaMemoryValue, SsaMemoryValueId, SsaValueDefinition,
    };

    const OFFSET: i64 = -8;

    fn base_pcode() -> PcodeFunction {
        PcodeFunction {
            blocks: vec![
                PcodeBasicBlock {
                    index: 0,
                    start_address: 0x1000,
                    successors: vec![1],
                    ops: vec![],
                },
                PcodeBasicBlock {
                    index: 1,
                    start_address: 0x1010,
                    successors: vec![],
                    ops: vec![PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Store,
                        address: 0x1010,
                        output: None,
                        inputs: vec![
                            Varnode::constant(3, 8),
                            Varnode {
                                space_id: UNIQUE_SPACE_ID,
                                offset: 0,
                                size: 8,
                                is_constant: false,
                                constant_val: 0,
                            },
                            Varnode::constant(2, 4),
                        ],
                        asm_mnemonic: None,
                    }],
                },
            ],
        }
    }

    /// Two `SsaMemoryHighVariable` groups (A: member value 0, B: member
    /// value 1), both storage-keyed to `OFFSET`, with *interfering*
    /// individual covers in block 1 (A spans the whole block, B is a
    /// narrower overlapping sub-range) -- the exact shape a genuine
    /// stack-slot-reuse-for-a-different-variable bug would produce. The
    /// current access (`SsaOpSite{block:1, op:0}`, a `Store`) writes value 1
    /// (group B).
    fn builder_with_interfering_groups(owner: SsaMemoryHighVariableId) -> PreviewBuilder<'static> {
        let pcode: &'static PcodeFunction = Box::leak(Box::new(base_pcode()));
        let options: &'static MlilPreviewOptions =
            Box::leak(Box::new(MlilPreviewOptions::default()));
        let mut builder = PreviewBuilder::new(pcode, options, None);

        let storage = SsaMemoryStorageKey {
            region: SsaMemoryRegion::Stack,
            space_id: 3,
            offset: i128::from(OFFSET),
            size: 4,
        };
        builder.scalar_ssa.memory_values = vec![
            SsaMemoryValue {
                id: SsaMemoryValueId(0),
                storage,
                definition: SsaValueDefinition::Input,
            },
            SsaMemoryValue {
                id: SsaMemoryValueId(1),
                storage,
                definition: SsaValueDefinition::Operation(SsaOpSite { block: 1, op: 0 }),
            },
        ];
        builder.scalar_ssa.value_memory_high_variables =
            vec![SsaMemoryHighVariableId(0), SsaMemoryHighVariableId(1)];
        builder.scalar_ssa.memory_high_variables = vec![
            SsaMemoryHighVariable {
                id: SsaMemoryHighVariableId(0),
                members: vec![SsaMemoryValueId(0)],
                cover: vec![SsaCoverBlock {
                    block: 1,
                    start: 0,
                    end_exclusive: 10,
                }],
            },
            SsaMemoryHighVariable {
                id: SsaMemoryHighVariableId(1),
                members: vec![SsaMemoryValueId(1)],
                cover: vec![SsaCoverBlock {
                    block: 1,
                    start: 4,
                    end_exclusive: 6,
                }],
            },
        ];
        builder.scalar_ssa.memory_value_covers = vec![
            vec![SsaCoverBlock {
                block: 1,
                start: 0,
                end_exclusive: 10,
            }],
            vec![SsaCoverBlock {
                block: 1,
                start: 4,
                end_exclusive: 6,
            }],
        ];
        builder.scalar_ssa.memory_operation_outputs.insert(
            SsaOpSite { block: 1, op: 0 },
            vec![fission_midend_core::ir::SsaMemoryAccessPiece {
                byte_offset: 0,
                value: SsaMemoryValueId(1),
            }],
        );
        builder.current_lowering_site = Some(LoweringSite {
            block_idx: 1,
            op_idx: 0,
        });
        builder
            .stack_slot_memory_owners
            .insert(OFFSET, vec![owner]);
        builder
    }

    #[test]
    fn different_interfering_group_is_unsafe() {
        let builder = builder_with_interfering_groups(SsaMemoryHighVariableId(0));
        assert!(builder.cover_proves_stack_slot_reuse_unsafe(OFFSET));
    }

    #[test]
    fn same_group_already_owns_name_is_safe() {
        let builder = builder_with_interfering_groups(SsaMemoryHighVariableId(1));
        assert!(!builder.cover_proves_stack_slot_reuse_unsafe(OFFSET));
    }

    #[test]
    fn no_recorded_owner_yet_is_safe() {
        let mut builder = builder_with_interfering_groups(SsaMemoryHighVariableId(0));
        builder.stack_slot_memory_owners.remove(&OFFSET);
        assert!(!builder.cover_proves_stack_slot_reuse_unsafe(OFFSET));
    }

    #[test]
    fn different_non_interfering_group_is_safe() {
        let mut builder = builder_with_interfering_groups(SsaMemoryHighVariableId(0));
        // Move group A's cover away from group B's -- no more overlap.
        builder.scalar_ssa.memory_high_variables[0].cover = vec![SsaCoverBlock {
            block: 1,
            start: 6,
            end_exclusive: 10,
        }];
        builder.scalar_ssa.memory_value_covers[0] = vec![SsaCoverBlock {
            block: 1,
            start: 6,
            end_exclusive: 10,
        }];
        assert!(!builder.cover_proves_stack_slot_reuse_unsafe(OFFSET));
    }

    #[test]
    fn record_stack_slot_memory_owner_adds_current_group() {
        let mut builder = builder_with_interfering_groups(SsaMemoryHighVariableId(0));
        builder.stack_slot_memory_owners.remove(&OFFSET);
        builder.record_stack_slot_memory_owner(OFFSET);
        assert_eq!(
            builder.stack_slot_memory_owners.get(&OFFSET),
            Some(&vec![SsaMemoryHighVariableId(1)])
        );
    }
}
