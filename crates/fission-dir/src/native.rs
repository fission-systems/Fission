//! First executable P-code-native DIR slice.
//!
//! The reconstructor deliberately admits only contiguous, side-effect-free
//! integer regions.  Unsupported memory, call, and control-flow operations are
//! rejected instead of being approximated.  The verifier executes the original
//! P-code with `fission-emulator` and then asks `fission-solver` whether any
//! input can distinguish it from the reconstructed expression.

use std::collections::HashMap;

use fission_emulator::{
    MachineState,
    pcode::eval::{Evaluator, StepResult},
};
use fission_pcode::{PcodeOp, PcodeOpcode, Varnode};
use fission_solver::{SatResult, Solver, SymExpr};

use crate::product::{
    AssumptionKind, CallObservation, CandidateSemantics, CandidateSource, DirCandidate,
    DirCounterexample, DirReconstructor, DirVerifier, EvidenceStrength, ObservationScope,
    ObservedLocation, PcodeFoundation, PcodeNativeBinaryOp, PcodeNativeExpr, PcodeNativeInput,
    PcodeNativeRegion, PcodeRegionSelection, UnresolvedEffect, UnresolvedEffectKind,
    ValidationAssumption, ValidationBudget, ValidationEvidence, ValidationVerdict,
};

/// Reconstructs one explicitly selected P-code region.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PcodeNativeReconstructor {
    selection: PcodeRegionSelection,
    value_space_ids: Vec<u64>,
}

impl PcodeNativeReconstructor {
    #[must_use]
    pub fn new(
        selection: PcodeRegionSelection,
        value_space_ids: impl IntoIterator<Item = u64>,
    ) -> Self {
        let mut value_space_ids = value_space_ids.into_iter().collect::<Vec<_>>();
        value_space_ids.sort_unstable();
        value_space_ids.dedup();
        Self {
            selection,
            value_space_ids,
        }
    }

    #[must_use]
    pub fn selection(&self) -> PcodeRegionSelection {
        self.selection
    }

    #[must_use]
    pub fn value_space_ids(&self) -> &[u64] {
        &self.value_space_ids
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PcodeNativeReconstructError {
    #[error("P-code block {block_index} does not exist")]
    MissingBlock { block_index: u32 },
    #[error("P-code block index {block_index} is ambiguous")]
    AmbiguousBlock { block_index: u32 },
    #[error(
        "region [{first_op}, {end_op}) is outside block {block_index} with {block_ops} operations"
    )]
    RegionOutOfBounds {
        block_index: u32,
        first_op: usize,
        end_op: usize,
        block_ops: usize,
    },
    #[error("empty P-code regions are not reconstructable")]
    EmptyRegion,
    #[error("operation {op_index} uses unsupported opcode {opcode:?}")]
    UnsupportedOpcode {
        op_index: usize,
        opcode: PcodeOpcode,
    },
    #[error("operation {op_index} has a missing output")]
    MissingOutput { op_index: usize },
    #[error("operation {op_index} has unsupported {size}-byte varnode")]
    UnsupportedWidth { op_index: usize, size: u32 },
    #[error(
        "operation {op_index} accesses space {space_id}, which is not admitted as a non-memory value space"
    )]
    UnadmittedAddressSpace { op_index: usize, space_id: u64 },
    #[error(
        "operation {op_index} has incompatible widths: inputs {input_sizes:?}, output {output_size}"
    )]
    WidthMismatch {
        op_index: usize,
        input_sizes: Vec<u32>,
        output_size: u32,
    },
    #[error(
        "region contains overlapping non-identical varnodes in space {space_id}: \
         [0x{left_offset:x}, +{left_size}) and [0x{right_offset:x}, +{right_size})"
    )]
    OverlappingVarnodes {
        space_id: u64,
        left_offset: u64,
        left_size: u32,
        right_offset: u64,
        right_size: u32,
    },
    #[error("selected output {output:?} is not defined by the region")]
    OutputNotDefined { output: ObservedLocation },
}

impl DirReconstructor for PcodeNativeReconstructor {
    type Error = PcodeNativeReconstructError;

    fn reconstruct(&self, foundation: &PcodeFoundation) -> Result<Vec<DirCandidate>, Self::Error> {
        let raw = foundation.pcode().as_raw();
        let mut matching_blocks = raw
            .blocks
            .iter()
            .filter(|block| block.index == self.selection.block_index);
        let block = matching_blocks
            .next()
            .ok_or(PcodeNativeReconstructError::MissingBlock {
                block_index: self.selection.block_index,
            })?;
        if matching_blocks.next().is_some() {
            return Err(PcodeNativeReconstructError::AmbiguousBlock {
                block_index: self.selection.block_index,
            });
        }
        if self.selection.op_count == 0 {
            return Err(PcodeNativeReconstructError::EmptyRegion);
        }
        let end = self
            .selection
            .first_op
            .checked_add(self.selection.op_count)
            .ok_or(PcodeNativeReconstructError::RegionOutOfBounds {
                block_index: block.index,
                first_op: self.selection.first_op,
                end_op: usize::MAX,
                block_ops: block.ops.len(),
            })?;
        let Some(ops) = block.ops.get(self.selection.first_op..end) else {
            return Err(PcodeNativeReconstructError::RegionOutOfBounds {
                block_index: block.index,
                first_op: self.selection.first_op,
                end_op: end,
                block_ops: block.ops.len(),
            });
        };

        reject_overlapping_varnodes(ops, self.selection.output)?;

        let mut values = HashMap::<ObservedLocation, PcodeNativeExpr>::new();
        let mut inputs = Vec::<PcodeNativeInput>::new();

        for (relative_index, op) in ops.iter().enumerate() {
            let op_index = self.selection.first_op + relative_index;
            let output = op
                .output
                .as_ref()
                .ok_or(PcodeNativeReconstructError::MissingOutput { op_index })?;
            validate_width(op_index, output.size)?;
            validate_value_space(op_index, output, &self.value_space_ids)?;
            let output_location = location_of(output);
            let expression =
                reconstruct_op(op, op_index, &self.value_space_ids, &values, &mut inputs)?;
            values.insert(output_location, expression);
        }

        let expression = values.get(&self.selection.output).cloned().ok_or(
            PcodeNativeReconstructError::OutputNotDefined {
                output: self.selection.output,
            },
        )?;
        let region = PcodeNativeRegion {
            selection: self.selection,
            value_space_ids: self.value_space_ids.clone(),
            inputs,
            expression,
        };
        let pseudocode = render_region_pseudocode(&region);
        let candidate_id = format!(
            "pcode-native-b{}-{}-{}-s{}-o{:x}-n{}",
            self.selection.block_index,
            self.selection.first_op,
            end,
            self.selection.output.space_id,
            self.selection.output.offset,
            self.selection.output.size
        );

        Ok(vec![DirCandidate {
            candidate_id,
            source: CandidateSource::PcodeNative,
            pseudocode,
            semantics: CandidateSemantics::PcodeNativeRegion(region),
        }])
    }
}

