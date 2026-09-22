//! Conservative pointer provenance and memory-effect guard recovery.
//!
//! This module derives bounded RAM/stack ranges and call effects from lifted
//! P-code after scalar SSA has established the value identities.  Memory SSA
//! consumes these guards; scalar SSA owns the orchestration and validation.
#![allow(clippy::too_many_arguments)]

use super::*;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PointerRange {
    pub(super) region: SsaMemoryRegion,
    pub(super) minimum: i128,
    pub(super) maximum: i128,
    pub(super) step: u64,
    pub(super) exact: bool,
}

impl PointerRange {
    fn exact(region: SsaMemoryRegion, offset: i128) -> Self {
        Self {
            region,
            minimum: offset,
            maximum: offset,
            step: 0,
            exact: true,
        }
    }

    fn shifted(self, delta: i64) -> Option<Self> {
        let shift = |value: i128| value.checked_add(i128::from(delta));
        Some(Self {
            minimum: shift(self.minimum)?,
            maximum: shift(self.maximum)?,
            ..self
        })
    }

    fn union(self, other: Self) -> Option<Self> {
        if self.region != other.region {
            return None;
        }
        if self.exact && other.exact {
            let minimum = self.minimum.min(other.minimum);
            let maximum = self.maximum.max(other.maximum);
            Some(Self {
                region: self.region,
                minimum,
                maximum,
                step: u64::try_from(maximum - minimum).unwrap_or(0),
                exact: minimum == maximum,
            })
        } else {
            Some(Self {
                region: self.region,
                minimum: self.minimum.min(other.minimum),
                maximum: self.maximum.max(other.maximum),
                step: 0,
                exact: false,
            })
        }
    }
}

pub(super) fn build_dynamic_guards(
    pcode: &PcodeFunction,
    reachable: &BTreeSet<usize>,
    ssa: &NirScalarSsa,
    options: &MlilPreviewOptions,
    type_context: Option<&PreviewTypeContext>,
) -> BTreeMap<SsaOpSite, SsaDynamicGuard> {
    let mut guards = BTreeMap::new();
    for &block in reachable {
        let Some(pcode_block) = pcode.blocks.get(block) else {
            continue;
        };
        for (op_index, op) in pcode_block.ops.iter().enumerate() {
            let site = SsaOpSite {
                block: block as u32,
                op: op_index as u32,
            };
            if is_call_return_address_scaffold_store(pcode, options, site, op) {
                continue;
            }
            let guard = match op.opcode {
                PcodeOpcode::Load if op.inputs.len() >= 2 => memory_guard(
                    pcode,
                    ssa,
                    options,
                    site,
                    SsaDynamicGuardKind::Load,
                    op.inputs.first().and_then(encoded_space_id),
                    1,
                    op.output.as_ref().map_or(0, |output| output.size),
                ),
                PcodeOpcode::Store if op.inputs.len() >= 2 => {
                    let (space_id, pointer_input) = if op.inputs.len() >= 3 {
                        (op.inputs.first().and_then(encoded_space_id), 1)
                    } else {
                        (None, 0)
                    };
                    let width = op.inputs.last().map_or(0, |value| value.size);
                    memory_guard(
                        pcode,
                        ssa,
                        options,
                        site,
                        SsaDynamicGuardKind::Store,
                        space_id,
                        pointer_input,
                        width,
                    )
                }
                PcodeOpcode::Call | PcodeOpcode::CallInd => {
                    call_guard(op, site, options, type_context)
                }
                _ => continue,
            };
            guards.insert(site, guard);
        }
    }
    guards
}

