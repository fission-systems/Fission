//! Orphan-goto repair tests (production calls free-fn in `pass/structuring`).
//!
//! Owner: `fission_midend_structuring::try_repair_orphan_gotos` (ADR 0012).

#[cfg(test)]
mod tests {
    use super::super::super::cleanup::{has_orphan_goto_labels, orphan_goto_labels};
    use crate::midend::PreviewBuilder;
    use crate::midend::ir::{MlilPreviewOptions, StructuringEngineKind};
    use crate::{PcodeBasicBlock, PcodeFunction, PcodeOp, PcodeOpcode, Varnode};
    use fission_midend_core::ir::NirType;
    use fission_midend_prehir::{PreHirExpr, PreHirLValue, PreHirStmt};
    use fission_midend_structuring::try_repair_orphan_gotos;

    fn test_options() -> MlilPreviewOptions {
        MlilPreviewOptions {
            pe_x64_only: true,
            is_64bit: true,
            is_big_endian: false,
            pointer_size: 8,
            format: "PE".to_string(),
            image_base: 0,
            sections: Vec::new(),
            region_linearize_structuring: false,
            force_linear_structuring: false,
            conservative_irreducible_fallback: false,
            structuring_engine: StructuringEngineKind::GraphCollapseV1,
            selection_axis: Default::default(),
            global_names: Default::default(),
            global_sizes: Default::default(),
            relocation_names: Default::default(),
            declared_signatures: Default::default(),
            function_type_aliases: Default::default(),
            calling_convention: Default::default(),
            userops: Default::default(),
            cspec_param_offsets: None,
            cspec_float_param_offsets: None,
            cspec_float_shares_int_slots: false,
            cspec_stack_arg_base: None,
            cspec_stack_pointer_offset: None,
            cspec_unaffected_offsets: Vec::new(),
            cspec_extrapop: None,
            sla_register_map: None,
            cspec_return_offset: None,
            cspec_float_return_offset: None,
            cspec_return_target: None,
            cspec_alloca_probe_targets: Vec::new(),
            pspec_programcounter: None,
            pspec_tracked_context: Vec::new(),
            pspec_hidden_registers: Default::default(),
            is_data_ref_origin: false,
        }
    }

    #[test]
    fn try_repair_orphan_gotos_returns_none_for_unknown_label() {
        let dummy = PcodeFunction { blocks: Vec::new() };
        let options = test_options();
        let mut builder = PreviewBuilder::new(&dummy, &options, None);
        let body = vec![PreHirStmt::Goto("block_deadbeef".to_string())];
        assert!(orphan_goto_labels(&body).contains(&"block_deadbeef".to_string()));
        assert!(try_repair_orphan_gotos(&mut builder, body).is_none());
    }

    #[test]
    fn try_repair_orphan_gotos_noop_when_already_valid() {
        let dummy = PcodeFunction { blocks: Vec::new() };
        let options = test_options();
        let mut builder = PreviewBuilder::new(&dummy, &options, None);
        let body = vec![
            PreHirStmt::Label("block_100".to_string()),
            PreHirStmt::Return(None),
        ];
        assert!(!has_orphan_goto_labels(&body));
        let repaired = try_repair_orphan_gotos(&mut builder, body.clone()).expect("noop repair");
        assert!(!has_orphan_goto_labels(&repaired));
    }

