//! Combinational HDL and dependency views of borrowed canonical FIR.
//!
//! This is a bounded two-valued bit-vector projection. It does not provide
//! clocks, physical timing, device placement, bitstreams or continuous dynamics.
use crate::{
    CompiledInstruction, FirOp, FslError, IntBinaryOp, IntConversion, IntPredicate, StackContract,
    ValueId,
};
use std::fmt::Write;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AxisContract {
    pub id: &'static str,
    pub initial_slice: &'static str,
    pub unsupported: &'static str,
}

/// Research axes are contracts, not evidence that a target device is supported.
pub const AXES: &[AxisContract] = &[
    AxisContract {
        id: "fpga",
        initial_slice: "combinational bit-vector HDL",
        unsupported: "board mapping and bitstream execution",
    },
    AxisContract {
        id: "dsp",
        initial_slice: "authored fixed-width arithmetic kernels",
        unsupported: "vendor DSP instruction decoding",
    },
    AxisContract {
        id: "tensor",
        initial_slice: "fixed-shape scalar elaboration",
        unsupported: "first-class tensor layout and NPU or TPU execution",
    },
    AxisContract {
        id: "asic",
        initial_slice: "combinational HDL for generic synthesis",
        unsupported: "PDK cells physical design and fabrication",
    },
    AxisContract {
        id: "neuromorphic",
        initial_slice: "explicit externally advanced discrete state step",
        unsupported: "asynchronous spike timing and hardware execution",
    },
    AxisContract {
        id: "pim",
        initial_slice: "caller-supplied bank words and placement metadata",
        unsupported: "in-memory operations coherence and hardware execution",
    },
    AxisContract {
        id: "analog",
        initial_slice: "declared fixed-point discrete approximation",
        unsupported: "continuous dynamics circuit equations and error proof",
    },
    AxisContract {
        id: "dataflow",
        initial_slice: "acyclic canonical value dependencies",
        unsupported: "stream rates PE scheduling routing and CGRA execution",
    },
];

fn error(message: &str) -> FslError {
    FslError::at(1, 1, message)
}

#[derive(Clone, Copy, Debug)]
enum Wire {
    Input(usize),
    Value(ValueId),
}
impl Wire {
    fn name(self) -> String {
        match self {
            Self::Input(i) => format!("in{i}"),
            Self::Value(v) => format!("v{}", v.0),
        }
    }
}

/// Borrowing the single op/value table prevents mutation after admission.
/// The wrapper supplies exactly the minimum stack prefix, bottom slot first.
#[derive(Debug)]
pub struct FabricPlan<'a> {
    body: &'a CompiledInstruction,
    input_slots: usize,
    output_slots: usize,
}
impl<'a> FabricPlan<'a> {
    pub fn new(body: &'a CompiledInstruction) -> Result<Self, FslError> {
        body.validate()?;
        if !body.blocks.is_empty() || body.ops.len() > 512 || body.values.len() > 256 {
            return Err(error("fabric projection requires implicit straight-line FIR; at most 512 ops and 256 values"));
        }
        if body.ops.iter().any(|op| {
            !matches!(
                op,
                FirOp::VmStackPop { .. }
                    | FirOp::VmStackPush { .. }
                    | FirOp::IntConstant { .. }
                    | FirOp::IntAddWrap { .. }
                    | FirOp::IntBinary { .. }
                    | FirOp::IntConvert { .. }
                    | FirOp::IntCompare { .. }
            )
        }) {
            return Err(error("fabric projection refuses memory register lane quantum guest-PC and unsupported effects"));
        }
        let contract = StackContract::for_instruction(body)?;
        let outputs = i64::try_from(contract.required_input).map_err(|_| error("input extent"))?
            + contract.final_delta;
        if contract.required_input > 16 || !(1..=16).contains(&outputs) {
            return Err(error(
                "fabric projection admits at most 16 input slots and 1..16 output slots",
            ));
        }
        Ok(Self {
            body,
            input_slots: contract.required_input,
            output_slots: outputs as usize,
        })
    }
    pub fn input_slots(&self) -> usize {
        self.input_slots
    }
    pub fn output_slots(&self) -> usize {
        self.output_slots
    }

