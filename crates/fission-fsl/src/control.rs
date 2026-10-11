//! Block structure and stack execution of the same canonical FIR op table.
use crate::semantics::width_mask;
use crate::{
    CompiledInstruction, ExecutionStatus, FirBlock, FirEdge, FirOp, FirTerminator, FslError,
    IntPredicate, IntegerSign, OutputLayer, StackContract, ValueId,
};
use std::collections::{HashSet, VecDeque};
use std::fmt::Write;

fn error(message: &str) -> FslError {
    FslError::at(1, 1, message)
}

pub(crate) fn has_control(instruction: &CompiledInstruction) -> bool {
    !instruction.blocks.is_empty() || instruction.ops.iter().any(FirOp::requires_control_version)
}

/// Legacy bodies have an implicit entry block and return, without altering
/// their package bytes or duplicating the op table.
pub fn blocks(instruction: &CompiledInstruction) -> Vec<FirBlock> {
    if instruction.blocks.is_empty() {
        vec![FirBlock {
            name: "entry".into(),
            parameters: vec![],
            start: 0,
            end: instruction.ops.len() as u16,
            terminator: FirTerminator::Return,
        }]
    } else {
        instruction.blocks.clone()
    }
}

pub(crate) fn edges(terminator: &FirTerminator) -> Vec<&FirEdge> {
    match terminator {
        FirTerminator::Return => vec![],
        FirTerminator::Branch(edge) => vec![edge],
        FirTerminator::CondBranch {
            on_true, on_false, ..
        } => vec![on_true, on_false],
    }
}

pub(crate) fn validate_structure(
    instruction: &CompiledInstruction,
) -> Result<Vec<FirBlock>, FslError> {
    let blocks = blocks(instruction);
    if blocks.len() > 256 || !blocks[0].parameters.is_empty() {
        return Err(error(
            "entry has no external block parameters; maximum 256 blocks",
        ));
    }
    let mut end = 0;
    let mut names = HashSet::new();
    for block in &blocks {
        if block.name.is_empty()
            || block.name.len() > 65535
            || !names.insert(&block.name)
            || block.start != end
            || block.start > block.end
            || usize::from(block.end) > instruction.ops.len()
        {
            return Err(error("invalid block name or op ownership range"));
        }
        end = block.end;
        for edge in edges(&block.terminator) {
            let target = blocks
                .get(usize::from(edge.target))
                .ok_or_else(|| error("branch target out of range"))?;
            if edge.arguments.len() != target.parameters.len() {
                return Err(error("branch argument arity mismatch"));
            }
            for (&arg, &param) in edge.arguments.iter().zip(&target.parameters) {
                let a = instruction
                    .values
                    .get(usize::from(arg.0))
                    .ok_or_else(|| error("branch argument id out of range"))?;
                let p = instruction
                    .values
                    .get(usize::from(param.0))
                    .ok_or_else(|| error("block parameter id out of range"))?;
                if a.ty != p.ty {
                    return Err(error("branch argument type mismatch"));
                }
            }
        }
    }
    if usize::from(end) != instruction.ops.len() {
        return Err(error("blocks must own all FIR ops exactly once"));
    }
    let mut reached = vec![false; blocks.len()];
    let mut queue = VecDeque::from([0]);
    while let Some(index) = queue.pop_front() {
        if reached[index] {
            continue;
        }
        reached[index] = true;
        for edge in edges(&blocks[index].terminator) {
            queue.push_back(usize::from(edge.target));
        }
    }
    if reached.iter().any(|&v| !v) {
        return Err(error("unreachable FIR block"));
    }
    Ok(blocks)
}