fn reconstruct_op(
    op: &PcodeOp,
    op_index: usize,
    value_space_ids: &[u64],
    values: &HashMap<ObservedLocation, PcodeNativeExpr>,
    inputs: &mut Vec<PcodeNativeInput>,
) -> Result<PcodeNativeExpr, PcodeNativeReconstructError> {
    let output = op
        .output
        .as_ref()
        .ok_or(PcodeNativeReconstructError::MissingOutput { op_index })?;
    match op.opcode {
        PcodeOpcode::Copy => {
            require_widths(op, op_index, &[output.size])?;
            value_for(&op.inputs[0], op_index, value_space_ids, values, inputs)
        }
        PcodeOpcode::IntAdd
        | PcodeOpcode::IntSub
        | PcodeOpcode::IntAnd
        | PcodeOpcode::IntOr
        | PcodeOpcode::IntXor => {
            require_widths(op, op_index, &[output.size, output.size])?;
            let left = value_for(&op.inputs[0], op_index, value_space_ids, values, inputs)?;
            let right = value_for(&op.inputs[1], op_index, value_space_ids, values, inputs)?;
            let op = match op.opcode {
                PcodeOpcode::IntAdd => PcodeNativeBinaryOp::Add,
                PcodeOpcode::IntSub => PcodeNativeBinaryOp::Subtract,
                PcodeOpcode::IntAnd => PcodeNativeBinaryOp::BitAnd,
                PcodeOpcode::IntOr => PcodeNativeBinaryOp::BitOr,
                PcodeOpcode::IntXor => PcodeNativeBinaryOp::BitXor,
                _ => unreachable!(),
            };
            Ok(PcodeNativeExpr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
                bits: bits_for_size(output.size),
            })
        }
        PcodeOpcode::IntEqual
        | PcodeOpcode::IntNotEqual
        | PcodeOpcode::IntLess
        | PcodeOpcode::IntLessEqual => {
            let input_size = op.inputs[0].size;
            validate_width(op_index, input_size)?;
            require_widths(op, op_index, &[input_size, input_size])?;
            let left = value_for(&op.inputs[0], op_index, value_space_ids, values, inputs)?;
            let right = value_for(&op.inputs[1], op_index, value_space_ids, values, inputs)?;
            let op = match op.opcode {
                PcodeOpcode::IntEqual => PcodeNativeBinaryOp::Equal,
                PcodeOpcode::IntNotEqual => PcodeNativeBinaryOp::NotEqual,
                PcodeOpcode::IntLess => PcodeNativeBinaryOp::UnsignedLess,
                PcodeOpcode::IntLessEqual => PcodeNativeBinaryOp::UnsignedLessEqual,
                _ => unreachable!(),
            };
            Ok(PcodeNativeExpr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
                bits: bits_for_size(output.size),
            })
        }
        opcode => Err(PcodeNativeReconstructError::UnsupportedOpcode { op_index, opcode }),
    }
}

fn require_widths(
    op: &PcodeOp,
    op_index: usize,
    expected_inputs: &[u32],
) -> Result<(), PcodeNativeReconstructError> {
    let output_size = op.output.as_ref().map_or(0, |output| output.size);
    let actual_inputs = op.inputs.iter().map(|input| input.size).collect::<Vec<_>>();
    for input in &op.inputs {
        validate_width(op_index, input.size)?;
    }
    if actual_inputs != expected_inputs {
        return Err(PcodeNativeReconstructError::WidthMismatch {
            op_index,
            input_sizes: actual_inputs,
            output_size,
        });
    }
    Ok(())
}

fn validate_width(op_index: usize, size: u32) -> Result<(), PcodeNativeReconstructError> {
    if (1..=8).contains(&size) {
        Ok(())
    } else {
        Err(PcodeNativeReconstructError::UnsupportedWidth { op_index, size })
    }
}

fn value_for(
    varnode: &Varnode,
    op_index: usize,
    value_space_ids: &[u64],
    values: &HashMap<ObservedLocation, PcodeNativeExpr>,
    inputs: &mut Vec<PcodeNativeInput>,
) -> Result<PcodeNativeExpr, PcodeNativeReconstructError> {
    validate_width(op_index, varnode.size)?;
    let bits = bits_for_size(varnode.size);
    if varnode.is_constant {
        return Ok(PcodeNativeExpr::Constant {
            value: mask_value(varnode.constant_val as u64, bits),
            bits,
        });
    }

    validate_value_space(op_index, varnode, value_space_ids)?;
    let location = location_of(varnode);
    if let Some(expression) = values.get(&location) {
        return Ok(expression.clone());
    }
    if let Some(index) = inputs.iter().position(|input| input.location == location) {
        return Ok(PcodeNativeExpr::Input { index, bits });
    }

    let index = inputs.len();
    inputs.push(PcodeNativeInput {
        name: input_name(location),
        location,
    });
    Ok(PcodeNativeExpr::Input { index, bits })
}

fn validate_value_space(
    op_index: usize,
    varnode: &Varnode,
    value_space_ids: &[u64],
) -> Result<(), PcodeNativeReconstructError> {
    if !varnode.is_constant
        && varnode.space_id != 0
        && value_space_ids.binary_search(&varnode.space_id).is_ok()
    {
        Ok(())
    } else {
        Err(PcodeNativeReconstructError::UnadmittedAddressSpace {
            op_index,
            space_id: varnode.space_id,
        })
    }
}

