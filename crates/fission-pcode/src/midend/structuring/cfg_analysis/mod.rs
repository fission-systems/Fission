//! CFG fact cache: edge classification, dominators, postdominators, SCC.

use super::*;

mod dom;
mod edge;
mod follow;
mod goto_selector;
mod postdom;
mod scc;
mod trace_dag;
pub(crate) mod util;

pub(crate) use dom::{DomTree, DominanceFrontier, ImmDomTree};
pub(crate) use edge::{CfgAnalysis, EdgeClass};
pub(crate) use follow::dom_based_fallthrough_successor;
pub(crate) use goto_selector::select_bad_edge;
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
}

#[cfg(test)]
mod tests;

impl<'a> crate::midend::builder::PreviewBuilder<'a> {
    pub(crate) fn compute_follow_blocks(&self) -> Vec<Option<usize>> {
        let total_blocks_for_follow = self.pcode.blocks.len() + self.virtual_block_map.len();
        let dom_tree = self.cfg_facts.dominators();
        let dom_frontier = self.cfg_fact_cache().dominance_frontier();
        let imm_postdom = self.cfg_fact_cache().immediate_postdominators();

        (0..total_blocks_for_follow)
            .map(|i| {
                let succs = self.successors.get(i)?;
                if succs.len() < 2 {
                    return None;
                }

                if total_blocks_for_follow <= 500 {
                    let mut trace_dag =
                        TraceDag::new(&self.successors, &self.predecessors, dom_tree);

                    if let Ok(Some(exitblock)) = trace_dag.find_follow_block(i) {
                        if exitblock > i {
                            return Some(exitblock);
                        }
                    }
                }

                let follow = imm_postdom.nearest_common_postdominator(succs)?;
                if follow <= i {
                    return None;
                }
                let has_frontier_witness = succs
                    .iter()
                    .copied()
                    .any(|succ| succ == follow || dom_frontier.contains(succ, follow));
                has_frontier_witness.then_some(follow)
            })
            .collect()
    }
}