pub(crate) fn validate_terminator(
    instruction: &CompiledInstruction,
    block: &FirBlock,
    defined: &[bool],
) -> Result<(), FslError> {
    let read = |id: ValueId| -> Result<(), FslError> {
        if !defined.get(usize::from(id.0)).copied().unwrap_or(false) {
            return Err(error("terminator uses a value outside its block scope"));
        }
        Ok(())
    };
    if let FirTerminator::CondBranch { condition, .. } = block.terminator {
        read(condition)?;
        let ty = instruction.values[usize::from(condition.0)].ty;
        if ty.bits != 1 || ty.sign != IntegerSign::Unsigned {
            return Err(error("conditional branch requires u1"));
        }
    }
    for edge in edges(&block.terminator) {
        for &arg in &edge.arguments {
            read(arg)?;
        }
    }
    Ok(())
}

/// Conservative preflight over every syntactic path. Equal stack deltas are
/// required at joins and returns; cyclic graphs remain representable but this
/// execution backend refuses them. No effects occur before preflight succeeds.
pub(crate) fn stack_contract(instruction: &CompiledInstruction) -> Result<StackContract, FslError> {
    if instruction.values.iter().any(|v| v.ty.bits > 64)
        || instruction.ops.iter().any(|op| {
            !matches!(
                op,
                FirOp::IntConvert { .. }
                    | FirOp::IntConstant { .. }
                    | FirOp::IntCompare { .. }
                    | FirOp::IntAddWrap { .. }
                    | FirOp::VmStackPop { .. }
                    | FirOp::VmStackPush { .. }
            )
        })
    {
        return Err(error(
            "control execution supports 1..64 bit integer/stack FIR only",
        ));
    }
    let blocks = blocks(instruction);
    let mut indegree = vec![0usize; blocks.len()];
    for block in &blocks {
        for edge in edges(&block.terminator) {
            indegree[usize::from(edge.target)] += 1;
        }
    }
    let mut queue: VecDeque<_> = indegree
        .iter()
        .enumerate()
        .filter_map(|(i, &d)| (d == 0).then_some(i))
        .collect();
    let mut depths = vec![None; blocks.len()];
    depths[0] = Some(0i64);
    let mut low = 0;
    let mut high = 0;
    let mut final_delta = None;
    let mut visited = 0;
    while let Some(index) = queue.pop_front() {
        visited += 1;
        let block = &blocks[index];
        let mut delta = depths[index].ok_or_else(|| error("invalid CFG stack propagation"))?;
        for op in &instruction.ops[usize::from(block.start)..usize::from(block.end)] {
            match op {
                FirOp::VmStackPop { .. } => delta -= 1,
                FirOp::VmStackPush { .. } => delta += 1,
                _ => {}
            }
            low = low.min(delta);
            high = high.max(delta);
        }
        if block.terminator == FirTerminator::Return {
            if final_delta.is_some_and(|previous| previous != delta) {
                return Err(error("unequal stack deltas at FIR returns"));
            }
            final_delta = Some(delta);
        }
        for edge in edges(&block.terminator) {
            let target = usize::from(edge.target);
            if depths[target].is_some_and(|previous| previous != delta) {
                return Err(error("unequal stack deltas at FIR join"));
            }
            depths[target] = Some(delta);
            indegree[target] -= 1;
            if indegree[target] == 0 {
                queue.push_back(target);
            }
        }
    }
    if visited != blocks.len() {
        return Err(error(
            "cyclic FIR is unsupported by the acyclic execution backend",
        ));
    }
    Ok(StackContract {
        required_input: (-low) as usize,
        extra_capacity: high as usize,
        final_delta: final_delta.ok_or_else(|| error("CFG has no return"))?,
    })
}

/// Scalar conversion on unsigned storage: no signed host shifts or overflow.
pub(crate) fn convert(value: u64, source: u16, target: u16, kind: crate::IntConversion) -> u64 {
    let extension =
        if kind == crate::IntConversion::SignExtend && value & (1u64 << (source - 1)) != 0 {
            !width_mask(source)
        } else {
            0
        };
    (value | extension) & width_mask(target)
}

