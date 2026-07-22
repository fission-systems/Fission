//! Orphan-goto repair tests (production calls free-fn in `pass/structuring`).
//!
//! Owner: `fission_midend_structuring::try_repair_orphan_gotos` (ADR 0012).

#[cfg(test)]
mod tests {
    use super::super::super::cleanup::{has_orphan_goto_labels, orphan_goto_labels};
    use crate::PcodeFunction;
    use crate::midend::PreviewBuilder;
    use crate::midend::ir::{MlilPreviewOptions, StructuringEngineKind};
use fission_midend_dir::{DirStmt};
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
            cspec_float_return_offset: None,
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
        let body = vec![DirStmt::Goto("block_deadbeef".to_string())];
        assert!(orphan_goto_labels(&body).contains(&"block_deadbeef".to_string()));
        assert!(try_repair_orphan_gotos(&mut builder, body).is_none());
    }

    #[test]
    fn try_repair_orphan_gotos_noop_when_already_valid() {
        let dummy = PcodeFunction { blocks: Vec::new() };
        let options = test_options();
        let mut builder = PreviewBuilder::new(&dummy, &options, None);
        let body = vec![
            DirStmt::Label("block_100".to_string()),
            DirStmt::Return(None),
        ];
        assert!(!has_orphan_goto_labels(&body));
        let repaired = try_repair_orphan_gotos(&mut builder, body.clone()).expect("noop repair");
        assert!(!has_orphan_goto_labels(&repaired));
    }
}
