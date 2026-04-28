fn semantic_ops_for_kind(construct_tpl_kind: CompiledConstructTplKind) -> Vec<CompiledSemanticOp> {
    use CompiledArithmeticOpcode as Arith;
    use CompiledConstructTplKind as Kind;
    use CompiledSemanticOp as Op;

    vec![match construct_tpl_kind {
        Kind::Unsupported => Op::Nop,
        Kind::Nop => Op::Nop,
        Kind::Ret => Op::Return,
        Kind::Call => Op::Call,
        Kind::Jmp => Op::Jump,
        Kind::Jcc => Op::ConditionalJump,
        Kind::Mov => Op::Copy,
        Kind::AddressOf => Op::AddressOf,
        Kind::StackStore => Op::StackStore,
        Kind::StackLoad => Op::StackLoad,
        Kind::FrameTeardown => Op::FrameTeardown,
        Kind::Add => Op::Binary { opcode: Arith::Add },
        Kind::Sub => Op::Binary { opcode: Arith::Sub },
        Kind::And => Op::Binary { opcode: Arith::And },
        Kind::Or => Op::Binary { opcode: Arith::Or },
        Kind::Xor => Op::Binary { opcode: Arith::Xor },
        Kind::Imul | Kind::Mul => Op::Binary { opcode: Arith::Mul },
        Kind::Shl => Op::Binary { opcode: Arith::Shl },
        Kind::Shr => Op::Binary { opcode: Arith::Shr },
        Kind::Sar => Op::Binary { opcode: Arith::Sar },
        Kind::Inc => Op::Binary { opcode: Arith::Inc },
        Kind::Dec => Op::Binary { opcode: Arith::Dec },
        Kind::Cmp => Op::Compare { bitwise: false },
        Kind::Test => Op::Compare { bitwise: true },
        Kind::Movzx => Op::Extend { signed: false },
        Kind::Movsx | Kind::Movsxd => Op::Extend { signed: true },
        Kind::Setcc => Op::SetCc,
        Kind::Cbw => Op::AccumulatorExtend { src_size: 1, dst_size: 2 },
        Kind::Cwde => Op::AccumulatorExtend { src_size: 2, dst_size: 4 },
        Kind::Cdqe => Op::AccumulatorExtend { src_size: 4, dst_size: 8 },
        Kind::Generic => Op::Nop,
    }]
}

