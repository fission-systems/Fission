use super::*;
#[test]
fn preview_type_hints_resolve_indirect_import_call_through_entry_param_alias() {
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x140006260,
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Copy,
                    address: 0x140006260,
                    output: Some(reg(0x30, 8)),
                    inputs: vec![reg(0x10, 8)],
                    asm_mnemonic: Some("MOV RSI,RDX".to_string()),
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::CallInd,
                    address: 0x140006270,
                    output: None,
                    inputs: vec![uniq(0x100, 8), reg(0x08, 8), reg(0x30, 8)],
                    asm_mnemonic: Some("CALL qword ptr [0x14012c378]".to_string()),
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::Return,
                    address: 0x140006280,
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

    assert!(rendered.contains("LPRECT param_2"));
    assert!(rendered.contains("GetClientRect(param_1, param_2)"));
}

#[test]
fn preview_type_hints_recover_indirect_import_args_from_block_register_setup() {
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x140006260,
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Copy,
                    address: 0x140006260,
                    output: Some(reg(0x30, 8)),
                    inputs: vec![reg(0x10, 8)],
                    asm_mnemonic: Some("MOV RSI,RDX".to_string()),
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::Copy,
                    address: 0x140006264,
                    output: Some(reg(0x08, 8)),
                    inputs: vec![reg(0x08, 8)],
                    asm_mnemonic: Some("MOV RCX,RCX".to_string()),
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::Copy,
                    address: 0x140006268,
                    output: Some(reg(0x10, 8)),
                    inputs: vec![reg(0x30, 8)],
                    asm_mnemonic: Some("MOV RDX,RSI".to_string()),
                },
                PcodeOp {
                    seq_num: 3,
                    opcode: PcodeOpcode::CallInd,
                    address: 0x140006270,
                    output: None,
                    inputs: vec![uniq(0x100, 8)],
                    asm_mnemonic: Some("CALL qword ptr [0x14012c378]".to_string()),
                },
                PcodeOp {
                    seq_num: 4,
                    opcode: PcodeOpcode::Return,
                    address: 0x140006280,
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

    assert!(rendered.contains("LPRECT param_2"));
    assert!(rendered.contains("GetClientRect(param_1, param_2)"));
}
