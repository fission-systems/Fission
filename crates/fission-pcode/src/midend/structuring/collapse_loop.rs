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
) -> Result<Vec<HirStmt>, MlilPreviewError> {
    fission_midend_structuring::build_sese_region_body(
        builder,
        0,
        total_nodes,
        HashMap::default(),
    )
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
    use crate::pcode::PcodeFunction;

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
            global_names: Default::default(),
            global_sizes: Default::default(),
            relocation_names: Default::default(),
            calling_convention: Default::default(),
            userops: Default::default(),
            cspec_param_offsets: None,
            cspec_stack_arg_base: None,
            cspec_extrapop: None,
            sla_register_map: None,
            cspec_return_offset: None,
            cspec_return_target: None,
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
}
