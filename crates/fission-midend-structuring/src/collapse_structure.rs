//! Ghidra-aligned `CollapseStructure` integration (reference: `blockaction.cc`).
//!
//! Provides:
//! - **LoopBody::emitLikelyEdges** — unstructured exit + back-edge candidates
//! - **TraceDAG pushBranches** — DAG-level likely gotos when stuck
//! - **select_goto** — next edge to virtualize during collapse
//! - **collapse_all_virtualize_pass** — one virtualization step for the SESE driver
//!
//! Does not copy Ghidra C++; reimplements the invariant that collapse virtualizes
//! *one* unstructured edge at a time rather than multi-emitting residual blocks.

use crate::HashSet;
use crate::cfg_analysis::CfgAnalysis;
use crate::cfg_analysis::{DomTree, TraceDag};
use crate::collapse_loop::apply_virtual_goto_edge;
use crate::host::StructuringHost;
use crate::loop_analysis::LoopBody;
use fission_midend_core::ir::MlilPreviewError;

/// Ghidra `LoopBody::emitLikelyEdges`: exit edges (priority) then preferred back edge.
///
/// Order (reverse of Ghidra list push — first returned is first to try):
/// 1. Body → exit edges (loop exits that may be unstructured breaks)
/// 2. Preferred tail → head back edge last (most delayed)
pub fn emit_likely_edges_for_loop(
    loop_body: &LoopBody,
    successors: &[Vec<usize>],
) -> Vec<(usize, usize)> {
    let body_set: HashSet<usize> = loop_body.body.iter().copied().collect();
    let mut exits = Vec::new();
    let mut back = None;

    for &bl in &loop_body.body {
        if bl >= successors.len() {
            continue;
        }
        for &succ in &successors[bl] {
            if body_set.contains(&succ) {
                if succ == loop_body.head && loop_body.tails.contains(&bl) {
                    // Prefer first ordered tail's back edge (tails[0] preferred).
                    if back.is_none() || loop_body.tails.first().is_some_and(|&t| t == bl) {
                        back = Some((bl, succ));
                    }
                }
                continue;
            }
            // Edge leaves the loop body → exit edge.
            // Do not emit canonical exit edges (targeting loop_body.exit_idx)
            // as unstructured exit candidates; they are structured loop exits.
            if loop_body.exit_idx == Some(succ) {
                continue;
            }
            exits.push((bl, succ));
        }
    }

    // Ghidra puts delayed exit just before final backedge; we put exits first
    // then back edge (select_goto tries front first → exits preferred over latch).
    let mut out = exits;
    if let Some(be) = back {
        out.push(be);
    }
    out
}

/// Collect likely unstructured edges for the current innermost loop, or for the
/// whole DAG if no loop remains (Ghidra `CollapseStructure::updateLoopBody`).
pub fn collect_likely_gotos(
    entry: usize,
    exit: usize,
    successors: &[Vec<usize>],
    predecessors: &[Vec<usize>],
    loops: &[LoopBody],
    already_virtual: &HashSet<(usize, usize)>,
) -> Vec<(usize, usize)> {
    let mut likely = Vec::new();

    let dom = DomTree::analyze(successors, predecessors);
    let mut ordered: Vec<&LoopBody> = loops
        .iter()
        .filter(|lb| lb.head >= entry && lb.head < exit)
        .collect();
    // Innermost first: deeper dominance depth first, then smaller body size.
    ordered.sort_by(|a, b| {
        dom.dominance_depth(b.head)
            .cmp(&dom.dominance_depth(a.head))
            .then_with(|| a.body.len().cmp(&b.body.len()))
            .then(a.head.cmp(&b.head))
    });

    for lb in &ordered {
        for edge in emit_likely_edges_for_loop(lb, successors) {
            if !already_virtual.contains(&edge)
                && successors.get(edge.0).is_some_and(|s| s.contains(&edge.1))
            {
                likely.push(edge);
            }
        }
        // One loop's edges first (innermost); Ghidra processes one loopbody at a time.
        if !likely.is_empty() {
            break;
        }
    }

    // DAG TraceDAG pass over the window when no loop edges remain.
    if likely.is_empty() && exit > entry {
        let mut dag = TraceDag::new(successors, predecessors, &dom);
        if let Some(edge) = dag.select_likely_unstructured_edge(entry, exit, already_virtual) {
            likely.push(edge);
        }
        for &edge in dag.likely_gotos() {
            if !already_virtual.contains(&edge)
                && !likely.contains(&edge)
                && successors.get(edge.0).is_some_and(|s| s.contains(&edge.1))
            {
                likely.push(edge);
            }
        }
    }

    likely
}