fn reject_overlapping_varnodes(
    ops: &[PcodeOp],
    selected_output: ObservedLocation,
) -> Result<(), PcodeNativeReconstructError> {
    let mut locations = vec![selected_output];
    for op in ops {
        if let Some(output) = &op.output
            && !output.is_constant
        {
            locations.push(location_of(output));
        }
        locations.extend(
            op.inputs
                .iter()
                .filter(|input| !input.is_constant)
                .map(location_of),
        );
    }
    locations.sort_by_key(|location| (location.space_id, location.offset, location.size));
    locations.dedup();

    for (index, left) in locations.iter().enumerate() {
        for right in &locations[index + 1..] {
            if left.space_id != right.space_id {
                break;
            }
            let left_end = left.offset.saturating_add(u64::from(left.size));
            let right_end = right.offset.saturating_add(u64::from(right.size));
            if left.offset < right_end && right.offset < left_end {
                return Err(PcodeNativeReconstructError::OverlappingVarnodes {
                    space_id: left.space_id,
                    left_offset: left.offset,
                    left_size: left.size,
                    right_offset: right.offset,
                    right_size: right.size,
                });
            }
        }
    }
    Ok(())
}

/// Emulator + solver verifier for [`PcodeNativeRegion`] candidates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PcodeObservationVerifier {
    default_trace_limit: u64,
}

impl Default for PcodeObservationVerifier {
    fn default() -> Self {
        Self {
            default_trace_limit: 32,
        }
    }
}

impl PcodeObservationVerifier {
    #[must_use]
    pub fn new(default_trace_limit: u64) -> Self {
        Self {
            default_trace_limit: default_trace_limit.max(1),
        }
    }
}

impl DirVerifier for PcodeObservationVerifier {
    fn verify(
        &self,
        foundation: &PcodeFoundation,
        candidate: &DirCandidate,
        scope: &ObservationScope,
        budget: ValidationBudget,
    ) -> (ValidationVerdict, ValidationEvidence) {
        let CandidateSemantics::PcodeNativeRegion(region) = &candidate.semantics else {
            return unsupported(
                budget,
                UnresolvedEffectKind::Other,
                "candidate has no executable P-code-native semantics",
            );
        };
        if candidate.source != CandidateSource::PcodeNative {
            return unsupported(
                budget,
                UnresolvedEffectKind::Other,
                "candidate source is not PcodeNative",
            );
        }
        if let Err(detail) = validate_candidate_contract(foundation, candidate, region) {
            return unsupported(
                budget,
                UnresolvedEffectKind::Other,
                &format!("invalid P-code-native candidate contract: {detail}"),
            );
        }
        if !supports_scope(scope, region.selection.output) {
            return unsupported(
                budget,
                UnresolvedEffectKind::Other,
                "verifier supports exactly one location-only observation",
            );
        }
        if budget.max_paths == Some(0) {
            return inconclusive_budget(budget, "single P-code path exceeds max_paths=0");
        }
        if budget.max_traces == Some(0) {
            return inconclusive_budget(budget, "concrete validation exceeds max_traces=0");
        }
        if budget.solver_timeout_ms.is_some() {
            return unsupported(
                budget,
                UnresolvedEffectKind::SolverTimeout,
                "the in-process solver does not yet enforce wall-clock timeouts",
            );
        }

        let trace_limit = budget.max_traces.unwrap_or(self.default_trace_limit);
        let samples = deterministic_samples(&region.inputs, trace_limit);
        let mut executed_traces = 0_u64;
        for sample in &samples {
            let foundation_value = match execute_foundation_region(foundation, region, sample) {
                Ok(value) => value,
                Err(detail) => {
                    return inconclusive_backend(
                        budget,
                        executed_traces,
                        format!("emulator failed: {detail}"),
                    );
                }
            };
            let candidate_value = evaluate_expression(&region.expression, sample);
            executed_traces = executed_traces.saturating_add(1);
            if foundation_value != candidate_value {
                return counterexample_evidence(
                    budget,
                    executed_traces,
                    region,
                    sample,
                    foundation_value,
                    candidate_value,
                    "concrete emulator observation differs",
                );
            }
        }

        match prove_symbolically(foundation, region) {
            Ok(SymbolicOutcome::Equivalent) => (
                ValidationVerdict::Equivalent,
                ValidationEvidence {
                    strength: EvidenceStrength::Universal,
                    budget,
                    explored_paths: 1,
                    executed_traces,
                    assumptions: value_space_assumptions(region),
                    unresolved_effects: Vec::new(),
                    counterexample: None,
                    detail: format!(
                        "{executed_traces} emulator traces agreed; solver proved the \
                         single-path region equivalent for all admitted inputs"
                    ),
                },
            ),
            Ok(SymbolicOutcome::Counterexample(sample)) => {
                match execute_foundation_region(foundation, region, &sample) {
                    Ok(foundation_value) => {
                        let candidate_value = evaluate_expression(&region.expression, &sample);
                        if foundation_value != candidate_value {
                            counterexample_evidence(
                                budget,
                                executed_traces,
                                region,
                                &sample,
                                foundation_value,
                                candidate_value,
                                "solver counterexample replayed in the emulator",
                            )
                        } else {
                            inconclusive_backend(
                                budget,
                                executed_traces,
                                "solver returned a model that did not replay as a difference",
                            )
                        }
                    }
                    Err(detail) => inconclusive_backend(
                        budget,
                        executed_traces,
                        format!("solver model could not be replayed in emulator: {detail}"),
                    ),
                }
            }
            Ok(SymbolicOutcome::Unknown) => (
                ValidationVerdict::Equivalent,
                ValidationEvidence {
                    strength: EvidenceStrength::Traces,
                    budget,
                    explored_paths: 1,
                    executed_traces,
                    assumptions: value_space_assumptions(region),
                    unresolved_effects: vec![UnresolvedEffect {
                        kind: UnresolvedEffectKind::Other,
                        description: "solver returned unknown".to_string(),
                    }],
                    counterexample: None,
                    detail: format!(
                        "{executed_traces} emulator traces agreed; symbolic result was unknown"
                    ),
                },
            ),
            Err(detail) => (
                ValidationVerdict::Equivalent,
                ValidationEvidence {
                    strength: EvidenceStrength::Traces,
                    budget,
                    explored_paths: 1,
                    executed_traces,
                    assumptions: value_space_assumptions(region),
                    unresolved_effects: vec![UnresolvedEffect {
                        kind: UnresolvedEffectKind::Other,
                        description: detail.clone(),
                    }],
                    counterexample: None,
                    detail: format!(
                        "{executed_traces} emulator traces agreed; solver failed: {detail}"
                    ),
                },
            ),
        }
    }
}

