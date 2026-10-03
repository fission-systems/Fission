//! Fission-owned FSL source compiler and typed FIR package.
//!
//! This crate intentionally has no dependency on `fission-sleigh`, `.sla`,
//! JSON, or P-code. Fixed-width encoding plans support 8/32/64/128-bit words.
//! Executable FIR currently supports a small typed VM-stack/arithmetic dialect
//! and narrow register/flag and masked-lane state dialects; other GPU semantic bodies remain
//! explicitly unsupported.

pub mod abi;
pub mod encoding;
mod gpu_output;
pub mod jit;
pub mod library;
pub mod output;
pub mod package;
mod parser;
pub mod registers;
pub mod semantics;
mod state;
mod wave;
pub use state::{execute_decoded, MachineState};
pub use wave::{execute_wave, ValueDomain, WaveContract, WaveState};

pub use encoding::{BitField, DecodedInstruction, Encoding};
pub use jit::{emit_aot_object, JitDecoder, NativeFirOp, NativeLift};
pub use output::{emit_instruction, OutputLayer};
pub use package::{
    AddressUnit, ByteOrder, CompiledInstruction, Evidence, FirOp, FslcPackage, IntegerSign,
    ValueDef, ValueId, ValueType, FSL_PACKAGE_VERSION,
};
pub use semantics::{execute_instruction, ExecutionStatus, StackContract};

use std::collections::HashMap;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FslError {
    pub line: usize,
    pub column: usize,
    pub message: String,
}

impl FslError {
    fn at(line: usize, column: usize, message: impl Into<String>) -> Self {
        Self {
            line,
            column,
            message: message.into(),
        }
    }
}

impl fmt::Display for FslError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.column, self.message)
    }
}

impl std::error::Error for FslError {}

