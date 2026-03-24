use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum EdgeClass {
    Tree,
    Back,
    Forward,
    Cross,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CfgAnalysis {
    roots: Vec<usize>,
    edge_classes: HashMap<(usize, usize), EdgeClass>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DomTree {
    roots: Vec<usize>,
    dominators: HashMap<usize, HashSet<usize>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PostDomTree {
    exits: Vec<usize>,
    postdominators: HashMap<usize, HashSet<usize>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SccAnalysis {
    components: Vec<Vec<usize>>,
    component_of: Vec<usize>,
    irreducible: Vec<IrreducibleComponent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IrreducibleComponent {
    pub(crate) component_index: usize,
    pub(crate) headers: Vec<usize>,
}

impl CfgAnalysis {
    pub(crate) fn analyze(successors: &[Vec<usize>], predecessors: &[Vec<usize>]) -> Self {
        let node_count = successors.len();
        if node_count == 0 {
            return Self {
                roots: Vec::new(),
                edge_classes: HashMap::new(),
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
        let mut discovery_time = vec![0usize; node_count];
        let mut time = 0usize;
        let mut edge_classes = HashMap::new();

        for root in roots.iter().copied() {
            if color[root] != 0 {
                continue;
            }
            classify_edges_depth_first(
                root,
                successors,
                &mut color,
                &mut discovery_time,
                &mut time,
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
                &mut discovery_time,
                &mut time,
                &mut edge_classes,
            );
        }

        Self {
            roots,
            edge_classes,
        }
    }

    pub(crate) fn roots(&self) -> &[usize] {
        &self.roots
    }

    #[cfg(test)]
    pub(crate) fn class_of(&self, src: usize, dst: usize) -> Option<EdgeClass> {
        self.edge_classes.get(&(src, dst)).copied()
    }

    pub(crate) fn count_class(&self, class: EdgeClass) -> usize {
        self.edge_classes
            .values()
            .filter(|edge_class| **edge_class == class)
            .count()
    }
}

impl DomTree {
    pub(crate) fn analyze(successors: &[Vec<usize>], predecessors: &[Vec<usize>]) -> Self {
        let node_count = successors.len();
        if node_count == 0 {
            return Self {
                roots: Vec::new(),
                dominators: HashMap::new(),
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

        let mut dominators = HashMap::new();
        for root in roots.iter().copied() {
            let component = reachable_from(root, successors);
            if component.is_empty() {
                continue;
            }
            let local = compute_dominator_sets(&component, predecessors, root);
            dominators.extend(local);
        }

        for idx in 0..node_count {
            if dominators.contains_key(&idx) {
                continue;
            }
            roots.push(idx);
            let component = reachable_from(idx, successors);
            if component.is_empty() {
                dominators.insert(idx, HashSet::from([idx]));
                continue;
            }
            let local = compute_dominator_sets(&component, predecessors, idx);
            dominators.extend(local);
        }

        Self { roots, dominators }
    }

    pub(crate) fn roots(&self) -> &[usize] {
        &self.roots
    }

    #[cfg(test)]
    pub(crate) fn dominates(&self, dom: usize, node: usize) -> bool {
        self.dominators
            .get(&node)
            .is_some_and(|set| set.contains(&dom))
    }

    pub(crate) fn nearest_common_dominator(&self, nodes: &[usize]) -> Option<usize> {
        nearest_common_from_sets(&self.dominators, nodes)
    }
}

impl PostDomTree {
    pub(crate) fn analyze(successors: &[Vec<usize>], predecessors: &[Vec<usize>]) -> Self {
        let node_count = successors.len();
        if node_count == 0 {
            return Self {
                exits: Vec::new(),
                postdominators: HashMap::new(),
            };
        }

        let mut exits = successors
            .iter()
            .enumerate()
            .filter_map(|(idx, succs)| succs.is_empty().then_some(idx))
            .collect::<Vec<_>>();
        if exits.is_empty() {
            exits.push(node_count - 1);
        }

        let mut postdominators = HashMap::new();
        for exit in exits.iter().copied() {
            let component = reverse_reachable_from(exit, predecessors);
            if component.is_empty() {
                continue;
            }
            let local = compute_postdominator_sets_for_exit(&component, successors, exit);
            postdominators.extend(local);
        }

        for idx in 0..node_count {
            postdominators
                .entry(idx)
                .or_insert_with(|| HashSet::from([idx]));
        }

        Self {
            exits,
            postdominators,
        }
    }

    pub(crate) fn analyze_window_with_exit(
        successors: &[Vec<usize>],
        window: &HashSet<usize>,
        exit: usize,
    ) -> Option<Self> {
        if !window.contains(&exit) {
            return None;
        }
        let sets = compute_postdominator_sets_for_exit(window, successors, exit);
        Some(Self {
            exits: vec![exit],
            postdominators: sets,
        })
    }

    pub(crate) fn exits(&self) -> &[usize] {
        &self.exits
    }

    pub(crate) fn postdominators(&self) -> &HashMap<usize, HashSet<usize>> {
        &self.postdominators
    }

    #[cfg(test)]
    pub(crate) fn nearest_common_postdominator(&self, nodes: &[usize]) -> Option<usize> {
        nearest_common_from_sets(&self.postdominators, nodes)
    }
}

impl SccAnalysis {
    pub(crate) fn analyze(successors: &[Vec<usize>], predecessors: &[Vec<usize>]) -> Self {
        let node_count = successors.len();
        let mut tarjan = TarjanState::new(node_count);
        for node in 0..node_count {
            if tarjan.indices[node].is_none() {
                tarjan.strong_connect(node, successors);
            }
        }

        let mut irreducible = Vec::new();
        for (component_index, component) in tarjan.components.iter().enumerate() {
            if component.len() < 2 {
                continue;
            }
            let component_set = component.iter().copied().collect::<HashSet<_>>();
            let mut headers = HashSet::new();
            for node in component.iter().copied() {
                for pred in predecessors.get(node).into_iter().flatten().copied() {
                    if !component_set.contains(&pred) {
                        headers.insert(node);
                    }
                }
            }
            if headers.len() >= 2 {
                let mut sorted_headers = headers.into_iter().collect::<Vec<_>>();
                sorted_headers.sort_unstable();
                irreducible.push(IrreducibleComponent {
                    component_index,
                    headers: sorted_headers,
                });
            }
        }

        Self {
            components: tarjan.components,
            component_of: tarjan.component_of,
            irreducible,
        }
    }

    pub(crate) fn component_count(&self) -> usize {
        self.components.len()
    }

    #[cfg(test)]
    pub(crate) fn irreducible_components(&self) -> &[IrreducibleComponent] {
        &self.irreducible
    }

    pub(crate) fn irreducible_count(&self) -> usize {
        self.irreducible.len()
    }

    pub(crate) fn is_irreducible_node(&self, node: usize) -> bool {
        let Some(component_idx) = self.component_of.get(node).copied() else {
            return false;
        };
        self.irreducible
            .iter()
            .any(|entry| entry.component_index == component_idx)
    }

    pub(crate) fn irreducible_header_total_count(&self) -> usize {
        self.irreducible.iter().map(|component| component.headers.len()).sum()
    }

}

#[derive(Debug)]
struct TarjanState {
    index: usize,
    indices: Vec<Option<usize>>,
    lowlink: Vec<usize>,
    stack: Vec<usize>,
    on_stack: Vec<bool>,
    components: Vec<Vec<usize>>,
    component_of: Vec<usize>,
}

impl TarjanState {
    fn new(node_count: usize) -> Self {
        Self {
            index: 0,
            indices: vec![None; node_count],
            lowlink: vec![0; node_count],
            stack: Vec::new(),
            on_stack: vec![false; node_count],
            components: Vec::new(),
            component_of: vec![usize::MAX; node_count],
        }
    }

    fn strong_connect(&mut self, node: usize, successors: &[Vec<usize>]) {
        self.indices[node] = Some(self.index);
        self.lowlink[node] = self.index;
        self.index += 1;
        self.stack.push(node);
        self.on_stack[node] = true;

        for succ in successors[node].iter().copied() {
            if succ >= successors.len() {
                continue;
            }
            if self.indices[succ].is_none() {
                self.strong_connect(succ, successors);
                self.lowlink[node] = self.lowlink[node].min(self.lowlink[succ]);
            } else if self.on_stack[succ]
                && let Some(succ_index) = self.indices[succ]
            {
                self.lowlink[node] = self.lowlink[node].min(succ_index);
            }
        }

        let Some(node_index) = self.indices[node] else {
            return;
        };
        if self.lowlink[node] != node_index {
            return;
        }

        let mut component = Vec::new();
        while let Some(w) = self.stack.pop() {
            self.on_stack[w] = false;
            self.component_of[w] = self.components.len();
            component.push(w);
            if w == node {
                break;
            }
        }
        component.sort_unstable();
        self.components.push(component);
    }
}

fn classify_edges_depth_first(
    node: usize,
    successors: &[Vec<usize>],
    color: &mut [u8],
    discovery_time: &mut [usize],
    time: &mut usize,
    edge_classes: &mut HashMap<(usize, usize), EdgeClass>,
) {
    *time += 1;
    discovery_time[node] = *time;
    color[node] = 1;

    for succ in successors[node].iter().copied() {
        if succ >= successors.len() {
            continue;
        }
        let class = match color[succ] {
            0 => EdgeClass::Tree,
            1 => EdgeClass::Back,
            _ => {
                if discovery_time[node] < discovery_time[succ] {
                    EdgeClass::Forward
                } else {
                    EdgeClass::Cross
                }
            }
        };
        edge_classes.insert((node, succ), class);
        if class == EdgeClass::Tree {
            classify_edges_depth_first(succ, successors, color, discovery_time, time, edge_classes);
        }
    }

    color[node] = 2;
}

impl<'a> PreviewBuilder<'a> {
    pub(super) fn analyze_cfg_edges(&self) -> CfgAnalysis {
        CfgAnalysis::analyze(&self.successors, &self.predecessors)
    }

    pub(super) fn analyze_cfg_dominators(&self) -> DomTree {
        DomTree::analyze(&self.successors, &self.predecessors)
    }

    pub(super) fn analyze_cfg_postdominators(&self) -> PostDomTree {
        PostDomTree::analyze(&self.successors, &self.predecessors)
    }

    pub(super) fn analyze_cfg_scc(&self) -> SccAnalysis {
        SccAnalysis::analyze(&self.successors, &self.predecessors)
    }
}

fn nearest_common_from_sets(
    sets: &HashMap<usize, HashSet<usize>>,
    nodes: &[usize],
) -> Option<usize> {
    let mut iter = nodes.iter().copied();
    let first = iter.next()?;
    let mut common = sets.get(&first)?.clone();
    for node in iter {
        let set = sets.get(&node)?;
        common = common.intersection(set).copied().collect();
        if common.is_empty() {
            return None;
        }
    }
    common
        .into_iter()
        .max_by_key(|candidate| sets.get(candidate).map_or(0, HashSet::len))
}

fn reachable_from(root: usize, successors: &[Vec<usize>]) -> HashSet<usize> {
    let mut seen = HashSet::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node >= successors.len() || !seen.insert(node) {
            continue;
        }
        for succ in successors[node].iter().copied() {
            stack.push(succ);
        }
    }
    seen
}

fn reverse_reachable_from(exit: usize, predecessors: &[Vec<usize>]) -> HashSet<usize> {
    let mut seen = HashSet::new();
    let mut stack = vec![exit];
    while let Some(node) = stack.pop() {
        if node >= predecessors.len() || !seen.insert(node) {
            continue;
        }
        for pred in predecessors[node].iter().copied() {
            stack.push(pred);
        }
    }
    seen
}

fn compute_dominator_sets(
    nodes: &HashSet<usize>,
    predecessors: &[Vec<usize>],
    root: usize,
) -> HashMap<usize, HashSet<usize>> {
    let mut dom = HashMap::new();
    for node in nodes.iter().copied() {
        if node == root {
            dom.insert(node, HashSet::from([root]));
        } else {
            dom.insert(node, nodes.clone());
        }
    }

    let mut changed = true;
    let max_iterations = nodes.len().saturating_mul(nodes.len().max(1));
    let mut iterations = 0usize;
    while changed && iterations < max_iterations {
        iterations += 1;
        changed = false;
        for node in nodes.iter().copied() {
            if node == root {
                continue;
            }
            let in_component_preds = predecessors[node]
                .iter()
                .copied()
                .filter(|pred| nodes.contains(pred))
                .collect::<Vec<_>>();
            if in_component_preds.is_empty() {
                dom.insert(node, HashSet::from([node]));
                continue;
            }
            let mut intersection = dom
                .get(&in_component_preds[0])
                .cloned()
                .unwrap_or_else(|| nodes.clone());
            for pred in in_component_preds.iter().skip(1) {
                if let Some(pred_set) = dom.get(pred) {
                    intersection = intersection.intersection(pred_set).copied().collect();
                }
            }
            intersection.insert(node);
            if dom.get(&node) != Some(&intersection) {
                dom.insert(node, intersection);
                changed = true;
            }
        }
    }
    dom
}

fn compute_postdominator_sets_for_exit(
    nodes: &HashSet<usize>,
    successors: &[Vec<usize>],
    exit: usize,
) -> HashMap<usize, HashSet<usize>> {
    let mut postdom = HashMap::new();
    for node in nodes.iter().copied() {
        if node == exit {
            postdom.insert(node, HashSet::from([exit]));
        } else {
            postdom.insert(node, nodes.clone());
        }
    }

    let mut changed = true;
    let max_iterations = nodes.len().saturating_mul(nodes.len().max(1));
    let mut iterations = 0usize;
    while changed && iterations < max_iterations {
        iterations += 1;
        changed = false;
        for node in nodes.iter().copied() {
            if node == exit {
                continue;
            }
            let in_component_succs = successors[node]
                .iter()
                .copied()
                .filter(|succ| nodes.contains(succ))
                .collect::<Vec<_>>();
            if in_component_succs.is_empty() {
                postdom.insert(node, HashSet::from([node]));
                continue;
            }
            let mut intersection = postdom
                .get(&in_component_succs[0])
                .cloned()
                .unwrap_or_else(|| nodes.clone());
            for succ in in_component_succs.iter().skip(1) {
                if let Some(succ_set) = postdom.get(succ) {
                    intersection = intersection.intersection(succ_set).copied().collect();
                }
            }
            intersection.insert(node);
            if postdom.get(&node) != Some(&intersection) {
                postdom.insert(node, intersection);
                changed = true;
            }
        }
    }
    postdom
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cfg_analysis_classifies_diamond_edges() {
        let successors = vec![vec![1, 2], vec![3], vec![3], vec![]];
        let predecessors = build_predecessor_index_map(&successors);

        let analysis = CfgAnalysis::analyze(&successors, &predecessors);

        assert_eq!(analysis.class_of(0, 1), Some(EdgeClass::Tree));
        assert_eq!(analysis.class_of(0, 2), Some(EdgeClass::Tree));
        assert_eq!(analysis.class_of(1, 3), Some(EdgeClass::Tree));
        assert_eq!(analysis.class_of(2, 3), Some(EdgeClass::Cross));
        assert_eq!(analysis.count_class(EdgeClass::Back), 0);
    }

    #[test]
    fn cfg_analysis_classifies_single_loop_back_edge() {
        let successors = vec![vec![1], vec![2], vec![1, 3], vec![]];
        let predecessors = build_predecessor_index_map(&successors);

        let analysis = CfgAnalysis::analyze(&successors, &predecessors);

        assert_eq!(analysis.class_of(2, 1), Some(EdgeClass::Back));
        assert_eq!(analysis.count_class(EdgeClass::Back), 1);
    }

    #[test]
    fn cfg_analysis_classifies_multi_header_scc_with_back_and_cross_edges() {
        let successors = vec![vec![1, 2], vec![2], vec![1, 3], vec![]];
        let predecessors = build_predecessor_index_map(&successors);

        let analysis = CfgAnalysis::analyze(&successors, &predecessors);

        assert_eq!(analysis.class_of(2, 1), Some(EdgeClass::Back));
        assert_eq!(analysis.class_of(1, 2), Some(EdgeClass::Tree));
        assert!(analysis.count_class(EdgeClass::Back) >= 1);
    }

    #[test]
    fn dom_tree_finds_nearest_common_dominator_for_diamond() {
        let successors = vec![vec![1, 2], vec![3], vec![3], vec![]];
        let predecessors = build_predecessor_index_map(&successors);
        let dom = DomTree::analyze(&successors, &predecessors);

        assert!(dom.dominates(0, 1));
        assert!(dom.dominates(0, 2));
        assert_eq!(dom.nearest_common_dominator(&[1, 2]), Some(0));
        assert_eq!(dom.nearest_common_dominator(&[3, 2]), Some(0));
    }

    #[test]
    fn postdom_tree_finds_common_postdominator_for_diamond() {
        let successors = vec![vec![1, 2], vec![3], vec![3], vec![]];
        let predecessors = build_predecessor_index_map(&successors);
        let postdom = PostDomTree::analyze(&successors, &predecessors);

        assert_eq!(postdom.nearest_common_postdominator(&[1, 2]), Some(3));
        assert_eq!(postdom.nearest_common_postdominator(&[0, 1]), Some(3));
    }

    #[test]
    fn scc_analysis_identifies_irreducible_multi_header_component() {
        let successors = vec![vec![1, 2], vec![3], vec![3], vec![1, 2], vec![]];
        let predecessors = build_predecessor_index_map(&successors);
        let scc = SccAnalysis::analyze(&successors, &predecessors);

        assert!(scc.component_count() >= 2);
        assert_eq!(scc.irreducible_count(), 1);
        assert_eq!(scc.irreducible_header_total_count(), 2);
        let irr = &scc.irreducible_components()[0];
        assert_eq!(irr.headers, vec![1, 2]);
    }

    #[test]
    fn scc_analysis_does_not_mark_single_header_loop_irreducible() {
        let successors = vec![vec![1], vec![2], vec![1, 3], vec![]];
        let predecessors = build_predecessor_index_map(&successors);
        let scc = SccAnalysis::analyze(&successors, &predecessors);

        assert_eq!(scc.irreducible_count(), 0);
    }

    #[test]
    fn scc_analysis_reports_irreducible_membership_by_node() {
        let successors = vec![vec![1, 2], vec![3], vec![3], vec![1, 2], vec![]];
        let predecessors = build_predecessor_index_map(&successors);
        let scc = SccAnalysis::analyze(&successors, &predecessors);

        assert!(scc.is_irreducible_node(1));
        assert!(scc.is_irreducible_node(2));
        assert!(scc.is_irreducible_node(3));
        assert!(!scc.is_irreducible_node(0));
        assert!(!scc.is_irreducible_node(4));
    }
}