fn validate_candidate_contract(
    foundation: &PcodeFoundation,
    candidate: &DirCandidate,
    region: &PcodeNativeRegion,
) -> Result<(), String> {
    let canonical =
        PcodeNativeReconstructor::new(region.selection, region.value_space_ids.iter().copied())
            .reconstruct(foundation)
            .map_err(|error| error.to_string())?
            .into_iter()
            .next()
            .ok_or_else(|| "canonical reconstruction produced no candidate".to_string())?;
    let CandidateSemantics::PcodeNativeRegion(canonical_region) = canonical.semantics else {
        return Err("canonical reconstruction produced unmodeled semantics".to_string());
    };
    if region.inputs != canonical_region.inputs {
        return Err("live-in locations or deterministic input order changed".to_string());
    }
    if candidate.pseudocode != render_region_pseudocode(region) {
        return Err("pseudocode does not match the executable candidate semantics".to_string());
    }

    let root_bits = validate_expression_contract(&region.expression, &region.inputs)?;
    let output_bits = bits_for_size(region.selection.output.size);
    if root_bits != output_bits {
        return Err(format!(
            "root expression is {root_bits} bits but output is {output_bits} bits"
        ));
    }
    Ok(())
}

fn validate_expression_contract(
    expression: &PcodeNativeExpr,
    inputs: &[PcodeNativeInput],
) -> Result<u32, String> {
    match expression {
        PcodeNativeExpr::Input { index, bits } => {
            let input = inputs
                .get(*index)
                .ok_or_else(|| format!("input index {index} is out of bounds"))?;
            let expected = bits_for_size(input.location.size);
            if *bits != expected {
                return Err(format!(
                    "input {index} is declared as {bits} bits but its location is {expected} bits"
                ));
            }
            Ok(*bits)
        }
        PcodeNativeExpr::Constant { bits, .. } => {
            validate_expression_bits(*bits)?;
            Ok(*bits)
        }
        PcodeNativeExpr::Binary {
            op,
            left,
            right,
            bits,
        } => {
            validate_expression_bits(*bits)?;
            let left_bits = validate_expression_contract(left, inputs)?;
            let right_bits = validate_expression_contract(right, inputs)?;
            if left_bits != right_bits {
                return Err(format!(
                    "{op:?} operands have different widths: {left_bits} and {right_bits}"
                ));
            }
            if matches!(
                op,
                PcodeNativeBinaryOp::Add
                    | PcodeNativeBinaryOp::Subtract
                    | PcodeNativeBinaryOp::BitAnd
                    | PcodeNativeBinaryOp::BitOr
                    | PcodeNativeBinaryOp::BitXor
            ) && left_bits != *bits
            {
                return Err(format!(
                    "{op:?} result is {bits} bits but operands are {left_bits} bits"
                ));
            }
            Ok(*bits)
        }
    }
}

fn validate_expression_bits(bits: u32) -> Result<(), String> {
    if (8..=64).contains(&bits) && bits.is_multiple_of(8) {
        Ok(())
    } else {
        Err(format!(
            "expression width {bits} is not an admitted 1..=8-byte width"
        ))
    }
}

fn supports_scope(scope: &ObservationScope, output: ObservedLocation) -> bool {
    !scope.return_value
        && !scope.exit_kind
        && scope.live_out_locations == [output]
        && scope.memory_ranges.is_empty()
        && scope.calls == CallObservation::Ignored
        && !scope.traps
}

fn value_space_assumptions(region: &PcodeNativeRegion) -> Vec<ValidationAssumption> {
    vec![ValidationAssumption {
        kind: AssumptionKind::Memory,
        description: format!(
            "P-code spaces {:?} are register/unique-style value spaces, not memory",
            region.value_space_ids
        ),
    }]
}

fn execute_foundation_region(
    foundation: &PcodeFoundation,
    region: &PcodeNativeRegion,
    sample: &[u64],
) -> Result<u64, String> {
    if sample.len() != region.inputs.len() {
        return Err(format!(
            "sample has {} values for {} live-ins",
            sample.len(),
            region.inputs.len()
        ));
    }
    let raw = foundation.pcode().as_raw();
    let block = raw
        .blocks
        .iter()
        .find(|block| block.index == region.selection.block_index)
        .ok_or_else(|| format!("block {} disappeared", region.selection.block_index))?;
    let end = region
        .selection
        .first_op
        .checked_add(region.selection.op_count)
        .ok_or_else(|| "region end overflowed".to_string())?;
    let ops = block
        .ops
        .get(region.selection.first_op..end)
        .ok_or_else(|| "region no longer fits its foundation block".to_string())?;

    let mut state = MachineState::new();
    for (input, value) in region.inputs.iter().zip(sample) {
        write_location(&mut state, input.location, *value)?;
    }
    let mut solver = Solver::new();
    let mut evaluator = Evaluator::new(&mut state, &mut solver);
    for op in ops {
        match evaluator.step(op).map_err(|error| error.to_string())? {
            StepResult::Next => {}
            _ => return Err(format!("opcode {:?} changed control flow", op.opcode)),
        }
    }
    read_location(&mut state, region.selection.output)
}

fn write_location(
    state: &mut MachineState,
    location: ObservedLocation,
    value: u64,
) -> Result<(), String> {
    let bytes = value.to_le_bytes();
    state
        .write_space(
            location.space_id,
            location.offset,
            &bytes[..location.size as usize],
        )
        .map_err(|error| error.to_string())
}