/// Ghidra `CollapseStructure::selectGoto`: first still-live likely edge.
pub fn select_goto(
    entry: usize,
    exit: usize,
    successors: &[Vec<usize>],
    predecessors: &[Vec<usize>],
    already_virtual: &[(usize, usize)],
) -> Option<(usize, usize)> {
    let virtual_set: HashSet<(usize, usize)> = already_virtual.iter().copied().collect();
    let cfg = CfgAnalysis::analyze(successors, predecessors);
    let irred: HashSet<(usize, usize)> = HashSet::default();
    let loops = LoopBody::identify_loops(successors, predecessors, &cfg, &irred);
    let likely = collect_likely_gotos(entry, exit, successors, predecessors, &loops, &virtual_set);
    likely.into_iter().find(|&(f, t)| {
        !virtual_set.contains(&(f, t)) && successors.get(f).is_some_and(|s| s.contains(&t))
    })
}

/// One collapse-all virtualization step: pick selectGoto edge and apply FAS virtual.
pub fn collapse_all_virtualize_one(
    host: &mut impl StructuringHost,
    entry: usize,
    exit: usize,
) -> Result<bool, MlilPreviewError> {
    let Some((from, to)) = select_goto(
        entry,
        exit,
        host.successors(),
        host.predecessors(),
        host.fas_virtual_edges(),
    ) else {
        return Ok(false);
    };
    Ok(apply_virtual_goto_edge(host, from, to))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cfg_analysis::CfgAnalysis;

    #[test]
    fn emit_likely_edges_omits_canonical_exit_and_retains_irregular_exit_and_back() {
        // head=1, body={1,2}, tail=2, canonical exit=3, irregular break exit=4:
        // 0→1, 0→4 (external pred for 4 so 4 is outside loop), 1→2, 2→1 (back),
        // 2→3 (canonical exit), 2→4 (irregular exit)
        let succs = vec![vec![1, 4], vec![2], vec![1, 3, 4], vec![], vec![]];
        let preds = vec![vec![], vec![0, 2], vec![1], vec![2], vec![0, 2]];
        let cfg = CfgAnalysis::analyze(&succs, &preds);
        let loops = LoopBody::identify_loops(&succs, &preds, &cfg, &HashSet::default());
        let lb = loops.iter().find(|l| l.head == 1).expect("loop at 1");
        assert_eq!(lb.exit_idx, Some(3), "canonical exit should be 3");

        let edges = emit_likely_edges_for_loop(lb, &succs);
        assert!(
            !edges.iter().any(|&(f, t)| f == 2 && t == 3),
            "canonical exit 2→3 must NOT be emitted as unstructured candidate: {edges:?}"
        );
        assert!(
            edges.iter().any(|&(f, t)| f == 2 && t == 4),
            "irregular exit 2→4 must be emitted: {edges:?}"
        );
        assert!(
            edges.iter().any(|&(f, t)| f == 2 && t == 1),
            "back edge 2→1 must be emitted: {edges:?}"
        );
    }

    #[test]
    fn collect_likely_gotos_sorts_innermost_loops_first() {
        // Nested loops:
        // Outer loop head=1, body={1, 2, 3, 4}, tail=4
        // Inner loop head=2, body={2, 3}, tail=3
        // 0 -> 1 -> 2 -> 3 -> 2 (inner back)
        // 3 -> 4 -> 1 (outer back)
        // 4 -> 5 (outer exit)
        let succs = vec![
            vec![1],    // 0
            vec![2],    // 1 (outer head)
            vec![3],    // 2 (inner head)
            vec![2, 4], // 3 (inner tail back to 2, or to 4)
            vec![1, 5], // 4 (outer tail back to 1, or to 5)
            vec![],     // 5
        ];
        let mut preds = vec![Vec::new(); succs.len()];
        for (u, s_list) in succs.iter().enumerate() {
            for &v in s_list {
                preds[v].push(u);
            }
        }
        let cfg = CfgAnalysis::analyze(&succs, &preds);
        let loops = LoopBody::identify_loops(&succs, &preds, &cfg, &HashSet::default());
        assert_eq!(loops.len(), 2);

        let likely = collect_likely_gotos(0, 6, &succs, &preds, &loops, &HashSet::default());
        // Innermost loop (head 2) should be processed first!
        // Inner loop's edge is back-edge (3, 2).
        assert_eq!(
            likely,
            vec![(3, 2)],
            "innermost loop edges must be selected first"
        );
    }

    #[test]
    fn select_goto_returns_real_edge() {
        let succs = vec![vec![1], vec![2], vec![1, 3], vec![]];
        let preds = vec![vec![], vec![0, 2], vec![1], vec![2]];
        let edge = select_goto(0, 4, &succs, &preds, &[]).expect("edge");
        assert!(succs[edge.0].contains(&edge.1), "{edge:?}");
    }
}
