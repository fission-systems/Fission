//! Function-level Ghidra-style iterative collapse (env-gated alternative to SESE tree).

use super::*;
use super::cfg_analysis::select_bad_edge;

pub(crate) fn collapse_loop_admission_enabled() -> bool {
    std::env::var_os("FISSION_COLLAPSE_LOOP").is_some()
}

/// Collapse the full function body without SESE region decomposition.
pub(crate) fn structure_cfg_via_collapse_loop(
    builder: &mut PreviewBuilder,
    total_nodes: usize,
) -> Result<Vec<HirStmt>, MlilPreviewError> {
    builder.build_sese_region_body(0, total_nodes, HashMap::default())
}

impl<'a> PreviewBuilder<'a> {
    pub(crate) fn try_virtualize_one_bad_edge(
        &mut self,
        entry: usize,
        exit: usize,
    ) -> Result<bool, MlilPreviewError> {
        let Some((from, to)) = select_bad_edge(
            entry,
            exit,
            &self.successors,
            &self.predecessors,
            &self.fas_virtual_edges,
        ) else {
            return Ok(false);
        };
        Ok(self.apply_virtual_goto_edge(from, to))
    }

    pub(crate) fn apply_virtual_goto_edge(&mut self, from: usize, to: usize) -> bool {
        if self
            .fas_virtual_edges
            .iter()
            .any(|&(src, dst)| src == from && dst == to)
        {
            return false;
        }
        let Some(pos) = self
            .successors
            .get(from)
            .and_then(|succs| succs.iter().position(|&succ| succ == to))
        else {
            return false;
        };
        self.successors[from].remove(pos);
        if let Some(preds) = self.predecessors.get_mut(to) {
            preds.retain(|&pred| pred != from);
        }
        self.fas_virtual_edges.push((from, to));
        self.telemetry.structuring.fas_virtual_goto_count += 1;
        self.telemetry
            .structuring
            .structuring_select_bad_edge_count += 1;
        self.terminator_cache.remove(&from);
        self.refresh_cfg_fact_cache();
        true
    }

    pub(crate) fn is_virtual_goto_edge(&self, from: usize, to: usize) -> bool {
        self.fas_virtual_edges
            .iter()
            .any(|&(src, dst)| src == from && dst == to)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nir::types::MlilPreviewOptions;
    use crate::nir::types::StructuringEngineKind;
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
