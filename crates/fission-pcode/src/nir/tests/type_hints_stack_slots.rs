use super::*;
#[test]
fn preview_type_hints_name_rsp_aggregate_slot_as_local() {
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x140006260,
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Copy,
                    address: 0x140006260,
                    output: Some(uniq(0x200, 8)),
                    inputs: vec![reg(0x30, 8)],
                    asm_mnemonic: Some("PUSH RSI".to_string()),
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::IntSub,
                    address: 0x140006261,
                    output: Some(reg(0x20, 8)),
                    inputs: vec![reg(0x20, 8), cst(0x58, 8)],
                    asm_mnemonic: Some("SUB RSP,0x58".to_string()),
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::Copy,
                    address: 0x140006262,
                    output: Some(reg(0x30, 8)),
                    inputs: vec![reg(0x10, 8)],
                    asm_mnemonic: Some("MOV RSI,RDX".to_string()),
                },
                PcodeOp {
                    seq_num: 3,
                    opcode: PcodeOpcode::Call,
                    address: 0x140006263,
                    output: None,
                    inputs: vec![cst(0x14012c378, 8), reg(0x08, 8), reg(0x10, 8)],
                    asm_mnemonic: Some("CALL qword ptr [0x14012c378]".to_string()),
                },
                PcodeOp {
                    seq_num: 4,
                    opcode: PcodeOpcode::Load,
                    address: 0x140006264,
                    output: Some(uniq(0x6c80, 16)),
                    inputs: vec![cst(0xb3f820180, 8), uniq(0x4e80, 8)],
                    asm_mnemonic: Some("MOVUPS XMM0, xmmword ptr [RSP + 0x2c]".to_string()),
                },
                PcodeOp {
                    seq_num: 5,
                    opcode: PcodeOpcode::Copy,
                    address: 0x140006265,
                    output: Some(uniq(0x8fd00, 16)),
                    inputs: vec![uniq(0x6c80, 16)],
                    asm_mnemonic: Some("MOVUPS XMM0, xmmword ptr [RSP + 0x2c]".to_string()),
                },
                PcodeOp {
                    seq_num: 6,
                    opcode: PcodeOpcode::Store,
                    address: 0x140006266,
                    output: None,
                    inputs: vec![cst(0xb3f820180, 8), reg(0x30, 8), uniq(0x8fd00, 16)],
                    asm_mnemonic: Some("MOVUPS xmmword ptr [RSI], XMM0".to_string()),
                },
                PcodeOp {
                    seq_num: 7,
                    opcode: PcodeOpcode::Return,
                    address: 0x140006267,
                    output: None,
                    inputs: vec![cst(1, 8)],
                    asm_mnemonic: Some("RET".to_string()),
                },
            ],
        }],
    };

    let mut context = PreviewTypeContext::default();
    context
        .call_targets
        .insert(0x14012c378, "GetClientRect".to_string());
    context.call_param_rules.push(PreviewCallParamRule {
        callee_name: "GetClientRect".to_string(),
        arg_index: 1,
        pointer_alias: "LPRECT".to_string(),
        pointee_alias: "RECT".to_string(),
        pointer_size: 8,
        pointee_sizes: vec![16],
    });

    let rendered = render_mlil_preview_with_context(
        &func,
        "FUN_0x140006260",
        0x140006260,
        &preview_options(),
        Some(&context),
    )
    .expect("preview render should succeed");

    assert!(
        rendered.contains("LPRECT param_2"),
        "rendered:\n{}",
        rendered
    );
    assert!(
        rendered.contains("RECT local_34;"),
        "rendered:\n{}",
        rendered
    );
    assert!(
        rendered.contains("*param_2 = local_34;"),
        "rendered:\n{}",
        rendered
    );
}
