//! Bounded instruction-level CFG facts used by dynamic taint analysis.

use std::collections::{HashMap, HashSet, VecDeque};

use fission_loader::loader::LoadedBinary;
use fission_pcode::ir::PcodeOpcode;
use fission_sleigh::runtime::{PackedContextOverride, RuntimeSleighFrontend};

use crate::pcode::state::MachineState;

const MAX_RECONVERGENCE_NODES: usize = 512;

/// Find a concrete instruction that postdominates both arms of a conditional.
///
/// The graph is deliberately bounded and clipped to the containing function
/// when the loader knows its range (otherwise to the executable section).
/// Unknown transfers, decode failures, region exits, and the exploration limit
/// become exits in the graph. They can prevent a proof, but cannot invent a
/// join. A missing proof means control-dependent taint is not extended.
pub(crate) fn conditional_reconvergence(
    binary: &LoadedBinary,
    state: &MachineState,
    sleigh: &RuntimeSleighFrontend,
    context: Option<PackedContextOverride>,
    branch_pc: u64,
    taken_pc: u64,
    fallthrough_pc: u64,
) -> Option<u64> {
    if taken_pc == fallthrough_pc {
        return Some(taken_pc);
    }
    let section = binary.executable_section_containing(branch_pc)?;
    let section_start = section.virtual_address;
    let section_end = section_start.checked_add(section.virtual_size.max(section.file_size))?;
    let function = binary.function_at(branch_pc);
    let function_range = function
        .filter(|function| !function.is_import && function.size > 0)
        .and_then(|function| {
            function
                .address
                .checked_add(function.size)
                .map(|end| (function.address, end))
        });
    let ram = state.ram_space();

    let admissible = |pc: u64| {
        pc >= section_start
            && pc < section_end
            && binary.executable_section_containing(pc).is_some()
            && function_range.is_none_or(|(start, end)| pc >= start && pc < end)
    };

    let mut graph = HashMap::<u64, Vec<u64>>::new();
    let mut queued = HashSet::new();
    let mut queue = VecDeque::new();
    for start in [taken_pc, fallthrough_pc] {
        if admissible(start) && queued.insert(start) {
            queue.push_back(start);
        }
    }

    while let Some(pc) = queue.pop_front() {
        if graph.len() >= MAX_RECONVERGENCE_NODES {
            break;
        }
        let Some(available) = binary.available_execution_bytes(pc) else {
            graph.insert(pc, Vec::new());
            continue;
        };
        let bytes = state.read_space_readonly(ram, pc, available.min(16)).ok()?;
        let Ok((ops, length, _)) =
            sleigh.decode_and_lift_with_context_override(&bytes, pc, context)
        else {
            graph.insert(pc, Vec::new());
            continue;
        };
        if length == 0 {
            graph.insert(pc, Vec::new());
            continue;
        }
        let next = pc.wrapping_add(length);

        // A direct guest branch is an instruction-level edge. Relative
        // p-code branches stay inside one instruction and do not add CFG nodes.
        let successors = if let Some(op) = ops.iter().find(|op| {
            matches!(
                op.opcode,
                PcodeOpcode::Branch
                    | PcodeOpcode::CBranch
                    | PcodeOpcode::BranchInd
                    | PcodeOpcode::Return
            )
        }) {
            match op.opcode {
                PcodeOpcode::Branch => op
                    .inputs
                    .first()
                    .filter(|dest| dest.space_id != 0 && !dest.is_constant)
                    .map(|dest| vec![dest.offset])
                    .unwrap_or_else(|| vec![next]),
                PcodeOpcode::CBranch => op
                    .inputs
                    .first()
                    .filter(|dest| dest.space_id != 0 && !dest.is_constant)
                    .map(|dest| vec![dest.offset, next])
                    .unwrap_or_else(|| vec![next]),
                PcodeOpcode::BranchInd | PcodeOpcode::Return => Vec::new(),
                _ => unreachable!(),
            }
        } else {
            // Calls return to the following instruction in this intraprocedural
            // graph; their callee is not part of the caller's control region.
            vec![next]
        };

        for successor in &successors {
            if admissible(*successor) && queued.insert(*successor) {
                queue.push_back(*successor);
            }
        }
        graph.insert(pc, successors);
    }

    nearest_common_postdominator(taken_pc, fallthrough_pc, &graph)
}

