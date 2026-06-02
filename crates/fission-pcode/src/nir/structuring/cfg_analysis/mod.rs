//! CFG fact cache: edge classification, dominators, postdominators, SCC.

use super::*;

mod dom;
mod edge;
mod postdom;
mod scc;
pub(crate) mod util;
mod trace_dag;

pub(crate) use dom::{DomTree, DominanceFrontier, ImmDomTree};
pub(crate) use edge::{CfgAnalysis, EdgeClass};
pub(crate) use postdom::{ImmPostDomTree, PostDomTree};
pub(crate) use scc::SccAnalysis;
pub(crate) use trace_dag::{TraceDag, TraceDagError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CfgFactCache {
    edge_analysis: CfgAnalysis,
    dom_tree: DomTree,
    imm_dom_tree: ImmDomTree,
    dom_frontier: DominanceFrontier,
    postdom_tree: PostDomTree,
    imm_postdom_tree: ImmPostDomTree,
    scc_analysis: SccAnalysis,
}

impl CfgFactCache {
    pub(crate) fn analyze(successors: &[Vec<usize>], predecessors: &[Vec<usize>]) -> Self {
        let imm_dom_tree = ImmDomTree::compute(successors, predecessors);
        Self {
            edge_analysis: CfgAnalysis::analyze(successors, predecessors),
            dom_tree: DomTree::analyze(successors, predecessors),
            dom_frontier: DominanceFrontier::compute(predecessors, &imm_dom_tree),
            imm_dom_tree,
            postdom_tree: PostDomTree::analyze(successors, predecessors),
            imm_postdom_tree: ImmPostDomTree::compute(successors, predecessors),
            scc_analysis: SccAnalysis::analyze(successors, predecessors),
        }
    }

    pub(crate) fn edges(&self) -> &CfgAnalysis {
        &self.edge_analysis
    }

    pub(crate) fn dominators(&self) -> &DomTree {
        &self.dom_tree
    }

    pub(crate) fn dominance_frontier(&self) -> &DominanceFrontier {
        &self.dom_frontier
    }

    pub(crate) fn postdominators(&self) -> &PostDomTree {
        &self.postdom_tree
    }

    pub(crate) fn immediate_postdominators(&self) -> &ImmPostDomTree {
        &self.imm_postdom_tree
    }

    pub(crate) fn scc(&self) -> &SccAnalysis {
        &self.scc_analysis
    }
}

impl<'a> PreviewBuilder<'a> {
    pub(super) fn cfg_fact_cache(&self) -> &CfgFactCache {
        &self.cfg_facts
    }

    pub(super) fn refresh_cfg_fact_cache(&mut self) {
        self.cfg_facts = CfgFactCache::analyze(&self.successors, &self.predecessors);
        self.dom_tree = self.cfg_facts.dominators().clone();
    }

    pub(super) fn analyze_cfg_dominators(&self) -> DomTree {
        self.cfg_facts.dominators().clone()
    }

    pub(super) fn analyze_cfg_scc(&self) -> SccAnalysis {
        self.cfg_facts.scc().clone()
    }
}

#[cfg(test)]
mod tests;
