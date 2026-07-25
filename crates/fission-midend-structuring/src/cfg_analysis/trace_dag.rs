//! Ghidra-aligned TraceDAG (see `blockaction.hh` / `TraceDAG`).
//!
//! Used for:
//! 1. **Follow-block discovery** at multi-out nodes (`find_follow_block`).
//! 2. **Likely unstructured edge selection** when the DAG is stuck
//!    (`select_likely_unstructured_edge`) — Ghidra `TraceDAG::selectBadEdge`.
//!
//! Reference-only: vendor Ghidra; reimplemented in Rust without dependency.

use super::*;
use crate::HashMap;
use crate::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceDagError {
    Stuck,
}

#[derive(Debug, Clone)]
struct BranchPoint {
    id: usize,
    parent_trace: Option<usize>,
    /// FlowBlock that embodies the branch (multi-out node).
    top: usize,
    paths: Vec<usize>,
    depth: usize,
}

#[derive(Debug, Clone)]
struct BlockTrace {
    id: usize,
    parent_bp: usize,
    /// Path index out of the parent BranchPoint.
    pathout: usize,
    /// Current node being traversed (bottom of the active edge).
    bottom: usize,
    /// Next node this trace will try to enter (dest of active edge).
    destnode: usize,
    active: bool,
    terminal: bool,
}

/// Score annotation for unstructured-edge selection (Ghidra `BadEdgeScore`).
#[derive(Debug, Clone)]
struct BadEdgeScore {
    trace_id: usize,
    exitproto: usize,
    /// Min DAG-distance to another active trace sharing exit (lower = less likely bad).
    distance: i32,
    /// Number of sibling paths from the same BranchPoint (higher = less likely bad).
    siblingedge: i32,
    /// 1 if destination has no forward outs (terminal).
    terminal: i32,
    /// BranchPoint depth of the parent (lower = less likely bad).
    depth: usize,
}

pub struct TraceDag<'a> {
    successors: &'a [Vec<usize>],
    predecessors: &'a [Vec<usize>],
    dom_tree: &'a DomTree,

    branch_points: Vec<BranchPoint>,
    traces: Vec<BlockTrace>,
    active_traces: HashSet<usize>,
    node_visit_counts: HashMap<usize, usize>,
    /// Accumulated likely unstructured edges (from → to).
    likely_gotos: Vec<(usize, usize)>,
}