fn read_location(state: &mut MachineState, location: ObservedLocation) -> Result<u64, String> {
    let bytes = state
        .read_space(location.space_id, location.offset, location.size as usize)
        .map_err(|error| error.to_string())?;
    Ok(bytes
        .iter()
        .enumerate()
        .fold(0_u64, |value, (index, byte)| {
            value | (u64::from(*byte) << (index * 8))
        }))
}

enum SymbolicOutcome {
    Equivalent,
    Counterexample(Vec<u64>),
    Unknown,
}

fn prove_symbolically(
    foundation: &PcodeFoundation,
    region: &PcodeNativeRegion,
) -> Result<SymbolicOutcome, String> {
    let mut solver = Solver::new();
    let mut symbolic_inputs = Vec::with_capacity(region.inputs.len());
    for input in &region.inputs {
        let bits = bits_for_size(input.location.size);
        let expression = SymExpr::new_var(&input.name, bits);
        if let SymExpr::Var { id, .. } = expression {
            solver.nodes.insert(id, expression.clone());
        }
        symbolic_inputs.push(expression);
    }

    let foundation_expression =
        lower_foundation_symbolically(foundation, region, &symbolic_inputs)?;
    let candidate_expression = lower_candidate_symbolically(&region.expression, &symbolic_inputs)?;
    solver.assert(SymExpr::new_neq(
        foundation_expression,
        candidate_expression,
    ));

    match solver.check_sat().map_err(|error| error.to_string())? {
        SatResult::Unsat => Ok(SymbolicOutcome::Equivalent),
        SatResult::Unknown => Ok(SymbolicOutcome::Unknown),
        SatResult::Sat => {
            let sample = symbolic_inputs
                .iter()
                .map(|expression| {
                    solver
                        .bv_theory
                        .evaluate_expr_in_model(&solver.sat, expression)
                })
                .collect();
            Ok(SymbolicOutcome::Counterexample(sample))
        }
    }
}

fn lower_foundation_symbolically(
    foundation: &PcodeFoundation,
    region: &PcodeNativeRegion,
    inputs: &[SymExpr],
) -> Result<SymExpr, String> {
    let raw = foundation.pcode().as_raw();
    let block = raw
        .blocks
        .iter()
        .find(|block| block.index == region.selection.block_index)
        .ok_or_else(|| format!("block {} disappeared", region.selection.block_index))?;
    let end = region
        .selection
        .first_op
        .checked_add(region.selection.op_count)
        .ok_or_else(|| "region end overflowed".to_string())?;
    let ops = block
        .ops
        .get(region.selection.first_op..end)
        .ok_or_else(|| "region no longer fits its foundation block".to_string())?;

    let input_map = region
        .inputs
        .iter()
        .zip(inputs)
        .map(|(input, expression)| (input.location, expression.clone()))
        .collect::<HashMap<_, _>>();
    let mut values = HashMap::<ObservedLocation, SymExpr>::new();

    for op in ops {
        let output = op
            .output
            .as_ref()
            .ok_or_else(|| format!("opcode {:?} lost its output", op.opcode))?;
        let get = |varnode: &Varnode| -> Result<SymExpr, String> {
            if varnode.is_constant {
                return Ok(SymExpr::new_const(
                    mask_value(varnode.constant_val as u64, bits_for_size(varnode.size)),
                    bits_for_size(varnode.size),
                ));
            }
            let location = location_of(varnode);
            values
                .get(&location)
                .or_else(|| input_map.get(&location))
                .cloned()
                .ok_or_else(|| format!("missing symbolic live-in {location:?}"))
        };
        let expression = match op.opcode {
            PcodeOpcode::Copy => get(&op.inputs[0])?,
            PcodeOpcode::IntAdd => {
                SymExpr::Add(Box::new(get(&op.inputs[0])?), Box::new(get(&op.inputs[1])?))
            }
            PcodeOpcode::IntSub => {
                SymExpr::Sub(Box::new(get(&op.inputs[0])?), Box::new(get(&op.inputs[1])?))
            }
            PcodeOpcode::IntAnd => {
                SymExpr::And(Box::new(get(&op.inputs[0])?), Box::new(get(&op.inputs[1])?))
            }
            PcodeOpcode::IntOr => {
                SymExpr::Or(Box::new(get(&op.inputs[0])?), Box::new(get(&op.inputs[1])?))
            }
            PcodeOpcode::IntXor => {
                SymExpr::Xor(Box::new(get(&op.inputs[0])?), Box::new(get(&op.inputs[1])?))
            }
            PcodeOpcode::IntEqual => widen_boolean(
                SymExpr::Eq(Box::new(get(&op.inputs[0])?), Box::new(get(&op.inputs[1])?)),
                bits_for_size(output.size),
            ),
            PcodeOpcode::IntNotEqual => widen_boolean(
                SymExpr::Neq(Box::new(get(&op.inputs[0])?), Box::new(get(&op.inputs[1])?)),
                bits_for_size(output.size),
            ),
            PcodeOpcode::IntLess => widen_boolean(
                SymExpr::Ult(Box::new(get(&op.inputs[0])?), Box::new(get(&op.inputs[1])?)),
                bits_for_size(output.size),
            ),
            PcodeOpcode::IntLessEqual => widen_boolean(
                SymExpr::Ule(Box::new(get(&op.inputs[0])?), Box::new(get(&op.inputs[1])?)),
                bits_for_size(output.size),
            ),
            opcode => return Err(format!("unsupported symbolic P-code opcode {opcode:?}")),
        };
        values.insert(location_of(output), expression);
    }

    values
        .remove(&region.selection.output)
        .ok_or_else(|| "symbolic region did not define its selected output".to_string())
}

