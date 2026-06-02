use super::cfg_analysis::{CfgAnalysis, EdgeClass};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub(crate) struct LoopBody {
    pub head: usize,
    pub tails: Vec<usize>,
    pub body: Vec<usize>,
    /// Canonical exit: the successor of a tail block (or body block) that lies outside the loop
    /// body. Used by while-structuring as the exit arm of the head's conditional branch.
    pub exit_idx: Option<usize>,
    /// All blocks reachable immediately outside the loop body (targets of body→exit edges).
    /// Each element is a block index that is NOT inside `body`. Multiple exits means multiple
    /// potential break targets.
    pub all_exits: Vec<usize>,
}

impl LoopBody {
    pub(crate) fn identify_loops(
        successors: &[Vec<usize>],
        predecessors: &[Vec<usize>],
        cfg_analysis: &CfgAnalysis,
        irreducible_edges: &HashSet<(usize, usize)>,
    ) -> Vec<LoopBody> {
        let mut loops: HashMap<usize, Vec<usize>> = HashMap::new();

        // 1. Group all tails by their head
        for (&(tail, head), &class) in cfg_analysis.edge_classes() {
            if class == EdgeClass::Back {
                loops.entry(head).or_default().push(tail);
            }
        }

        let mut bodies = Vec::new();
        // Process loops inside out (innermost first).
        for (head, tails) in loops {
            let mut loop_body = LoopBody {
                head,
                tails: tails.clone(),
                body: Vec::new(),
                exit_idx: None,
                all_exits: Vec::new(),
            };
            loop_body.find_base(predecessors, irreducible_edges);
            // Phase A: find initial exit_idx from tails so that `extend` has a boundary.
            loop_body.find_initial_exit(successors, irreducible_edges);
            // Phase B: grow body with the boundary in place.
            loop_body.extend(predecessors, successors, irreducible_edges);
            // Phase C: re-scan the full body (after extend) to collect all exits.
            loop_body.find_all_exits(successors, irreducible_edges);
            bodies.push(loop_body);
        }
        bodies
    }

    fn find_base(
        &mut self,
        predecessors: &[Vec<usize>],
        irreducible_edges: &HashSet<(usize, usize)>,
    ) {
        let mut marked = HashSet::new();

        self.body.push(self.head);
        marked.insert(self.head);

        for &tail in &self.tails {
            if marked.insert(tail) {
                self.body.push(tail);
            }
        }

        let mut i = 1; // start from body[1], skipping head
        while i < self.body.len() {
            let cur_block = self.body[i];
            i += 1;

            for &pred in &predecessors[cur_block] {
                if irreducible_edges.contains(&(pred, cur_block)) {
                    continue;
                }
                if marked.insert(pred) {
                    self.body.push(pred);
                }
            }
        }
    }

    /// Quick pre-extend scan: finds the first exit reachable from tail blocks so that
    /// `extend` has a known boundary and does not pull the exit into the body.
    fn find_initial_exit(
        &mut self,
        successors: &[Vec<usize>],
        irreducible_edges: &HashSet<(usize, usize)>,
    ) {
        let body_set: HashSet<usize> = self.body.iter().copied().collect();

        for &tail in &self.tails {
            if tail >= successors.len() {
                continue;
            }
            for &succ in &successors[tail] {
                if irreducible_edges.contains(&(tail, succ)) {
                    continue;
                }
                if !body_set.contains(&succ) {
                    self.exit_idx = Some(succ);
                    return;
                }
            }
        }

        for &bl in &self.body {
            if self.tails.contains(&bl) {
                continue;
            }
            if bl >= successors.len() {
                continue;
            }
            for &succ in &successors[bl] {
                if irreducible_edges.contains(&(bl, succ)) {
                    continue;
                }
                if !body_set.contains(&succ) {
                    self.exit_idx = Some(succ);
                    return;
                }
            }
        }
    }