/// Parse and type-check one FSL source file into a portable compiled package.
pub fn compile_source(source: &str) -> Result<FslcPackage, FslError> {
    let parsed = parser::parse(source)?;
    if parsed.instructions.is_empty() {
        return Err(FslError::at(
            1,
            1,
            "language must define at least one instruction",
        ));
    }
    if parsed.instructions.len() > 256 {
        return Err(FslError::at(
            1,
            1,
            "the initial package supports at most 256 instructions",
        ));
    }

    let mut dispatch: [Option<u16>; 256] = [None; 256];
    let mut instructions: Vec<CompiledInstruction> = Vec::with_capacity(parsed.instructions.len());
    for (instruction_index, instruction) in parsed.instructions.into_iter().enumerate() {
        if let Some(opcode) = instruction.encoding.opcode() {
            if dispatch[opcode as usize].is_some() {
                return Err(FslError::at(
                    instruction.line,
                    instruction.column,
                    "duplicate byte opcode",
                ));
            }
        }

        let mut values = Vec::<ValueDef>::new();
        let mut value_ids = HashMap::<String, ValueId>::new();
        let mut ops = Vec::<FirOp>::new();
        let field_id = |name: &str| -> Result<u16, FslError> {
            instruction
                .encoding
                .fields
                .iter()
                .position(|f| f.name == name)
                .map(|i| i as u16)
                .ok_or_else(|| {
                    FslError::at(
                        instruction.line,
                        instruction.column,
                        format!("unknown register index field {name}"),
                    )
                })
        };
        for statement in instruction.statements {
            match statement {
                parser::Statement::LaneMaskRead {
                    name,
                    ty,
                    lanes,
                    line,
                    column,
                } => {
                    let output = define_value(&mut values, &mut value_ids, name, ty, line, column)?;
                    ops.push(FirOp::LaneMaskRead { output, lanes });
                }
                parser::Statement::LaneRead {
                    name,
                    ty,
                    field,
                    bias,
                    mask,
                    line,
                    column,
                } => {
                    let mask = require_value(&values, &value_ids, &mask, line, column)?;
                    let output = define_value(&mut values, &mut value_ids, name, ty, line, column)?;
                    ops.push(FirOp::LaneRead {
                        output,
                        field: field_id(&field)?,
                        bias,
                        mask,
                    });
                }
                parser::Statement::LaneWrite {
                    field,
                    value,
                    mask,
                    line,
                    column,
                } => {
                    let value = require_value(&values, &value_ids, &value, line, column)?;
                    let mask = require_value(&values, &value_ids, &mask, line, column)?;
                    ops.push(FirOp::LaneWrite {
                        field: field_id(&field)?,
                        value,
                        mask,
                    });
                }
                parser::Statement::RegisterRead {
                    name,
                    ty,
                    field,
                    line,
                    column,
                } => {
                    let output = define_value(&mut values, &mut value_ids, name, ty, line, column)?;
                    ops.push(FirOp::RegisterRead {
                        output,
                        field: field_id(&field)?,
                    });
                }
                parser::Statement::FlagRead {
                    name,
                    ty,
                    slot,
                    line,
                    column,
                } => {
                    let output = define_value(&mut values, &mut value_ids, name, ty, line, column)?;
                    ops.push(FirOp::FlagRead { output, slot });
                }
                parser::Statement::RegisterWrite {
                    field,
                    value,
                    line,
                    column,
                } => {
                    let value = require_value(&values, &value_ids, &value, line, column)?;
                    ops.push(FirOp::RegisterWrite {
                        field: field_id(&field)?,
                        value,
                    });
                }
                parser::Statement::FlagWrite {
                    slot,
                    value,
                    line,
                    column,
                } => {
                    let value = require_value(&values, &value_ids, &value, line, column)?;
                    ops.push(FirOp::FlagWrite { slot, value });
                }
                parser::Statement::AddCarry {
                    name,
                    ty,
                    left,
                    right,
                    line,
                    column,
                } => {
                    let left = require_value(&values, &value_ids, &left, line, column)?;
                    let right = require_value(&values, &value_ids, &right, line, column)?;
                    let output = define_value(&mut values, &mut value_ids, name, ty, line, column)?;
                    ops.push(FirOp::IntAddCarry {
                        output,
                        left,
                        right,
                    });
                }
                parser::Statement::AddCarryIn {
                    name,
                    ty,
                    left,
                    right,
                    carry,
                    line,
                    column,
                } => {
                    let left = require_value(&values, &value_ids, &left, line, column)?;
                    let right = require_value(&values, &value_ids, &right, line, column)?;
                    let carry = require_value(&values, &value_ids, &carry, line, column)?;
                    let output = define_value(&mut values, &mut value_ids, name, ty, line, column)?;
                    ops.push(FirOp::IntAddCarryIn {
                        output,
                        left,
                        right,
                        carry,
                    });
                }
                parser::Statement::Unsupported => ops.push(FirOp::Unsupported),
                parser::Statement::StackPop {
                    name,
                    ty,
                    line,
                    column,
                } => {
                    let id = define_value(&mut values, &mut value_ids, name, ty, line, column)?;
                    ops.push(FirOp::VmStackPop { output: id });
                }
                parser::Statement::AddWrap {
                    name,
                    ty,
                    left,
                    right,
                    line,
                    column,
                } => {
                    let left_id = require_value(&values, &value_ids, &left, line, column)?;
                    let right_id = require_value(&values, &value_ids, &right, line, column)?;
                    let left_ty = &values[left_id.0 as usize].ty;
                    let right_ty = &values[right_id.0 as usize].ty;
                    if left_ty != &ty || right_ty != &ty {
                        return Err(FslError::at(
                            line,
                            column,
                            format!(
                                "wrapping add has type {ty}, but operands are {left_ty} and {right_ty}"
                            ),
                        ));
                    }
                    let output = define_value(&mut values, &mut value_ids, name, ty, line, column)?;
                    ops.push(FirOp::IntAddWrap {
                        output,
                        left: left_id,
                        right: right_id,
                    });
                }
                parser::Statement::AddWrapCarry {
                    name,
                    ty,
                    left,
                    right,
                    carry,
                    line,
                    column,
                } => {
                    let left_id = require_value(&values, &value_ids, &left, line, column)?;
                    let right_id = require_value(&values, &value_ids, &right, line, column)?;
                    let carry_id = require_value(&values, &value_ids, &carry, line, column)?;
                    let left_ty = &values[left_id.0 as usize].ty;
                    let right_ty = &values[right_id.0 as usize].ty;
                    if left_ty != &ty || right_ty != &ty {
                        return Err(FslError::at(
                            line,
                            column,
                            format!(
                                "carry-in add has type {ty}, but operands are {left_ty} and {right_ty}"
                            ),
                        ));
                    }
                    let output = define_value(&mut values, &mut value_ids, name, ty, line, column)?;
                    ops.push(FirOp::IntAddWrapCarry {
                        output,
                        left: left_id,
                        right: right_id,
                        carry: carry_id,
                    });
                }
                parser::Statement::StackPush {
                    value,
                    line,
                    column,
                } => {
                    let value = require_value(&values, &value_ids, &value, line, column)?;
                    ops.push(FirOp::VmStackPush { value });
                }
            }
        }
        if ops.is_empty() {
            return Err(FslError::at(
                instruction.line,
                instruction.column,
                "instruction semantics cannot be empty",
            ));
        }

        if let Some(opcode) = instruction.encoding.opcode() {
            dispatch[opcode as usize] = Some(instruction_index as u16);
        }
        instructions.push(CompiledInstruction {
            name: instruction.name,
            mnemonic: instruction.mnemonic,
            encoding: instruction.encoding,
            evidence: instruction.evidence,
            values,
            ops,
        });
    }

    let package = FslcPackage {
        version: if instructions
            .iter()
            .flat_map(|i| &i.ops)
            .any(FirOp::requires_lane_version)
        {
            FSL_PACKAGE_VERSION
        } else if instructions
            .iter()
            .flat_map(|i| &i.ops)
            .any(FirOp::requires_carry_in_version)
        {
            4
        } else if instructions
            .iter()
            .flat_map(|i| &i.ops)
            .any(FirOp::requires_state_version)
        {
            3
        } else {
            2
        },
        language: parsed.language,
        byte_order: parsed.byte_order,
        address_unit: parsed.address_unit,
        instructions,
        dispatch,
    };
    package.validate()?;
    Ok(package)
}

