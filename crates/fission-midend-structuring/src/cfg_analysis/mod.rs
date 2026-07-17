//! Pure CFG analysis facts (dom, postdom, SCC, follow, trace DAG).

mod dom;
mod edge;
mod follow;
mod goto_selector;
mod postdom;
mod scc;
// tests re-enabled after build_predecessor helpers land in this crate
// #[cfg(test)]
// mod tests;
mod trace_dag;
pub mod util;

pub use dom::{DomTree, DominanceFrontier, ImmDomTree};
pub use edge::{CfgAnalysis, EdgeClass};
pub use follow::dom_based_fallthrough_successor;
pub use goto_selector::select_bad_edge;
pub use postdom::{ImmPostDomTree, PostDomTree};
pub use scc::SccAnalysis;
pub use trace_dag::{TraceDag, TraceDagError};


/// Cached CFG facts (edges, dom, postdom, SCC) for a function body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CfgFactCache {
    edge_analysis: CfgAnalysis,
    dom_tree: DomTree,
    imm_dom_tree: ImmDomTree,
    dom_frontier: DominanceFrontier,
    postdom_tree: PostDomTree,
    imm_postdom_tree: ImmPostDomTree,
    scc_analysis: SccAnalysis,
}

impl CfgFactCache {
    pub fn analyze(successors: &[Vec<usize>], predecessors: &[Vec<usize>]) -> Self {
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

    pub fn edges(&self) -> &CfgAnalysis {
        &self.edge_analysis
    }
    pub fn dominators(&self) -> &DomTree {
        &self.dom_tree
    }
    pub fn dominance_frontier(&self) -> &DominanceFrontier {
        &self.dom_frontier
    }
    pub fn postdominators(&self) -> &PostDomTree {
        &self.postdom_tree
    }
    pub fn immediate_postdominators(&self) -> &ImmPostDomTree {
        &self.imm_postdom_tree
    }
    pub fn scc(&self) -> &SccAnalysis {
        &self.scc_analysis
    }
}

/// Compute follow blocks for multi-successor nodes (free-function form of
/// the former PreviewBuilder::compute_follow_blocks method).
pub fn compute_follow_blocks(
    successors: &[Vec<usize>],
    predecessors: &[Vec<usize>],
    facts: &CfgFactCache,
    total_blocks: usize,
) -> Vec<Option<usize>> {
    let dom_tree = facts.dominators();
    let dom_frontier = facts.dominance_frontier();
    let imm_postdom = facts.immediate_postdominators();

    (0..total_blocks)
        .map(|i| {
            let succs = successors.get(i)?;
            if succs.len() < 2 {
                return None;
            }
            if total_blocks <= 500 {
                let mut trace_dag = TraceDag::new(successors, predecessors, dom_tree);
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
