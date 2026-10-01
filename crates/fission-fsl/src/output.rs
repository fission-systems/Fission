//! Consume-only output projections from one canonical FIR body.

use std::fmt::Write;

use crate::semantics::width_mask;
use crate::{CompiledInstruction, FirOp, FslError, StackContract};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputLayer {
    Fir,
    C,
    Rust,
}

/// Project one FIR instruction into diagnostic FIR or compilable source.
/// Executable output uses unsigned bit-vector slots even for signed FIR types,
/// preserving wrapping arithmetic without signed-overflow undefined behavior.
/// Symbols must start with `fsl_` and contain ASCII identifier characters.
/// C callers must provide a depth pointer disjoint from the stack storage.
pub fn emit_instruction(
    instruction: &CompiledInstruction,
    layer: OutputLayer,
    symbol: &str,
) -> Result<String, FslError> {
    instruction.validate()?;
    if layer == OutputLayer::Fir {
        let mut text = format!(
            "instruction {} encoding={:?}\n",
            instruction.name, instruction.encoding
        );
        for op in &instruction.ops {
            match *op {
                FirOp::RegisterRead { output, field } => {
                    writeln!(
                        text,
                        "  %v{}: {} = register.read {}",
                        output.0,
                        instruction.values[usize::from(output.0)].ty,
                        instruction.encoding.fields[usize::from(field)].name
                    )
                    .unwrap();
                }
                FirOp::RegisterWrite { field, value } => {
                    writeln!(
                        text,
                        "  register.write {}, %v{}",
                        instruction.encoding.fields[usize::from(field)].name,
                        value.0
                    )
                    .unwrap();
                }
                FirOp::FlagWrite { slot, value } => {
                    writeln!(text, "  flag.write {slot}, %v{}", value.0).unwrap();
                }
                FirOp::IntAddCarry {
                    output,
                    left,
                    right,
                } => {
                    writeln!(
                        text,
                        "  %v{}: u1 = int.add.carry %v{}, %v{}",
                        output.0, left.0, right.0
                    )
                    .unwrap();
                }
                FirOp::Unsupported => text.push_str("  unsupported semantics\n"),
                FirOp::VmStackPop { output } => {
                    writeln!(
                        text,
                        "  %v{}: {} = vm.stack.pop",
                        output.0,
                        instruction.values[usize::from(output.0)].ty
                    )
                    .unwrap();
                }
                FirOp::IntAddWrap {
                    output,
                    left,
                    right,
                } => {
                    writeln!(
                        text,
                        "  %v{}: {} = int.add.wrap %v{}, %v{}",
                        output.0,
                        instruction.values[usize::from(output.0)].ty,
                        left.0,
                        right.0
                    )
                    .unwrap();
                }
                FirOp::VmStackPush { value } => {
                    writeln!(text, "  vm.stack.push %v{}", value.0).unwrap();
                }
            }
        }
        return Ok(text);
    }
    if !symbol.starts_with("fsl_")
        || !symbol
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_')
    {
        return Err(FslError::at(
            1,
            1,
            "output symbol must be an ASCII identifier starting with fsl_",
        ));
    }
    if instruction.ops.iter().any(FirOp::requires_state_version) {
        return crate::state::emit_state_instruction(instruction, layer, symbol);
    }
    let contract = StackContract::for_instruction(instruction)?;
    let mut text = match layer {
        OutputLayer::C => format!("#include <stdint.h>\n#include <stddef.h>\n\n/* Bit-vector slots; depth storage must be disjoint from stack storage.\n * Status: 0 success, 1 underflow, 2 capacity, 3 invalid state. */\nuint32_t {symbol}(uint64_t *stack, size_t *depth, size_t capacity) {{\n    if (stack == NULL || depth == NULL || *depth > capacity) return 3;\n"),
        OutputLayer::Rust => format!("pub fn {symbol}(stack: &mut [u64], depth: &mut usize) -> u32 {{\n    let capacity = stack.len();\n    if *depth > capacity {{ return 3; }}\n"),
        OutputLayer::Fir => unreachable!(),
    };
    if contract.required_input > 0 {
        match layer {
            OutputLayer::C => writeln!(
                text,
                "    if (*depth < {}) return 1;",
                contract.required_input
            )
            .unwrap(),
            OutputLayer::Rust => writeln!(
                text,
                "    if *depth < {} {{ return 1; }}",
                contract.required_input
            )
            .unwrap(),
            _ => unreachable!(),
        }
    }
    if contract.extra_capacity > 0 {
        match layer {
            OutputLayer::C => writeln!(
                text,
                "    if ({} > capacity - *depth) return 2;",
                contract.extra_capacity
            )
            .unwrap(),
            OutputLayer::Rust => writeln!(
                text,
                "    if {} > capacity - *depth {{ return 2; }}",
                contract.extra_capacity
            )
            .unwrap(),
            _ => unreachable!(),
        }
    }
    text.push_str(if layer == OutputLayer::C {
        "    size_t sp = *depth;\n"
    } else {
        "    let mut sp = *depth;\n"
    });
    for op in &instruction.ops {
        match (*op, layer) {
            (FirOp::VmStackPop { output }, OutputLayer::C) => {
                let mask = width_mask(instruction.values[usize::from(output.0)].ty.bits);
                writeln!(
                    text,
                    "    uint64_t v{} = stack[--sp] & UINT64_C(0x{mask:x});\n    (void)v{};",
                    output.0, output.0
                )
                .unwrap();
            }
            (FirOp::VmStackPop { output }, OutputLayer::Rust) => {
                let mask = width_mask(instruction.values[usize::from(output.0)].ty.bits);
                writeln!(
                    text,
                    "    sp -= 1;\n    let _v{} = stack[sp] & 0x{mask:x}u64;",
                    output.0
                )
                .unwrap();
            }
            (
                FirOp::IntAddWrap {
                    output,
                    left,
                    right,
                },
                OutputLayer::C,
            ) => {
                let mask = width_mask(instruction.values[usize::from(output.0)].ty.bits);
                writeln!(
                    text,
                    "    uint64_t v{} = (v{} + v{}) & UINT64_C(0x{mask:x});\n    (void)v{};",
                    output.0, left.0, right.0, output.0
                )
                .unwrap();
            }
            (
                FirOp::IntAddWrap {
                    output,
                    left,
                    right,
                },
                OutputLayer::Rust,
            ) => {
                let mask = width_mask(instruction.values[usize::from(output.0)].ty.bits);
                writeln!(
                    text,
                    "    let _v{} = _v{}.wrapping_add(_v{}) & 0x{mask:x}u64;",
                    output.0, left.0, right.0
                )
                .unwrap();
            }
            (FirOp::VmStackPush { value }, OutputLayer::C) => {
                writeln!(text, "    stack[sp++] = v{};", value.0).unwrap();
            }
            (FirOp::VmStackPush { value }, OutputLayer::Rust) => {
                writeln!(text, "    stack[sp] = _v{};\n    sp += 1;", value.0).unwrap();
            }
            (_, OutputLayer::Fir) => unreachable!(),
            (
                FirOp::RegisterRead { .. }
                | FirOp::RegisterWrite { .. }
                | FirOp::FlagWrite { .. }
                | FirOp::IntAddCarry { .. },
                _,
            ) => unreachable!("state projection handled above"),
            (FirOp::Unsupported, _) => unreachable!("unsupported execution rejected"),
        }
    }
    text.push_str(if layer == OutputLayer::C {
        "    *depth = sp;\n    return 0;\n}\n"
    } else {
        "    *depth = sp;\n    0\n}\n"
    });
    Ok(text)
}
