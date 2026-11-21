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
    let mut pointers = PointerResolver::new(pcode, ssa, options);
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
                    &mut pointers,
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
                        &mut pointers,
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

pub(in crate::midend) fn resolve_call_target_address(
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
    pointers: &mut PointerResolver<'_>,
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
        pointers.input(site, pointer_input)
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

/// How deep a pointer chain is followed before it counts as unknown.
pub(super) const POINTER_RESOLUTION_BUDGET: usize = 64;

/// What one resolution leaned on besides the SSA graph.
///
/// A value's range is a property of the graph alone unless the walk was cut
/// short, either by the depth budget or by meeting a value it was already
/// inside. `floor` is the lowest budget any step below was entered with, so
/// the same walk can be replayed from any budget that leaves it at least one.
#[derive(Clone, Copy)]
struct Probe {
    floor: usize,
    clipped: bool,
}

impl Probe {
    fn new(budget: usize) -> Self {
        Self {
            floor: budget,
            clipped: false,
        }
    }

    /// Every resolver step starts here: no budget left is an unknown answer
    /// that says nothing about the value itself.
    fn enter(&mut self, budget: usize) -> bool {
        if budget == 0 {
            self.clipped = true;
            return false;
        }
        self.floor = self.floor.min(budget);
        true
    }
}

/// A value's range, and the least budget from which the walk that found it
/// runs to the end.
#[derive(Clone, Copy)]
struct ResolvedPointer {
    range: Option<PointerRange>,
    need: usize,
}

/// Pointer provenance over one function's scalar SSA.
///
/// Stack-pointer chains share almost everything: each call and each push
/// derives a new stack pointer from the last, and every join merges them in a
/// phi. Walking the chain afresh for each load and store revisits a phi's
/// operands once per path that reaches it, which doubles at every join. A
/// range that did not depend on where the walk came from is therefore kept
/// and reused for as long as this resolver lives.
pub(super) struct PointerResolver<'a> {
    pcode: &'a PcodeFunction,
    ssa: &'a NirScalarSsa,
    options: &'a MlilPreviewOptions,
    /// Values on the path from the query to the step being resolved.
    visiting: Vec<SsaValueId>,
    resolved: crate::fast_hash::FastMap<SsaValueId, ResolvedPointer>,
    probe: Probe,
    #[cfg(test)]
    pub(super) definitions_walked: usize,
    #[cfg(test)]
    pub(super) reuse_resolved: bool,
}

impl<'a> PointerResolver<'a> {
    pub(super) fn new(
        pcode: &'a PcodeFunction,
        ssa: &'a NirScalarSsa,
        options: &'a MlilPreviewOptions,
    ) -> Self {
        Self {
            pcode,
            ssa,
            options,
            visiting: Vec::new(),
            resolved: crate::fast_hash::FastMap::default(),
            probe: Probe::new(POINTER_RESOLUTION_BUDGET),
            #[cfg(test)]
            definitions_walked: 0,
            #[cfg(test)]
            reuse_resolved: true,
        }
    }

    /// The range of the pointer read as `input_index` of the operation at
    /// `site`.
    pub(super) fn input(&mut self, site: SsaOpSite, input_index: usize) -> Option<PointerRange> {
        self.begin_query();
        self.resolve_input(site, input_index, POINTER_RESOLUTION_BUDGET)
    }

    /// The range of the pointer held by `value_id`.
    pub(super) fn value(&mut self, value_id: SsaValueId) -> Option<PointerRange> {
        self.begin_query();
        self.resolve_value(value_id, POINTER_RESOLUTION_BUDGET)
    }

    fn begin_query(&mut self) {
        self.visiting.clear();
        self.probe = Probe::new(POINTER_RESOLUTION_BUDGET);
    }

    fn resolve_input(
        &mut self,
        site: SsaOpSite,
        input_index: usize,
        budget: usize,
    ) -> Option<PointerRange> {
        if !self.probe.enter(budget) {
            return None;
        }
        let op = self
            .pcode
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
        let pieces = self.ssa.operation_inputs.get(&SsaUseSite {
            block: site.block,
            op: site.op,
            input: input_index as u32,
        })?;
        if pieces.len() != 1 {
            return None;
        }
        let value = self.ssa.value(pieces[0].value)?;
        if value.storage.size != input.size {
            return None;
        }
        self.resolve_value(value.id, budget - 1)
    }

