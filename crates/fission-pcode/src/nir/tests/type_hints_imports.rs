use super::*;
#[test]
fn preview_type_hints_resolve_indirect_import_call_through_entry_param_alias() {
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x140006260,
            successors: vec![],
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
                    address: 0x140006268,
                    output: Some(uniq(0x100, 8)),
                    inputs: vec![cst(0x14012c378, 8)],
                    asm_mnemonic: Some("COPY".to_string()),
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::CallInd,
                    address: 0x140006270,
                    output: None,
                    inputs: vec![uniq(0x100, 8), reg(0x08, 8), reg(0x30, 8)],
                    asm_mnemonic: Some("CALL qword ptr [0x14012c378]".to_string()),
                },
                PcodeOp {
                    seq_num: 3,
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
    context.call_target_refs.insert(
        0x14012c378,
        CallTargetRef {
            address: Some(0x14012c378),
            symbol: "GetClientRect".to_string(),
            provenance: CallTargetProvenance::Import,
            edge_kind: CallEdgeKind::Import,
            confidence: 255,
        },
    );
    context.call_param_rules.push(PreviewCallParamRule {
        callee_address: None,
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
            successors: vec![],
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
                    opcode: PcodeOpcode::Copy,
                    address: 0x14000626c,
                    output: Some(uniq(0x100, 8)),
                    inputs: vec![cst(0x14012c378, 8)],
                    asm_mnemonic: Some("COPY".to_string()),
                },
                PcodeOp {
                    seq_num: 4,
                    opcode: PcodeOpcode::CallInd,
                    address: 0x140006270,
                    output: None,
                    inputs: vec![uniq(0x100, 8)],
                    asm_mnemonic: Some("CALL qword ptr [0x14012c378]".to_string()),
                },
                PcodeOp {
                    seq_num: 5,
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
    context.call_target_refs.insert(
        0x14012c378,
        CallTargetRef {
            address: Some(0x14012c378),
            symbol: "GetClientRect".to_string(),
            provenance: CallTargetProvenance::Import,
            edge_kind: CallEdgeKind::Import,
            confidence: 255,
        },
    );
    context.call_param_rules.push(PreviewCallParamRule {
        callee_address: None,
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
fn preview_type_hints_call_arg_recovery_falls_back_to_param_surface_on_unsupported_setup() {
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x140006260,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Copy,
                    address: 0x140006260,
                    output: Some(reg(0x08, 8)),
                    inputs: vec![reg(0x08, 8)],
                    asm_mnemonic: Some("NOP".to_string()),
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::Copy,
                    address: 0x140006268,
                    output: Some(uniq(0x100, 8)),
                    inputs: vec![cst(0x14012c378, 8)],
                    asm_mnemonic: Some("COPY".to_string()),
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::CallInd,
                    address: 0x140006270,
                    output: None,
                    inputs: vec![uniq(0x100, 8)],
                    asm_mnemonic: Some("CALL qword ptr [0x14012c378]".to_string()),
                },
                PcodeOp {
                    seq_num: 3,
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
    context.call_target_refs.insert(
        0x14012c378,
        CallTargetRef {
            address: Some(0x14012c378),
            symbol: "GetClientRect".to_string(),
            provenance: CallTargetProvenance::Import,
            edge_kind: CallEdgeKind::Import,
            confidence: 255,
        },
    );

    let rendered = render_mlil_preview_with_context(
        &func,
        "FUN_0x140006260",
        0x140006260,
        &preview_options(),
        Some(&context),
    )
    .expect("preview render should succeed with register-surface fallback");

    assert!(rendered.contains("GetClientRect(param_1)"), "{rendered}");
}

#[test]
fn preview_call_target_refs_resolve_direct_import_call_target() {
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x140006260,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Call,
                    address: 0x140006260,
                    output: None,
                    inputs: vec![cst(0x14012c378, 8)],
                    asm_mnemonic: Some("CALL 0x14012c378".to_string()),
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::Return,
                    address: 0x140006268,
                    output: None,
                    inputs: vec![cst(1, 8)],
                    asm_mnemonic: Some("RET".to_string()),
                },
            ],
        }],
    };

    let mut context = PreviewTypeContext::default();
    context.call_target_refs.insert(
        0x14012c378,
        CallTargetRef {
            address: Some(0x14012c378),
            symbol: "CloseHandle".to_string(),
            provenance: CallTargetProvenance::Import,
            edge_kind: CallEdgeKind::Import,
            confidence: 255,
        },
    );

    let rendered = render_mlil_preview_with_context(
        &func,
        "FUN_0x140006260",
        0x140006260,
        &preview_options(),
        Some(&context),
    )
    .expect("preview render should succeed");
    let stats = take_last_nir_build_stats().expect("build stats");

    assert!(rendered.contains("CloseHandle()"), "{rendered}");
    assert_eq!(stats.call_target_exact_index_hit_count, 1);
    assert_eq!(stats.call_target_import_resolved_count, 1);
    assert_eq!(stats.call_target_direct_symbol_resolved_count, 0);
    assert_eq!(stats.call_target_unresolved_sub_fallback_count, 0);
    assert_eq!(stats.call_target_context_missing_count, 0);
    assert_eq!(stats.call_target_unresolved_no_exact_identity_count, 0);
}