fn nearest_common_postdominator(
    first: u64,
    second: u64,
    graph: &HashMap<u64, Vec<u64>>,
) -> Option<u64> {
    if first == second {
        return Some(first);
    }
    let mut addresses = graph.keys().copied().collect::<Vec<_>>();
    addresses.sort_unstable();
    let index = addresses
        .iter()
        .enumerate()
        .map(|(index, address)| (*address, index))
        .collect::<HashMap<_, _>>();
    let (Some(&first), Some(&second)) = (index.get(&first), index.get(&second)) else {
        return None;
    };

    let exit = addresses.len();
    let mut successors = vec![Vec::<usize>::new(); exit];
    let mut has_exit_edge = vec![false; exit];
    for (address, edges) in graph {
        let source = index[address];
        if edges.is_empty() {
            has_exit_edge[source] = true;
        }
        for edge in edges {
            if let Some(target) = index.get(edge) {
                successors[source].push(*target);
            } else {
                has_exit_edge[source] = true;
            }
        }
        successors[source].sort_unstable();
        successors[source].dedup();
    }

    // Treat DFS back-edges as possible escapes. This conservative edge makes
    // the fixed-point proof safe around loops without requiring loop execution
    // or assuming that an iteration eventually terminates.
    let mut colors = vec![0u8; exit];
    for node in 0..exit {
        if colors[node] == 0 {
            mark_back_edges(node, &successors, &mut colors, &mut has_exit_edge);
        }
    }

    let mut postdominators = vec![vec![true; exit + 1]; exit + 1];
    postdominators[exit].fill(false);
    postdominators[exit][exit] = true;
    loop {
        let mut changed = false;
        for node in (0..exit).rev() {
            let mut edges = successors[node].clone();
            if has_exit_edge[node] {
                edges.push(exit);
            }
            if edges.is_empty() {
                edges.push(exit);
            }
            let mut next = vec![true; exit + 1];
            for edge in edges {
                for (slot, present) in next.iter_mut().zip(&postdominators[edge]) {
                    *slot &= *present;
                }
            }
            next[node] = true;
            if next != postdominators[node] {
                postdominators[node] = next;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    (0..exit)
        .filter(|candidate| postdominators[first][*candidate] && postdominators[second][*candidate])
        .max_by_key(|candidate| {
            postdominators[*candidate]
                .iter()
                .filter(|present| **present)
                .count()
        })
        .map(|candidate| addresses[candidate])
}

fn mark_back_edges(
    node: usize,
    successors: &[Vec<usize>],
    colors: &mut [u8],
    has_exit_edge: &mut [bool],
) {
    colors[node] = 1;
    for &target in &successors[node] {
        match colors[target] {
            0 => mark_back_edges(target, successors, colors, has_exit_edge),
            1 => has_exit_edge[node] = true,
            _ => {}
        }
    }
    colors[node] = 2;
}

#[cfg(test)]
mod tests {
    use super::nearest_common_postdominator;
    use std::collections::HashMap;

    #[test]
    fn finds_diamond_reconvergence() {
        let graph = HashMap::from([(10, vec![30]), (20, vec![30]), (30, vec![40]), (40, vec![])]);
        assert_eq!(nearest_common_postdominator(10, 20, &graph), Some(30));
    }

    #[test]
    fn finds_nested_reconvergence_before_the_outer_join() {
        let graph = HashMap::from([
            (10, vec![20, 30]),
            (20, vec![40, 50]),
            (30, vec![60]),
            (40, vec![60]),
            (50, vec![60]),
            (60, vec![70]),
            (70, vec![]),
        ]);
        assert_eq!(nearest_common_postdominator(40, 50, &graph), Some(60));
        assert_eq!(nearest_common_postdominator(20, 30, &graph), Some(60));
    }

    #[test]
    fn unknown_exit_prevents_a_false_join() {
        let graph = HashMap::from([
            (10, vec![30, 99]),
            (20, vec![30]),
            (30, vec![40]),
            (40, vec![]),
        ]);
        assert_eq!(nearest_common_postdominator(10, 20, &graph), None);
    }
}
