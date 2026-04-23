use anyhow::Result;

use crate::compiler::{CompiledDecisionProbe, CompiledExecutableConstructor, CompiledFrontend};

pub trait DecisionProbeEvaluator {
    fn probe_value(&mut self, probe: CompiledDecisionProbe) -> Result<u8>;
}

#[derive(Debug, Clone)]
pub struct RuntimeSelection<'a> {
    pub constructor: &'a CompiledExecutableConstructor,
    pub trace: RuntimeMatchTrace,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeMatchTrace {
    pub root_bucket: String,
    pub probes: Vec<RuntimeMatchProbe>,
    pub leaf_constructor_indexes: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeMatchProbe {
    pub probe: CompiledDecisionProbe,
    pub value: u8,
}

pub fn select_constructor<'a, E, M>(
    compiled: &'a CompiledFrontend,
    roots: impl IntoIterator<Item = (String, usize)>,
    mut evaluator_factory: impl FnMut() -> E,
    mut constructor_matches: M,
) -> Option<RuntimeSelection<'a>>
where
    E: DecisionProbeEvaluator,
    M: FnMut(&CompiledExecutableConstructor) -> Result<()>,
{
    for (root_bucket, root_node_index) in roots {
        let mut evaluator = evaluator_factory();
        if let Some(selection) = walk_decision_tree(
            compiled,
            root_node_index,
            &root_bucket,
            &mut evaluator,
            &mut constructor_matches,
        ) {
            return Some(selection);
        }
    }
    None
}

fn walk_decision_tree<'a, E, M>(
    compiled: &'a CompiledFrontend,
    root_node_index: usize,
    root_bucket: &str,
    evaluator: &mut E,
    constructor_matches: &mut M,
) -> Option<RuntimeSelection<'a>>
where
    E: DecisionProbeEvaluator,
    M: FnMut(&CompiledExecutableConstructor) -> Result<()>,
{
    let mut node_index = root_node_index;
    let mut trace = RuntimeMatchTrace {
        root_bucket: root_bucket.to_string(),
        probes: Vec::new(),
        leaf_constructor_indexes: Vec::new(),
    };

    loop {
        let node = compiled.decision_tree.nodes.get(node_index)?;
        match node.probe {
            CompiledDecisionProbe::Terminal => {
                trace.leaf_constructor_indexes = node.leaf_constructor_indexes.clone();
                for constructor_index in &node.leaf_constructor_indexes {
                    let constructor = compiled.executable_constructors.get(*constructor_index)?;
                    if !constructor.runtime_ready {
                        continue;
                    }
                    if constructor_matches(constructor).is_ok() {
                        return Some(RuntimeSelection { constructor, trace });
                    }
                }
                return None;
            }
            probe => {
                let value = evaluator.probe_value(probe).ok()?;
                trace.probes.push(RuntimeMatchProbe { probe, value });
                let edge = node.branches.iter().find(|edge| edge.value == value)?;
                node_index = edge.next_node_index;
            }
        }
    }
}
