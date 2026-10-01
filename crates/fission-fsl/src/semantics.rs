//! Validation and executable meaning of the canonical FIR instruction body.

use std::collections::HashSet;

use crate::{CompiledInstruction, FirOp, FslError, ValueId};

impl CompiledInstruction {
    /// Validate SSA definitions, operand order, widths, and arithmetic types.
    /// This also applies to packages loaded from disk and edited by API users.
    pub fn validate(&self) -> Result<(), FslError> {
        self.encoding.validate()?;
        if self.ops.is_empty() || self.ops.len() > u16::MAX as usize {
            return Err(FslError::at(1, 1, "invalid FIR operation count"));
        }
        if self.values.len() > u16::MAX as usize {
            return Err(FslError::at(1, 1, "too many FIR values"));
        }
        let mut names = HashSet::new();
        for (index, value) in self.values.iter().enumerate() {
            if usize::from(value.id.0) != index {
                return Err(FslError::at(1, 1, "FIR values must be densely indexed"));
            }
            if value.name.is_empty() || !names.insert(&value.name) {
                return Err(FslError::at(
                    1,
                    1,
                    "FIR value names must be nonempty and unique",
                ));
            }
            if value.ty.bits == 0 || value.ty.bits > 4096 {
                return Err(FslError::at(1, 1, "invalid FIR integer width"));
            }
        }
        if self.ops.contains(&FirOp::Unsupported) {
            if self.ops != [FirOp::Unsupported] || !self.values.is_empty() {
                return Err(FslError::at(
                    1,
                    1,
                    "unsupported semantics must be the entire instruction body",
                ));
            }
            return Ok(());
        }
        let mut defined = vec![false; self.values.len()];
        let read = |id: ValueId, defined: &[bool]| -> Result<(), FslError> {
            if !defined.get(usize::from(id.0)).copied().unwrap_or(false) {
                return Err(FslError::at(
                    1,
                    1,
                    format!("FIR value {} is used before definition", id.0),
                ));
            }
            Ok(())
        };
        for op in &self.ops {
            let output = match *op {
                FirOp::Unsupported => unreachable!("unsupported body handled above"),
                FirOp::VmStackPop { output } => Some(output),
                FirOp::IntAddWrap {
                    output,
                    left,
                    right,
                } => {
                    read(left, &defined)?;
                    read(right, &defined)?;
                    let output_type = self
                        .values
                        .get(usize::from(output.0))
                        .map(|v| v.ty)
                        .ok_or_else(|| FslError::at(1, 1, "FIR output id is out of range"))?;
                    if self.values[usize::from(left.0)].ty != output_type
                        || self.values[usize::from(right.0)].ty != output_type
                    {
                        return Err(FslError::at(
                            1,
                            1,
                            "FIR wrapping add operand and result types differ",
                        ));
                    }
                    Some(output)
                }
                FirOp::VmStackPush { value } => {
                    read(value, &defined)?;
                    None
                }
            };
            if let Some(output) = output {
                let slot = defined
                    .get_mut(usize::from(output.0))
                    .ok_or_else(|| FslError::at(1, 1, "FIR output id is out of range"))?;
                if *slot {
                    return Err(FslError::at(1, 1, "FIR value is defined more than once"));
                }
                *slot = true;
            }
        }
        if defined.iter().any(|defined| !defined) {
            return Err(FslError::at(1, 1, "declared FIR value has no definition"));
        }
        Ok(())
    }
}

/// Stack requirements derived from ordered FIR effects, not another IR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StackContract {
    pub required_input: usize,
    pub extra_capacity: usize,
    pub final_delta: i64,
}

impl StackContract {
    /// Admit the current exact execution domain: bit vectors of 1..=64 bits.
    pub fn for_instruction(instruction: &CompiledInstruction) -> Result<Self, FslError> {
        instruction.validate()?;
        if instruction.ops.contains(&FirOp::Unsupported) {
            return Err(FslError::at(1, 1, "instruction has unsupported semantics"));
        }
        if instruction.values.iter().any(|value| value.ty.bits > 64) {
            return Err(FslError::at(1, 1, "execution output supports integer widths 1..=64; wider FIR is preserved but unsupported here"));
        }
        let mut delta = 0i64;
        let mut low = 0i64;
        let mut high = 0i64;
        for op in &instruction.ops {
            match op {
                FirOp::VmStackPop { .. } => delta -= 1,
                FirOp::VmStackPush { .. } => delta += 1,
                FirOp::IntAddWrap { .. } => {}
                FirOp::Unsupported => unreachable!("unsupported execution rejected"),
            }
            low = low.min(delta);
            high = high.max(delta);
        }
        Ok(Self {
            required_input: (-low) as usize,
            extra_capacity: high as usize,
            final_delta: delta,
        })
    }
}

/// Status ABI shared by the reference evaluator and recompiled C/Rust output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum ExecutionStatus {
    Success = 0,
    StackUnderflow = 1,
    CapacityExceeded = 2,
    InvalidState = 3,
}

/// Execute ordered FIR effects over untagged VM bit-vector stack slots.
/// Pop truncates to the declared type; add wraps at that width. Failure leaves
/// the active stack unchanged. This contract does not model JVM verification
/// or exceptions, and does not claim to execute a whole JVM method.
pub fn execute_instruction(
    instruction: &CompiledInstruction,
    stack: &mut Vec<u64>,
    capacity: usize,
) -> Result<ExecutionStatus, FslError> {
    let contract = StackContract::for_instruction(instruction)?;
    if stack.len() > capacity {
        return Ok(ExecutionStatus::InvalidState);
    }
    if stack.len() < contract.required_input {
        return Ok(ExecutionStatus::StackUnderflow);
    }
    if contract.extra_capacity > capacity - stack.len() {
        return Ok(ExecutionStatus::CapacityExceeded);
    }
    let mut values = vec![0u64; instruction.values.len()];
    for op in &instruction.ops {
        match *op {
            FirOp::Unsupported => unreachable!("unsupported execution rejected"),
            FirOp::VmStackPop { output } => {
                values[usize::from(output.0)] = stack.pop().expect("validated stack contract")
                    & width_mask(instruction.values[usize::from(output.0)].ty.bits);
            }
            FirOp::IntAddWrap {
                output,
                left,
                right,
            } => {
                values[usize::from(output.0)] = values[usize::from(left.0)]
                    .wrapping_add(values[usize::from(right.0)])
                    & width_mask(instruction.values[usize::from(output.0)].ty.bits);
            }
            FirOp::VmStackPush { value } => stack.push(values[usize::from(value.0)]),
        }
    }
    Ok(ExecutionStatus::Success)
}

pub(crate) fn width_mask(bits: u16) -> u64 {
    if bits == 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    }
}