    /// Collect all exits after the body has been fully extended.
    /// Scans every body block's successors and records those outside the body into `all_exits`.
    /// Does NOT modify `exit_idx` (already set by `find_initial_exit`).
    fn find_all_exits(
        &mut self,
        successors: &[Vec<usize>],
        irreducible_edges: &HashSet<(usize, usize)>,
    ) {
        let body_set: HashSet<usize> = self.body.iter().copied().collect();
        let mut seen = HashSet::new();

        for &bl in &self.body {
            if bl >= successors.len() {
                continue;
            }
            for &succ in &successors[bl] {
                if irreducible_edges.contains(&(bl, succ)) {
                    continue;
                }
                if !body_set.contains(&succ) && seen.insert(succ) {
                    self.all_exits.push(succ);
                }
            }
        }

        self.all_exits.sort_unstable();
    }

    /// Returns true if `block_idx` is a recognized exit destination (break target) for this loop.
    pub(crate) fn is_exit(&self, block_idx: usize) -> bool {
        self.all_exits.binary_search(&block_idx).is_ok()
    }

    /// Returns all CFG edges `(src, exit_block)` where `src` is inside the loop body
    /// and `exit_block` is outside (a potential `break` target).
    ///
    /// All exits are returned, not just the canonical one.  Callers can build a set of
    /// break-labels from this to rewrite `Goto(exit)` → `Break` for every exit that
    /// shares the same post-loop continuation.
    pub fn find_break_exits(
        &self,
        successors: &[Vec<usize>],
        irreducible_edges: &HashSet<(usize, usize)>,
    ) -> Vec<(usize, usize)> {
        let body_set: HashSet<usize> = self.body.iter().copied().collect();
        let mut exits = Vec::new();
        for &src in &self.body {
            if src >= successors.len() {
                continue;
            }
            for &succ in &successors[src] {
                if irreducible_edges.contains(&(src, succ)) {
                    continue;
                }
                if !body_set.contains(&succ) {
                    exits.push((src, succ));
                }
            }
        }
        exits
    }

    /// Returns all CFG edges `(src, head)` where `src` is inside the loop body,
    /// `src` is **not** a tail (natural back-edge), and the edge targets the loop header.
    ///
    /// These are explicit mid-body jumps back to the loop header — `continue` candidates
    /// that are not already covered by the natural loop-closing back-edge.
    pub fn find_continue_edges(
        &self,
        successors: &[Vec<usize>],
        irreducible_edges: &HashSet<(usize, usize)>,
    ) -> Vec<(usize, usize)> {
        let body_set: HashSet<usize> = self.body.iter().copied().collect();
        let tail_set: HashSet<usize> = self.tails.iter().copied().collect();
        let mut continues = Vec::new();
        for &src in &self.body {
            if tail_set.contains(&src) {
                continue; // natural back-edge — already handled by the loop condition
            }
            if src >= successors.len() {
                continue;
            }
            for &succ in &successors[src] {
                if irreducible_edges.contains(&(src, succ)) {
                    continue;
                }
                if succ == self.head && body_set.contains(&src) {
                    continues.push((src, succ));
                }
            }
        }
        continues
    }

    /// Returns `true` if this loop has **no exits** from its body — i.e., it is an
    /// infinite-loop candidate.  Callers should lower such loops as `while(true) { … }`
    /// with any `break` statements injected inside the body.
    pub fn is_infinite_loop_candidate(&self) -> bool {
        self.all_exits.is_empty()
    }

    fn extend(
        &mut self,
        predecessors: &[Vec<usize>],
        successors: &[Vec<usize>],
        irreducible_edges: &HashSet<(usize, usize)>,
    ) {
        let mut marked_body: HashSet<usize> = self.body.iter().copied().collect();
        let mut visit_counts: HashMap<usize, usize> = HashMap::new();

        let mut i = 0;
        while i < self.body.len() {
            let bl = self.body[i];
            i += 1;

            if bl >= successors.len() {
                continue;
            }

            for &succ in &successors[bl] {
                if irreducible_edges.contains(&(bl, succ)) {
                    continue;
                }
                if marked_body.contains(&succ) {
                    continue;
                }
                if Some(succ) == self.exit_idx {
                    continue; // Do NOT extend into exit_idx
                }

                *visit_counts.entry(succ).or_insert(0) += 1;
                let count = visit_counts[&succ];

                // Add block if all its structured predecessors are in the body
                let expected_in = predecessors[succ]
                    .iter()
                    .filter(|&&p| !irreducible_edges.contains(&(p, succ)))
                    .count();

                if count == expected_in {
                    marked_body.insert(succ);
                    self.body.push(succ);
                }
            }
        }
    }
}