    /// Classical reference of exactly one combinational invocation. Recurrence
    /// or external time advance is supplied explicitly by the caller.
    pub fn evaluate(&self, inputs: &[u64]) -> Result<Vec<u64>, FslError> {
        if inputs.len() != self.input_slots {
            return Err(error("fabric reference input arity mismatch"));
        }
        let mut stack = inputs.to_vec();
        let capacity = self.input_slots + StackContract::for_instruction(self.body)?.extra_capacity;
        if crate::execute_instruction(self.body, &mut stack, capacity)?
            != crate::ExecutionStatus::Success
            || stack.len() != self.output_slots
        {
            return Err(error("fabric reference stack contract disagreement"));
        }
        Ok(stack)
    }

    fn symbol(symbol: &str) -> Result<(), FslError> {
        if !symbol.starts_with("fsl_")
            || symbol.len() > 64
            || !symbol
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_')
        {
            return Err(error(
                "fabric symbol requires fsl_ prefix and at most 64 ASCII identifier bytes",
            ));
        }
        Ok(())
    }

    /// Synthesizable Verilog-2005 subset. External unknown/high-impedance
    /// behavior is outside the FIR two-valued execution contract.
    pub fn emit_verilog(&self, symbol: &str) -> Result<String, FslError> {
        Self::symbol(symbol)?;
        let mut text = String::from("// Canonical FIR projection: combinational, two-valued, no clock or timing guarantee.\n");
        let ports: Vec<_> = (0..self.input_slots)
            .map(|i| format!("input wire [63:0] in{i}"))
            .chain((0..self.output_slots).map(|i| format!("output wire [63:0] out{i}")))
            .collect();
        writeln!(text, "module {symbol}(\n    {}\n);", ports.join(",\n    ")).unwrap();
        for value in &self.body.values {
            writeln!(text, "wire [{}:0] v{};", value.ty.bits - 1, value.id.0).unwrap();
        }
        let mut stack: Vec<_> = (0..self.input_slots).map(Wire::Input).collect();
        for (index, op) in self.body.ops.iter().enumerate() {
            writeln!(text, "// FIR operation {index}").unwrap();
            let result = match *op {
                FirOp::VmStackPop { output } => Some((
                    output,
                    stack
                        .pop()
                        .ok_or_else(|| error("stack admission disagreement"))?
                        .name(),
                )),
                FirOp::VmStackPush { value } => {
                    stack.push(Wire::Value(value));
                    None
                }
                FirOp::IntConstant { output, value } => Some((
                    output,
                    format!(
                        "{}'h{value:x}",
                        self.body.values[usize::from(output.0)].ty.bits
                    ),
                )),
                FirOp::IntAddWrap {
                    output,
                    left,
                    right,
                } => Some((output, format!("v{} + v{}", left.0, right.0))),
                FirOp::IntBinary {
                    output,
                    left,
                    right,
                    op,
                } => {
                    let expression = match op {
                        IntBinaryOp::ShiftLeft | IntBinaryOp::LogicalShiftRight => {
                            let bits = self.body.values[usize::from(output.0)].ty.bits;
                            let operator = if op == IntBinaryOp::ShiftLeft {
                                "<<"
                            } else {
                                ">>"
                            };
                            format!(
                                "(v{} >= 64'd{bits}) ? {bits}'d0 : (v{} {operator} v{})",
                                right.0, left.0, right.0
                            )
                        }
                        _ => {
                            let operator = match op {
                                IntBinaryOp::Sub => "-",
                                IntBinaryOp::And => "&",
                                IntBinaryOp::Or => "|",
                                IntBinaryOp::Xor => "^",
                                IntBinaryOp::Mul => "*",
                                _ => unreachable!(),
                            };
                            format!("v{} {operator} v{}", left.0, right.0)
                        }
                    };
                    Some((output, expression))
                }
                FirOp::IntConvert {
                    output,
                    input,
                    kind,
                } => Some((
                    output,
                    if kind == IntConversion::SignExtend {
                        format!("$signed(v{})", input.0)
                    } else {
                        format!("v{}", input.0)
                    },
                )),
                FirOp::IntCompare {
                    output,
                    left,
                    right,
                    predicate,
                } => Some((
                    output,
                    match predicate {
                        IntPredicate::Equal => format!("v{} == v{}", left.0, right.0),
                        IntPredicate::UnsignedLess => format!("v{} < v{}", left.0, right.0),
                        IntPredicate::SignedLess => {
                            format!("$signed(v{}) < $signed(v{})", left.0, right.0)
                        }
                    },
                )),
                _ => return Err(error("fabric admission disagreement")),
            };
            if let Some((id, expression)) = result {
                writeln!(text, "assign v{} = {expression};", id.0).unwrap();
            }
        }
        if stack.len() != self.output_slots {
            return Err(error("fabric output stack disagreement"));
        }
        for (index, source) in stack.into_iter().enumerate() {
            writeln!(text, "assign out{index} = {};", source.name()).unwrap();
        }
        text.push_str("endmodule\n");
        Ok(text)
    }