#[test]
fn preview_call_target_refs_resolve_direct_symbol_call_target() {
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x140006260,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Call,
                    address: 0x140006260,
                    output: None,
                    inputs: vec![cst(0x140010000, 8)],
                    asm_mnemonic: Some("CALL 0x140010000".to_string()),
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::Return,
                    address: 0x140006268,
                    output: None,
                    inputs: vec![cst(1, 8)],
                    asm_mnemonic: Some("RET".to_string()),
                },
            ],
        }],
    };

    let mut context = PreviewTypeContext::default();
    context.call_target_refs.insert(
        0x140010000,
        CallTargetRef {
            address: Some(0x140010000),
            symbol: "sqlite3Malloc".to_string(),
            provenance: CallTargetProvenance::Direct,
            edge_kind: CallEdgeKind::Direct,
            confidence: 224,
        },
    );

    let rendered = render_mlil_preview_with_context(
        &func,
        "FUN_0x140006260",
        0x140006260,
        &preview_options(),
        Some(&context),
    )
    .expect("preview render should succeed");
    let stats = take_last_nir_build_stats().expect("build stats");

    assert!(rendered.contains("sqlite3Malloc()"), "{rendered}");
    assert_eq!(stats.call_target_exact_index_hit_count, 1);
    assert_eq!(stats.call_target_import_resolved_count, 0);
    assert_eq!(stats.call_target_direct_symbol_resolved_count, 1);
    assert_eq!(stats.call_target_unresolved_sub_fallback_count, 0);
    assert_eq!(stats.call_target_context_missing_count, 0);
    assert_eq!(stats.call_target_unresolved_no_exact_identity_count, 0);
}

#[test]
fn preview_call_target_missing_context_keeps_sub_fallback() {
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x140006260,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Call,
                    address: 0x140006260,
                    output: None,
                    inputs: vec![cst(0x140010000, 8)],
                    asm_mnemonic: Some("CALL 0x140010000".to_string()),
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::Return,
                    address: 0x140006268,
                    output: None,
                    inputs: vec![cst(1, 8)],
                    asm_mnemonic: Some("RET".to_string()),
                },
            ],
        }],
    };

    let rendered = render_mlil_preview_with_context(
        &func,
        "FUN_0x140006260",
        0x140006260,
        &preview_options(),
        None,
    )
    .expect("preview render should succeed");
    let stats = take_last_nir_build_stats().expect("build stats");

    assert!(rendered.contains("sub_140010000()"), "{rendered}");
    assert_eq!(stats.call_target_import_resolved_count, 0);
    assert_eq!(stats.call_target_direct_symbol_resolved_count, 0);
    assert_eq!(stats.call_target_unresolved_sub_fallback_count, 1);
    assert_eq!(stats.call_target_context_missing_count, 1);
}

#[test]
fn preview_call_target_legacy_map_does_not_promote_exact_identity() {
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x140006260,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Call,
                    address: 0x140006260,
                    output: None,
                    inputs: vec![cst(0x140010000, 8)],
                    asm_mnemonic: Some("CALL 0x140010000".to_string()),
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::Return,
                    address: 0x140006268,
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
        .insert(0x140010000, "LegacyOnly".to_string());

    let rendered = render_mlil_preview_with_context(
        &func,
        "FUN_0x140006260",
        0x140006260,
        &preview_options(),
        Some(&context),
    )
    .expect("preview render should succeed");
    let stats = take_last_nir_build_stats().expect("build stats");

    assert!(rendered.contains("sub_140010000()"), "{rendered}");
    assert!(!rendered.contains("LegacyOnly("), "{rendered}");
    assert_eq!(stats.call_target_exact_index_hit_count, 0);
    assert_eq!(stats.call_target_direct_symbol_resolved_count, 0);
    assert_eq!(stats.call_target_unresolved_sub_fallback_count, 1);
    assert_eq!(stats.call_target_unresolved_no_exact_identity_count, 1);
}

