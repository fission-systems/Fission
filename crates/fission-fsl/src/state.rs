//! Generic register/flag execution projection of the canonical FIR.
use std::fmt::Write;

use crate::semantics::width_mask;
use crate::{
    CompiledInstruction, DecodedInstruction, ExecutionStatus, FirOp, FslError, FslcPackage,
    OutputLayer,
};

/// One register bank of untagged bit-vector slots and one bank of u1 flags.
/// Profile evidence supplies architectural mappings (e.g. flag 0 = SCC).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachineState {
    pub registers: Vec<u64>,
    pub flags: Vec<u64>,
}

fn admit(instruction: &CompiledInstruction) -> Result<(), FslError> {
    instruction.validate()?;
    if instruction.values.iter().any(|v| v.ty.bits > 64)
        || instruction.ops.iter().any(|op| {
            matches!(
                op,
                FirOp::Unsupported | FirOp::VmStackPop { .. } | FirOp::VmStackPush { .. }
            )
        })
    {
        return Err(FslError::at(1, 1, "register execution requires supported 1..64 bit state FIR; stack effects cannot be mixed"));
    }
    if !instruction.ops.iter().any(|op| {
        matches!(
            op,
            FirOp::RegisterRead { .. } | FirOp::RegisterWrite { .. } | FirOp::FlagWrite { .. }
        )
    }) {
        return Err(FslError::at(
            1,
            1,
            "no register or flag effects in instruction",
        ));
    }
    Ok(())
}

/// Validate the decoded observation against its package before execution.
/// All preconditions are checked before mutations; rejected execution preserves
/// registers and flags. Aliasing between source/destination SGPRs is supported
/// through ordered FIR effects, with no mnemonic-specific executor code.
pub fn execute_decoded(
    package: &FslcPackage,
    decoded: &DecodedInstruction,
    state: &mut MachineState,
) -> Result<ExecutionStatus, FslError> {
    package.reencode(decoded, &[])?;
    let instruction = &package.instructions[decoded.instruction_index];
    admit(instruction)?;
    if state.flags.iter().any(|&v| v > 1) {
        return Ok(ExecutionStatus::InvalidState);
    }
    let index = |field: u16| decoded.fields[usize::from(field)].1;
    for op in &instruction.ops {
        match *op {
            FirOp::RegisterRead { field, .. } | FirOp::RegisterWrite { field, .. }
                if index(field) >= state.registers.len() as u64 =>
            {
                return Ok(ExecutionStatus::InvalidState)
            }
            FirOp::FlagWrite { slot, .. } if usize::from(slot) >= state.flags.len() => {
                return Ok(ExecutionStatus::InvalidState)
            }
            _ => {}
        }
    }
    let mut values = vec![0u64; instruction.values.len()];
    for op in &instruction.ops {
        match *op {
            FirOp::RegisterRead { output, field } => {
                values[usize::from(output.0)] = state.registers[index(field) as usize]
                    & width_mask(instruction.values[usize::from(output.0)].ty.bits)
            }
            FirOp::RegisterWrite { field, value } => {
                state.registers[index(field) as usize] = values[usize::from(value.0)]
            }
            FirOp::FlagWrite { slot, value } => {
                state.flags[usize::from(slot)] = values[usize::from(value.0)]
            }
            FirOp::IntAddWrap {
                output,
                left,
                right,
            } => {
                values[usize::from(output.0)] = values[usize::from(left.0)]
                    .wrapping_add(values[usize::from(right.0)])
                    & width_mask(instruction.values[usize::from(output.0)].ty.bits)
            }
            FirOp::IntAddCarry {
                output,
                left,
                right,
            } => {
                let bits = instruction.values[usize::from(left.0)].ty.bits;
                values[usize::from(output.0)] = ((u128::from(values[usize::from(left.0)])
                    + u128::from(values[usize::from(right.0)]))
                    >> bits) as u64;
            }
            _ => unreachable!("state contract checked before effects"),
        }
    }
    Ok(ExecutionStatus::Success)
}