    /// A dependency view, not an independently executable IR, schedule or route.
    pub fn emit_dot(&self, symbol: &str) -> Result<String, FslError> {
        Self::symbol(symbol)?;
        let mut text =
            format!("digraph {symbol} {{\n  // Untimed dependency projection; no CGRA mapping.\n");
        let mut stack: Vec<_> = (0..self.input_slots).map(Wire::Input).collect();
        for i in 0..self.input_slots {
            writeln!(text, "  in{i} [label=\"input slot {i}\"];").unwrap();
        }
        for (index, op) in self.body.ops.iter().enumerate() {
            let result = match *op {
                FirOp::VmStackPop { output } => Some((
                    output,
                    "stack.pop",
                    vec![],
                    Some(
                        stack
                            .pop()
                            .ok_or_else(|| error("stack admission disagreement"))?,
                    ),
                )),
                FirOp::VmStackPush { value } => {
                    stack.push(Wire::Value(value));
                    None
                }
                FirOp::IntConstant { output, .. } => Some((output, "constant", vec![], None)),
                FirOp::IntAddWrap {
                    output,
                    left,
                    right,
                } => Some((output, "add.wrap", vec![left, right], None)),
                FirOp::IntBinary {
                    output,
                    left,
                    right,
                    op,
                } => Some((output, op.name(), vec![left, right], None)),
                FirOp::IntConvert { output, input, .. } => {
                    Some((output, "convert", vec![input], None))
                }
                FirOp::IntCompare {
                    output,
                    left,
                    right,
                    ..
                } => Some((output, "compare", vec![left, right], None)),
                _ => return Err(error("fabric admission disagreement")),
            };
            if let Some((id, label, operands, input)) = result {
                writeln!(
                    text,
                    "  v{} [label=\"FIR op {index}: {label} ({} bits)\"];",
                    id.0,
                    self.body.values[usize::from(id.0)].ty.bits
                )
                .unwrap();
                for operand in operands {
                    writeln!(text, "  v{} -> v{};", operand.0, id.0).unwrap();
                }
                if let Some(source) = input {
                    writeln!(text, "  {} -> v{};", source.name(), id.0).unwrap();
                }
            }
        }
        for (index, source) in stack.into_iter().enumerate() {
            writeln!(
                text,
                "  out{index} [label=\"output slot {index}\"];\n  {} -> out{index};",
                source.name()
            )
            .unwrap();
        }
        text.push_str("}\n");
        Ok(text)
    }
}