fn call_guard(
    op: &PcodeOp,
    site: SsaOpSite,
    options: &MlilPreviewOptions,
    type_context: Option<&PreviewTypeContext>,
) -> SsaDynamicGuard {
    let target = resolve_direct_call_target(op, options, type_context);
    let summary = target
        .as_ref()
        .and_then(|target| type_context?.call_effect_summaries.get(target));
    let effect = if summary.is_some_and(|summary| summary.may_call_unknown == Some(false)) {
        let summary = summary.expect("closed summary was checked");
        match (
            summary.reads_memory.unwrap_or(true),
            summary.writes_memory.unwrap_or(true),
        ) {
            (false, false) => SsaMemoryEffect::None,
            (true, false) => SsaMemoryEffect::Read,
            (false, true) => SsaMemoryEffect::Write,
            (true, true) => SsaMemoryEffect::ReadWrite,
        }
    } else {
        SsaMemoryEffect::ReadWrite
    };
    SsaDynamicGuard {
        site,
        kind: SsaDynamicGuardKind::Call,
        effect,
        region: None,
        space_id: None,
        minimum_offset: 0,
        maximum_offset_exclusive: None,
        step: 0,
        precision: SsaGuardRangePrecision::Unknown,
        call_target: target,
        call_effect_source: summary.and_then(|summary| summary.source.clone()),
    }
}

fn resolve_direct_call_target(
    op: &PcodeOp,
    options: &MlilPreviewOptions,
    type_context: Option<&PreviewTypeContext>,
) -> Option<String> {
    if let Some(symbol) = options
        .relocation_names
        .get(&op.address)
        .filter(|symbol| !symbol.is_empty())
    {
        return Some(symbol.clone());
    }
    let address = op
        .inputs
        .first()
        .filter(|target| target.is_constant)?
        .offset;
    resolve_call_target_address(address, type_context)
}

pub(in crate::midend) fn resolve_lifted_direct_call_target(
    op: &PcodeOp,
    options: &MlilPreviewOptions,
    type_context: Option<&PreviewTypeContext>,
) -> Option<String> {
    if let Some(symbol) = options
        .relocation_names
        .get(&op.address)
        .filter(|symbol| !symbol.is_empty())
    {
        return Some(symbol.clone());
    }
    let target = op.inputs.first()?;
    if op.opcode != PcodeOpcode::Call && !target.is_constant {
        return None;
    }
    resolve_call_target_address(target.offset, type_context)
}

fn resolve_call_target_address(
    address: u64,
    type_context: Option<&PreviewTypeContext>,
) -> Option<String> {
    let context = type_context?;
    if context.ambiguous_call_targets.contains(&address) {
        return None;
    }
    context
        .call_target_refs
        .get(&address)
        .or_else(|| context.iat_target_refs.get(&address))
        .map(|target| target.symbol.clone())
        .or_else(|| context.call_targets.get(&address).cloned())
}

fn encoded_space_id(varnode: &Varnode) -> Option<u64> {
    varnode.is_constant.then_some(varnode.offset)
}

fn memory_guard(
    pcode: &PcodeFunction,
    ssa: &NirScalarSsa,
    options: &MlilPreviewOptions,
    site: SsaOpSite,
    kind: SsaDynamicGuardKind,
    space_id: Option<u64>,
    pointer_input: usize,
    width: u32,
) -> SsaDynamicGuard {
    let effect = match kind {
        SsaDynamicGuardKind::Load => SsaMemoryEffect::Read,
        SsaDynamicGuardKind::Store => SsaMemoryEffect::Write,
        SsaDynamicGuardKind::Call => unreachable!("calls do not have pointer ranges"),
    };
    let resolved = if width == 0 || space_id.is_none() {
        None
    } else {
        resolve_pointer_input(
            pcode,
            ssa,
            options,
            site,
            pointer_input,
            &mut BTreeSet::new(),
            64,
        )
    };
    let Some(range) = resolved else {
        return SsaDynamicGuard {
            site,
            kind,
            effect,
            region: None,
            space_id,
            minimum_offset: 0,
            maximum_offset_exclusive: None,
            step: 0,
            precision: SsaGuardRangePrecision::Unknown,
            call_target: None,
            call_effect_source: None,
        };
    };
    let Some(maximum_offset_exclusive) = range.maximum.checked_add(i128::from(width)) else {
        return SsaDynamicGuard {
            site,
            kind,
            effect,
            region: None,
            space_id,
            minimum_offset: 0,
            maximum_offset_exclusive: None,
            step: 0,
            precision: SsaGuardRangePrecision::Unknown,
            call_target: None,
            call_effect_source: None,
        };
    };
    SsaDynamicGuard {
        site,
        kind,
        effect,
        region: Some(range.region),
        space_id,
        minimum_offset: range.minimum,
        maximum_offset_exclusive: Some(maximum_offset_exclusive),
        step: range.step,
        precision: if range.exact {
            SsaGuardRangePrecision::Exact
        } else {
            SsaGuardRangePrecision::Bounded
        },
        call_target: None,
        call_effect_source: None,
    }
}

