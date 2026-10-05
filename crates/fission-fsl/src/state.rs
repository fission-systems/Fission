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

pub(crate) fn admit(instruction: &CompiledInstruction) -> Result<(), FslError> {
    admit_with(instruction, false)
}

/// Like `admit`, but also admits guest PC effects. Only a sequence that supplies
/// an origin address and consumes the next-PC result may use this.
pub(crate) fn admit_guest(instruction: &CompiledInstruction) -> Result<(), FslError> {
    admit_with(instruction, true)
}

pub(crate) fn uses_guest_pc(instruction: &CompiledInstruction) -> bool {
    instruction.ops.iter().any(|op| {
        matches!(
            op,
            FirOp::GuestPcRead { .. } | FirOp::GuestNextPcWrite { .. }
        )
    })
}

fn admit_with(instruction: &CompiledInstruction, guest_pc: bool) -> Result<(), FslError> {
    instruction.validate()?;
    if !guest_pc && uses_guest_pc(instruction) {
        return Err(FslError::at(
            1,
            1,
            "guest PC effects require a sequence origin and cannot execute standalone",
        ));
    }
    crate::control::validate_acyclic(instruction)?;
    if instruction.values.iter().any(|v| v.ty.bits > 64)
        || instruction.ops.iter().any(|op| {
            matches!(
                op,
                FirOp::Unsupported
                    | FirOp::VmStackPop { .. }
                    | FirOp::VmStackPush { .. }
                    | FirOp::LaneMaskRead { .. }
                    | FirOp::LaneRead { .. }
                    | FirOp::LaneWrite { .. }
            )
        })
    {
        return Err(FslError::at(1, 1, "register execution requires supported 1..64 bit state FIR; stack effects cannot be mixed"));
    }
    if !instruction.ops.iter().any(|op| {
        matches!(
            op,
            FirOp::RegisterRead { .. }
                | FirOp::FlagRead { .. }
                | FirOp::RegisterWrite { .. }
                | FirOp::FlagWrite { .. }
                | FirOp::GuestNextPcWrite { .. }
        )
    }) {
        return Err(FslError::at(
            1,
            1,
            "no register, flag or guest next-pc effects in instruction",
        ));
    }
    Ok(())
}

/// Fields must already be validated against the canonical package observation.
pub(crate) fn bank_indices_fit(
    instruction: &CompiledInstruction,
    decoded: &DecodedInstruction,
    register_count: usize,
    flag_count: usize,
) -> bool {
    instruction.ops.iter().all(|op| match *op {
        FirOp::RegisterRead { field, .. } | FirOp::RegisterWrite { field, .. } => {
            decoded.fields[usize::from(field)].1 < register_count as u64
        }
        FirOp::FlagRead { slot, .. } | FirOp::FlagWrite { slot, .. } => {
            usize::from(slot) < flag_count
        }
        _ => true,
    })
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
    execute_decoded_at(package, decoded, state, None).map(|(status, _)| status)
}

