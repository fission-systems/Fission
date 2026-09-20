//! Function-level Ghidra-style iterative collapse (env-gated alternative to SESE tree).
//!
//! Edge virtualization free functions live in `fission-midend-structuring`; this
//! module keeps the SESE-entry wrapper that still needs `PreviewBuilder`.

use super::*;
pub use fission_midend_structuring::collapse_loop::{
    apply_virtual_goto_edge, collapse_loop_admission_enabled, is_virtual_goto_edge,
    try_virtualize_one_bad_edge,
};

/// Collapse the full function body without SESE region decomposition.
///
/// Thin host entry: delegates to midend-structuring free-fn
/// [`fission_midend_structuring::build_sese_region_body`].
pub(crate) fn structure_cfg_via_collapse_loop(
    builder: &mut PreviewBuilder,
    total_nodes: usize,
) -> Result<Vec<PreHirStmt>, MlilPreviewError> {
    fission_midend_structuring::build_sese_region_body(builder, 0, total_nodes, HashMap::default())
        .map(|(body, _achieved_exit, _extra_members)| body)
}

impl<'a> PreviewBuilder<'a> {
    pub(crate) fn try_virtualize_one_bad_edge(
        &mut self,
        entry: usize,
        exit: usize,
    ) -> Result<bool, MlilPreviewError> {
        try_virtualize_one_bad_edge(self, entry, exit)
    }

    pub(crate) fn apply_virtual_goto_edge(&mut self, from: usize, to: usize) -> bool {
        apply_virtual_goto_edge(self, from, to)
    }

    pub(crate) fn is_virtual_goto_edge(&self, from: usize, to: usize) -> bool {
        is_virtual_goto_edge(self, from, to)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midend::ir::MlilPreviewOptions;
    use crate::midend::ir::StructuringEngineKind;
    use crate::pcode::{PcodeBasicBlock, PcodeFunction, PcodeOp, PcodeOpcode, Varnode};

    fn conditional_function() -> PcodeFunction {
        PcodeFunction {
            blocks: vec![
                PcodeBasicBlock {
                    index: 0,
                    start_address: 0x1000,
                    successors: vec![2, 1],
                    ops: vec![PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x1000,
                        output: None,
                        inputs: vec![Varnode::constant(0x1020, 8), Varnode::constant(1, 1)],
                        asm_mnemonic: None,
                    }],
                },
                PcodeBasicBlock {
                    index: 1,
                    start_address: 0x1010,
                    successors: Vec::new(),
                    ops: vec![PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Return,
                        address: 0x1010,
                        output: None,
                        inputs: Vec::new(),
                        asm_mnemonic: None,
                    }],
                },
                PcodeBasicBlock {
                    index: 2,
                    start_address: 0x1020,
                    successors: vec![3],
                    ops: vec![
                        PcodeOp {
                            seq_num: 2,
                            opcode: PcodeOpcode::Copy,
                            address: 0x1020,
                            output: Some(Varnode {
                                space_id: crate::midend::support::RUST_SLEIGH_REGISTER_SPACE_ID,
                                offset: 0,
                                size: 8,
                                is_constant: false,
                                constant_val: 0,
                            }),
                            inputs: vec![Varnode::constant(7, 8)],
                            asm_mnemonic: None,
                        },
                        PcodeOp {
                            seq_num: 3,
                            opcode: PcodeOpcode::Branch,
                            address: 0x1021,
                            output: None,
                            inputs: vec![Varnode::constant(0x1030, 8)],
                            asm_mnemonic: None,
                        },
                    ],
                },
                PcodeBasicBlock {
                    index: 3,
                    start_address: 0x1030,
                    successors: Vec::new(),
                    ops: vec![PcodeOp {
                        seq_num: 4,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x1030,
                        output: None,
                        inputs: vec![Varnode::constant(0x1020, 8), Varnode::constant(1, 1)],
                        asm_mnemonic: None,
                    }],
                },
            ],
        }
    }

    #[test]
    fn apply_virtual_goto_edge_removes_cfg_edge() {
        let dummy = PcodeFunction { blocks: Vec::new() };
        let options = MlilPreviewOptions {
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
        };
        let mut builder = PreviewBuilder::new(&dummy, &options, None);
        builder.successors = vec![vec![1], vec![0], vec![]];
        builder.predecessors = vec![vec![], vec![0], vec![]];
        assert!(builder.apply_virtual_goto_edge(1, 0));
        assert!(builder.is_virtual_goto_edge(1, 0));
        assert!(builder.successors[1].is_empty());
        assert!(builder.predecessors[0].is_empty());
    }

    #[test]
    fn fas_virtual_targets_are_kept_in_jump_target_inventory() {
        let pcode = conditional_function();
        let options = MlilPreviewOptions::default();
        let mut builder = PreviewBuilder::new(&pcode, &options, None);
        builder.successors = vec![Vec::new(), Vec::new(), Vec::new(), Vec::new()];
        builder.predecessors = vec![Vec::new(), Vec::new(), Vec::new(), Vec::new()];
        builder.fas_virtual_edges.push((0, 2));

        let targets = builder.collect_jump_targets().expect("target inventory");
        assert!(targets.contains(&builder.block_target_key(2)));
    }

    #[test]
    fn linear_fas_materialization_emits_the_removed_conditional_edge_and_label() {
        let pcode = conditional_function();
        let options = MlilPreviewOptions::default();
        let mut builder = PreviewBuilder::new(&pcode, &options, None);
        assert!(builder.apply_virtual_goto_edge(0, 2));

        let body = fission_midend_structuring::build_linear_multiblock_body(&mut builder, false)
            .expect("linear fallback should preserve the virtual edge");
        let target_label = fission_midend_structuring::block_label(builder.block_target_key(2));
        assert!(
            body.iter().any(|stmt| matches!(
                stmt,
                PreHirStmt::Label(label) if label == &target_label
            )),
            "removed FAS target must receive a label: {body:?}"
        );
        assert!(
            body.iter().any(|stmt| matches!(
                stmt,
                PreHirStmt::If { then_body, .. }
                    if then_body.iter().any(|nested| matches!(
                        nested,
                        PreHirStmt::Goto(label) if label == &target_label
                    ))
            )),
            "removed conditional edge must remain an explicit goto: {body:?}"
        );
    }
}
