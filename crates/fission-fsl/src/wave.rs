//! Lane effects and execution projections of the same canonical FIR.
use std::fmt::Write;

use crate::semantics::width_mask;
use crate::{
    CompiledInstruction, DecodedInstruction, ExecutionStatus, FirOp, FslError, FslcPackage,
    MachineState, OutputLayer, ValueId,
};

/// Element width remains in ValueType; effect domains are derived from typed
/// producers, never from an architecture name or mnemonic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueDomain {
    Uniform,
    Mask(u16),
    Lanes(u16),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WaveState {
    pub scalar: MachineState,
    pub lanes: u16,
    pub exec: u64,
    /// Register-major layout: register * lanes + lane. All slots must exist,
    /// including inactive lanes. Reads capture all lanes; writes are masked.
    pub lane_registers: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WaveContract {
    pub lanes: u16,
    pub domains: Vec<ValueDomain>,
}

pub(crate) fn validate_domains(
    instruction: &CompiledInstruction,
) -> Result<Vec<ValueDomain>, FslError> {
    let mut domains = vec![ValueDomain::Uniform; instruction.values.len()];
    let mut extent = None;
    let error = || {
        FslError::at(1, 1, "incompatible FIR value domains; masks are not integer operands and lane values cannot enter uniform effects")
    };
    for op in &instruction.ops {
        let at = |id: ValueId| domains[usize::from(id.0)];
        let result = match *op {
            FirOp::IntConvert { output, input, .. } => {
                if at(input) != ValueDomain::Uniform {
                    return Err(error());
                }
                Some((output, ValueDomain::Uniform))
            }
            FirOp::IntConstant { output, .. } => Some((output, ValueDomain::Uniform)),
            FirOp::IntCompare {
                output,
                left,
                right,
                ..
            } => {
                if at(left) != ValueDomain::Uniform || at(right) != ValueDomain::Uniform {
                    return Err(error());
                }
                Some((output, ValueDomain::Uniform))
            }
            FirOp::LaneMaskRead { output, lanes } => {
                if extent.is_some_and(|n| n != lanes) {
                    return Err(error());
                }
                extent = Some(lanes);
                Some((output, ValueDomain::Mask(lanes)))
            }
            FirOp::LaneRead { output, mask, .. } => {
                let ValueDomain::Mask(lanes) = at(mask) else {
                    return Err(error());
                };
                Some((output, ValueDomain::Lanes(lanes)))
            }
            FirOp::LaneWrite { value, mask, .. } => {
                let ValueDomain::Mask(lanes) = at(mask) else {
                    return Err(error());
                };
                if at(value) != ValueDomain::Uniform && at(value) != ValueDomain::Lanes(lanes) {
                    return Err(error());
                }
                None
            }
            FirOp::IntAddWrap {
                output,
                left,
                right,
            } => {
                let domain = match (at(left), at(right)) {
                    (ValueDomain::Uniform, d) | (d, ValueDomain::Uniform)
                        if !matches!(d, ValueDomain::Mask(_)) =>
                    {
                        d
                    }
                    (ValueDomain::Lanes(a), ValueDomain::Lanes(b)) if a == b => {
                        ValueDomain::Lanes(a)
                    }
                    _ => return Err(error()),
                };
                Some((output, domain))
            }
            FirOp::FieldRead { output, .. } | FirOp::GuestPcRead { output } => {
                Some((output, ValueDomain::Uniform))
            }
            FirOp::RegisterWrite { value, .. }
            | FirOp::FlagWrite { value, .. }
            | FirOp::GuestNextPcWrite { value }
            | FirOp::VmStackPush { value } => {
                if at(value) != ValueDomain::Uniform {
                    return Err(error());
                }
                None
            }
            FirOp::IntAddCarry {
                output,
                left,
                right,
            } => {
                if at(left) != ValueDomain::Uniform || at(right) != ValueDomain::Uniform {
                    return Err(error());
                }
                Some((output, ValueDomain::Uniform))
            }
            FirOp::IntAddCarryIn {
                output,
                left,
                right,
                carry,
            }
            | FirOp::IntAddWrapCarry {
                output,
                left,
                right,
                carry,
            } => {
                if [left, right, carry]
                    .iter()
                    .any(|&v| at(v) != ValueDomain::Uniform)
                {
                    return Err(error());
                }
                Some((output, ValueDomain::Uniform))
            }
            FirOp::RegisterRead { output, .. }
            | FirOp::FlagRead { output, .. }
            | FirOp::VmStackPop { output } => Some((output, ValueDomain::Uniform)),
            FirOp::Unsupported => None,
        };
        if let Some((output, domain)) = result {
            domains[usize::from(output.0)] = domain;
        }
    }
    Ok(domains)
}

impl WaveContract {
    pub fn for_instruction(instruction: &CompiledInstruction) -> Result<Self, FslError> {
        instruction.validate()?;
        if crate::control::has_control(instruction) {
            return Err(FslError::at(
                1,
                1,
                "control FIR is unsupported by the wave backend",
            ));
        }
        if instruction.values.iter().any(|v| v.ty.bits > 64)
            || instruction.ops.iter().any(|op| {
                !matches!(
                    op,
                    FirOp::LaneMaskRead { .. }
                        | FirOp::LaneRead { .. }
                        | FirOp::LaneWrite { .. }
                        | FirOp::RegisterRead { .. }
                        | FirOp::RegisterWrite { .. }
                        | FirOp::FlagRead { .. }
                        | FirOp::FlagWrite { .. }
                        | FirOp::IntAddWrap { .. }
                )
            })
        {
            return Err(FslError::at(1, 1, "wave execution admits lane/register/flag effects and wrapping addition at widths 1..64 only"));
        }
        let lanes = instruction
            .ops
            .iter()
            .find_map(|op| match op {
                FirOp::LaneMaskRead { lanes, .. } => Some(*lanes),
                _ => None,
            })
            .ok_or_else(|| FslError::at(1, 1, "wave instruction needs an explicit mask extent"))?;
        Ok(Self {
            lanes,
            domains: validate_domains(instruction)?,
        })
    }
}

/// Every precondition is checked before effects, even for EXEC=0. Scalar
/// effects execute once; arithmetic broadcasts uniform inputs per lane.
pub fn execute_wave(
    package: &FslcPackage,
    decoded: &DecodedInstruction,
    state: &mut WaveState,
) -> Result<ExecutionStatus, FslError> {
    package.reencode(decoded, &[])?;
    let instruction = &package.instructions[decoded.instruction_index];
    let contract = WaveContract::for_instruction(instruction)?;
    let n = usize::from(contract.lanes);
    if state.lanes != contract.lanes
        || state.exec & !width_mask(contract.lanes) != 0
        || !state.lane_registers.len().is_multiple_of(n)
        || state.scalar.flags.iter().any(|&v| v > 1)
    {
        return Ok(ExecutionStatus::InvalidState);
    }
    let field = |f: u16| decoded.fields[usize::from(f)].1;
    let banks = state.lane_registers.len() / n;
    for op in &instruction.ops {
        let invalid = match *op {
            FirOp::LaneRead { field: f, bias, .. } => {
                field(f) < bias || field(f) - bias >= banks as u64
            }
            FirOp::LaneWrite { field: f, .. } => field(f) >= banks as u64,
            FirOp::RegisterRead { field: f, .. } | FirOp::RegisterWrite { field: f, .. } => {
                field(f) >= state.scalar.registers.len() as u64
            }
            FirOp::FlagRead { slot, .. } | FirOp::FlagWrite { slot, .. } => {
                usize::from(slot) >= state.scalar.flags.len()
            }
            _ => false,
        };
        if invalid {
            return Ok(ExecutionStatus::InvalidState);
        }
    }
    let mut values = vec![Vec::<u64>::new(); instruction.values.len()];
    for op in &instruction.ops {
        match *op {
            FirOp::LaneMaskRead { output, .. } => values[usize::from(output.0)] = vec![state.exec],
            FirOp::LaneRead {
                output,
                field: f,
                bias,
                ..
            } => {
                let start = (field(f) - bias) as usize * n;
                let mask = width_mask(instruction.values[usize::from(output.0)].ty.bits);
                values[usize::from(output.0)] = state.lane_registers[start..start + n]
                    .iter()
                    .map(|v| v & mask)
                    .collect();
            }
            FirOp::RegisterRead { output, field: f } => {
                values[usize::from(output.0)] = vec![
                    state.scalar.registers[field(f) as usize]
                        & width_mask(instruction.values[usize::from(output.0)].ty.bits),
                ]
            }
            FirOp::FlagRead { output, slot } => {
                values[usize::from(output.0)] = vec![state.scalar.flags[usize::from(slot)]]
            }
            FirOp::IntAddWrap {
                output,
                left,
                right,
            } => {
                let count = if matches!(
                    contract.domains[usize::from(output.0)],
                    ValueDomain::Lanes(_)
                ) {
                    n
                } else {
                    1
                };
                let mask = width_mask(instruction.values[usize::from(output.0)].ty.bits);
                let lhs = &values[usize::from(left.0)];
                let rhs = &values[usize::from(right.0)];
                let sum = (0..count)
                    .map(|lane| {
                        lhs[if lhs.len() == 1 { 0 } else { lane }]
                            .wrapping_add(rhs[if rhs.len() == 1 { 0 } else { lane }])
                            & mask
                    })
                    .collect();
                values[usize::from(output.0)] = sum;
            }
            FirOp::LaneWrite {
                field: f,
                value,
                mask,
            } => {
                let input = &values[usize::from(value.0)];
                let active = values[usize::from(mask.0)][0];
                for lane in 0..n {
                    if active & (1u64 << lane) != 0 {
                        state.lane_registers[field(f) as usize * n + lane] =
                            input[if input.len() == 1 { 0 } else { lane }];
                    }
                }
            }
            FirOp::RegisterWrite { field: f, value } => {
                state.scalar.registers[field(f) as usize] = values[usize::from(value.0)][0]
            }
            FirOp::FlagWrite { slot, value } => {
                state.scalar.flags[usize::from(slot)] = values[usize::from(value.0)][0]
            }
            _ => unreachable!("wave contract checked"),
        }
    }
    Ok(ExecutionStatus::Success)
}

pub(crate) fn emit_wave_instruction(
    instruction: &CompiledInstruction,
    layer: OutputLayer,
    symbol: &str,
) -> Result<String, FslError> {
    let contract = WaveContract::for_instruction(instruction)?;
    let n = contract.lanes;
    let c = layer == OutputLayer::C;
    let literal = |v: u64| {
        if c {
            format!("UINT64_C(0x{v:x})")
        } else {
            format!("0x{v:x}u64")
        }
    };
    let mut text = if c {
        format!("#include <stdint.h>\n#include <stddef.h>\n/* All arrays must be valid, disjoint storage; status 3 rejects before mutation. */\nuint32_t {symbol}(uint64_t *registers, size_t register_count, uint64_t *flags, size_t flag_count, uint64_t *lane_registers, size_t lane_slots, uint16_t lanes, uint64_t exec, const uint64_t *fields, size_t field_count) {{\n    if ((!registers && register_count) || (!flags && flag_count) || (!lane_registers && lane_slots) || (!fields && field_count)) return 3;\n")
    } else {
        format!("pub fn {symbol}(registers: &mut [u64], flags: &mut [u64], lane_registers: &mut [u64], lanes: u16, exec: u64, fields: &[u64]) -> u32 {{\n    let _ = &registers;\n")
    };
    let mut guard = |condition: String| {
        if c {
            writeln!(text, "    if ({condition}) return 3;").unwrap();
        } else {
            writeln!(text, "    if {condition} {{ return 3; }}").unwrap();
        }
    };
    guard(format!(
        "lanes != {n} || (exec & {}{}) != 0 || {} % {n} != 0",
        if c { "~" } else { "!" },
        literal(width_mask(n)),
        if c {
            "lane_slots"
        } else {
            "lane_registers.len()"
        }
    ));
    guard(format!(
        "{} != {}",
        if c { "field_count" } else { "fields.len()" },
        instruction.encoding.fields.len()
    ));
    for (i, field) in instruction.encoding.fields.iter().enumerate() {
        let mask = width_mask(field.bits);
        let fixed = ((instruction.encoding.mask >> field.offset) & u128::from(mask)) as u64;
        let expected = ((instruction.encoding.value >> field.offset) & u128::from(mask)) as u64;
        if field.bits < 64 {
            guard(format!("fields[{i}] > {}", literal(mask)));
        }
        if fixed != 0 {
            guard(format!(
                "(fields[{i}] & {}) != {}",
                literal(fixed),
                literal(expected)
            ));
        }
        for v in &field.excluded {
            guard(format!("fields[{i}] == {}", literal(*v)));
        }
    }
    for op in &instruction.ops {
        match *op {
            FirOp::LaneRead { field, bias, .. } => {
                if bias != 0 {
                    guard(format!("fields[{field}] < {}", literal(bias)));
                }
                guard(format!(
                    "fields[{field}] - {} >= {} / {n}",
                    literal(bias),
                    if c {
                        "lane_slots"
                    } else {
                        "lane_registers.len() as u64"
                    }
                ));
            }
            FirOp::LaneWrite { field, .. } => guard(format!(
                "fields[{field}] >= {} / {n}",
                if c {
                    "lane_slots"
                } else {
                    "lane_registers.len() as u64"
                }
            )),
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
        "    for (size_t i=0; i<flag_count; ++i) if (flags[i] > 1) return 3;\n"
    } else {
        "    if flags.iter().any(|&v| v > 1) { return 3; }\n"
    });
    let value = |v: ValueId, lane: bool| {
        let suffix = if lane && matches!(contract.domains[usize::from(v.0)], ValueDomain::Lanes(_))
        {
            "[lane]"
        } else {
            ""
        };
        format!("{}v{}{suffix}", if c { "" } else { "_" }, v.0)
    };
    let declare = |output: ValueId, expression: String, text: &mut String| {
        if matches!(
            contract.domains[usize::from(output.0)],
            ValueDomain::Lanes(_)
        ) {
            if c {
                writeln!(text, "    uint64_t v{}[{n}]; (void)v{};\n    for (size_t lane=0; lane<{n}; ++lane) v{}[lane] = {expression};", output.0, output.0, output.0).unwrap();
            } else {
                writeln!(text, "    let mut _v{} = [0u64; {n}];\n    for lane in 0..{n} {{ _v{}[lane] = {expression}; }}", output.0, output.0).unwrap();
            }
        } else if c {
            writeln!(
                text,
                "    uint64_t v{} = {expression}; (void)v{};",
                output.0, output.0
            )
            .unwrap();
        } else {
            writeln!(text, "    let _v{} = {expression};", output.0).unwrap();
        }
    };
    let index = |f: u16| {
        if c {
            format!("(size_t)fields[{f}]")
        } else {
            format!("fields[{f}] as usize")
        }
    };
    for op in &instruction.ops {
        match *op {
            FirOp::LaneMaskRead { output, .. } => declare(output, "exec".into(), &mut text),
            FirOp::LaneRead {
                output,
                field,
                bias,
                ..
            } => declare(
                output,
                format!(
                    "lane_registers[({} - {bias}) * {n} + lane] & {}",
                    index(field),
                    literal(width_mask(
                        instruction.values[usize::from(output.0)].ty.bits
                    ))
                ),
                &mut text,
            ),
            FirOp::RegisterRead { output, field } => declare(
                output,
                format!(
                    "registers[{}] & {}",
                    index(field),
                    literal(width_mask(
                        instruction.values[usize::from(output.0)].ty.bits
                    ))
                ),
                &mut text,
            ),
            FirOp::FlagRead { output, slot } => {
                declare(output, format!("flags[{slot}]"), &mut text)
            }
            FirOp::IntAddWrap {
                output,
                left,
                right,
            } => {
                let expression = if c {
                    format!("({} + {})", value(left, true), value(right, true))
                } else {
                    format!("{}.wrapping_add({})", value(left, true), value(right, true))
                };
                declare(
                    output,
                    format!(
                        "{expression} & {}",
                        literal(width_mask(
                            instruction.values[usize::from(output.0)].ty.bits
                        ))
                    ),
                    &mut text,
                );
            }
            FirOp::LaneWrite {
                field,
                value: input,
                mask,
            } => {
                if c {
                    writeln!(text, "    for (size_t lane=0; lane<{n}; ++lane) if (({} >> lane) & 1) lane_registers[{} * {n} + lane] = {};", value(mask, false), index(field), value(input, true)).unwrap();
                } else {
                    writeln!(text, "    for lane in 0..{n} {{ if ({} >> lane) & 1 != 0 {{ lane_registers[({}) * {n} + lane] = {}; }} }}", value(mask, false), index(field), value(input, true)).unwrap();
                }
            }
            FirOp::RegisterWrite {
                field,
                value: input,
            } => {
                writeln!(
                    text,
                    "    registers[{}] = {};",
                    index(field),
                    value(input, false)
                )
                .unwrap();
            }
            FirOp::FlagWrite { slot, value: input } => {
                writeln!(text, "    flags[{slot}] = {};", value(input, false)).unwrap();
            }
            _ => unreachable!("wave contract checked"),
        }
    }
    text.push_str(if c {
        "    return 0;\n}\n"
    } else {
        "    0\n}\n"
    });
    Ok(text)
}