/// `guest_pc` is the origin address when executed inside a sequence; only then
/// are guest PC ops admitted. Returns the optional next-PC written by the body
/// (`None` means fallthrough). A second next-PC write is `InvalidState`.
pub(crate) fn execute_decoded_at(
    package: &FslcPackage,
    decoded: &DecodedInstruction,
    state: &mut MachineState,
    guest_pc: Option<u64>,
) -> Result<(ExecutionStatus, Option<u64>), FslError> {
    package.reencode(decoded, &[])?;
    let instruction = &package.instructions[decoded.instruction_index];
    if guest_pc.is_some() {
        admit_guest(instruction)?;
    } else {
        admit(instruction)?;
    }
    let mut next_pc: Option<u64> = None;
    if state.flags.iter().any(|&v| v > 1) {
        return Ok((ExecutionStatus::InvalidState, None));
    }
    let index = |field: u16| decoded.fields[usize::from(field)].1;
    if !bank_indices_fit(
        instruction,
        decoded,
        state.registers.len(),
        state.flags.len(),
    ) {
        return Ok((ExecutionStatus::InvalidState, None));
    }
    let mut values = vec![0u64; instruction.values.len()];
    let blocks = crate::control::blocks(instruction);
    let mut pc = 0;
    loop {
        let block = &blocks[pc];
        for op in &instruction.ops[usize::from(block.start)..usize::from(block.end)] {
            match *op {
                FirOp::IntConvert {
                    output,
                    input,
                    kind,
                } => {
                    values[usize::from(output.0)] = crate::control::convert(
                        values[usize::from(input.0)],
                        instruction.values[usize::from(input.0)].ty.bits,
                        instruction.values[usize::from(output.0)].ty.bits,
                        kind,
                    );
                }
                FirOp::IntConstant { output, value } => values[usize::from(output.0)] = value,
                FirOp::IntCompare {
                    output,
                    left,
                    right,
                    predicate,
                } => {
                    values[usize::from(output.0)] = crate::control::compare(
                        values[usize::from(left.0)],
                        values[usize::from(right.0)],
                        instruction.values[usize::from(left.0)].ty.bits,
                        predicate,
                    );
                }
                FirOp::RegisterRead { output, field } => {
                    values[usize::from(output.0)] = state.registers[index(field) as usize]
                        & width_mask(instruction.values[usize::from(output.0)].ty.bits)
                }
                FirOp::FlagRead { output, slot } => {
                    values[usize::from(output.0)] = state.flags[usize::from(slot)]
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
                FirOp::IntAddCarryIn {
                    output,
                    left,
                    right,
                    carry,
                } => {
                    let bits = instruction.values[usize::from(left.0)].ty.bits;
                    let sum = u128::from(values[usize::from(left.0)])
                        + u128::from(values[usize::from(right.0)])
                        + u128::from(values[usize::from(carry.0)]);
                    values[usize::from(output.0)] = (sum >> bits) as u64;
                }
                FirOp::IntAddWrapCarry {
                    output,
                    left,
                    right,
                    carry,
                } => {
                    let bits = instruction.values[usize::from(output.0)].ty.bits;
                    let mask = width_mask(bits);
                    let sum = u128::from(values[usize::from(left.0)])
                        + u128::from(values[usize::from(right.0)])
                        + u128::from(values[usize::from(carry.0)]);
                    values[usize::from(output.0)] = (sum & u128::from(mask)) as u64;
                }
                FirOp::FieldRead { output, field } => values[usize::from(output.0)] = index(field),
                FirOp::GuestPcRead { output } => {
                    values[usize::from(output.0)] = guest_pc.expect("guest pc admitted with origin")
                }
                FirOp::GuestNextPcWrite { value } => {
                    if next_pc.is_some() {
                        return Ok((ExecutionStatus::InvalidState, None));
                    }
                    next_pc = Some(values[usize::from(value.0)]);
                }
                _ => unreachable!("state contract checked before effects"),
            }
        }
        match crate::control::advance(block, &blocks, &mut values) {
            Some(next) => pc = next,
            None => return Ok((ExecutionStatus::Success, next_pc)),
        }
    }
}

/// C arrays (registers, flags and fields) must be disjoint valid storage for the
/// supplied lengths; Rust slices enforce this safe borrowing contract.
pub(crate) fn emit_state_instruction(
    instruction: &CompiledInstruction,
    layer: OutputLayer,
    symbol: &str,
) -> Result<String, FslError> {
    emit_state_impl(instruction, layer, symbol, false)
}

/// Sequence-only projection. Bodies using guest PC ops gain the extra parameters
/// `guest_pc`, `next_pc`, `next_pc_set`; all other bodies keep the standalone ABI.
pub(crate) fn emit_state_instruction_guest(
    instruction: &CompiledInstruction,
    layer: OutputLayer,
    symbol: &str,
) -> Result<String, FslError> {
    emit_state_impl(instruction, layer, symbol, true)
}

fn emit_state_impl(
    instruction: &CompiledInstruction,
    layer: OutputLayer,
    symbol: &str,
    guest: bool,
) -> Result<String, FslError> {
    admit_with(instruction, guest)?;
    let pc_abi = guest && uses_guest_pc(instruction);
    let c = layer == OutputLayer::C;
    let mut text = if c {
        format!("#include <stdint.h>\n#include <stddef.h>\n/* Disjoint arrays; status 0 success, 3 invalid state. */\nuint32_t {symbol}(uint64_t *registers, size_t register_count, uint64_t *flags, size_t flag_count, const uint64_t *fields, size_t field_count{}) {{\n    if (!registers || !flags || !fields{}) return 3;\n",
            if pc_abi { ", uint64_t guest_pc, uint64_t *next_pc, uint32_t *next_pc_set" } else { "" },
            if pc_abi { " || !next_pc || !next_pc_set" } else { "" })
    } else {
        format!(
            "pub fn {symbol}(registers: &mut [u64], flags: &mut [u64], fields: &[u64]{}) -> u32 {{\n",
            if pc_abi { ", guest_pc: u64, next_pc: &mut u64, next_pc_set: &mut bool" } else { "" }
        )
    };
    if !instruction
        .ops
        .iter()
        .any(|op| matches!(op, FirOp::RegisterRead { .. } | FirOp::RegisterWrite { .. }))
    {
        text.push_str(if c {
            "    (void)register_count;\n"
        } else {
            "    let _ = registers;\n"
        });
    }
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
            FirOp::FlagRead { slot, .. } | FirOp::FlagWrite { slot, .. } => guard(format!(
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
    let structured = crate::control::has_control(instruction);
    let blocks = crate::control::blocks(instruction);
    if structured {
        let mutable_pc = blocks
            .iter()
            .any(|b| b.terminator != crate::FirTerminator::Return);
        if c {
            writeln!(
                text,
                "uint64_t v[{}] = {{0}}; (void)v;\nsize_t pc = 0;\nfor (;;) {{ switch (pc) {{",
                instruction.values.len().max(1)
            )
            .unwrap();
        } else {
            writeln!(
                text,
                "let mut v = [0u64; {}];\nlet {}pc = 0usize;\nloop {{ match pc {{",
                instruction.values.len().max(1),
                if mutable_pc { "mut " } else { "" }
            )
            .unwrap();
        }
    }
    let value = |id: crate::ValueId| {
        if structured {
            format!("v[{}]", id.0)
        } else {
            format!("{}v{}", if c { "" } else { "_" }, id.0)
        }
    };
    for (block_index, block) in blocks.iter().enumerate() {
        if structured {
            writeln!(
                text,
                "{} {{",
                if c {
                    format!("case {block_index}:")
                } else {
                    format!("{block_index} =>")
                }
            )
            .unwrap();
        }
        for op in &instruction.ops[usize::from(block.start)..usize::from(block.end)] {
            let definition = match *op {
                FirOp::IntConvert {
                    output,
                    input,
                    kind,
                } => Some((
                    output,
                    crate::control::conversion_expression(
                        instruction,
                        output,
                        input,
                        kind,
                        &value(input),
                        c,
                    ),
                )),
                FirOp::IntConstant { output, value } => Some((
                    output,
                    format!("0x{value:x}{}", if c { "ULL" } else { "u64" }),
                )),
                FirOp::IntCompare {
                    output,
                    left,
                    right,
                    predicate,
                } => {
                    let expression = match predicate {
                        crate::IntPredicate::Equal => {
                            format!("{} == {}", value(left), value(right))
                        }
                        crate::IntPredicate::UnsignedLess => {
                            format!("{} < {}", value(left), value(right))
                        }
                        crate::IntPredicate::SignedLess => {
                            let sign =
                                1u64 << (instruction.values[usize::from(left.0)].ty.bits - 1);
                            let suffix = if c { "ULL" } else { "u64" };
                            format!(
                                "({} ^ 0x{sign:x}{suffix}) < ({} ^ 0x{sign:x}{suffix})",
                                value(left),
                                value(right)
                            )
                        }
                    };
                    Some((
                        output,
                        if c {
                            format!("({expression})")
                        } else {
                            format!("({expression}) as u64")
                        },
                    ))
                }
                FirOp::RegisterRead { output, field } => {
                    let mask = width_mask(instruction.values[usize::from(output.0)].ty.bits);
                    let expression = if c {
                        format!("registers[f{field}] & UINT64_C(0x{mask:x})")
                    } else {
                        format!("registers[_f{field}] & 0x{mask:x}u64")
                    };
                    Some((output, expression))
                }
                FirOp::FlagRead { output, slot } => Some((
                    output,
                    if c {
                        format!("flags[{slot}] & UINT64_C(0x1)")
                    } else {
                        format!("flags[{slot}] & 0x1u64")
                    },
                )),
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
                FirOp::IntAddCarryIn {
                    output,
                    left,
                    right,
                    carry,
                } => {
                    let mask = width_mask(instruction.values[usize::from(left.0)].ty.bits);
                    let expression = if c {
                        format!(
                        "({} > UINT64_C(0x{mask:x}) - {} || ({} != 0 && {} == UINT64_C(0x{mask:x}) - {}))",
                        value(left),
                        value(right),
                        value(carry),
                        value(left),
                        value(right)
                    )
                    } else {
                        format!(
                        "u64::from({} > 0x{mask:x}u64 - {} || ({} != 0 && {} == 0x{mask:x}u64 - {}))",
                        value(left),
                        value(right),
                        value(carry),
                        value(left),
                        value(right)
                    )
                    };
                    Some((output, expression))
                }
                FirOp::IntAddWrapCarry {
                    output,
                    left,
                    right,
                    carry,
                } => {
                    let mask = width_mask(instruction.values[usize::from(output.0)].ty.bits);
                    Some((
                        output,
                        if c {
                            format!(
                                "({} + {} + {}) & UINT64_C(0x{mask:x})",
                                value(left),
                                value(right),
                                value(carry)
                            )
                        } else {
                            format!(
                                "{}.wrapping_add({}).wrapping_add({}) & 0x{mask:x}u64",
                                value(left),
                                value(right),
                                value(carry)
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
                FirOp::FieldRead { output, field } => Some((output, format!("fields[{field}]"))),
                FirOp::GuestPcRead { output } => Some((output, "guest_pc".to_string())),
                FirOp::GuestNextPcWrite { value: input } => {
                    if c {
                        writeln!(text, "    if (*next_pc_set) return 3;\n    *next_pc = {};\n    *next_pc_set = 1;", value(input)).unwrap();
                    } else {
                        writeln!(text, "    if *next_pc_set {{ return 3; }}\n    *next_pc = {};\n    *next_pc_set = true;", value(input)).unwrap();
                    }
                    None
                }
                _ => unreachable!("state contract checked"),
            };
            if let Some((output, expression)) = definition {
                if structured {
                    writeln!(text, "{} = {expression};", value(output)).unwrap();
                } else if c {
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
        if structured {
            crate::control::emit_terminator(&mut text, block, &blocks, c, "return 0;\n");
            text.push_str("}\n");
        }
    }
    if structured {
        text.push_str(if c {
            "default: return 3;\n} }\n}\n"
        } else {
            "_ => return 3,\n} }\n}\n"
        });
        return Ok(text);
    }
    text.push_str(if c {
        "    return 0;\n}\n"
    } else {
        "    0\n}\n"
    });
    Ok(text)
}