fn lower_candidate_symbolically(
    expression: &PcodeNativeExpr,
    inputs: &[SymExpr],
) -> Result<SymExpr, String> {
    match expression {
        PcodeNativeExpr::Input { index, .. } => inputs
            .get(*index)
            .cloned()
            .ok_or_else(|| format!("candidate input index {index} is out of bounds")),
        PcodeNativeExpr::Constant { value, bits } => {
            Ok(SymExpr::new_const(mask_value(*value, *bits), *bits))
        }
        PcodeNativeExpr::Binary {
            op,
            left,
            right,
            bits,
        } => {
            let left = lower_candidate_symbolically(left, inputs)?;
            let right = lower_candidate_symbolically(right, inputs)?;
            Ok(match op {
                PcodeNativeBinaryOp::Add => SymExpr::Add(Box::new(left), Box::new(right)),
                PcodeNativeBinaryOp::Subtract => SymExpr::Sub(Box::new(left), Box::new(right)),
                PcodeNativeBinaryOp::BitAnd => SymExpr::And(Box::new(left), Box::new(right)),
                PcodeNativeBinaryOp::BitOr => SymExpr::Or(Box::new(left), Box::new(right)),
                PcodeNativeBinaryOp::BitXor => SymExpr::Xor(Box::new(left), Box::new(right)),
                PcodeNativeBinaryOp::Equal => {
                    widen_boolean(SymExpr::Eq(Box::new(left), Box::new(right)), *bits)
                }
                PcodeNativeBinaryOp::NotEqual => {
                    widen_boolean(SymExpr::Neq(Box::new(left), Box::new(right)), *bits)
                }
                PcodeNativeBinaryOp::UnsignedLess => {
                    widen_boolean(SymExpr::Ult(Box::new(left), Box::new(right)), *bits)
                }
                PcodeNativeBinaryOp::UnsignedLessEqual => {
                    widen_boolean(SymExpr::Ule(Box::new(left), Box::new(right)), *bits)
                }
            })
        }
    }
}

fn widen_boolean(boolean: SymExpr, bits: u32) -> SymExpr {
    if bits == 1 {
        boolean
    } else {
        SymExpr::Ite {
            cond: Box::new(boolean),
            t: Box::new(SymExpr::new_const(1, bits)),
            f: Box::new(SymExpr::new_const(0, bits)),
        }
    }
}

fn evaluate_expression(expression: &PcodeNativeExpr, inputs: &[u64]) -> u64 {
    match expression {
        PcodeNativeExpr::Input { index, bits } => inputs
            .get(*index)
            .copied()
            .map_or(0, |value| mask_value(value, *bits)),
        PcodeNativeExpr::Constant { value, bits } => mask_value(*value, *bits),
        PcodeNativeExpr::Binary {
            op,
            left,
            right,
            bits,
        } => {
            let left = evaluate_expression(left, inputs);
            let right = evaluate_expression(right, inputs);
            let value = match op {
                PcodeNativeBinaryOp::Add => left.wrapping_add(right),
                PcodeNativeBinaryOp::Subtract => left.wrapping_sub(right),
                PcodeNativeBinaryOp::BitAnd => left & right,
                PcodeNativeBinaryOp::BitOr => left | right,
                PcodeNativeBinaryOp::BitXor => left ^ right,
                PcodeNativeBinaryOp::Equal => u64::from(left == right),
                PcodeNativeBinaryOp::NotEqual => u64::from(left != right),
                PcodeNativeBinaryOp::UnsignedLess => u64::from(left < right),
                PcodeNativeBinaryOp::UnsignedLessEqual => u64::from(left <= right),
            };
            mask_value(value, *bits)
        }
    }
}

fn deterministic_samples(inputs: &[PcodeNativeInput], limit: u64) -> Vec<Vec<u64>> {
    let limit = usize::try_from(limit).unwrap_or(usize::MAX);
    if limit == 0 {
        return Vec::new();
    }
    if inputs.is_empty() {
        return vec![Vec::new()];
    }

    let mut samples = Vec::new();
    let patterns = [0_u64, 1, u64::MAX, 0xaaaaaaaaaaaaaaaa, 0x5555555555555555];
    for pattern in patterns {
        push_unique_sample(
            &mut samples,
            inputs
                .iter()
                .map(|input| mask_value(pattern, bits_for_size(input.location.size)))
                .collect(),
            limit,
        );
    }
    for (index, input) in inputs.iter().enumerate() {
        let bits = bits_for_size(input.location.size);
        let max = mask_value(u64::MAX, bits);
        let sign = 1_u64 << (bits - 1);
        for value in [1, max, max.saturating_sub(1), sign, sign.saturating_sub(1)] {
            let mut sample = vec![0; inputs.len()];
            sample[index] = value;
            push_unique_sample(&mut samples, sample, limit);
        }
    }
    samples.truncate(limit);
    samples
}

fn push_unique_sample(samples: &mut Vec<Vec<u64>>, sample: Vec<u64>, limit: usize) {
    if samples.len() < limit && !samples.contains(&sample) {
        samples.push(sample);
    }
}

fn counterexample_evidence(
    budget: ValidationBudget,
    executed_traces: u64,
    region: &PcodeNativeRegion,
    sample: &[u64],
    foundation_value: u64,
    candidate_value: u64,
    detail: &str,
) -> (ValidationVerdict, ValidationEvidence) {
    (
        ValidationVerdict::Counterexample,
        ValidationEvidence {
            strength: EvidenceStrength::None,
            budget,
            explored_paths: 1,
            executed_traces,
            assumptions: Vec::new(),
            unresolved_effects: Vec::new(),
            counterexample: Some(DirCounterexample {
                inputs: region
                    .inputs
                    .iter()
                    .zip(sample)
                    .map(|(input, value)| (input.name.clone(), *value))
                    .collect(),
                foundation_observation: format!(
                    "{} = 0x{foundation_value:x}",
                    render_location(region.selection.output)
                ),
                candidate_observation: format!(
                    "{} = 0x{candidate_value:x}",
                    render_location(region.selection.output)
                ),
            }),
            detail: detail.to_string(),
        },
    )
}

fn unsupported(
    budget: ValidationBudget,
    kind: UnresolvedEffectKind,
    detail: &str,
) -> (ValidationVerdict, ValidationEvidence) {
    let mut evidence = ValidationEvidence::none(detail);
    evidence.budget = budget;
    evidence.unresolved_effects.push(UnresolvedEffect {
        kind,
        description: detail.to_string(),
    });
    (ValidationVerdict::Unsupported, evidence)
}

