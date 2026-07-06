import re

content = open('crates/fission-pcode/src/nir/builder/state.rs').read()

if 'fn compute_follow_blocks' not in content:
    with open('crates/fission-pcode/src/nir/builder/state.rs', 'a') as f:
        f.write('''
impl<'a> PreviewBuilder<'a> {
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
                    let mut trace_dag = crate::nir::structuring::cfg_analysis::TraceDag::new(
                        &self.successors,
                        &self.predecessors,
                        dom_tree,
                    );
                    
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
''')