pub(super) fn resolve_pointer_input(
    pcode: &PcodeFunction,
    ssa: &NirScalarSsa,
    options: &MlilPreviewOptions,
    site: SsaOpSite,
    input_index: usize,
    visiting: &mut BTreeSet<SsaValueId>,
    budget: usize,
) -> Option<PointerRange> {
    if budget == 0 {
        return None;
    }
    let op = pcode
        .blocks
        .get(site.block as usize)?
        .ops
        .get(site.op as usize)?;
    let input = op.inputs.get(input_index)?;
    if input.is_constant {
        return Some(PointerRange::exact(
            SsaMemoryRegion::Ram,
            i128::from(input.offset),
        ));
    }
    let pieces = ssa.operation_inputs.get(&SsaUseSite {
        block: site.block,
        op: site.op,
        input: input_index as u32,
    })?;
    if pieces.len() != 1 {
        return None;
    }
    let value = ssa.value(pieces[0].value)?;
    if value.storage.size != input.size {
        return None;
    }
    resolve_pointer_value(pcode, ssa, options, value.id, visiting, budget - 1)
}

/// A constant whose bit-pattern has been resolved through scalar SSA.
///
/// Keeping the source width is important: `IntZExt` and `IntSExt` have
/// different meanings for a high-bit-set narrow literal even when both are
/// eventually consumed by the same pointer arithmetic operation.
#[derive(Clone, Copy, Debug)]
struct ResolvedConstant {
    bits: u64,
    size: u32,
}

impl ResolvedConstant {
    fn from_literal(varnode: &Varnode) -> Option<Self> {
        if !varnode.is_constant {
            return None;
        }
        Some(Self {
            bits: varnode.offset & size_mask(varnode.size)?,
            size: varnode.size,
        })
    }

    fn zero_extend_to(self, size: u32) -> Option<Self> {
        (size >= self.size).then_some(Self {
            bits: self.bits & size_mask(size)?,
            size,
        })
    }

    fn sign_extend_to(self, size: u32) -> Option<Self> {
        if size < self.size {
            return None;
        }
        Some(Self {
            bits: signed_bits(self.bits, self.size)? as u64 & size_mask(size)?,
            size,
        })
    }

    fn signed_at(self, size: u32) -> Option<i64> {
        signed_bits(self.bits, size)
    }
}

/// Resolve an arithmetic operand to a signed constant even when SLEIGH has
/// materialized the literal through a scalar temporary first. Many ARM
/// prologues lift an immediate as `Copy unique <- const(k)` followed by
/// `IntSub sp, unique`; treating only literal operands as constants makes the
/// later stack pointer chain look unknown even though scalar SSA proves the
/// temporary's definition exactly.
pub(super) fn resolve_constant_input(
    pcode: &PcodeFunction,
    ssa: &NirScalarSsa,
    site: SsaOpSite,
    input_index: usize,
    visiting: &mut BTreeSet<SsaValueId>,
    budget: usize,
) -> Option<i64> {
    if budget == 0 {
        return None;
    }
    let op = pcode
        .blocks
        .get(site.block as usize)?
        .ops
        .get(site.op as usize)?;
    let input = op.inputs.get(input_index)?;
    let resolved = resolve_constant_input_bits(pcode, ssa, site, input_index, visiting, budget)?;
    resolved.signed_at(input.size)
}