pub(crate) fn conversion_expression(
    instruction: &CompiledInstruction,
    output: ValueId,
    input: ValueId,
    kind: crate::IntConversion,
    expression: &str,
    c: bool,
) -> String {
    let source = instruction.values[usize::from(input.0)].ty.bits;
    let target = instruction.values[usize::from(output.0)].ty.bits;
    let suffix = if c { "ULL" } else { "u64" };
    let mask = width_mask(target);
    if kind == crate::IntConversion::SignExtend {
        // x ^ sign followed by wrapping subtraction sign extends a bit vector.
        let sign = 1u64 << (source - 1);
        if c {
            format!("((({expression}) ^ 0x{sign:x}ULL) - 0x{sign:x}ULL) & 0x{mask:x}ULL")
        } else {
            format!("(({expression}) ^ 0x{sign:x}u64).wrapping_sub(0x{sign:x}u64) & 0x{mask:x}u64")
        }
    } else {
        format!("({expression}) & 0x{mask:x}{suffix}")
    }
}

pub(crate) fn validate_acyclic(instruction: &CompiledInstruction) -> Result<(), FslError> {
    let blocks = blocks(instruction);
    let mut indegree = vec![0usize; blocks.len()];
    for block in &blocks {
        for edge in edges(&block.terminator) {
            indegree[usize::from(edge.target)] += 1;
        }
    }
    let mut queue: VecDeque<_> = indegree
        .iter()
        .enumerate()
        .filter_map(|(i, &n)| (n == 0).then_some(i))
        .collect();
    let mut count = 0;
    while let Some(i) = queue.pop_front() {
        count += 1;
        for edge in edges(&blocks[i].terminator) {
            let target = usize::from(edge.target);
            indegree[target] -= 1;
            if indegree[target] == 0 {
                queue.push_back(target);
            }
        }
    }
    if count != blocks.len() {
        return Err(error(
            "cyclic FIR is unsupported by the acyclic execution backend",
        ));
    }
    Ok(())
}

/// Apply an edge with simultaneous argument transfer; state effects stay ordered
/// in the caller's single runtime context.
pub(crate) fn advance(block: &FirBlock, blocks: &[FirBlock], values: &mut [u64]) -> Option<usize> {
    let edge = match &block.terminator {
        FirTerminator::Return => return None,
        FirTerminator::Branch(edge) => edge,
        FirTerminator::CondBranch {
            condition,
            on_true,
            on_false,
        } => {
            if values[usize::from(condition.0)] != 0 {
                on_true
            } else {
                on_false
            }
        }
    };
    let arguments: Vec<_> = edge
        .arguments
        .iter()
        .map(|id| values[usize::from(id.0)])
        .collect();
    for (&param, value) in blocks[usize::from(edge.target)]
        .parameters
        .iter()
        .zip(arguments)
    {
        values[usize::from(param.0)] = value;
    }
    Some(usize::from(edge.target))
}

pub(crate) fn compare(a: u64, b: u64, bits: u16, predicate: IntPredicate) -> u64 {
    u64::from(match predicate {
        IntPredicate::Equal => a == b,
        IntPredicate::UnsignedLess => a < b,
        IntPredicate::SignedLess => (a ^ (1u64 << (bits - 1))) < (b ^ (1u64 << (bits - 1))),
    })
}

pub(crate) fn execute(
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
    let blocks = blocks(instruction);
    let mut values = vec![0u64; instruction.values.len()];
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
                    values[usize::from(output.0)] = convert(
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
                    values[usize::from(output.0)] = compare(
                        values[usize::from(left.0)],
                        values[usize::from(right.0)],
                        instruction.values[usize::from(left.0)].ty.bits,
                        predicate,
                    )
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
                FirOp::VmStackPop { output } => {
                    values[usize::from(output.0)] =
                        stack.pop().expect("validated CFG stack contract")
                            & width_mask(instruction.values[usize::from(output.0)].ty.bits)
                }
                FirOp::VmStackPush { value } => stack.push(values[usize::from(value.0)]),
                _ => unreachable!("unsupported control execution refused"),
            }
        }
        match advance(block, &blocks, &mut values) {
            Some(next) => pc = next,
            None => return Ok(ExecutionStatus::Success),
        }
    }
}

