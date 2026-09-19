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
            dual_layer_structuring: false,
            global_names: Default::default(),
            global_sizes: Default::default(),
            relocation_names: Default::default(),
            declared_signatures: Default::default(),
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
}