fn resolve_constant_input_bits(
    pcode: &PcodeFunction,
    ssa: &NirScalarSsa,
    site: SsaOpSite,
    input_index: usize,
    visiting: &mut BTreeSet<SsaValueId>,
    budget: usize,
) -> Option<ResolvedConstant> {
    if budget == 0 {
        return None;
    }
    let op = pcode
        .blocks
        .get(site.block as usize)?
        .ops
        .get(site.op as usize)?;
    let input = op.inputs.get(input_index)?;
    if let Some(value) = ResolvedConstant::from_literal(input) {
        return Some(value);
    }
    if !is_register_space_id(input.space_id) && !is_unique_space_id(input.space_id) {
        return None;
    }
    let pieces = ssa.operation_inputs.get(&SsaUseSite {
        block: site.block,
        op: site.op,
        input: input_index as u32,
    })?;
    if pieces.len() != 1 {
        return None;
    }
    let value = ssa.value(pieces[0].value)?;
    if value.storage.size != input.size {
        return None;
    }
    resolve_constant_value(pcode, ssa, value.id, visiting, budget - 1)
}

fn resolve_constant_value(
    pcode: &PcodeFunction,
    ssa: &NirScalarSsa,
    value_id: SsaValueId,
    visiting: &mut BTreeSet<SsaValueId>,
    budget: usize,
) -> Option<ResolvedConstant> {
    if budget == 0 || !visiting.insert(value_id) {
        return None;
    }
    let value = ssa.value(value_id)?;
    let resolved = match value.definition {
        SsaValueDefinition::Operation(site) => {
            let op = pcode
                .blocks
                .get(site.block as usize)?
                .ops
                .get(site.op as usize)?;
            let output_size = op.output.as_ref()?.size;
            match op.opcode {
                PcodeOpcode::Copy | PcodeOpcode::Cast => {
                    // COPY has equal input/output sizes by construction. CAST
                    // is a type-only operation in raw p-code; if a
                    // decompiler-added CAST changes size, its numeric
                    // extension/truncation semantics are not encoded here, so
                    // do not guess them in a pointer proof.
                    let input =
                        resolve_constant_input_bits(pcode, ssa, site, 0, visiting, budget - 1)?;
                    (input.size == output_size).then_some(input)
                }
                PcodeOpcode::IntZExt => {
                    resolve_constant_input_bits(pcode, ssa, site, 0, visiting, budget - 1)?
                        .zero_extend_to(output_size)
                }
                PcodeOpcode::IntSExt => {
                    resolve_constant_input_bits(pcode, ssa, site, 0, visiting, budget - 1)?
                        .sign_extend_to(output_size)
                }
                _ => None,
            }
        }
        SsaValueDefinition::Input | SsaValueDefinition::Phi { .. } => None,
    };
    visiting.remove(&value_id);
    resolved
}

