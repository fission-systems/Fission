use super::*;
use std::collections::{HashSet, HashMap};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TraceDagError {
    Stuck,
}

#[derive(Debug, Clone)]
struct BranchPoint {
    id: usize,
    parent_trace: Option<usize>,
    paths: Vec<usize>,
}

#[derive(Debug, Clone)]
struct BlockTrace {
    id: usize,
    parent_bp: usize,
    destnode: usize,
    active: bool,
    terminal: bool,
}

pub(crate) struct TraceDag<'a> {
    successors: &'a [Vec<usize>],
    predecessors: &'a [Vec<usize>],
    dom_tree: &'a DomTree,
    
    branch_points: Vec<BranchPoint>,
    traces: Vec<BlockTrace>,
    active_traces: HashSet<usize>,
    node_visit_counts: HashMap<usize, usize>,
}

impl<'a> TraceDag<'a> {
    pub(crate) fn new(
        successors: &'a [Vec<usize>],
        predecessors: &'a [Vec<usize>],
        dom_tree: &'a DomTree,
    ) -> Self {
        Self {
            successors,
            predecessors,
            dom_tree,
            branch_points: Vec::new(),
            traces: Vec::new(),
            active_traces: HashSet::new(),
            node_visit_counts: HashMap::new(),
        }
    }

    fn create_branch_point(&mut self, parent_trace: Option<usize>, destnode: usize) -> usize {
        let bp_id = self.branch_points.len();
        let mut paths = Vec::new();
        
        let forward_succs: Vec<usize> = self.successors[destnode]
            .iter()
            .copied()
            .filter(|&succ| !self.dom_tree.dominates(succ, destnode))
            .collect();
            
        for succ in forward_succs {
            let trace_id = self.traces.len();
            paths.push(trace_id);
            self.traces.push(BlockTrace {
                id: trace_id,
                parent_bp: bp_id,
                destnode: succ,
                active: true,
                terminal: false,
            });
            self.active_traces.insert(trace_id);
            *self.node_visit_counts.entry(succ).or_insert(0) += 1;
        }
        
        self.branch_points.push(BranchPoint {
            id: bp_id,
            parent_trace,
            paths,
        });
        
        bp_id
    }

    /// Returns `Some(Some(node))` if converged at `node`.
    /// Returns `Some(None)` if all paths are terminal.
    /// Returns `None` if cannot retire yet.
    fn check_retirement(&self, bp_id: usize) -> Option<Option<usize>> {
        let bp = &self.branch_points[bp_id];
        let mut outblock = None;
        
        for &trace_id in &bp.paths {
            let trace = &self.traces[trace_id];
            if !trace.active {
                return None; // Waiting on a child branch point to retire
            }
            if trace.terminal {
                continue;
            }
            if let Some(existing) = outblock {
                if existing != trace.destnode {
                    return None; // Active paths diverge
                }
            } else {
                outblock = Some(trace.destnode);
            }
        }
        
        Some(outblock)
    }

    fn check_open(&self, trace_id: usize, start_idx: usize) -> bool {
        let trace = &self.traces[trace_id];
        if trace.terminal {
            return false;
        }
        let dest = trace.destnode;
        
        let visited = self.node_visit_counts.get(&dest).copied().unwrap_or(0);
        
        let mut expected = 0;
        for &pred in &self.predecessors[dest] {
            if self.dom_tree.dominates(dest, pred) {
                continue; // Ignore back-edges
            }
            if pred == start_idx || self.dom_tree.dominates(start_idx, pred) {
                expected += 1;
            }
        }
        
        visited >= expected
    }

    fn retire_branch(&mut self, bp_id: usize, exitblock_opt: Option<usize>) {
        let bp = self.branch_points[bp_id].clone();
        
        for &trace_id in &bp.paths {
            self.active_traces.remove(&trace_id);
        }
        
        if let Some(parent_trace_id) = bp.parent_trace {
            let parent_trace = &mut self.traces[parent_trace_id];
            if let Some(exitblock) = exitblock_opt {
                parent_trace.destnode = exitblock;
                parent_trace.active = true;
                parent_trace.terminal = false;
                self.active_traces.insert(parent_trace_id);
                // We advance the parent trace, so we must record a visit to the new destnode!
                *self.node_visit_counts.entry(exitblock).or_insert(0) += 1;
            } else {
                parent_trace.active = true;
                parent_trace.terminal = true;
                self.active_traces.insert(parent_trace_id);
            }
        }
    }

    pub(crate) fn find_follow_block(&mut self, start_idx: usize) -> Result<Option<usize>, TraceDagError> {
        let succs = &self.successors[start_idx];
        if succs.len() < 2 {
            return Ok(None);
        }

        let root_bp = self.create_branch_point(None, start_idx);
        
        let mut stuck_count = 0;
        let mut total_steps = 0;
        
        while !self.active_traces.is_empty() {
            total_steps += 1;
            if total_steps > 200 {
                return Ok(None);
            }
            let mut progress = false;
            
            // Check if root branch point can retire (success condition)
            if let Some(exitblock_opt) = self.check_retirement(root_bp) {
                return Ok(exitblock_opt);
            }
            
            // Try to retire any other branch point or open a trace
            let active_list: Vec<usize> = self.active_traces.iter().copied().collect();
            
            for trace_id in active_list {
                let parent_bp = self.traces[trace_id].parent_bp;
                
                if let Some(exitblock_opt) = self.check_retirement(parent_bp) {
                    self.retire_branch(parent_bp, exitblock_opt);
                    progress = true;
                    break;
                }
                
                if self.check_open(trace_id, start_idx) {
                    let dest = self.traces[trace_id].destnode;
                    let forward_succs: Vec<usize> = self.successors[dest]
                        .iter()
                        .copied()
                        .filter(|&succ| !self.dom_tree.dominates(succ, dest))
                        .collect();
                        
                    let num_out = forward_succs.len();
                    
                    if num_out == 0 {
                        self.traces[trace_id].terminal = true;
                    } else if num_out == 1 {
                        let next = forward_succs[0];
                        self.traces[trace_id].destnode = next;
                        *self.node_visit_counts.entry(next).or_insert(0) += 1;
                    } else {
                        // Open a new branch point
                        self.traces[trace_id].active = false;
                        self.active_traces.remove(&trace_id);
                        self.create_branch_point(Some(trace_id), dest);
                    }
                    progress = true;
                    break;
                }
            }
            
            if !progress {
                stuck_count += 1;
                if stuck_count > 10 {
                    // Stuck, likely unstructured edges or irreducible loop.
                    return Ok(None);
                }
            } else {
                stuck_count = 0;
            }
        }
        
        // If we exit the loop, all traces terminated, but we didn't retire root.
        // Actually, if active_traces becomes empty, root should have retired.
        if let Some(exitblock_opt) = self.check_retirement(root_bp) {
            Ok(exitblock_opt)
        } else {
            Ok(None)
        }
    }
}