/// C arrays (registers, flags and fields) must be disjoint valid storage for the
/// supplied lengths; Rust slices enforce this safe borrowing contract.
pub(crate) fn emit_state_instruction(
    instruction: &CompiledInstruction,
    layer: OutputLayer,
    symbol: &str,
) -> Result<String, FslError> {
    admit(instruction)?;
    let c = layer == OutputLayer::C;
    let mut text = if c {
        format!("#include <stdint.h>\n#include <stddef.h>\n/* Disjoint arrays; status 0 success, 3 invalid state. */\nuint32_t {symbol}(uint64_t *registers, size_t register_count, uint64_t *flags, size_t flag_count, const uint64_t *fields, size_t field_count) {{\n    if (!registers || !flags || !fields) return 3;\n")
    } else {
        format!(
            "pub fn {symbol}(registers: &mut [u64], flags: &mut [u64], fields: &[u64]) -> u32 {{\n"
        )
    };
    let mut guard = |condition: String| {
        if c {
            writeln!(text, "    if ({condition}) return 3;").unwrap();
        } else {
            writeln!(text, "    if {condition} {{ return 3; }}").unwrap();
        }
    };
    guard(format!(
        "{} != {}",
        if c { "field_count" } else { "fields.len()" },
        instruction.encoding.fields.len()
    ));
    for (i, field) in instruction.encoding.fields.iter().enumerate() {
        let mask = width_mask(field.bits);
        let fixed_mask = ((instruction.encoding.mask >> field.offset) & u128::from(mask)) as u64;
        let fixed_value = ((instruction.encoding.value >> field.offset) & u128::from(mask)) as u64;
        let literal = |value: u64| {
            if c {
                format!("UINT64_C(0x{value:x})")
            } else {
                format!("0x{value:x}u64")
            }
        };
        guard(format!("fields[{i}] > {}", literal(mask)));
        if fixed_mask != 0 {
            guard(format!(
                "(fields[{i}] & {}) != {}",
                literal(fixed_mask),
                literal(fixed_value)
            ));
        }
        for value in &field.excluded {
            guard(format!("fields[{i}] == {}", literal(*value)));
        }
    }
    for op in &instruction.ops {
        match op {
            FirOp::RegisterRead { field, .. } | FirOp::RegisterWrite { field, .. } => {
                guard(format!(
                    "fields[{field}] >= {}",
                    if c {
                        "register_count"
                    } else {
                        "registers.len() as u64"
                    }
                ))
            }
            FirOp::FlagWrite { slot, .. } => guard(format!(
                "{} <= {slot}",
                if c { "flag_count" } else { "flags.len()" }
            )),
            _ => {}
        }
    }
    text.push_str(if c {
        "    for (size_t i = 0; i < flag_count; ++i) if (flags[i] > 1) return 3;\n"
    } else {
        "    if flags.iter().any(|&value| value > 1) { return 3; }\n"
    });
    for (i, _) in instruction.encoding.fields.iter().enumerate() {
        if c {
            writeln!(text, "    size_t f{i} = (size_t)fields[{i}]; (void)f{i};").unwrap();
        } else {
            writeln!(text, "    let _f{i} = fields[{i}] as usize;").unwrap();
        }
    }
    let value = |id: crate::ValueId| format!("{}v{}", if c { "" } else { "_" }, id.0);
    for op in &instruction.ops {
        let definition = match *op {
            FirOp::RegisterRead { output, field } => {
                let mask = width_mask(instruction.values[usize::from(output.0)].ty.bits);
                let expression = if c {
                    format!("registers[f{field}] & UINT64_C(0x{mask:x})")
                } else {
                    format!("registers[_f{field}] & 0x{mask:x}u64")
                };
                Some((output, expression))
            }
            FirOp::IntAddWrap {
                output,
                left,
                right,
            } => {
                let mask = width_mask(instruction.values[usize::from(output.0)].ty.bits);
                Some((
                    output,
                    if c {
                        format!(
                            "({} + {}) & UINT64_C(0x{mask:x})",
                            value(left),
                            value(right)
                        )
                    } else {
                        format!(
                            "{}.wrapping_add({}) & 0x{mask:x}u64",
                            value(left),
                            value(right)
                        )
                    },
                ))
            }
            FirOp::IntAddCarry {
                output,
                left,
                right,
            } => {
                let mask = width_mask(instruction.values[usize::from(left.0)].ty.bits);
                Some((
                    output,
                    if c {
                        format!("{} > UINT64_C(0x{mask:x}) - {}", value(left), value(right))
                    } else {
                        format!(
                            "u64::from({} > 0x{mask:x}u64 - {})",
                            value(left),
                            value(right)
                        )
                    },
                ))
            }
            FirOp::RegisterWrite {
                field,
                value: input,
            } => {
                writeln!(
                    text,
                    "    registers[{}f{field}] = {};",
                    if c { "" } else { "_" },
                    value(input)
                )
                .unwrap();
                None
            }
            FirOp::FlagWrite { slot, value: input } => {
                writeln!(text, "    flags[{slot}] = {};", value(input)).unwrap();
                None
            }
            _ => unreachable!("state contract checked"),
        };
        if let Some((output, expression)) = definition {
            if c {
                writeln!(
                    text,
                    "    uint64_t {} = {expression}; (void){};",
                    value(output),
                    value(output)
                )
                .unwrap();
            } else {
                writeln!(text, "    let {} = {expression};", value(output)).unwrap();
            }
        }
    }
    text.push_str(if c {
        "    return 0;\n}\n"
    } else {
        "    0\n}\n"
    });
    Ok(text)
}