pub(crate) fn emit(
    instruction: &CompiledInstruction,
    layer: OutputLayer,
    symbol: &str,
) -> Result<String, FslError> {
    let blocks = blocks(instruction);
    if layer == OutputLayer::Fir {
        let mut text = format!("instruction {}\n", instruction.name);
        for value in &instruction.values {
            writeln!(text, "  %v{}: {} name={}", value.id.0, value.ty, value.name).unwrap();
        }
        for (index, block) in blocks.iter().enumerate() {
            writeln!(
                text,
                "block {index} {} parameters={:?}",
                block.name, block.parameters
            )
            .unwrap();
            for op in &instruction.ops[usize::from(block.start)..usize::from(block.end)] {
                writeln!(text, "  {op:?}").unwrap();
            }
            writeln!(text, "  {:?}", block.terminator).unwrap();
        }
        return Ok(text);
    }
    if !matches!(layer, OutputLayer::C | OutputLayer::Rust) {
        return Err(error(
            "structured/constant/comparison FIR is unsupported by this output backend",
        ));
    }
    if instruction.ops.iter().any(FirOp::requires_state_version) {
        return crate::state::emit_state_instruction(instruction, layer, symbol);
    }
    let contract = StackContract::for_instruction(instruction)?;
    let c = layer == OutputLayer::C;
    let guard = if c {
        format!(
            "{}{}",
            if contract.required_input == 0 {
                String::new()
            } else {
                format!("  if (*depth < {}) return 1;\n", contract.required_input)
            },
            if contract.extra_capacity == 0 {
                String::new()
            } else {
                format!(
                    "  if ({} > capacity - *depth) return 2;\n",
                    contract.extra_capacity
                )
            }
        )
    } else {
        format!(
            "{}{}",
            if contract.required_input == 0 {
                String::new()
            } else {
                format!(
                    "  if *depth < {} {{ return 1; }}\n",
                    contract.required_input
                )
            },
            if contract.extra_capacity == 0 {
                String::new()
            } else {
                format!(
                    "  if {} > capacity - *depth {{ return 2; }}\n",
                    contract.extra_capacity
                )
            }
        )
    };
    let mutable_sp = instruction
        .ops
        .iter()
        .any(|op| matches!(op, FirOp::VmStackPop { .. } | FirOp::VmStackPush { .. }));
    let mutable_pc = blocks.iter().any(|b| b.terminator != FirTerminator::Return);
    let mutability = |needed| if needed { "mut " } else { "" };
    let mut text = if c {
        format!("#include <stdint.h>\n#include <stddef.h>\nuint32_t {symbol}(uint64_t *stack, size_t *depth, size_t capacity) {{\n  if (stack == NULL || depth == NULL || *depth > capacity) return 3;\n{guard}  size_t sp = *depth;\n  uint64_t v[{}] = {{0}};\n  (void)v;\n  size_t pc = 0;\n  for (;;) {{ switch (pc) {{\n", instruction.values.len().max(1))
    } else {
        format!("pub fn {symbol}(stack: &mut [u64], depth: &mut usize) -> u32 {{\n  let capacity = stack.len();\n  if *depth > capacity {{ return 3; }}\n{guard}  let {}sp = *depth;\n  let {}v = [0u64; {}];\n  let _ = &v;\n  let {}pc = 0usize;\n  loop {{ match pc {{\n", mutability(mutable_sp), mutability(!instruction.values.is_empty()), instruction.values.len().max(1), mutability(mutable_pc))
    };
    for (index, block) in blocks.iter().enumerate() {
        writeln!(
            text,
            "{} {{",
            if c {
                format!("case {index}:")
            } else {
                format!("{index} =>")
            }
        )
        .unwrap();
        for op in &instruction.ops[usize::from(block.start)..usize::from(block.end)] {
            let expression = match *op {
                FirOp::IntConvert {
                    output,
                    input,
                    kind,
                } => Some((
                    output,
                    conversion_expression(
                        instruction,
                        output,
                        input,
                        kind,
                        &format!("v[{}]", input.0),
                        c,
                    ),
                )),
                FirOp::IntConstant { output, value } => Some((
                    output,
                    format!("0x{value:x}{}", if c { "ULL" } else { "u64" }),
                )),
                FirOp::VmStackPop { output } => {
                    let mask = width_mask(instruction.values[usize::from(output.0)].ty.bits);
                    text.push_str("sp -= 1;\n");
                    Some((
                        output,
                        format!("stack[sp] & 0x{mask:x}{}", if c { "ULL" } else { "u64" }),
                    ))
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
                            format!("(v[{}] + v[{}]) & UINT64_C(0x{mask:x})", left.0, right.0)
                        } else {
                            format!("v[{}].wrapping_add(v[{}]) & 0x{mask:x}u64", left.0, right.0)
                        },
                    ))
                }
                FirOp::IntCompare {
                    output,
                    left,
                    right,
                    predicate,
                } => {
                    let expression = match predicate {
                        IntPredicate::Equal => format!("v[{}] == v[{}]", left.0, right.0),
                        IntPredicate::UnsignedLess => format!("v[{}] < v[{}]", left.0, right.0),
                        IntPredicate::SignedLess => {
                            let sign =
                                1u64 << (instruction.values[usize::from(left.0)].ty.bits - 1);
                            format!(
                                "(v[{}] ^ 0x{sign:x}{}) < (v[{}] ^ 0x{sign:x}{})",
                                left.0,
                                if c { "ULL" } else { "u64" },
                                right.0,
                                if c { "ULL" } else { "u64" }
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
                FirOp::VmStackPush { value } => {
                    writeln!(text, "stack[sp] = v[{}];\nsp += 1;", value.0).unwrap();
                    None
                }
                _ => unreachable!("unsupported projection refused"),
            };
            if let Some((id, expression)) = expression {
                writeln!(text, "v[{}] = {expression};", id.0).unwrap();
            }
        }
        emit_terminator(&mut text, block, &blocks, c, "*depth = sp;\nreturn 0;\n");
        text.push_str("}\n");
    }
    text.push_str(if c {
        "default: return 3;\n} }\n}\n"
    } else {
        "_ => return 3,\n} }\n}\n"
    });
    Ok(text)
}

pub(crate) fn emit_terminator(
    text: &mut String,
    block: &FirBlock,
    blocks: &[FirBlock],
    c: bool,
    on_return: &str,
) {
    match &block.terminator {
        FirTerminator::Return => text.push_str(on_return),
        FirTerminator::Branch(edge) => emit_edge(text, edge, blocks, c),
        FirTerminator::CondBranch {
            condition,
            on_true,
            on_false,
        } => {
            writeln!(
                text,
                "{} {{",
                if c {
                    format!("if (v[{}] != 0)", condition.0)
                } else {
                    format!("if v[{}] != 0", condition.0)
                }
            )
            .unwrap();
            emit_edge(text, on_true, blocks, c);
            text.push_str("} else {\n");
            emit_edge(text, on_false, blocks, c);
            text.push_str("}\n");
        }
    }
    if c && block.terminator != FirTerminator::Return {
        text.push_str("break;\n");
    }
}

fn emit_edge(text: &mut String, edge: &FirEdge, blocks: &[FirBlock], c: bool) {
    // One local edge array enforces parallel argument transfer in both outputs.
    if !edge.arguments.is_empty() {
        let values = edge
            .arguments
            .iter()
            .map(|id| format!("v[{}]", id.0))
            .collect::<Vec<_>>()
            .join(", ");
        writeln!(
            text,
            "{}",
            if c {
                format!("uint64_t edge[] = {{{values}}};")
            } else {
                format!("let edge = [{values}];")
            }
        )
        .unwrap();
        for (i, param) in blocks[usize::from(edge.target)]
            .parameters
            .iter()
            .enumerate()
        {
            writeln!(text, "v[{}] = edge[{i}];", param.0).unwrap();
        }
    }
    writeln!(text, "pc = {};", edge.target).unwrap();
}