fn inconclusive_budget(
    budget: ValidationBudget,
    detail: &str,
) -> (ValidationVerdict, ValidationEvidence) {
    let mut evidence = ValidationEvidence::none(detail);
    evidence.budget = budget;
    evidence.unresolved_effects.push(UnresolvedEffect {
        kind: UnresolvedEffectKind::PathBudget,
        description: detail.to_string(),
    });
    (ValidationVerdict::Inconclusive, evidence)
}

fn inconclusive_backend(
    budget: ValidationBudget,
    executed_traces: u64,
    detail: impl Into<String>,
) -> (ValidationVerdict, ValidationEvidence) {
    let detail = detail.into();
    let mut evidence = ValidationEvidence::none(detail.clone());
    evidence.budget = budget;
    evidence.executed_traces = executed_traces;
    evidence.unresolved_effects.push(UnresolvedEffect {
        kind: UnresolvedEffectKind::Other,
        description: detail,
    });
    (ValidationVerdict::Inconclusive, evidence)
}

fn render_expression(expression: &PcodeNativeExpr, inputs: &[PcodeNativeInput]) -> String {
    match expression {
        PcodeNativeExpr::Input { index, .. } => inputs
            .get(*index)
            .map_or_else(|| format!("input_{index}"), |input| input.name.clone()),
        PcodeNativeExpr::Constant { value, .. } => format!("0x{value:x}"),
        PcodeNativeExpr::Binary {
            op, left, right, ..
        } => {
            let operator = match op {
                PcodeNativeBinaryOp::Add => "+",
                PcodeNativeBinaryOp::Subtract => "-",
                PcodeNativeBinaryOp::BitAnd => "&",
                PcodeNativeBinaryOp::BitOr => "|",
                PcodeNativeBinaryOp::BitXor => "^",
                PcodeNativeBinaryOp::Equal => "==",
                PcodeNativeBinaryOp::NotEqual => "!=",
                PcodeNativeBinaryOp::UnsignedLess => "<u",
                PcodeNativeBinaryOp::UnsignedLessEqual => "<=u",
            };
            format!(
                "({} {operator} {})",
                render_expression(left, inputs),
                render_expression(right, inputs)
            )
        }
    }
}

fn render_region_pseudocode(region: &PcodeNativeRegion) -> String {
    format!(
        "{} = {};",
        render_location(region.selection.output),
        render_expression(&region.expression, &region.inputs)
    )
}

fn input_name(location: ObservedLocation) -> String {
    format!(
        "in_s{}_o{:x}_n{}",
        location.space_id, location.offset, location.size
    )
}

fn render_location(location: ObservedLocation) -> String {
    format!(
        "s{}[0x{:x}:{}]",
        location.space_id, location.offset, location.size
    )
}

fn location_of(varnode: &Varnode) -> ObservedLocation {
    ObservedLocation {
        space_id: varnode.space_id,
        offset: varnode.offset,
        size: varnode.size,
    }
}

const fn bits_for_size(size: u32) -> u32 {
    size * 8
}

const fn mask_value(value: u64, bits: u32) -> u64 {
    if bits >= 64 {
        value
    } else {
        value & ((1_u64 << bits) - 1)
    }
}

#[cfg(test)]
mod tests {
    use fission_pcode::{PcodeBasicBlock, PcodeFunction};

    use super::*;
    use crate::product::{AssuranceBand, DirPipeline};

    fn varnode(space_id: u64, offset: u64, size: u32) -> Varnode {
        Varnode {
            space_id,
            offset,
            size,
            is_constant: false,
            constant_val: 0,
        }
    }

    fn arithmetic_foundation() -> (PcodeFoundation, PcodeRegionSelection) {
        let input_a = varnode(2, 0, 4);
        let input_b = varnode(2, 8, 4);
        let temporary = varnode(1, 0x100, 4);
        let output = varnode(2, 16, 4);
        let function = PcodeFunction {
            blocks: vec![PcodeBasicBlock {
                index: 7,
                start_address: 0x401000,
                successors: Vec::new(),
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x401000,
                        output: Some(temporary.clone()),
                        inputs: vec![input_a, Varnode::constant(1, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::IntXor,
                        address: 0x401004,
                        output: Some(output.clone()),
                        inputs: vec![temporary, input_b],
                        asm_mnemonic: None,
                    },
                ],
            }],
        };
        let selection = PcodeRegionSelection {
            block_index: 7,
            first_op: 0,
            op_count: 2,
            output: location_of(&output),
        };
        (PcodeFoundation::new(0x401000, function).unwrap(), selection)
    }

