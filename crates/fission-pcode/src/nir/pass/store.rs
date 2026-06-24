use crate::nir::pass::NirFunc;
use crate::nir::structuring::cfg_analysis::TraceDag;
use crate::nir::structuring::loop_analysis::LoopBody;
use crate::nir::structuring::{CfgAnalysis, CfgFactCache};

pub(crate) struct AnalysisStore {
    cfg_version: Option<usize>,
    cfg_facts: Option<CfgFactCache>,
    loop_bodies: Option<Vec<LoopBody>>,
    follow_blocks: Option<Vec<Option<usize>>>,
}

impl AnalysisStore {
    pub(crate) fn new() -> Self {
        Self {
            cfg_version: None,
            cfg_facts: None,
            loop_bodies: None,
            follow_blocks: None,
        }
    }

    fn ensure_up_to_date(&mut self, ir: &NirFunc<'_, '_>) {
        if self.cfg_version != Some(ir.cfg_version()) || self.cfg_facts.is_none() {
            let successors = ir.successors();
            let predecessors = ir.predecessors();

            let cfg_facts = CfgFactCache::analyze(successors, predecessors);
            let cfg_analysis = CfgAnalysis::analyze(successors, predecessors);
            let dom_tree = cfg_facts.dominators();
            let irreducible_edges = cfg_analysis.irreducible_edges(dom_tree);

            let loop_bodies = LoopBody::identify_loops(
                successors,
                predecessors,
                &cfg_analysis,
                &irreducible_edges,
            );

            let dom_frontier = cfg_facts.dominance_frontier();
            let imm_postdom = cfg_facts.immediate_postdominators();
            let total_blocks = ir.block_count();

            let follow_blocks: Vec<Option<usize>> = (0..total_blocks)
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
                .collect();

            self.cfg_facts = Some(cfg_facts);
            self.loop_bodies = Some(loop_bodies);
            self.follow_blocks = Some(follow_blocks);
            self.cfg_version = Some(ir.cfg_version());
        }
    }

    pub(crate) fn cfg_facts(&mut self, ir: &NirFunc<'_, '_>) -> &CfgFactCache {
        self.ensure_up_to_date(ir);
        self.cfg_facts.as_ref().unwrap()
    }

    pub(crate) fn loop_bodies(&mut self, ir: &NirFunc<'_, '_>) -> &[LoopBody] {
        self.ensure_up_to_date(ir);
        self.loop_bodies.as_deref().unwrap()
    }

    pub(crate) fn follow_blocks(&mut self, ir: &NirFunc<'_, '_>) -> &[Option<usize>] {
        self.ensure_up_to_date(ir);
        self.follow_blocks.as_deref().unwrap()
    }

    pub(crate) fn invalidate(&mut self) {
        self.cfg_version = None;
        self.cfg_facts = None;
        self.loop_bodies = None;
        self.follow_blocks = None;
    }

    #[cfg(test)]
    pub(crate) fn cfg_version_for_test(&self) -> Option<usize> {
        self.cfg_version
    }
}
