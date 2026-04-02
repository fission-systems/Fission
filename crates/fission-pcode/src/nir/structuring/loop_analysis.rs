use std::collections::{HashMap, HashSet};
use super::cfg_analysis::{CfgAnalysis, EdgeClass};

#[derive(Debug, Clone)]
pub(crate) struct LoopBody {
    pub head: usize,
    pub tails: Vec<usize>,
    pub body: Vec<usize>,
    pub exit_idx: Option<usize>,
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
        // Ghidra processes loops inside out.
        for (head, tails) in loops {
            let mut loop_body = LoopBody {
                head,
                tails: tails.clone(),
                body: Vec::new(),
                exit_idx: None,
            };
            loop_body.find_base(predecessors, irreducible_edges);
            loop_body.find_exit(successors, irreducible_edges);
            loop_body.extend(predecessors, successors, irreducible_edges);
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

    fn find_exit(
        &mut self,
        successors: &[Vec<usize>],
        irreducible_edges: &HashSet<(usize, usize)>,
    ) {
        let marked: HashSet<usize> = self.body.iter().copied().collect();
        // Look for an exit from tails
        for &tail in &self.tails {
            for &succ in &successors[tail] {
                if irreducible_edges.contains(&(tail, succ)) {
                    continue;
                }
                if !marked.contains(&succ) {
                    self.exit_idx = Some(succ);
                    return; // Since we don't have container info yet, return first
                }
            }
        }

        // Look for an exit from anywhere else in the body
        for &bl in &self.body {
            // we already did tails. (Technically in Fission we can just filter out tails).
            if self.tails.contains(&bl) {
                continue;
            }
            for &succ in &successors[bl] {
                if irreducible_edges.contains(&(bl, succ)) {
                    continue;
                }
                if !marked.contains(&succ) {
                    self.exit_idx = Some(succ);
                    return;
                }
            }
        }
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

