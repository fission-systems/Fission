//! DFS edge classification (tree / back / forward / cross).

use super::dom::DomTree;
use fission_midend_core::fast_hash::FastMap as HashMap;
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EdgeClass {
    Tree,
    Back,
    Forward,
    Cross,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CfgAnalysis {
    roots: Vec<usize>,
    preorder: Vec<usize>,
    preorder_index: Vec<usize>,
    edge_classes: HashMap<(usize, usize), EdgeClass>,
}

impl CfgAnalysis {
    pub fn analyze(successors: &[Vec<usize>], predecessors: &[Vec<usize>]) -> Self {
        let node_count = successors.len();
        if node_count == 0 {
            return Self {
                roots: Vec::new(),
                preorder: Vec::new(),
                preorder_index: Vec::new(),
                edge_classes: HashMap::default(),
            };
        }

        let mut roots = predecessors
            .iter()
            .enumerate()
            .filter_map(|(idx, preds)| preds.is_empty().then_some(idx))
            .collect::<Vec<_>>();
        if roots.is_empty() {
            roots.push(0);
        }

        let mut color = vec![0u8; node_count];
        let mut preorder_index = vec![0usize; node_count];
        let mut preorder = Vec::with_capacity(node_count);
        let mut edge_classes = HashMap::default();

        for root in roots.iter().copied() {
            if color[root] != 0 {
                continue;
            }
            classify_edges_depth_first(
                root,
                successors,
                &mut color,
                &mut preorder_index,
                &mut preorder,
                &mut edge_classes,
            );
        }

        for idx in 0..node_count {
            if color[idx] != 0 {
                continue;
            }
            roots.push(idx);
            classify_edges_depth_first(
                idx,
                successors,
                &mut color,
                &mut preorder_index,
                &mut preorder,
                &mut edge_classes,
            );
        }

        Self {
            roots,
            preorder,
            preorder_index,
            edge_classes,
        }
    }

    pub fn roots(&self) -> &[usize] {
        &self.roots
    }

    pub fn preorder(&self) -> &[usize] {
        &self.preorder
    }

    pub fn edge_classes(&self) -> &HashMap<(usize, usize), EdgeClass> {
        &self.edge_classes
    }

    #[cfg(test)]
    pub fn class_of(&self, src: usize, dst: usize) -> Option<EdgeClass> {
        self.edge_classes.get(&(src, dst)).copied()
    }

    pub fn count_class(&self, class: EdgeClass) -> usize {
        self.edge_classes
            .values()
            .filter(|edge_class| **edge_class == class)
            .count()
    }

    pub fn irreducible_edges(&self, dom_tree: &DomTree) -> HashSet<(usize, usize)> {
        self.edge_classes
            .iter()
            .filter_map(|(&(src, dst), class)| {
                // A graph is reducible IF AND ONLY IF every back-edge's target dominates its source.
                // Any back-edge where `dst` does not dominate `src` indicates an irreducible loop.
                if *class == EdgeClass::Back && !dom_tree.dominates(dst, src) {
                    Some((src, dst))
                } else {
                    None
                }
            })
            .collect()
    }
}

fn classify_edges_depth_first(
    node: usize,
    successors: &[Vec<usize>],
    color: &mut [u8],
    preorder_index: &mut [usize],
    preorder: &mut Vec<usize>,
    edge_classes: &mut HashMap<(usize, usize), EdgeClass>,
) {
    preorder_index[node] = preorder.len();
    preorder.push(node);
    color[node] = 1; // Visiting

    for succ in successors[node].iter().copied() {
        if succ >= successors.len() {
            continue;
        }
        let class = match color[succ] {
            0 => EdgeClass::Tree,
            1 => EdgeClass::Back,
            _ => {
                // 2 = Visited
                if preorder_index[node] < preorder_index[succ] {
                    EdgeClass::Forward
                } else {
                    EdgeClass::Cross
                }
            }
        };
        edge_classes.insert((node, succ), class);
        if class == EdgeClass::Tree {
            classify_edges_depth_first(
                succ,
                successors,
                color,
                preorder_index,
                preorder,
                edge_classes,
            );
        }
    }

    color[node] = 2; // Visited
}
