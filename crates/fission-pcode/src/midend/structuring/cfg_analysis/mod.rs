//! CFG fact cache re-export + PreviewBuilder CFG hooks.

use super::*;
pub use fission_midend_structuring::cfg_analysis::{
    CfgAnalysis, CfgFactCache, DomTree, DominanceFrontier, EdgeClass, ImmDomTree, ImmPostDomTree,
    PostDomTree, SccAnalysis, TraceDag, TraceDagError, compute_follow_blocks,
    dom_based_fallthrough_successor, select_bad_edge, util,
};

impl<'a> PreviewBuilder<'a> {
    pub(super) fn cfg_fact_cache(&self) -> &CfgFactCache {
        &self.cfg_facts
    }

    pub(crate) fn refresh_cfg_fact_cache(&mut self) {
        self.cfg_facts = CfgFactCache::analyze(&self.successors, &self.predecessors);
        self.dom_tree = self.cfg_facts.dominators().clone();
    }

    pub(super) fn analyze_cfg_dominators(&self) -> DomTree {
        self.cfg_facts.dominators().clone()
    }

    pub(super) fn analyze_cfg_scc(&self) -> SccAnalysis {
        self.cfg_facts.scc().clone()
    }

    pub(crate) fn compute_follow_blocks(&self) -> Vec<Option<usize>> {
        let total = self.pcode.blocks.len() + self.virtual_block_map.len();
        compute_follow_blocks(
            &self.successors,
            &self.predecessors,
            &self.cfg_facts,
            total,
        )
    }
}
