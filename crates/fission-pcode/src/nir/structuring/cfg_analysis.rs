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
    preorder: Vec<usize>,
    preorder_index: Vec<usize>,
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
                preorder: Vec::new(),
                preorder_index: Vec::new(),
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
        let mut preorder_index = vec![0usize; node_count];
        let mut preorder = Vec::with_capacity(node_count);
        let mut edge_classes = HashMap::new();

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

    pub(crate) fn roots(&self) -> &[usize] {
        &self.roots
    }

    pub(crate) fn preorder(&self) -> &[usize] {
        &self.preorder
    }

    pub(crate) fn edge_classes(&self) -> &HashMap<(usize, usize), EdgeClass> {
        &self.edge_classes
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

    pub(crate) fn irreducible_edges(&self, dom_tree: &DomTree) -> HashSet<(usize, usize)> {
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
        self.irreducible
            .iter()
            .map(|component| component.headers.len())
            .sum()
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
            _ => { // 2 = Visited
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

/// Immediate-postdominator tree computed via Cooper's algorithm on the reverse CFG.
///
/// For each node n, `idom[n]` is the unique node that immediately postdominates n
/// (i.e. the nearest strict postdominator of n on every path from n to any exit).
///
/// `nodes_by_rpo` stores indices ordered by reverse-postorder on the *reverse* CFG
/// (= postorder on the forward CFG), which is the traversal order required by
/// Cooper et al.'s "A Simple, Fast Dominance Algorithm" (2001).
///
/// Nodes unreachable from any exit (or disconnected loops) get `idom[n] = n`
/// (no strict postdominator).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImmPostDomTree {
    /// idom[n] = immediate postdominator of n, or n itself if none.
    idom: Vec<usize>,
    /// RPO numbers on the reverse CFG (used by Cooper's intersect).
    rpo_number: Vec<usize>,
    /// Sentinel: all nodes >= node_count are treated as virtual exits.
    node_count: usize,
}

impl ImmPostDomTree {
    /// Compute the immediate-postdominator tree using Cooper's algorithm.
    ///
    /// `successors[n]` = forward edges from n.
    /// `predecessors[n]` = reverse of successors (forward edges *into* n).
    pub(crate) fn compute(successors: &[Vec<usize>], predecessors: &[Vec<usize>]) -> Self {
        let node_count = successors.len();
        if node_count == 0 {
            return Self {
                idom: Vec::new(),
                rpo_number: Vec::new(),
                node_count: 0,
            };
        }

        // Find exit nodes (no forward successors) in the original CFG.
        // For postdominators, these are the roots of the reverse CFG.
        let mut exits: Vec<usize> = successors
            .iter()
            .enumerate()
            .filter_map(|(i, succs)| succs.is_empty().then_some(i))
            .collect();
        if exits.is_empty() {
            exits.push(node_count.saturating_sub(1));
        }

        // If there are multiple exits, add a virtual super-exit node (index = node_count).
        // On the reverse CFG the virtual node has predecessors = exits (forward) and
        // no successors (reverse); on the *forward* CFG perspective the virtual node
        // succeeds all exits.
        let total_nodes = node_count + if exits.len() > 1 { 1 } else { 0 };
        let super_exit = if exits.len() > 1 { Some(node_count) } else { None };

        // Build the reverse CFG (edges go from successor to predecessor in the original CFG).
        // In the reverse CFG: reverse_succs[v] = predecessors[v] (forward preds),
        //                     reverse_preds[v] = successors[v]  (forward succs).
        // We also hook super_exit → each real exit in the reverse CFG.
        let mut rev_succs: Vec<Vec<usize>> = predecessors.to_vec();
        rev_succs.resize(total_nodes, Vec::new());
        let mut rev_preds: Vec<Vec<usize>> = successors.to_vec();
        rev_preds.resize(total_nodes, Vec::new());
        if let Some(se) = super_exit {
            for &exit in &exits {
                rev_succs[se].push(exit);
                rev_preds[exit].push(se);
            }
        }

        let start = super_exit.unwrap_or(exits[0]);

        // Compute RPO of the reverse CFG starting from `start` (= virtual/actual exit).
        let rpo_order = compute_rpo(start, &rev_succs, total_nodes);

        // Build RPO number map: rpo_number[n] = position in RPO traversal.
        let mut rpo_number = vec![usize::MAX; total_nodes];
        for (pos, &n) in rpo_order.iter().enumerate() {
            rpo_number[n] = pos;
        }

        // Cooper's algorithm: iteratively compute idom.
        // UNDEF sentinel: idom[n] = total_nodes means "not yet assigned".
        const UNDEF: usize = usize::MAX;
        let mut idom = vec![UNDEF; total_nodes];
        idom[start] = start;

        let mut changed = true;
        while changed {
            changed = false;
            for &n in &rpo_order {
                if n == start {
                    continue;
                }
                // Predecessors in the reverse CFG = rev_preds[n] = forward successors of n.
                let mut new_idom = UNDEF;
                for &p in &rev_preds[n] {
                    if idom[p] == UNDEF {
                        continue; // predecessor not yet processed
                    }
                    if new_idom == UNDEF {
                        new_idom = p;
                    } else {
                        new_idom = cooper_intersect(new_idom, p, &idom, &rpo_number);
                    }
                }
                if new_idom == UNDEF {
                    // Unreachable from any exit; self-loop sentinel.
                    new_idom = n;
                }
                if idom[n] != new_idom {
                    idom[n] = new_idom;
                    changed = true;
                }
            }
        }

        // For nodes unreachable from the reverse-CFG root, fall back to self.
        for i in 0..total_nodes {
            if idom[i] == UNDEF {
                idom[i] = i;
            }
        }

        // Trim back to node_count (remove virtual super-exit slot if present).
        idom.truncate(node_count);
        rpo_number.truncate(node_count);

        // Remap any idom pointing at the virtual super-exit to self.
        for i in 0..node_count {
            if idom[i] >= node_count {
                idom[i] = i;
            }
        }

        Self { idom, rpo_number, node_count }
    }

    /// Immediate postdominator of `n`, or `None` if `n` has no strict postdominator
    /// (i.e. `n` is an exit node or part of a disconnected loop).
    pub(crate) fn immediate_postdominator(&self, n: usize) -> Option<usize> {
        let ipdom = self.idom.get(n).copied()?;
        if ipdom == n { None } else { Some(ipdom) }
    }

    /// Nearest common postdominator of a set of nodes (LCA in the idom tree).
    /// Returns `None` if the set is empty or no common postdominator exists.
    pub(crate) fn nearest_common_postdominator(&self, nodes: &[usize]) -> Option<usize> {
        let mut iter = nodes.iter().copied().filter(|&n| n < self.node_count);
        let mut result = iter.next()?;
        for n in iter {
            result = self.lca(result, n)?;
        }
        // LCA can return one of the input nodes themselves if one postdominates the other;
        // for "follow block" purposes we want a *strict* postdominator of the branch.
        // Return None if the LCA equals one of the original nodes and that node is a
        // branch arm (not a join block).  For simplicity we return the LCA as-is and
        // let callers filter based on index ordering.
        Some(result)
    }

    /// LCA (= nearest common postdominator) of two nodes using Cooper's intersect.
    fn lca(&self, mut a: usize, mut b: usize) -> Option<usize> {
        if a >= self.node_count || b >= self.node_count {
            return None;
        }
        // Walk up the idom tree until both fingers meet.
        let max_iter = self.node_count + 2;
        let mut steps = 0usize;
        while a != b {
            while self.rpo_number.get(a).copied().unwrap_or(usize::MAX)
                > self.rpo_number.get(b).copied().unwrap_or(usize::MAX)
            {
                let parent = self.idom[a];
                if parent == a {
                    return None; // no common ancestor
                }
                a = parent;
            }
            while self.rpo_number.get(b).copied().unwrap_or(usize::MAX)
                > self.rpo_number.get(a).copied().unwrap_or(usize::MAX)
            {
                let parent = self.idom[b];
                if parent == b {
                    return None;
                }
                b = parent;
            }
            steps += 1;
            if steps > max_iter {
                return None;
            }
        }
        Some(a)
    }
}

/// Cooper et al.'s `intersect(b1, b2)`: walk both fingers up the idom tree
/// (guided by RPO numbers) until they meet.  Returns the LCA node.
fn cooper_intersect(
    mut b1: usize,
    mut b2: usize,
    idom: &[usize],
    rpo_number: &[usize],
) -> usize {
    let n = idom.len();
    let rpo = |x: usize| rpo_number.get(x).copied().unwrap_or(usize::MAX);
    let max_iter = n + 2;
    let mut steps = 0usize;
    while b1 != b2 {
        while rpo(b1) > rpo(b2) {
            let p = idom[b1];
            if p == b1 || p >= n { return b1; }
            b1 = p;
        }
        while rpo(b2) > rpo(b1) {
            let p = idom[b2];
            if p == b2 || p >= n { return b2; }
            b2 = p;
        }
        steps += 1;
        if steps > max_iter { break; }
    }
    b1
}

/// Compute RPO order of `start` in the graph defined by `succs`.
fn compute_rpo(start: usize, succs: &[Vec<usize>], node_count: usize) -> Vec<usize> {
    let mut visited = vec![false; node_count];
    let mut postorder = Vec::with_capacity(node_count);
    dfs_postorder(start, succs, &mut visited, &mut postorder);
    // Nodes unreachable from `start` get appended in stable order.
    for i in 0..node_count {
        if !visited[i] {
            dfs_postorder(i, succs, &mut visited, &mut postorder);
        }
    }
    postorder.reverse(); // reverse postorder
    postorder
}

fn dfs_postorder(
    node: usize,
    succs: &[Vec<usize>],
    visited: &mut [bool],
    postorder: &mut Vec<usize>,
) {
    if node >= visited.len() || visited[node] {
        return;
    }
    visited[node] = true;
    for &s in &succs[node] {
        dfs_postorder(s, succs, visited, postorder);
    }
    postorder.push(node);
}

impl<'a> PreviewBuilder<'a> {
    pub(super) fn refresh_cfg_fact_cache(&mut self) {
        self.dom_tree = DomTree::analyze(&self.successors, &self.predecessors);
    }

    pub(super) fn analyze_cfg_edges(&self) -> CfgAnalysis {
        CfgAnalysis::analyze(&self.successors, &self.predecessors)
    }

    pub(super) fn analyze_cfg_dominators(&self) -> DomTree {
        self.dom_tree.clone()
    }

    pub(super) fn analyze_cfg_postdominators(&self) -> PostDomTree {
        PostDomTree::analyze(&self.successors, &self.predecessors)
    }

    pub(super) fn analyze_cfg_imm_postdominators(&self) -> ImmPostDomTree {
        ImmPostDomTree::compute(&self.successors, &self.predecessors)
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

    // ── ImmPostDomTree (Cooper algorithm) tests ────────────────────────────────

    #[test]
    fn imm_postdom_diamond_follow_is_join() {
        // 0 → {1, 2}; 1 → 3; 2 → 3; 3 → []
        let successors = vec![vec![1, 2], vec![3], vec![3], vec![]];
        let predecessors = build_predecessor_index_map(&successors);
        let ipd = ImmPostDomTree::compute(&successors, &predecessors);

        // Follow block of the branch at 0 should be 3 (join point).
        assert_eq!(ipd.nearest_common_postdominator(&[1, 2]), Some(3));
        // idom of 1 and 2 is 3.
        assert_eq!(ipd.immediate_postdominator(1), Some(3));
        assert_eq!(ipd.immediate_postdominator(2), Some(3));
        // idom of 3 is itself (exit node has no strict postdominator).
        assert_eq!(ipd.immediate_postdominator(3), None);
    }

    #[test]
    fn imm_postdom_linear_chain() {
        // 0 → 1 → 2 → 3 → []
        let successors = vec![vec![1], vec![2], vec![3], vec![]];
        let predecessors = build_predecessor_index_map(&successors);
        let ipd = ImmPostDomTree::compute(&successors, &predecessors);

        assert_eq!(ipd.immediate_postdominator(0), Some(1));
        assert_eq!(ipd.immediate_postdominator(2), Some(3));
        assert_eq!(ipd.immediate_postdominator(3), None);
    }

    #[test]
    fn imm_postdom_nested_diamond() {
        // 0 → {1, 2}; 1 → {3, 4}; 3 → 5; 4 → 5; 2 → 5; 5 → []
        let successors = vec![
            vec![1, 2], // 0
            vec![3, 4], // 1
            vec![5],    // 2
            vec![5],    // 3
            vec![5],    // 4
            vec![],     // 5
        ];
        let predecessors = build_predecessor_index_map(&successors);
        let ipd = ImmPostDomTree::compute(&successors, &predecessors);

        // Follow for outer branch (0): common postdom of {1,2} = 5.
        assert_eq!(ipd.nearest_common_postdominator(&[1, 2]), Some(5));
        // Follow for inner branch (1): common postdom of {3,4} = 5.
        assert_eq!(ipd.nearest_common_postdominator(&[3, 4]), Some(5));
    }

    #[test]
    fn imm_postdom_single_node_is_none() {
        let successors: Vec<Vec<usize>> = vec![vec![]];
        let predecessors = build_predecessor_index_map(&successors);
        let ipd = ImmPostDomTree::compute(&successors, &predecessors);
        assert_eq!(ipd.immediate_postdominator(0), None);
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