    #[test]
    fn reconstructs_and_proves_a_pcode_native_region() {
        let (foundation, selection) = arithmetic_foundation();
        let artifacts = DirPipeline::new(
            PcodeNativeReconstructor::new(selection, [1, 2]),
            PcodeObservationVerifier::default(),
        )
        .run(
            &foundation,
            &ObservationScope::location_only(selection.output),
            ValidationBudget::default(),
        )
        .unwrap();

        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].candidate.source, CandidateSource::PcodeNative);
        assert!(artifacts[0].candidate.pseudocode.contains("^"));
        assert_eq!(
            artifacts[0].assurance.band(),
            AssuranceBand::ConditionallyProven
        );
        assert!(artifacts[0].assurance.evidence().executed_traces > 0);
    }

    #[test]
    fn emulator_or_solver_rejects_a_tampered_candidate() {
        let (foundation, selection) = arithmetic_foundation();
        let mut candidate = PcodeNativeReconstructor::new(selection, [1, 2])
            .reconstruct(&foundation)
            .unwrap()
            .remove(0);
        {
            let CandidateSemantics::PcodeNativeRegion(region) = &mut candidate.semantics else {
                panic!("expected native region");
            };
            let PcodeNativeExpr::Binary { op, .. } = &mut region.expression else {
                panic!("expected binary expression");
            };
            *op = PcodeNativeBinaryOp::BitOr;
            candidate.pseudocode = render_region_pseudocode(region);
        }

        // The one concrete sample is all-zero and cannot distinguish XOR from
        // OR here; the symbolic query must find and replay the difference.
        let (verdict, evidence) = PcodeObservationVerifier::new(1).verify(
            &foundation,
            &candidate,
            &ObservationScope::location_only(selection.output),
            ValidationBudget::default(),
        );
        assert_eq!(verdict, ValidationVerdict::Counterexample);
        assert!(evidence.counterexample.is_some());
        assert!(evidence.detail.contains("solver counterexample"));
    }

    #[test]
    fn rejects_memory_regions_instead_of_approximating_them() {
        let output = varnode(1, 0, 4);
        let function = PcodeFunction {
            blocks: vec![PcodeBasicBlock {
                index: 0,
                start_address: 0x1000,
                successors: Vec::new(),
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Load,
                    address: 0x1000,
                    output: Some(output.clone()),
                    inputs: vec![Varnode::constant(3, 8), varnode(2, 0, 8)],
                    asm_mnemonic: None,
                }],
            }],
        };
        let foundation = PcodeFoundation::new(0x1000, function).unwrap();
        let error = PcodeNativeReconstructor::new(
            PcodeRegionSelection {
                block_index: 0,
                first_op: 0,
                op_count: 1,
                output: location_of(&output),
            },
            [1, 2],
        )
        .reconstruct(&foundation)
        .unwrap_err();

        assert!(matches!(
            error,
            PcodeNativeReconstructError::UnsupportedOpcode {
                opcode: PcodeOpcode::Load,
                ..
            }
        ));
    }

    #[test]
    fn rejects_an_observation_scope_with_hidden_effects() {
        let (foundation, selection) = arithmetic_foundation();
        let candidate = PcodeNativeReconstructor::new(selection, [1, 2])
            .reconstruct(&foundation)
            .unwrap()
            .remove(0);
        let (verdict, evidence) = PcodeObservationVerifier::default().verify(
            &foundation,
            &candidate,
            &ObservationScope::default(),
            ValidationBudget::default(),
        );

        assert_eq!(verdict, ValidationVerdict::Unsupported);
        assert_eq!(evidence.strength, EvidenceStrength::None);
    }

    #[test]
    fn refuses_to_assure_text_detached_from_candidate_semantics() {
        let (foundation, selection) = arithmetic_foundation();
        let mut candidate = PcodeNativeReconstructor::new(selection, [1, 2])
            .reconstruct(&foundation)
            .unwrap()
            .remove(0);
        candidate.pseudocode = "s2[0x10:4] = 0;".to_string();

        let (verdict, evidence) = PcodeObservationVerifier::default().verify(
            &foundation,
            &candidate,
            &ObservationScope::location_only(selection.output),
            ValidationBudget::default(),
        );
        assert_eq!(verdict, ValidationVerdict::Unsupported);
        assert!(evidence.detail.contains("pseudocode"));
    }

    #[test]
    fn rejects_overlapping_subregisters_in_the_first_slice() {
        let wide = varnode(2, 0, 8);
        let low = varnode(2, 0, 4);
        let output = varnode(1, 0x100, 8);
        let function = PcodeFunction {
            blocks: vec![PcodeBasicBlock {
                index: 0,
                start_address: 0x1000,
                successors: Vec::new(),
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::IntAdd,
                    address: 0x1000,
                    output: Some(output.clone()),
                    inputs: vec![wide, low],
                    asm_mnemonic: None,
                }],
            }],
        };
        let foundation = PcodeFoundation::new(0x1000, function).unwrap();
        let error = PcodeNativeReconstructor::new(
            PcodeRegionSelection {
                block_index: 0,
                first_op: 0,
                op_count: 1,
                output: location_of(&output),
            },
            [1, 2],
        )
        .reconstruct(&foundation)
        .unwrap_err();

        assert!(matches!(
            error,
            PcodeNativeReconstructError::OverlappingVarnodes { .. }
        ));
    }

    #[test]
    fn rejects_spaces_not_explicitly_admitted_as_non_memory() {
        let output = varnode(1, 0x100, 4);
        let function = PcodeFunction {
            blocks: vec![PcodeBasicBlock {
                index: 0,
                start_address: 0x1000,
                successors: Vec::new(),
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::IntAdd,
                    address: 0x1000,
                    output: Some(output.clone()),
                    inputs: vec![varnode(3, 0x2000, 4), Varnode::constant(1, 4)],
                    asm_mnemonic: None,
                }],
            }],
        };
        let foundation = PcodeFoundation::new(0x1000, function).unwrap();
        let error = PcodeNativeReconstructor::new(
            PcodeRegionSelection {
                block_index: 0,
                first_op: 0,
                op_count: 1,
                output: location_of(&output),
            },
            [1, 2],
        )
        .reconstruct(&foundation)
        .unwrap_err();

        assert_eq!(
            error,
            PcodeNativeReconstructError::UnadmittedAddressSpace {
                op_index: 0,
                space_id: 3,
            }
        );
    }

    #[test]
    fn comparison_output_is_zero_extended_to_its_storage_width() {
        let input_a = varnode(2, 0, 4);
        let input_b = varnode(2, 8, 4);
        let output = varnode(1, 0x100, 1);
        let function = PcodeFunction {
            blocks: vec![PcodeBasicBlock {
                index: 0,
                start_address: 0x1000,
                successors: Vec::new(),
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::IntLessEqual,
                    address: 0x1000,
                    output: Some(output.clone()),
                    inputs: vec![input_a, input_b],
                    asm_mnemonic: None,
                }],
            }],
        };
        let foundation = PcodeFoundation::new(0x1000, function).unwrap();
        let selection = PcodeRegionSelection {
            block_index: 0,
            first_op: 0,
            op_count: 1,
            output: location_of(&output),
        };
        let artifacts = DirPipeline::new(
            PcodeNativeReconstructor::new(selection, [1, 2]),
            PcodeObservationVerifier::default(),
        )
        .run(
            &foundation,
            &ObservationScope::location_only(selection.output),
            ValidationBudget::default(),
        )
        .unwrap();

        assert_eq!(
            artifacts[0].assurance.band(),
            AssuranceBand::ConditionallyProven
        );
    }
}