pub(super) fn resolve_pointer_value(
    pcode: &PcodeFunction,
    ssa: &NirScalarSsa,
    options: &MlilPreviewOptions,
    value_id: SsaValueId,
    visiting: &mut BTreeSet<SsaValueId>,
    budget: usize,
) -> Option<PointerRange> {
    if budget == 0 || !visiting.insert(value_id) {
        return None;
    }
    let value = ssa.value(value_id)?;
    let resolved = match value.definition {
        SsaValueDefinition::Input => (is_register_space_id(value.storage.space_id)
            && (options.cspec_stack_pointer_offset == Some(value.storage.offset)
                || options
                    .calling_convention
                    .native_stack_pointer_register_offset(options.is_64bit)
                    == Some(value.storage.offset)))
        .then(|| PointerRange::exact(SsaMemoryRegion::Stack, 0)),
        SsaValueDefinition::Phi { block } => {
            let phi = ssa
                .phis
                .get(&block)?
                .iter()
                .find(|phi| phi.output == value_id)?;
            let mut range: Option<PointerRange> = None;
            for operand in &phi.operands {
                let operand_range = resolve_pointer_value(
                    pcode,
                    ssa,
                    options,
                    operand.value,
                    visiting,
                    budget - 1,
                )?;
                range = Some(match range {
                    None => operand_range,
                    Some(current) => current.union(operand_range)?,
                });
            }
            range
        }
        SsaValueDefinition::Operation(site) => {
            let op = pcode
                .blocks
                .get(site.block as usize)?
                .ops
                .get(site.op as usize)?;
            let outputs = ssa.operation_outputs.get(&site)?;
            // A wide register write can be partitioned into several scalar
            // SSA pieces when an earlier narrow write established a lane
            // boundary (for example RDXD followed by a full-width RDX copy).
            // The pointer provenance belongs to the p-code operation's whole
            // output, not only to the one lane currently being inspected.
            // Requiring a single output piece loses stack addresses exactly
            // at this ABI boundary and prevents escaping-slot invalidation at
            // the following call.
            if !outputs.iter().any(|output| output.value == value_id) {
                None
            } else {
                match op.opcode {
                    PcodeOpcode::Copy | PcodeOpcode::Cast | PcodeOpcode::IntZExt => {
                        resolve_pointer_input(pcode, ssa, options, site, 0, visiting, budget - 1)
                    }
                    PcodeOpcode::IntAdd if op.inputs.len() == 2 => {
                        let left_is_literal = op.inputs[0].is_constant;
                        let right_is_literal = op.inputs[1].is_constant;
                        if !left_is_literal && right_is_literal {
                            let delta = resolve_constant_input(
                                pcode,
                                ssa,
                                site,
                                1,
                                &mut BTreeSet::new(),
                                budget - 1,
                            )?;
                            resolve_pointer_input(
                                pcode,
                                ssa,
                                options,
                                site,
                                0,
                                visiting,
                                budget - 1,
                            )?
                            .shifted(delta)
                        } else if left_is_literal && !right_is_literal {
                            let delta = resolve_constant_input(
                                pcode,
                                ssa,
                                site,
                                0,
                                &mut BTreeSet::new(),
                                budget - 1,
                            )?;
                            resolve_pointer_input(
                                pcode,
                                ssa,
                                options,
                                site,
                                1,
                                visiting,
                                budget - 1,
                            )?
                            .shifted(delta)
                        } else {
                            // A non-literal constant chain can be either an
                            // absolute pointer (for example an ARM MMIO
                            // address) or an arithmetic displacement. If
                            // both operands are materialized, only accept the
                            // case whose other operand is already proven to be
                            // the Stack base; otherwise guessing can create a
                            // negative RAM address from an absolute pointer.
                            let left_range = resolve_pointer_input(
                                pcode,
                                ssa,
                                options,
                                site,
                                0,
                                visiting,
                                budget - 1,
                            );
                            let right_range = resolve_pointer_input(
                                pcode,
                                ssa,
                                options,
                                site,
                                1,
                                visiting,
                                budget - 1,
                            );
                            let left_delta = resolve_constant_input(
                                pcode,
                                ssa,
                                site,
                                0,
                                &mut BTreeSet::new(),
                                budget - 1,
                            );
                            let right_delta = resolve_constant_input(
                                pcode,
                                ssa,
                                site,
                                1,
                                &mut BTreeSet::new(),
                                budget - 1,
                            );
                            if left_range
                                .is_some_and(|range| range.region == SsaMemoryRegion::Stack)
                            {
                                left_range?.shifted(right_delta?)
                            } else if right_range
                                .is_some_and(|range| range.region == SsaMemoryRegion::Stack)
                            {
                                right_range?.shifted(left_delta?)
                            } else {
                                None
                            }
                        }
                    }
                    PcodeOpcode::IntSub if op.inputs.len() == 2 => {
                        let base = resolve_pointer_input(
                            pcode,
                            ssa,
                            options,
                            site,
                            0,
                            visiting,
                            budget - 1,
                        )?;
                        if is_call_return_address_scaffold_sub(pcode, options, site, op) {
                            Some(base)
                        } else {
                            let delta = resolve_constant_input(
                                pcode,
                                ssa,
                                site,
                                1,
                                &mut BTreeSet::new(),
                                budget - 1,
                            )?;
                            base.shifted(delta.checked_neg()?)
                        }
                    }
                    _ => None,
                }
            }
        }
    };
    visiting.remove(&value_id);
    resolved
}

