//! Orphan-goto repair thin wrappers over midend free owners (ADR 0012).

use super::*;

impl<'a> PreviewBuilder<'a> {
    pub(super) fn find_block_index_by_label(&self, label: &str) -> Option<usize> {
        fission_midend_structuring::find_block_index_by_label(self, label)
    }

    pub(crate) fn try_repair_orphan_gotos(&mut self, body: Vec<HirStmt>) -> Option<Vec<HirStmt>> {
        fission_midend_structuring::try_repair_orphan_gotos(self, body)
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::cleanup::{has_orphan_goto_labels, orphan_goto_labels};
    use crate::PcodeFunction;
    use crate::midend::PreviewBuilder;
    use crate::midend::ir::{HirStmt, MlilPreviewOptions, StructuringEngineKind};

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
        }
    }

    #[test]
    fn try_repair_orphan_gotos_returns_none_for_unknown_label() {
        let dummy = PcodeFunction { blocks: Vec::new() };
        let options = test_options();
        let mut builder = PreviewBuilder::new(&dummy, &options, None);
        let body = vec![HirStmt::Goto("block_deadbeef".to_string())];
        assert!(orphan_goto_labels(&body).contains(&"block_deadbeef".to_string()));
        assert!(builder.try_repair_orphan_gotos(body).is_none());
    }

    #[test]
    fn try_repair_orphan_gotos_noop_when_already_valid() {
        let dummy = PcodeFunction { blocks: Vec::new() };
        let options = test_options();
        let mut builder = PreviewBuilder::new(&dummy, &options, None);
        let body = vec![
            HirStmt::Label("block_100".to_string()),
            HirStmt::Return(None),
        ];
        assert!(!has_orphan_goto_labels(&body));
        let repaired = builder
            .try_repair_orphan_gotos(body.clone())
            .expect("noop repair");
        assert!(!has_orphan_goto_labels(&repaired));
    }
}