#[test]
fn preview_callind_copy_only_constant_chain_resolves_exact_target() {
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x140006260,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Copy,
                    address: 0x140006260,
                    output: Some(uniq(0x100, 8)),
                    inputs: vec![cst(0x14012c378, 8)],
                    asm_mnemonic: Some("COPY".to_string()),
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::CallInd,
                    address: 0x140006268,
                    output: None,
                    inputs: vec![uniq(0x100, 8)],
                    asm_mnemonic: Some("CALL RAX".to_string()),
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::Return,
                    address: 0x140006270,
                    output: None,
                    inputs: vec![cst(1, 8)],
                    asm_mnemonic: Some("RET".to_string()),
                },
            ],
        }],
    };

    let mut context = PreviewTypeContext::default();
    context.call_target_refs.insert(
        0x14012c378,
        CallTargetRef {
            address: Some(0x14012c378),
            symbol: "CloseHandle".to_string(),
            provenance: CallTargetProvenance::Import,
            edge_kind: CallEdgeKind::Import,
            confidence: 255,
        },
    );

    let rendered = render_mlil_preview_with_context(
        &func,
        "FUN_0x140006260",
        0x140006260,
        &preview_options(),
        Some(&context),
    )
    .expect("preview render should succeed");
    let stats = take_last_nir_build_stats().expect("build stats");

    assert!(rendered.contains("CloseHandle()"), "{rendered}");
    assert_eq!(stats.call_target_exact_index_hit_count, 1);
    assert_eq!(stats.call_target_import_resolved_count, 1);
    assert_eq!(stats.call_target_indirect_const_resolved_count, 1);
    assert_eq!(stats.call_target_unresolved_sub_fallback_count, 0);
}

#[test]
fn preview_callind_load_from_iat_slot_resolves_exact_import_target() {
    let iat_slot = 0x14012c378_u64;
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x140006260,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Load,
                    address: 0x140006260,
                    output: Some(uniq(0x100, 8)),
                    inputs: vec![cst(0, 8), cst(iat_slot as i64, 8)],
                    asm_mnemonic: Some("LOAD IAT".to_string()),
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::CallInd,
                    address: 0x140006268,
                    output: None,
                    inputs: vec![uniq(0x100, 8)],
                    asm_mnemonic: Some("CALL qword ptr [0x14012c378]".to_string()),
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::Return,
                    address: 0x140006270,
                    output: None,
                    inputs: vec![cst(1, 8)],
                    asm_mnemonic: Some("RET".to_string()),
                },
            ],
        }],
    };

    let mut context = PreviewTypeContext::default();
    context.iat_target_refs.insert(
        iat_slot,
        CallTargetRef {
            address: Some(iat_slot),
            symbol: "CloseHandle".to_string(),
            provenance: CallTargetProvenance::Import,
            edge_kind: CallEdgeKind::Import,
            confidence: 255,
        },
    );

    let rendered = render_mlil_preview_with_context(
        &func,
        "FUN_0x140006260",
        0x140006260,
        &preview_options(),
        Some(&context),
    )
    .expect("preview render should succeed");
    let stats = take_last_nir_build_stats().expect("build stats");

    assert!(rendered.contains("CloseHandle()"), "{rendered}");
    assert_eq!(stats.call_target_iat_slot_resolved_count, 1);
    assert_eq!(stats.call_target_indirect_load_resolved_count, 1);
    assert_eq!(stats.call_target_import_resolved_count, 1);
    assert_eq!(stats.call_target_unresolved_sub_fallback_count, 0);
}

#[test]
fn preview_callind_load_from_non_iat_slot_keeps_existing_surface() {
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x140006260,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Load,
                    address: 0x140006260,
                    output: Some(uniq(0x100, 8)),
                    inputs: vec![cst(0, 8), cst(0x14012c378, 8)],
                    asm_mnemonic: Some("LOAD NON-IAT".to_string()),
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::CallInd,
                    address: 0x140006268,
                    output: None,
                    inputs: vec![uniq(0x100, 8)],
                    asm_mnemonic: Some("CALL qword ptr [0x14012c378]".to_string()),
                },
            ],
        }],
    };

    let rendered = render_mlil_preview_with_context(
        &func,
        "FUN_0x140006260",
        0x140006260,
        &preview_options(),
        Some(&PreviewTypeContext::default()),
    )
    .expect("preview render should succeed");
    let stats = take_last_nir_build_stats().expect("build stats");

    assert!(!rendered.contains("CloseHandle"), "{rendered}");
    assert_eq!(stats.call_target_iat_slot_resolved_count, 0);
    assert_eq!(stats.call_target_indirect_load_resolved_count, 0);
    assert_eq!(stats.call_target_indirect_rejected_non_iat_load_count, 1);
}