fn op_templates_for_constructor(
    operand_specs: &[CompiledOperandSpec],
    construct_tpl_kind: CompiledConstructTplKind,
) -> Vec<CompiledOpTpl> {
    use CompiledConstTpl as ConstTpl;
    use CompiledConstructTplKind as Kind;
    use CompiledFixedRegister as FixedReg;
    use CompiledOpTplOpcode as Opcode;
    use CompiledVarnodeTpl as VnTpl;

    let handle = |operand_index| VnTpl::Handle { operand_index };
    let effective_address = |operand_index| VnTpl::EffectiveAddress { operand_index };
    let condition_predicate = || VnTpl::ConditionPredicate;
    let temp = |id, size| VnTpl::Temp { id, size };
    let fixed = |reg, size| VnTpl::FixedRegister { reg, size };
    let flag = |bit| VnTpl::Flag { bit };
    let sized_const = |value: i64, size: u32| VnTpl::Const(ConstTpl::Integer { value, size });
    let binary_tpl = |opcode| {
        vec![CompiledOpTpl {
            opcode,
            output: Some(handle(0)),
            inputs: vec![handle(0), handle(1)],
            label: None,
        }]
    };

    match construct_tpl_kind {
        Kind::Nop | Kind::Unsupported | Kind::Generic => Vec::new(),
        Kind::Ret => vec![
            CompiledOpTpl {
                opcode: Opcode::Load,
                output: Some(temp(0, 8)),
                inputs: vec![sized_const(0, 8), fixed(FixedReg::StackPointer, 8)],
                label: None,
            },
            CompiledOpTpl {
                opcode: Opcode::IntAdd,
                output: Some(fixed(FixedReg::StackPointer, 8)),
                inputs: vec![fixed(FixedReg::StackPointer, 8), sized_const(8, 8)],
                label: None,
            },
            CompiledOpTpl {
                opcode: Opcode::Return,
                output: None,
                inputs: vec![temp(0, 8)],
                label: None,
            },
        ],
        Kind::Call => vec![
            CompiledOpTpl {
                opcode: Opcode::IntSub,
                output: Some(fixed(FixedReg::StackPointer, 8)),
                inputs: vec![fixed(FixedReg::StackPointer, 8), sized_const(8, 8)],
                label: None,
            },
            CompiledOpTpl {
                opcode: Opcode::Store,
                output: None,
                inputs: vec![
                    sized_const(0, 8),
                    fixed(FixedReg::StackPointer, 8),
                    VnTpl::Const(ConstTpl::InstNext),
                ],
                label: None,
            },
            CompiledOpTpl {
                opcode: Opcode::Call,
                output: None,
                inputs: vec![handle(0)],
                label: None,
            },
        ],
        Kind::Jmp => vec![CompiledOpTpl {
            opcode: Opcode::Branch,
            output: None,
            inputs: vec![handle(0)],
            label: None,
        }],
        Kind::Mov => {
            if operand_specs.len() < 2 { return Vec::new(); }
            vec![CompiledOpTpl {
                opcode: Opcode::Copy,
                output: Some(handle(0)),
                inputs: vec![handle(1)],
                label: None,
            }]
        }
        Kind::Movzx => {
            if operand_specs.len() < 2 { return Vec::new(); }
            vec![CompiledOpTpl {
                opcode: Opcode::IntZExt,
                output: Some(handle(0)),
                inputs: vec![handle(1)],
                label: None,
            }]
        }
        Kind::Movsx | Kind::Movsxd => {
            if operand_specs.len() < 2 { return Vec::new(); }
            vec![CompiledOpTpl {
                opcode: Opcode::IntSExt,
                output: Some(handle(0)),
                inputs: vec![handle(1)],
                label: None,
            }]
        }
        Kind::AddressOf => {
            if operand_specs.len() < 2 { return Vec::new(); }
            let dst_size = operand_spec_size(&operand_specs[0]);
            let mut ops = Vec::new();
            if dst_size < 8 {
                ops.push(CompiledOpTpl {
                    opcode: Opcode::Subpiece,
                    output: Some(temp(0, dst_size)),
                    inputs: vec![effective_address(1), sized_const(0, 8)],
                    label: None,
                });
                ops.push(CompiledOpTpl {
                    opcode: Opcode::Copy,
                    output: Some(handle(0)),
                    inputs: vec![temp(0, dst_size)],
                    label: None,
                });
            } else {
                ops.push(CompiledOpTpl {
                    opcode: Opcode::Copy,
                    output: Some(handle(0)),
                    inputs: vec![effective_address(1)],
                    label: None,
                });
            }
            ops
        }
        Kind::StackStore => {
            if operand_specs.is_empty() { return Vec::new(); }
            let value_size = operand_spec_size(&operand_specs[0]);
            let stack_size = value_size.max(8);
            vec![
                CompiledOpTpl {
                    opcode: Opcode::Copy,
                    output: Some(temp(0, 8)),
                    inputs: vec![fixed(FixedReg::StackPointer, 8)],
                    label: None,
                },
                CompiledOpTpl {
                    opcode: Opcode::IntSub,
                    output: Some(fixed(FixedReg::StackPointer, 8)),
                    inputs: vec![temp(0, 8), sized_const(i64::from(stack_size), 8)],
                    label: None,
                },
                CompiledOpTpl {
                    opcode: Opcode::Store,
                    output: None,
                    inputs: vec![
                        sized_const(0, 8),
                        fixed(FixedReg::StackPointer, 8),
                        handle(0),
                    ],
                    label: None,
                },
            ]
        }
        Kind::StackLoad => {
            if operand_specs.is_empty() { return Vec::new(); }
            let value_size = operand_spec_size(&operand_specs[0]);
            let stack_size = value_size.max(8);
            vec![
                CompiledOpTpl {
                    opcode: Opcode::Load,
                    output: Some(temp(0, stack_size)),
                    inputs: vec![sized_const(0, 8), fixed(FixedReg::StackPointer, 8)],
                    label: None,
                },
                CompiledOpTpl {
                    opcode: Opcode::Copy,
                    output: Some(handle(0)),
                    inputs: vec![temp(0, stack_size)],
                    label: None,
                },
                CompiledOpTpl {
                    opcode: Opcode::IntAdd,
                    output: Some(fixed(FixedReg::StackPointer, 8)),
                    inputs: vec![
                        fixed(FixedReg::StackPointer, 8),
                        sized_const(i64::from(stack_size), 8),
                    ],
                    label: None,
                },
            ]
        }
        Kind::FrameTeardown => vec![
            CompiledOpTpl {
                opcode: Opcode::Copy,
                output: Some(fixed(FixedReg::StackPointer, 8)),
                inputs: vec![fixed(FixedReg::FramePointer, 8)],
                label: None,
            },
            CompiledOpTpl {
                opcode: Opcode::Load,
                output: Some(temp(0, 8)),
                inputs: vec![sized_const(0, 8), fixed(FixedReg::StackPointer, 8)],
                label: None,
            },
            CompiledOpTpl {
                opcode: Opcode::Copy,
                output: Some(fixed(FixedReg::FramePointer, 8)),
                inputs: vec![temp(0, 8)],
                label: None,
            },
            CompiledOpTpl {
                opcode: Opcode::IntAdd,
                output: Some(fixed(FixedReg::StackPointer, 8)),
                inputs: vec![fixed(FixedReg::StackPointer, 8), sized_const(8, 8)],
                label: None,
            },
        ],
        Kind::Cmp | Kind::Test => {
            if operand_specs.len() < 2 { return Vec::new(); }
            let size = operand_specs.iter().take(2).map(operand_spec_size).max().unwrap_or(1).max(1);
            let is_test = matches!(construct_tpl_kind, Kind::Test);
            let mut ops = vec![CompiledOpTpl {
                opcode: Opcode::Copy,
                output: Some(temp(0, size)),
                inputs: vec![handle(0)],
                label: None,
            }];
            if is_test {
                ops.push(CompiledOpTpl {
                    opcode: Opcode::IntAnd,
                    output: Some(temp(1, size)),
                    inputs: vec![temp(0, size), handle(1)],
                    label: None,
                });
                ops.push(CompiledOpTpl {
                    opcode: Opcode::Copy,
                    output: Some(flag(0)),
                    inputs: vec![sized_const(0, 1)],
                    label: None,
                });
                ops.push(CompiledOpTpl {
                    opcode: Opcode::Copy,
                    output: Some(flag(11)),
                    inputs: vec![sized_const(0, 1)],
                    label: None,
                });
            } else {
                ops.push(CompiledOpTpl {
                    opcode: Opcode::IntLess,
                    output: Some(temp(2, 1)),
                    inputs: vec![temp(0, size), handle(1)],
                    label: None,
                });
                ops.push(CompiledOpTpl {
                    opcode: Opcode::IntSBorrow,
                    output: Some(temp(3, 1)),
                    inputs: vec![temp(0, size), handle(1)],
                    label: None,
                });
                ops.push(CompiledOpTpl {
                    opcode: Opcode::IntSub,
                    output: Some(temp(1, size)),
                    inputs: vec![temp(0, size), handle(1)],
                    label: None,
                });
                ops.push(CompiledOpTpl {
                    opcode: Opcode::Copy,
                    output: Some(flag(0)),
                    inputs: vec![temp(2, 1)],
                    label: None,
                });
                ops.push(CompiledOpTpl {
                    opcode: Opcode::Copy,
                    output: Some(flag(11)),
                    inputs: vec![temp(3, 1)],
                    label: None,
                });
            }
            ops.extend([
                CompiledOpTpl {
                    opcode: Opcode::IntSLess,
                    output: Some(temp(4, 1)),
                    inputs: vec![temp(1, size), sized_const(0, size)],
                    label: None,
                },
                CompiledOpTpl {
                    opcode: Opcode::IntEqual,
                    output: Some(temp(5, 1)),
                    inputs: vec![temp(1, size), sized_const(0, size)],
                    label: None,
                },
                CompiledOpTpl {
                    opcode: Opcode::IntAnd,
                    output: Some(temp(6, size)),
                    inputs: vec![temp(1, size), sized_const(0xff, size)],
                    label: None,
                },
                CompiledOpTpl {
                    opcode: Opcode::PopCount,
                    output: Some(temp(7, size)),
                    inputs: vec![temp(6, size)],
                    label: None,
                },
                CompiledOpTpl {
                    opcode: Opcode::IntAnd,
                    output: Some(temp(8, size)),
                    inputs: vec![temp(7, size), sized_const(1, size)],
                    label: None,
                },
                CompiledOpTpl {
                    opcode: Opcode::IntEqual,
                    output: Some(temp(9, 1)),
                    inputs: vec![temp(8, size), sized_const(0, size)],
                    label: None,
                },
                CompiledOpTpl {
                    opcode: Opcode::Copy,
                    output: Some(flag(7)),
                    inputs: vec![temp(4, 1)],
                    label: None,
                },
                CompiledOpTpl {
                    opcode: Opcode::Copy,
                    output: Some(flag(6)),
                    inputs: vec![temp(5, 1)],
                    label: None,
                },
                CompiledOpTpl {
                    opcode: Opcode::Copy,
                    output: Some(flag(2)),
                    inputs: vec![temp(9, 1)],
                    label: None,
                },
            ]);
            ops
        }
        Kind::Add => binary_tpl(Opcode::IntAdd),
        Kind::Sub => binary_tpl(Opcode::IntSub),
        Kind::And => binary_tpl(Opcode::IntAnd),
        Kind::Or => binary_tpl(Opcode::IntOr),
        Kind::Xor => binary_tpl(Opcode::IntXor),
        Kind::Imul | Kind::Mul => binary_tpl(Opcode::IntMult),
        Kind::Shl => binary_tpl(Opcode::IntLeft),
        Kind::Shr => binary_tpl(Opcode::IntRight),
        Kind::Sar => binary_tpl(Opcode::IntSRight),
        Kind::Inc | Kind::Dec => {
            let Some(size) = operand_specs.first().map(operand_spec_size) else { return Vec::new(); };
            vec![CompiledOpTpl {
                opcode: match construct_tpl_kind {
                    Kind::Inc => Opcode::IntAdd,
                    Kind::Dec => Opcode::IntSub,
                    _ => unreachable!(),
                },
                output: Some(handle(0)),
                inputs: vec![handle(0), sized_const(1, size)],
                label: None,
            }]
        }
        Kind::Cbw => vec![CompiledOpTpl {
            opcode: Opcode::IntSExt,
            output: Some(fixed(FixedReg::Accumulator, 2)),
            inputs: vec![fixed(FixedReg::Accumulator, 1)],
            label: None,
        }],
        Kind::Cwde => vec![CompiledOpTpl {
            opcode: Opcode::IntSExt,
            output: Some(fixed(FixedReg::Accumulator, 4)),
            inputs: vec![fixed(FixedReg::Accumulator, 2)],
            label: None,
        }],
        Kind::Cdqe => vec![CompiledOpTpl {
            opcode: Opcode::IntSExt,
            output: Some(fixed(FixedReg::Accumulator, 8)),
            inputs: vec![fixed(FixedReg::Accumulator, 4)],
            label: None,
        }],
        Kind::Jcc => {
            if operand_specs.is_empty() { return Vec::new(); }
            vec![CompiledOpTpl {
                opcode: Opcode::CBranch,
                output: None,
                inputs: vec![handle(0), condition_predicate()],
                label: None,
            }]
        }
        Kind::Setcc => {
            if operand_specs.is_empty() { return Vec::new(); }
            vec![CompiledOpTpl {
                opcode: Opcode::Copy,
                output: Some(handle(0)),
                inputs: vec![condition_predicate()],
                label: None,
            }]
        }
    }
}

fn operand_spec_size(spec: &CompiledOperandSpec) -> u32 {
    match spec {
        CompiledOperandSpec::TokenFieldExtraction { bit_width, .. }
        | CompiledOperandSpec::ContextFieldExtraction { bit_width, .. } => *bit_width / 8,
        CompiledOperandSpec::SlaTokenField {
            byte_start,
            byte_end,
            ..
        } => byte_end.saturating_sub(*byte_start) + 1,
        CompiledOperandSpec::SlaVarnodeList { entries, .. } => {
            entries.first().map(|entry| entry.size).unwrap_or(0)
        }
        CompiledOperandSpec::SlaValueMap {
            byte_start,
            byte_end,
            ..
        } => byte_end.saturating_sub(*byte_start) + 1,
        CompiledOperandSpec::SlaFixedVarnode { varnode } => varnode.size,
        CompiledOperandSpec::SlaPatternExpression { .. } => 0,
        CompiledOperandSpec::SubtableEvaluation { .. } => 0,
        CompiledOperandSpec::Immediate { size, .. }
        | CompiledOperandSpec::Relative { size }
        | CompiledOperandSpec::FixedRegister { size, .. } => *size,
    }
}