/// x86/x64 SLEIGH expands a call into `sp -= slot; Store(sp, return); Call`.
/// The callee's return restores that slot, but raw p-code has no matching
/// post-call `sp += slot` in the caller. Treating the scaffold subtraction as
/// persistent makes every later stack address drift by one word per call.
fn is_call_return_address_scaffold_sub(
    pcode: &PcodeFunction,
    options: &MlilPreviewOptions,
    site: SsaOpSite,
    op: &PcodeOp,
) -> bool {
    if op.opcode != PcodeOpcode::IntSub || op.inputs.len() != 2 {
        return false;
    }
    let Some(output) = op.output.as_ref() else {
        return false;
    };
    let input = &op.inputs[0];
    let stack_pointer_offset = options.cspec_stack_pointer_offset.or_else(|| {
        options
            .calling_convention
            .native_stack_pointer_register_offset(options.is_64bit)
    });
    if stack_pointer_offset != Some(output.offset)
        || !is_register_space_id(output.space_id)
        || output.space_id != input.space_id
        || output.offset != input.offset
        || output.size != input.size
        || signed_constant_delta(&op.inputs[1]) != Some(i64::from(options.pointer_size))
    {
        return false;
    }
    let Some(block) = pcode.blocks.get(site.block as usize) else {
        return false;
    };
    let op_index = site.op as usize;
    let (Some(store), Some(call)) = (block.ops.get(op_index + 1), block.ops.get(op_index + 2))
    else {
        return false;
    };
    if store.opcode != PcodeOpcode::Store
        || store.address != op.address
        || store.inputs.len() < 3
        || !matches!(call.opcode, PcodeOpcode::Call | PcodeOpcode::CallInd)
        || call.address != op.address
    {
        return false;
    }
    let address = &store.inputs[1];
    let return_address = store.inputs.last().expect("three store inputs");
    address.space_id == output.space_id
        && address.offset == output.offset
        && address.size == output.size
        && return_address.is_constant
}

fn is_call_return_address_scaffold_store(
    pcode: &PcodeFunction,
    options: &MlilPreviewOptions,
    site: SsaOpSite,
    op: &PcodeOp,
) -> bool {
    if op.opcode != PcodeOpcode::Store || site.op == 0 {
        return false;
    }
    let Some(block) = pcode.blocks.get(site.block as usize) else {
        return false;
    };
    let Some(sub) = block.ops.get(site.op as usize - 1) else {
        return false;
    };
    is_call_return_address_scaffold_sub(
        pcode,
        options,
        SsaOpSite {
            block: site.block,
            op: site.op - 1,
        },
        sub,
    )
}

fn signed_constant_delta(varnode: &Varnode) -> Option<i64> {
    ResolvedConstant::from_literal(varnode)?.signed_at(varnode.size)
}

fn size_mask(size: u32) -> Option<u64> {
    match size.checked_mul(8)? {
        1..=63 => Some((1_u64 << (size * 8)) - 1),
        64 => Some(u64::MAX),
        _ => None,
    }
}

fn signed_bits(bits: u64, size: u32) -> Option<i64> {
    let mask = size_mask(size)?;
    let bits = bits & mask;
    let width = size.checked_mul(8)?;
    if width == 64 {
        Some(bits as i64)
    } else {
        let sign_bit = 1_u64 << (width - 1);
        if bits & sign_bit == 0 {
            i64::try_from(bits).ok()
        } else {
            Some((bits | !mask) as i64)
        }
    }
}
