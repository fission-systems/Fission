//! Fission-owned FSL source compiler and typed FIR package.
//!
//! This crate intentionally has no dependency on `fission-sleigh`, `.sla`,
//! JSON, or P-code. Its first vertical slice supports exact one-byte opcodes
//! and a small typed VM-stack/arithmetic FIR dialect.

pub mod jit;
pub mod package;
mod parser;

pub use jit::{emit_aot_object, JitDecoder, NativeFirOp, NativeLift};
pub use package::{
    AddressUnit, ByteOrder, CompiledInstruction, Evidence, FirOp, FslcPackage, IntegerSign,
    ValueDef, ValueId, ValueType, FSL_PACKAGE_VERSION,
};

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
            "the initial byte-opcode decoder supports at most 256 instructions",
        ));
    }

    let mut dispatch: [Option<u16>; 256] = [None; 256];
    let mut instructions: Vec<CompiledInstruction> = Vec::with_capacity(parsed.instructions.len());
    for (instruction_index, instruction) in parsed.instructions.into_iter().enumerate() {
        if let Some(previous) = dispatch[instruction.opcode as usize] {
            let previous = &instructions[previous as usize];
            return Err(FslError::at(
                instruction.line,
                instruction.column,
                format!(
                    "opcode 0x{:02x} overlaps instruction {:?}",
                    instruction.opcode, previous.name
                ),
            ));
        }

        let mut values = Vec::<ValueDef>::new();
        let mut value_ids = HashMap::<String, ValueId>::new();
        let mut ops = Vec::<FirOp>::new();
        for statement in instruction.statements {
            match statement {
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

        dispatch[instruction.opcode as usize] = Some(instruction_index as u16);
        instructions.push(CompiledInstruction {
            name: instruction.name,
            mnemonic: instruction.mnemonic,
            opcode: instruction.opcode,
            evidence: instruction.evidence,
            values,
            ops,
        });
    }

    Ok(FslcPackage {
        version: FSL_PACKAGE_VERSION,
        language: parsed.language,
        byte_order: parsed.byte_order,
        address_unit: parsed.address_unit,
        instructions,
        dispatch,
    })
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