fn define_value(
    values: &mut Vec<ValueDef>,
    value_ids: &mut HashMap<String, ValueId>,
    name: String,
    ty: ValueType,
    line: usize,
    column: usize,
) -> Result<ValueId, FslError> {
    if value_ids.contains_key(&name) {
        return Err(FslError::at(
            line,
            column,
            format!("FIR value %{name} is defined more than once"),
        ));
    }
    if values.len() >= u16::MAX as usize {
        return Err(FslError::at(
            line,
            column,
            "instruction defines too many FIR values for the current package format",
        ));
    }
    let index = u16::try_from(values.len())
        .map_err(|_| FslError::at(line, column, "instruction defines too many FIR values"))?;
    let id = ValueId(index);
    value_ids.insert(name.clone(), id);
    values.push(ValueDef { id, name, ty });
    Ok(id)
}

fn require_value(
    values: &[ValueDef],
    value_ids: &HashMap<String, ValueId>,
    name: &str,
    line: usize,
    column: usize,
) -> Result<ValueId, FslError> {
    let id = value_ids
        .get(name)
        .copied()
        .ok_or_else(|| FslError::at(line, column, format!("unknown FIR value %{name}")))?;
    if values.get(id.0 as usize).is_none() {
        return Err(FslError::at(
            line,
            column,
            "invalid internal FIR value reference",
        ));
    }
    Ok(id)
}
