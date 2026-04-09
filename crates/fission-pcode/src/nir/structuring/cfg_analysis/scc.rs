//! Tarjan SCC + irreducible multi-header detection.

use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IrreducibleComponent {
    pub(crate) component_index: usize,
    pub(crate) headers: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SccAnalysis {
    components: Vec<Vec<usize>>,
    component_of: Vec<usize>,
    irreducible: Vec<IrreducibleComponent>,
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
