//! CFG fact helpers for follow-block and fallthrough resolution.

use super::{CfgAnalysis, EdgeClass};

/// Returns the unique tree-successor when the block has exactly one forward/tree edge.
pub fn dom_based_fallthrough_successor(
    idx: usize,
    successors: &[Vec<usize>],
    edges: &CfgAnalysis,
) -> Option<usize> {
    let succs = successors.get(idx)?;
    if succs.len() != 1 {
        return None;
    }
    let succ = succs[0];
    match edges.edge_classes().get(&(idx, succ)) {
        Some(EdgeClass::Tree) => Some(succ),
        _ => None,
    }
}

/// True when `target` is the immediate layout or dom-tree successor of `entry`.
pub fn is_cfg_fallthrough_successor(
    entry: usize,
    target: usize,
    layout_fallthrough: &[Option<usize>],
    successors: &[Vec<usize>],
    edges: &CfgAnalysis,
) -> bool {
    if layout_fallthrough.get(entry).copied().flatten() == Some(target) {
        return true;
    }
    dom_based_fallthrough_successor(entry, successors, edges) == Some(target)
}

/// True when `idx` is reached only via layout fallthrough from its sole predecessor.
pub fn is_dom_tree_entry(
    idx: usize,
    predecessors: &[Vec<usize>],
    layout_fallthrough: &[Option<usize>],
) -> bool {
    let preds = predecessors.get(idx).map(Vec::as_slice).unwrap_or(&[]);
    if preds.is_empty() {
        return true;
    }
    if preds.len() == 1 {
        let pred = preds[0];
        return layout_fallthrough.get(pred).copied().flatten() == Some(idx);
    }
    false
}