    #[test]
    fn orphan_repair_does_not_hijack_identical_unrelated_statement() {
        let target = PcodeBasicBlock {
            index: 1,
            start_address: 0x1010,
            successors: Vec::new(),
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Copy,
                    address: 0x1010,
                    output: Some(Varnode {
                        space_id: 1,
                        offset: 0,
                        size: 8,
                        is_constant: false,
                        constant_val: 0,
                    }),
                    inputs: vec![Varnode::constant(0, 8)],
                    asm_mnemonic: Some("mov rax, 0".to_string()),
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::Return,
                    address: 0x1018,
                    output: None,
                    inputs: vec![Varnode::constant(7, 8)],
                    asm_mnemonic: Some("ret".to_string()),
                },
            ],
        };
        let function = PcodeFunction {
            blocks: vec![
                PcodeBasicBlock {
                    index: 0,
                    start_address: 0x1000,
                    successors: vec![1],
                    ops: Vec::new(),
                },
                target,
            ],
        };
        let options = test_options();
        let mut builder = PreviewBuilder::new(&function, &options, None);
        let duplicate = PreHirStmt::Assign {
            lhs: PreHirLValue::Var("rax".to_string()),
            rhs: PreHirExpr::Const(
                0,
                NirType::Int {
                    bits: 64,
                    signed: false,
                },
            ),
        };
        let body = vec![duplicate, PreHirStmt::Goto("block_1010".to_string())];

        let repaired = try_repair_orphan_gotos(&mut builder, body).expect("repair");
        let assignment_count = repaired
            .iter()
            .filter(|stmt| matches!(stmt, PreHirStmt::Assign { .. }))
            .count();
        assert_eq!(
            assignment_count, 2,
            "the unrelated assignment and the target assignment must both survive repair: {repaired:?}"
        );
        assert!(
            repaired
                .iter()
                .any(|stmt| matches!(stmt, PreHirStmt::Return(Some(PreHirExpr::Var(name))) if name == "rax")),
            "the target terminator must remain attached to the repaired block: {repaired:?}"
        );
    }

    #[test]
    fn try_repair_orphan_gotos_supports_switch_terminator_target() {
        use fission_midend_structuring::linear_types::LoweredTerminator;

        let function = PcodeFunction {
            blocks: vec![
                PcodeBasicBlock {
                    index: 0,
                    start_address: 0x1000,
                    successors: vec![1],
                    ops: Vec::new(),
                },
                PcodeBasicBlock {
                    index: 1,
                    start_address: 0x1010,
                    successors: vec![2, 3],
                    ops: Vec::new(),
                },
                PcodeBasicBlock {
                    index: 2,
                    start_address: 0x1020,
                    successors: Vec::new(),
                    ops: Vec::new(),
                },
                PcodeBasicBlock {
                    index: 3,
                    start_address: 0x1030,
                    successors: Vec::new(),
                    ops: Vec::new(),
                },
            ],
        };
        let options = test_options();
        let mut builder = PreviewBuilder::new(&function, &options, None);
        builder.terminator_cache.insert(
            1,
            LoweredTerminator::Switch {
                expr: PreHirExpr::Var("sel".to_string()),
                targets: vec![0x1020],
                default_target: Some(0x1030),
                min_val: 0,
                proof: None,
            },
        );

        let body = vec![PreHirStmt::Goto("block_1010".to_string())];
        assert!(has_orphan_goto_labels(&body));

        let repaired = try_repair_orphan_gotos(&mut builder, body).expect("repair switch orphan");
        assert!(
            repaired
                .iter()
                .any(|stmt| matches!(stmt, PreHirStmt::Switch { .. })),
            "the repaired block must contain the LoweredTerminator::Switch: {repaired:?}"
        );
        assert!(!has_orphan_goto_labels(&repaired));
    }

    #[test]
    fn lower_loop_body_subgraph_cond_break_preserves_non_fallthrough_target() {
        use fission_midend_structuring::linear_types::LoweredTerminator;
        use fission_midend_structuring::lower_loop_body_subgraph;
        use rustc_hash::FxHashSet;

        let function = PcodeFunction {
            blocks: vec![
                PcodeBasicBlock {
                    index: 0,
                    start_address: 0x1000,
                    successors: vec![1],
                    ops: Vec::new(),
                },
                PcodeBasicBlock {
                    index: 1,
                    start_address: 0x1010,
                    successors: vec![3, 2],
                    ops: Vec::new(),
                },
                PcodeBasicBlock {
                    index: 2,
                    start_address: 0x1020,
                    successors: vec![0],
                    ops: Vec::new(),
                },
                PcodeBasicBlock {
                    index: 3,
                    start_address: 0x1030,
                    successors: Vec::new(),
                    ops: Vec::new(),
                },
            ],
        };
        let options = test_options();
        let mut builder = PreviewBuilder::new(&function, &options, None);
        // Block 1 ends in Cond { true_target: 0x1030 (exit/break), false_target: Some(0x1025) }
        // where 0x1025 != next_addr (which is 0x1020)
        builder.terminator_cache.insert(
            1,
            LoweredTerminator::Cond {
                cond: PreHirExpr::Var("break_cond".to_string()),
                true_target: 0x1030,
                false_target: Some(0x1025),
            },
        );

        let body_set: FxHashSet<usize> = [0, 1, 2].into_iter().collect();
        let stmts = lower_loop_body_subgraph(&mut builder, &body_set, 0, Some(3), 0, 0)
            .expect("lowered")
            .expect("body");

        // The if statement for block 1 must preserve the non-fallthrough false arm in else_body
        let cond_if = stmts
            .iter()
            .find(|stmt| matches!(stmt, PreHirStmt::If { .. }))
            .expect("if stmt");
        let PreHirStmt::If {
            then_body,
            else_body,
            ..
        } = cond_if
        else {
            unreachable!()
        };
        assert!(then_body.iter().any(|s| matches!(s, PreHirStmt::Break)));
        assert!(
            else_body
                .iter()
                .any(|s| matches!(s, PreHirStmt::Goto(lbl) if lbl == "block_1025")),
            "expected Goto(block_1025) in else_body: {else_body:?}"
        );
    }

    #[test]
    fn lower_loop_body_subgraph_uses_target_key_for_virtual_blocks() {
        use fission_midend_structuring::linear_types::LoweredTerminator;
        use fission_midend_structuring::lower_loop_body_subgraph;
        use rustc_hash::FxHashSet;

        let function = PcodeFunction {
            blocks: vec![
                PcodeBasicBlock {
                    index: 0,
                    start_address: 0x1000,
                    successors: vec![1],
                    ops: Vec::new(),
                },
                PcodeBasicBlock {
                    index: 1,
                    start_address: 0x1010,
                    successors: vec![2],
                    ops: Vec::new(),
                },
                PcodeBasicBlock {
                    index: 2,
                    start_address: 0x1020,
                    successors: Vec::new(),
                    ops: Vec::new(),
                },
            ],
        };
        let options = test_options();
        let mut builder = PreviewBuilder::new(&function, &options, None);
        // Simulate a virtual duplicated block for block 1 with synthetic target key
        let virtual_key = 0x1010 | (1u64 << 48); // with duplicate tag
        builder.block_target_keys[1] = virtual_key;

        // Block 0 branches to virtual block 1
        builder
            .terminator_cache
            .insert(0, LoweredTerminator::Goto(virtual_key));

        let body_set: FxHashSet<usize> = [0, 1].into_iter().collect();
        let stmts = lower_loop_body_subgraph(&mut builder, &body_set, 0, Some(2), 0, 0)
            .expect("lowered")
            .expect("body");

        // The virtual block label must be emitted because force_labels matched block_target_key
        let expected_label = fission_midend_structuring::helpers::block_label(virtual_key);
        assert!(
            stmts
                .iter()
                .any(|s| matches!(s, PreHirStmt::Label(lbl) if lbl == &expected_label)),
            "expected label {expected_label} in lowered statements: {stmts:?}"
        );
    }
}