    fn resolve_value(&mut self, value_id: SsaValueId, budget: usize) -> Option<PointerRange> {
        if !self.probe.enter(budget) {
            return None;
        }
        if let Some(known) = self.reusable(value_id)
            && budget >= known.need
        {
            // The walk this stands for went `need - 1` steps further down.
            self.probe.floor = self.probe.floor.min(budget + 1 - known.need);
            return known.range;
        }
        if self.visiting.contains(&value_id) {
            self.probe.clipped = true;
            return None;
        }
        self.visiting.push(value_id);
        let outer = std::mem::replace(&mut self.probe, Probe::new(budget));
        let range = self.resolve_definition(value_id, budget);
        let inner = std::mem::replace(&mut self.probe, outer);
        self.visiting.pop();
        if inner.clipped {
            // Cut short below: a walk from elsewhere, or with more budget,
            // may get further, so this answer belongs to this walk only.
            self.probe.clipped = true;
        } else {
            self.resolved.insert(
                value_id,
                ResolvedPointer {
                    range,
                    need: budget - inner.floor + 1,
                },
            );
        }
        self.probe.floor = self.probe.floor.min(inner.floor);
        range
    }

    fn reusable(&self, value_id: SsaValueId) -> Option<ResolvedPointer> {
        #[cfg(test)]
        if !self.reuse_resolved {
            return None;
        }
        self.resolved.get(&value_id).copied()
    }

    fn resolve_definition(&mut self, value_id: SsaValueId, budget: usize) -> Option<PointerRange> {
        #[cfg(test)]
        {
            self.definitions_walked += 1;
        }
        let ssa = self.ssa;
        let pcode = self.pcode;
        let options = self.options;
        let value = ssa.value(value_id)?;
        match value.definition {
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
                    let operand_range = self.resolve_value(operand.value, budget - 1)?;
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
                    return None;
                }
                match op.opcode {
                    PcodeOpcode::Copy | PcodeOpcode::Cast | PcodeOpcode::IntZExt => {
                        self.resolve_input(site, 0, budget - 1)
                    }
                    PcodeOpcode::IntAdd if op.inputs.len() == 2 => {
                        let left_is_literal = op.inputs[0].is_constant;
                        let right_is_literal = op.inputs[1].is_constant;
                        if !left_is_literal && right_is_literal {
                            let delta = self.constant_input(site, 1, budget - 1)?;
                            self.resolve_input(site, 0, budget - 1)?.shifted(delta)
                        } else if left_is_literal && !right_is_literal {
                            let delta = self.constant_input(site, 0, budget - 1)?;
                            self.resolve_input(site, 1, budget - 1)?.shifted(delta)
                        } else {
                            // A non-literal constant chain can be either an
                            // absolute pointer (for example an ARM MMIO
                            // address) or an arithmetic displacement. If
                            // both operands are materialized, only accept the
                            // case whose other operand is already proven to be
                            // the Stack base; otherwise guessing can create a
                            // negative RAM address from an absolute pointer.
                            let left_range = self.resolve_input(site, 0, budget - 1);
                            let right_range = self.resolve_input(site, 1, budget - 1);
                            let left_delta = self.constant_input(site, 0, budget - 1);
                            let right_delta = self.constant_input(site, 1, budget - 1);
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
                        let base = self.resolve_input(site, 0, budget - 1)?;
                        if is_call_return_address_scaffold_sub(pcode, options, site, op) {
                            Some(base)
                        } else {
                            let delta = self.constant_input(site, 1, budget - 1)?;
                            base.shifted(delta.checked_neg()?)
                        }
                    }
                    _ => None,
                }
            }
        }
    }

    fn constant_input(
        &mut self,
        site: SsaOpSite,
        input_index: usize,
        budget: usize,
    ) -> Option<i64> {
        constant_input(
            self.pcode,
            self.ssa,
            site,
            input_index,
            &mut BTreeSet::new(),
            budget,
            &mut self.probe,
        )
    }
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
#[cfg(test)]
pub(super) fn resolve_constant_input(
    pcode: &PcodeFunction,
    ssa: &NirScalarSsa,
    site: SsaOpSite,
    input_index: usize,
    visiting: &mut BTreeSet<SsaValueId>,
    budget: usize,
) -> Option<i64> {
    constant_input(
        pcode,
        ssa,
        site,
        input_index,
        visiting,
        budget,
        &mut Probe::new(budget),
    )
}