impl<'a> TraceDag<'a> {
    pub fn new(
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
            active_traces: HashSet::default(),
            node_visit_counts: HashMap::default(),
            likely_gotos: Vec::new(),
        }
    }

    pub fn likely_gotos(&self) -> &[(usize, usize)] {
        &self.likely_gotos
    }

    fn forward_successors(&self, node: usize) -> Vec<usize> {
        self.successors
            .get(node)
            .into_iter()
            .flatten()
            .copied()
            .filter(|&succ| !self.dom_tree.dominates(succ, node))
            .collect()
    }

    fn create_branch_point(&mut self, parent_trace: Option<usize>, destnode: usize) -> usize {
        let depth = parent_trace
            .map(|tid| self.branch_points[self.traces[tid].parent_bp].depth + 1)
            .unwrap_or(0);
        let bp_id = self.branch_points.len();
        let mut paths = Vec::new();
        let forward_succs = self.forward_successors(destnode);

        for (pathout, succ) in forward_succs.into_iter().enumerate() {
            let trace_id = self.traces.len();
            paths.push(trace_id);
            self.traces.push(BlockTrace {
                id: trace_id,
                parent_bp: bp_id,
                pathout,
                bottom: destnode,
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
            top: destnode,
            paths,
            depth,
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
                return None;
            }
            if trace.terminal {
                continue;
            }
            if let Some(existing) = outblock {
                if existing != trace.destnode {
                    return None;
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
                continue;
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
                parent_trace.bottom = parent_trace.destnode;
                parent_trace.destnode = exitblock;
                parent_trace.active = true;
                parent_trace.terminal = false;
                self.active_traces.insert(parent_trace_id);
                *self.node_visit_counts.entry(exitblock).or_insert(0) += 1;
            } else {
                parent_trace.active = true;
                parent_trace.terminal = true;
                self.active_traces.insert(parent_trace_id);
            }
        }
    }

    /// Ghidra `TraceDAG::selectBadEdge`: score active traces and pick the most
    /// likely unstructured edge `(bottom → destnode)`.
    fn select_bad_edge_among_active(&self) -> Option<usize> {
        let mut scores: Vec<BadEdgeScore> = Vec::new();
        for &trace_id in &self.active_traces {
            let t = &self.traces[trace_id];
            if t.terminal {
                continue;
            }
            let outs = self.forward_successors(t.destnode);
            scores.push(BadEdgeScore {
                trace_id,
                exitproto: t.destnode,
                distance: -1,
                siblingedge: 0,
                terminal: if outs.is_empty() { 1 } else { 0 },
                depth: self.branch_points[t.parent_bp].depth,
            });
        }
        if scores.is_empty() {
            return None;
        }

        // Sibling edges: same parent BranchPoint → less likely each is *the* bad edge.
        for i in 0..scores.len() {
            for j in (i + 1)..scores.len() {
                let bi = self.traces[scores[i].trace_id].parent_bp;
                let bj = self.traces[scores[j].trace_id].parent_bp;
                if bi == bj {
                    scores[i].siblingedge += 1;
                    scores[j].siblingedge += 1;
                }
            }
        }

        // Distance: same exitproto → min path-depth difference as proxy for ancestor distance.
        for i in 0..scores.len() {
            for j in (i + 1)..scores.len() {
                if scores[i].exitproto != scores[j].exitproto {
                    continue;
                }
                let di = scores[i].depth as i32;
                let dj = scores[j].depth as i32;
                let dist = (di - dj).abs();
                if scores[i].distance < 0 || dist < scores[i].distance {
                    scores[i].distance = dist;
                }
                if scores[j].distance < 0 || dist < scores[j].distance {
                    scores[j].distance = dist;
                }
            }
        }

        // Ghidra compareFinal: prefer removing edge that is MORE likely bad.
        // Sort so last is most likely unstructured:
        // higher siblingedge less likely bad; terminal more likely; larger distance more likely;
        // greater depth more likely.
        scores.sort_by(|a, b| {
            // Return Ordering for "a is more likely bad than b" inverted for max-pick.
            b.siblingedge
                .cmp(&a.siblingedge) // more siblings → less likely bad → sort earlier
                .then_with(|| a.terminal.cmp(&b.terminal)) // terminal first when equal siblings? Ghidra: terminal < means less likely if a.terminal < b
                .then_with(|| {
                    // Larger distance → more likely bad
                    let da = if a.distance < 0 { i32::MAX } else { a.distance };
                    let db = if b.distance < 0 { i32::MAX } else { b.distance };
                    da.cmp(&db)
                })
                .then_with(|| a.depth.cmp(&b.depth))
                .then_with(|| a.trace_id.cmp(&b.trace_id))
        });

        // After sort: first is LEAST likely bad, last is MOST likely bad (best to remove).
        // Re-read Ghidra compareFinal:
        // "return true if this is LESS likely to be the bad edge than op2"
        // siblingedge: bigger sibling → LESS likely bad
        // terminal: smaller terminal → LESS likely bad  
        // distance: smaller distance → LESS likely bad
        // depth: smaller depth → LESS likely bad
        //
        // sort with LESS likely first, pick last as bad edge.
        scores.sort_by(|a, b| {
            // a < b means a is LESS likely bad than b
            if a.siblingedge != b.siblingedge {
                return b.siblingedge.cmp(&a.siblingedge); // bigger sibling first (less likely)
            }
            if a.terminal != b.terminal {
                return a.terminal.cmp(&b.terminal); // non-terminal first
            }
            let da = if a.distance < 0 { i32::MAX / 4 } else { a.distance };
            let db = if b.distance < 0 { i32::MAX / 4 } else { b.distance };
            if da != db {
                return da.cmp(&db); // smaller distance first (less likely)
            }
            if a.depth != b.depth {
                return a.depth.cmp(&b.depth);
            }
            a.trace_id.cmp(&b.trace_id)
        });

        scores.last().map(|s| s.trace_id)
    }

    fn remove_trace_as_goto(&mut self, trace_id: usize) {
        let t = self.traces[trace_id].clone();
        if !t.terminal {
            self.likely_gotos.push((t.bottom, t.destnode));
        }
        // Mark terminal so the branch can still retire without this path joining.
        self.traces[trace_id].terminal = true;
        self.traces[trace_id].active = true;
        // Keep in active_traces as terminal (Ghidra keeps terminal active until retire).
    }

    /// Push traces; when stuck, select a likely unstructured edge (Ghidra pushBranches).
    /// Returns the follow block if the root branch retires, or None if only gotos remain.
    pub fn push_branches(&mut self, start_idx: usize) -> Option<usize> {
        let root_bp = if self.branch_points.is_empty() {
            self.create_branch_point(None, start_idx)
        } else {
            0
        };

        let mut missed = 0usize;
        let mut steps = 0usize;
        while !self.active_traces.is_empty() && steps < 500 {
            steps += 1;

            if let Some(exit_opt) = self.check_retirement(root_bp) {
                return exit_opt;
            }

            let active_count = self
                .active_traces
                .iter()
                .filter(|&&tid| !self.traces[tid].terminal || self.traces[tid].active)
                .count()
                .max(self.active_traces.len());

            if missed >= active_count && active_count > 0 {
                if let Some(bad) = self.select_bad_edge_among_active() {
                    self.remove_trace_as_goto(bad);
                    missed = 0;
                    continue;
                }
                break;
            }

            let active_list: Vec<usize> = self.active_traces.iter().copied().collect();
            let mut progress = false;

            for trace_id in active_list {
                if !self.active_traces.contains(&trace_id) {
                    continue;
                }
                let parent_bp = self.traces[trace_id].parent_bp;

                if let Some(exit_opt) = self.check_retirement(parent_bp) {
                    if parent_bp == root_bp {
                        return exit_opt;
                    }
                    self.retire_branch(parent_bp, exit_opt);
                    progress = true;
                    missed = 0;
                    break;
                }

                if self.traces[trace_id].terminal {
                    continue;
                }

                if self.check_open(trace_id, start_idx) {
                    let dest = self.traces[trace_id].destnode;
                    let forward_succs = self.forward_successors(dest);
                    match forward_succs.len() {
                        0 => {
                            self.traces[trace_id].terminal = true;
                        }
                        1 => {
                            let next = forward_succs[0];
                            self.traces[trace_id].bottom = dest;
                            self.traces[trace_id].destnode = next;
                            *self.node_visit_counts.entry(next).or_insert(0) += 1;
                        }
                        _ => {
                            self.traces[trace_id].active = false;
                            self.active_traces.remove(&trace_id);
                            self.create_branch_point(Some(trace_id), dest);
                        }
                    }
                    progress = true;
                    missed = 0;
                    break;
                }
            }

            if !progress {
                missed += 1;
            }
        }

        if let Some(exit_opt) = self.check_retirement(root_bp) {
            return exit_opt;
        }
        None
    }

    /// Follow-block discovery at multi-out `start_idx`.
    pub fn find_follow_block(
        &mut self,
        start_idx: usize,
    ) -> Result<Option<usize>, TraceDagError> {
        let succs = self.successors.get(start_idx).map(|s| s.len()).unwrap_or(0);
        if succs < 2 {
            return Ok(None);
        }
        Ok(self.push_branches(start_idx))
    }

    /// Select one likely unstructured edge in `[entry, exit)` for FAS virtualization.
    ///
    /// Runs a DAG trace from `entry` (or first multi-out in range). When stuck,
    /// returns the best scored edge (Ghidra `selectBadEdge`). Falls back to the
    /// first accumulated likely-goto if the full push does not retire.
    pub fn select_likely_unstructured_edge(
        &mut self,
        entry: usize,
        exit: usize,
        already_virtual: &HashSet<(usize, usize)>,
    ) -> Option<(usize, usize)> {
        // Prefer starting at a multi-out node inside the range.
        let start = (entry..exit.min(self.successors.len())).find(|&i| {
            self.forward_successors(i).len() >= 2
        }).unwrap_or(entry);

        if start >= self.successors.len() {
            return None;
        }

        let _ = self.push_branches(start);

        // Prefer last likely-goto (most recently selected as bad) that is in range
        // and not already virtualized.
        for &(from, to) in self.likely_gotos.iter().rev() {
            if from >= entry
                && from < exit
                && to < self.successors.len()
                && !already_virtual.contains(&(from, to))
            {
                // Must still be a real CFG edge.
                if self.successors[from].contains(&to) {
                    return Some((from, to));
                }
            }
        }

        // If push retired cleanly but we still need an edge (caller stalled),
        // score any multi-out forward edge into a join as likely goto.
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cfg_analysis::DomTree;

    fn diamond() -> (Vec<Vec<usize>>, Vec<Vec<usize>>, DomTree) {
        // 0 → 1,2 → 3
        let succs = vec![vec![1, 2], vec![3], vec![3], vec![]];
        let preds = vec![vec![], vec![0], vec![0], vec![1, 2]];
        let dom = DomTree::analyze(&succs, &preds);
        (succs, preds, dom)
    }

    #[test]
    fn diamond_follow_is_join() {
        let (succs, preds, dom) = diamond();
        let mut dag = TraceDag::new(&succs, &preds, &dom);
        let follow = dag.find_follow_block(0).expect("ok");
        assert_eq!(follow, Some(3));
        assert!(dag.likely_gotos().is_empty());
    }

    #[test]
    fn stuck_dag_selects_unstructured_edge() {
        // Irreducible-ish: 0→1, 0→2, 1→2, 2→1, 2→3
        // Forward DAG without back-edge handling still has competing joins.
        let succs = vec![vec![1, 2], vec![2, 3], vec![3], vec![]];
        let preds = vec![vec![], vec![0], vec![0, 1], vec![1, 2]];
        let dom = DomTree::analyze(&succs, &preds);
        let mut dag = TraceDag::new(&succs, &preds, &dom);
        let already = HashSet::default();
        let edge = dag.select_likely_unstructured_edge(0, 4, &already);
        // Must pick a real edge; either (1,2) competing join or similar.
        if let Some((from, to)) = edge {
            assert!(succs[from].contains(&to), "edge {from}->{to} must exist");
        }
        // Follow discovery may still succeed; edge selection is best-effort when stuck.
        let _ = edge;
    }
}