fn constant_input(
    pcode: &PcodeFunction,
    ssa: &NirScalarSsa,
    site: SsaOpSite,
    input_index: usize,
    visiting: &mut BTreeSet<SsaValueId>,
    budget: usize,
    probe: &mut Probe,
) -> Option<i64> {
    if !probe.enter(budget) {
        return None;
    }
    let op = pcode
        .blocks
        .get(site.block as usize)?
        .ops
        .get(site.op as usize)?;
    let input = op.inputs.get(input_index)?;
    let resolved = constant_input_bits(pcode, ssa, site, input_index, visiting, budget, probe)?;
    resolved.signed_at(input.size)
}

fn constant_input_bits(
    pcode: &PcodeFunction,
    ssa: &NirScalarSsa,
    site: SsaOpSite,
    input_index: usize,
    visiting: &mut BTreeSet<SsaValueId>,
    budget: usize,
    probe: &mut Probe,
) -> Option<ResolvedConstant> {
    if !probe.enter(budget) {
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
    constant_value(pcode, ssa, value.id, visiting, budget - 1, probe)
}

fn constant_value(
    pcode: &PcodeFunction,
    ssa: &NirScalarSsa,
    value_id: SsaValueId,
    visiting: &mut BTreeSet<SsaValueId>,
    budget: usize,
    probe: &mut Probe,
) -> Option<ResolvedConstant> {
    if !probe.enter(budget) {
        return None;
    }
    if !visiting.insert(value_id) {
        probe.clipped = true;
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
                        constant_input_bits(pcode, ssa, site, 0, visiting, budget - 1, probe)?;
                    (input.size == output_size).then_some(input)
                }
                PcodeOpcode::IntZExt => {
                    constant_input_bits(pcode, ssa, site, 0, visiting, budget - 1, probe)?
                        .zero_extend_to(output_size)
                }
                PcodeOpcode::IntSExt => {
                    constant_input_bits(pcode, ssa, site, 0, visiting, budget - 1, probe)?
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pcode::PcodeBasicBlock;

    /// The resolver as it was before values were kept: every query walks the
    /// whole chain again. Unchanged, so that the two can be compared.
    #[allow(dead_code)]
    mod reference {
        use super::super::*;

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
            let resolved =
                resolve_constant_input_bits(pcode, ssa, site, input_index, visiting, budget)?;
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
                            let input = resolve_constant_input_bits(
                                pcode,
                                ssa,
                                site,
                                0,
                                visiting,
                                budget - 1,
                            )?;
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
                                resolve_pointer_input(
                                    pcode,
                                    ssa,
                                    options,
                                    site,
                                    0,
                                    visiting,
                                    budget - 1,
                                )
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
    }

    const SPACE: i64 = 3;

    fn stack_pointer(options: &MlilPreviewOptions) -> Varnode {
        register(
            options
                .calling_convention
                .native_stack_pointer_register_offset(options.is_64bit)
                .expect("the default convention names a stack pointer"),
        )
    }

    fn register(offset: u64) -> Varnode {
        Varnode {
            space_id: REGISTER_SPACE_ID,
            offset,
            size: 8,
            is_constant: false,
            constant_val: 0,
        }
    }

    fn unique(offset: u64, size: u32) -> Varnode {
        Varnode {
            space_id: UNIQUE_SPACE_ID,
            offset,
            size,
            is_constant: false,
            constant_val: 0,
        }
    }

    fn op(
        address: u64,
        opcode: PcodeOpcode,
        output: Option<Varnode>,
        inputs: Vec<Varnode>,
    ) -> PcodeOp {
        PcodeOp {
            seq_num: 0,
            opcode,
            address,
            output,
            inputs,
            asm_mnemonic: None,
        }
    }

    fn function(blocks: Vec<Vec<PcodeOp>>) -> PcodeFunction {
        PcodeFunction {
            blocks: blocks
                .into_iter()
                .enumerate()
                .map(|(index, ops)| PcodeBasicBlock {
                    index: index as u32,
                    start_address: 0x1000 + index as u64 * 0x1000,
                    successors: Vec::new(),
                    ops,
                })
                .collect(),
        }
    }

    fn predecessors_of(successors: &[Vec<usize>]) -> Vec<Vec<usize>> {
        let mut predecessors = vec![Vec::new(); successors.len()];
        for (block, targets) in successors.iter().enumerate() {
            for &target in targets {
                predecessors[target].push(block);
            }
        }
        predecessors
    }

    /// `sp -= 8; *sp = return address; call` -- the shape SLEIGH lifts an
    /// x86 call to, all three at the call's address.
    fn call(address: u64, sp: &Varnode) -> Vec<PcodeOp> {
        vec![
            op(
                address,
                PcodeOpcode::IntSub,
                Some(sp.clone()),
                vec![sp.clone(), Varnode::constant(8, 8)],
            ),
            op(
                address,
                PcodeOpcode::Store,
                None,
                vec![
                    Varnode::constant(SPACE, 8),
                    sp.clone(),
                    Varnode::constant(address as i64 + 5, 8),
                ],
            ),
            op(
                address,
                PcodeOpcode::Call,
                None,
                vec![Varnode::constant(0x9000, 8)],
            ),
        ]
    }

    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }

        fn below(&mut self, bound: u64) -> u64 {
            self.next() % bound
        }
    }

    /// A random function over the stack pointer and three registers: calls
    /// and pushes that move the stack pointer, address arithmetic on it,
    /// copies between registers, loads and stores, over a CFG with joins and
    /// back edges. Long runs of calls take chains past the depth budget.
    fn random_function(
        rng: &mut Rng,
        options: &MlilPreviewOptions,
    ) -> (PcodeFunction, Vec<Vec<usize>>) {
        let sp = stack_pointer(options);
        let registers = [register(0x100), register(0x108), register(0x110)];
        let block_count = 2 + rng.below(9) as usize;
        let mut successors = vec![Vec::new(); block_count];
        for block in 0..block_count {
            if block + 1 < block_count {
                successors[block].push(block + 1);
            }
            if rng.below(3) == 0 {
                let target = rng.below(block_count as u64) as usize;
                if !successors[block].contains(&target) {
                    successors[block].push(target);
                }
            }
        }
        let mut blocks = Vec::with_capacity(block_count);
        for block in 0..block_count {
            let mut ops = Vec::new();
            let mut address = 0x1000 + block as u64 * 0x1000;
            let op_count = rng.below(7);
            for _ in 0..op_count {
                address += 0x10;
                let a = registers[rng.below(3) as usize].clone();
                let b = registers[rng.below(3) as usize].clone();
                let c = registers[rng.below(3) as usize].clone();
                let small = Varnode::constant(8 * (1 + rng.below(4) as i64), 8);
                match rng.below(14) {
                    0 | 1 => {
                        for _ in 0..1 + rng.below(12) {
                            address += 0x10;
                            ops.extend(call(address, &sp));
                        }
                    }
                    2 => {
                        ops.push(op(
                            address,
                            PcodeOpcode::IntSub,
                            Some(sp.clone()),
                            vec![sp.clone(), small],
                        ));
                        ops.push(op(
                            address,
                            PcodeOpcode::Store,
                            None,
                            vec![Varnode::constant(SPACE, 8), sp.clone(), a],
                        ));
                    }
                    3 => ops.push(op(
                        address,
                        PcodeOpcode::IntAdd,
                        Some(sp.clone()),
                        vec![sp.clone(), small],
                    )),
                    4 => ops.push(op(
                        address,
                        PcodeOpcode::IntAdd,
                        Some(a),
                        vec![sp.clone(), small],
                    )),
                    5 => ops.push(op(address, PcodeOpcode::IntAdd, Some(a), vec![b, c])),
                    6 => ops.push(op(address, PcodeOpcode::Copy, Some(a), vec![b])),
                    7 => ops.push(op(address, PcodeOpcode::Copy, Some(a), vec![sp.clone()])),
                    8 => {
                        // A displacement that reaches the add through a
                        // temporary, widened on the way.
                        let narrow = unique(0x200, 4);
                        let wide = unique(0x210, 8);
                        ops.push(op(
                            address,
                            PcodeOpcode::Copy,
                            Some(narrow.clone()),
                            vec![Varnode::constant(0x18, 4)],
                        ));
                        let extend = if rng.below(2) == 0 {
                            PcodeOpcode::IntZExt
                        } else {
                            PcodeOpcode::IntSExt
                        };
                        ops.push(op(address, extend, Some(wide.clone()), vec![narrow]));
                        let (left, right) = if rng.below(2) == 0 {
                            (b, wide)
                        } else {
                            (wide, b)
                        };
                        ops.push(op(address, PcodeOpcode::IntAdd, Some(a), vec![left, right]));
                    }
                    9 => ops.push(op(
                        address,
                        PcodeOpcode::Load,
                        Some(a),
                        vec![Varnode::constant(SPACE, 8), b],
                    )),
                    10 => ops.push(op(
                        address,
                        PcodeOpcode::Store,
                        None,
                        vec![Varnode::constant(SPACE, 8), a, b],
                    )),
                    11 => ops.push(op(address, PcodeOpcode::IntSub, Some(a), vec![b, small])),
                    12 => ops.push(op(
                        address,
                        PcodeOpcode::Copy,
                        Some(a),
                        vec![Varnode::constant(0x4000, 8)],
                    )),
                    _ => ops.push(op(
                        address,
                        PcodeOpcode::IntAdd,
                        Some(sp.clone()),
                        vec![b, c],
                    )),
                }
            }
            if block + 1 == block_count {
                ops.push(op(
                    address + 0x10,
                    PcodeOpcode::Return,
                    None,
                    vec![registers[0].clone()],
                ));
            }
            blocks.push(ops);
        }
        (function(blocks), successors)
    }

    /// Keeping a value's range changes how often it is computed and nothing
    /// else: every query a builder makes must answer as a fresh walk does.
    #[test]
    fn kept_ranges_answer_as_a_fresh_walk_does() {
        let options = MlilPreviewOptions::default();
        let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
        let mut some = 0usize;
        let mut clipped_by_budget = 0usize;
        for round in 0..400 {
            let (pcode, successors) = random_function(&mut rng, &options);
            let predecessors = predecessors_of(&successors);
            let ssa = scalar_ssa::build_scalar_ssa(&pcode, &successors, &predecessors);
            let mut queries = Vec::new();
            for (block_index, block) in pcode.blocks.iter().enumerate() {
                for (op_index, op) in block.ops.iter().enumerate() {
                    for input in 0..op.inputs.len() {
                        queries.push((
                            Some((
                                SsaOpSite {
                                    block: block_index as u32,
                                    op: op_index as u32,
                                },
                                input,
                            )),
                            None,
                        ));
                    }
                }
            }
            for value in &ssa.values {
                queries.push((None, Some(value.id)));
            }
            // One resolver for the whole function, asked in an arbitrary
            // order, so a kept range is met from many different walks.
            for index in (1..queries.len()).rev() {
                queries.swap(index, rng.below(index as u64 + 1) as usize);
            }
            let mut pointers = PointerResolver::new(&pcode, &ssa, &options);
            for (input, value) in queries {
                let (kept, fresh, unbounded) = match (input, value) {
                    (Some((site, input)), _) => (
                        pointers.input(site, input),
                        reference::resolve_pointer_input(
                            &pcode,
                            &ssa,
                            &options,
                            site,
                            input,
                            &mut BTreeSet::new(),
                            POINTER_RESOLUTION_BUDGET,
                        ),
                        reference::resolve_pointer_input(
                            &pcode,
                            &ssa,
                            &options,
                            site,
                            input,
                            &mut BTreeSet::new(),
                            usize::MAX,
                        ),
                    ),
                    (None, Some(id)) => (
                        pointers.value(id),
                        reference::resolve_pointer_value(
                            &pcode,
                            &ssa,
                            &options,
                            id,
                            &mut BTreeSet::new(),
                            POINTER_RESOLUTION_BUDGET,
                        ),
                        reference::resolve_pointer_value(
                            &pcode,
                            &ssa,
                            &options,
                            id,
                            &mut BTreeSet::new(),
                            usize::MAX,
                        ),
                    ),
                    (None, None) => unreachable!("a query names an input or a value"),
                };
                assert_eq!(
                    kept, fresh,
                    "round {round}: query {input:?} / {value:?} disagrees with a fresh walk"
                );
                some += usize::from(fresh.is_some());
                clipped_by_budget += usize::from(fresh != unbounded);
            }
        }
        // The comparison means something only if the functions reach both
        // sides of it: chains that resolve, and chains the budget cuts short.
        assert!(some > 1_000, "only {some} queries resolved");
        assert!(
            clipped_by_budget > 100,
            "only {clipped_by_budget} queries were cut short by the budget"
        );
    }

    /// A run of `if (..) f(); else g();` leaves one stack pointer per arm and
    /// a phi at each join. A walk that keeps nothing follows both arms of
    /// every join, so the work doubles with each one.
    #[test]
    fn a_stack_pointer_chain_through_joins_is_walked_once() {
        const JOINS: usize = 14;
        let options = MlilPreviewOptions::default();
        let sp = stack_pointer(&options);
        let mut blocks = vec![Vec::new()];
        let mut successors = vec![vec![1, 2]];
        for join in 0..JOINS {
            let base = 0x10_0000 * (join as u64 + 1);
            let first = blocks.len();
            blocks.push(call(base, &sp));
            blocks.push(call(base + 0x100, &sp));
            blocks.push(Vec::new());
            successors.push(vec![first + 2]);
            successors.push(vec![first + 2]);
            successors.push(if join + 1 == JOINS {
                Vec::new()
            } else {
                vec![first + 3, first + 4]
            });
        }
        let last = blocks.len() - 1;
        let slot = unique(0x300, 8);
        blocks[last].push(op(
            0xf0_0000,
            PcodeOpcode::IntAdd,
            Some(slot.clone()),
            vec![sp.clone(), Varnode::constant(0x10, 8)],
        ));
        blocks[last].push(op(
            0xf0_0010,
            PcodeOpcode::Load,
            Some(unique(0x308, 8)),
            vec![Varnode::constant(SPACE, 8), slot],
        ));
        let pcode = function(blocks);
        let predecessors = predecessors_of(&successors);
        let ssa = scalar_ssa::build_scalar_ssa(&pcode, &successors, &predecessors);
        let load = SsaOpSite {
            block: last as u32,
            op: 1,
        };
        let mut pointers = PointerResolver::new(&pcode, &ssa, &options);
        let kept = pointers.input(load, 1);
        assert!(
            kept.is_some_and(|range| range.region == SsaMemoryRegion::Stack && range.exact),
            "the load reads a stack slot: {kept:?}"
        );
        assert!(
            pointers.definitions_walked <= ssa.values.len(),
            "{} definitions walked for {} values",
            pointers.definitions_walked,
            ssa.values.len()
        );

        // The same question with nothing kept: same answer, and a walk per
        // path through the joins. This is what the bound above rules out.
        let mut forgetful = PointerResolver::new(&pcode, &ssa, &options);
        forgetful.reuse_resolved = false;
        assert_eq!(forgetful.input(load, 1), kept);
        assert!(
            forgetful.definitions_walked >= 1 << JOINS,
            "{} definitions walked without reuse",
            forgetful.definitions_walked
        );
    }
}
