//! A control-flow graph that shrinks as structuring proceeds.
//!
//! # Why this exists
//!
//! Every reference structurer collapses a matched region into a *single node*
//! and removes the consumed nodes from the graph it is still working on:
//!
//! - Ghidra: `BlockGraph::removeBlock` / `removeFromFlow` / `replaceEdgesThru`
//! - angr: `RegionIdentifier._update_graph`, `_abstract_acyclic_region`
//! - radeco: `structure_acyclic_sese_region` -> `graph.remove_node`
//!
//! Fission's collapse loop instead runs over a *fixed* successor/predecessor
//! array and records what has been consumed in side tables
//! (`active_child_map`, `RegionProof.members`). The graph never gets smaller.
//!
//! That difference is not cosmetic, and it was measured rather than assumed.
//! angr's structuring loop makes progress by conceding **one** goto and then
//! retrying every schema against a genuinely simpler graph. Fission has the
//! same concession step (`try_virtualize_one_bad_edge`), and enabling it on
//! the full corpus made the output *worse* -- gotos 1,551 -> 1,602 with 34
//! files regressed and two binaries failing outright -- because the retry
//! faces the same graph minus an edge rather than a graph with a region
//! folded away. The concession buys a jump without buying a smaller problem.
//!
//! Three further consequences of the static-graph model showed up as separate
//! bugs and dead ends during this work: region boundaries have to be fixed
//! up-front by the SESE tree (a collapse rule legitimately consuming past
//! that boundary was a real bug), a region that fails its proof is emitted as
//! residual with its edges as gotos and is never re-approached, and ported
//! mechanisms that assume a shrinking graph under-perform or backfire.
//!
//! # What a collapse means
//!
//! Replacing a set of nodes with one node is only sound when the region has a
//! single entry: every edge arriving from outside the region must land on the
//! same member. Otherwise the region has two ways in and cannot be one
//! statement. Edges wholly inside the region disappear with it -- including a
//! loop's back edge, which the structured body now expresses.
//!
//! The single-entry test is deliberately made against the **live** graph. A
//! region that has two entries now may have one later, once a neighbouring
//! region has folded away; under the static model that second chance never
//! arrives.

use crate::graph::BlockOwnership;
use fission_midend_prehir::PreHirStmt;
use crate::HashSet;

/// Identifier of a node in a [`CollapseGraph`]. Stable across collapses:
/// removed slots are tombstoned rather than compacted.
pub type NodeId = usize;

/// Why a proposed collapse was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CollapseError {
    /// A member id is not live.
    DeadMember(NodeId),
    /// `entry` was not among `members`.
    EntryNotAMember(NodeId),
    /// The member set was empty.
    EmptyRegion,
    /// An edge from outside the region lands on a member other than `entry`,
    /// so the region has more than one way in.
    SideEntry { from: NodeId, to: NodeId },
}

/// One live node: a stretch of the original CFG that has been folded into a
/// single unit of control flow.
#[derive(Debug, Clone)]
pub struct CollapseNode {
    /// Original CFG block indices this node now stands for.
    pub members: BlockOwnership,
    /// Block index this node is entered at, used for label identity.
    pub entry_block: usize,
    /// Structured statements, present once the node is a collapsed region.
    /// A freshly built leaf carries `None` and is lowered on demand.
    pub body: Option<Vec<PreHirStmt>>,
}

/// A live CFG that shrinks as regions are folded into single nodes.
#[derive(Debug, Clone)]
pub struct CollapseGraph {
    nodes: Vec<Option<CollapseNode>>,
    succ: Vec<Vec<NodeId>>,
    pred: Vec<Vec<NodeId>>,
    live: usize,
}

impl CollapseGraph {
    /// Build a leaf-per-block graph from a CFG adjacency listing.
    pub fn from_cfg(successors: &[Vec<usize>]) -> Self {
        let n = successors.len();
        let nodes = (0..n)
            .map(|idx| {
                Some(CollapseNode {
                    members: BlockOwnership::single(idx),
                    entry_block: idx,
                    body: None,
                })
            })
            .collect();
        let mut succ: Vec<Vec<NodeId>> = vec![Vec::new(); n];
        let mut pred: Vec<Vec<NodeId>> = vec![Vec::new(); n];
        for (u, outs) in successors.iter().enumerate() {
            for &v in outs {
                if v >= n || succ[u].contains(&v) {
                    continue;
                }
                succ[u].push(v);
                pred[v].push(u);
            }
        }
        Self {
            nodes,
            succ,
            pred,
            live: n,
        }
    }

    /// Slot count, live or retired -- the dense index space node ids live in.
    pub fn node_capacity(&self) -> usize {
        self.nodes.len()
    }

    pub fn live_count(&self) -> usize {
        self.live
    }

    pub fn is_live(&self, id: NodeId) -> bool {
        self.nodes.get(id).is_some_and(Option::is_some)
    }

    pub fn node(&self, id: NodeId) -> Option<&CollapseNode> {
        self.nodes.get(id)?.as_ref()
    }

    pub fn successors(&self, id: NodeId) -> &[NodeId] {
        self.succ.get(id).map_or(&[], Vec::as_slice)
    }

    pub fn predecessors(&self, id: NodeId) -> &[NodeId] {
        self.pred.get(id).map_or(&[], Vec::as_slice)
    }

    /// Live nodes, in id order.
    pub fn live_nodes(&self) -> impl Iterator<Item = NodeId> + '_ {
        (0..self.nodes.len()).filter(move |id| self.is_live(*id))
    }

    /// The sole surviving node, once structuring has folded everything into
    /// one -- the terminal condition of a collapse loop.
    pub fn sole_live_node(&self) -> Option<NodeId> {
        (self.live == 1).then(|| self.live_nodes().next())?
    }

    /// Check that `members` form a single-entry region entered at `entry`.
    ///
    /// Separated from [`Self::collapse`] so a caller can test a candidate
    /// without committing to it.
    pub fn check_single_entry(
        &self,
        members: &[NodeId],
        entry: NodeId,
    ) -> Result<(), CollapseError> {
        if members.is_empty() {
            return Err(CollapseError::EmptyRegion);
        }
        for &m in members {
            if !self.is_live(m) {
                return Err(CollapseError::DeadMember(m));
            }
        }
        if !members.contains(&entry) {
            return Err(CollapseError::EntryNotAMember(entry));
        }
        let set: HashSet<NodeId> = members.iter().copied().collect();
        for &m in members {
            if m == entry {
                continue;
            }
            if let Some(from) = self.pred[m].iter().find(|p| !set.contains(p)) {
                return Err(CollapseError::SideEntry { from: *from, to: m });
            }
        }
        Ok(())
    }

    /// Fold `members` into a single node carrying `body`.
    ///
    /// External in-edges are rewired onto the surviving node and external
    /// out-edges become its successors; edges wholly inside the region are
    /// dropped, which is what retires a structured loop's back edge. The
    /// surviving node reuses `entry`'s id so predecessors keep their target.
    pub fn collapse(
        &mut self,
        members: &[NodeId],
        entry: NodeId,
        body: Vec<PreHirStmt>,
    ) -> Result<NodeId, CollapseError> {
        self.check_single_entry(members, entry)?;
        let set: HashSet<NodeId> = members.iter().copied().collect();

        // Outgoing edges of the folded region: anything a member reaches that
        // is not itself a member. Self-edges vanish with the region.
        let mut out: Vec<NodeId> = Vec::new();
        for &m in members {
            for &s in &self.succ[m] {
                if !set.contains(&s) && !out.contains(&s) {
                    out.push(s);
                }
            }
        }
        // Incoming edges: every external predecessor of the entry.
        let mut incoming: Vec<NodeId> = Vec::new();
        for &p in &self.pred[entry] {
            if !set.contains(&p) && !incoming.contains(&p) {
                incoming.push(p);
            }
        }

        // Detach every member from its neighbours.
        for &m in members {
            for s in std::mem::take(&mut self.succ[m]) {
                self.pred[s].retain(|x| *x != m);
            }
            for p in std::mem::take(&mut self.pred[m]) {
                self.succ[p].retain(|x| *x != m);
            }
        }

        // Merge membership and retire the consumed slots.
        let mut merged = BlockOwnership::default();
        let mut entry_block = 0usize;
        for &m in members {
            let node = self.nodes[m].take().expect("liveness checked above");
            if m == entry {
                entry_block = node.entry_block;
            }
            merged.extend(node.members.iter());
            self.live -= 1;
        }

        // Reinstate the region as one node under the entry's id.
        self.nodes[entry] = Some(CollapseNode {
            members: merged,
            entry_block,
            body: Some(body),
        });
        self.live += 1;
        for s in out {
            if s == entry || self.succ[entry].contains(&s) {
                continue;
            }
            self.succ[entry].push(s);
            self.pred[s].push(entry);
        }
        for p in incoming {
            if p == entry || self.pred[entry].contains(&p) {
                continue;
            }
            self.pred[entry].push(p);
            self.succ[p].push(entry);
        }
        Ok(entry)
    }

    /// Drop an edge, the concession a structurer makes when no region matches.
    ///
    /// Unlike [`Self::collapse`] this does not shrink the graph; it is the
    /// caller's record that the edge will be emitted as a jump.
    pub fn virtualize_edge(&mut self, from: NodeId, to: NodeId) -> bool {
        if !self.is_live(from) || !self.is_live(to) {
            return false;
        }
        let had = self.succ[from].contains(&to);
        self.succ[from].retain(|x| *x != to);
        self.pred[to].retain(|x| *x != from);
        had
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fission_midend_prehir::PreHirExpr;

    fn stmt(name: &str) -> PreHirStmt {
        PreHirStmt::Expr(PreHirExpr::Var(name.to_string()))
    }

    /// 0 -> 1 -> 2, plus 0 -> 2 (a diamond-free triangle).
    fn triangle() -> CollapseGraph {
        CollapseGraph::from_cfg(&[vec![1, 2], vec![2], vec![]])
    }

    #[test]
    fn builds_a_leaf_per_block() {
        let g = triangle();
        assert_eq!(g.live_count(), 3);
        assert_eq!(g.successors(0), &[1, 2]);
        assert_eq!(g.predecessors(2), &[0, 1]);
        assert_eq!(g.node(1).unwrap().members.iter().collect::<Vec<_>>(), [1]);
    }

    #[test]
    fn collapsing_shrinks_the_graph_and_rewires_edges() {
        // Fold {0,1} -- entered at 0 -- into one node.
        let mut g = triangle();
        let id = g.collapse(&[0, 1], 0, vec![stmt("body")]).unwrap();
        assert_eq!(id, 0, "the region keeps its entry's id");
        assert_eq!(g.live_count(), 2, "the graph is strictly smaller");
        assert!(!g.is_live(1), "the consumed member is gone");
        // The internal 0->1 edge vanished; the external 1->2 edge moved onto
        // the surviving node, deduplicated against 0->2.
        assert_eq!(g.successors(0), &[2]);
        assert_eq!(g.predecessors(2), &[0]);
        let n = g.node(0).unwrap();
        assert_eq!(n.members.iter().collect::<Vec<_>>(), [0, 1]);
        assert_eq!(n.body.as_deref(), Some([stmt("body")].as_slice()));
    }

    #[test]
    fn refuses_a_region_with_a_side_entry() {
        // 0 -> 1, 0 -> 2, 2 -> 1: node 1 is reachable from outside {0,1}.
        let g = CollapseGraph::from_cfg(&[vec![1, 2], vec![], vec![1]]);
        assert_eq!(
            g.check_single_entry(&[0, 1], 0),
            Err(CollapseError::SideEntry { from: 2, to: 1 })
        );
    }

    #[test]
    fn a_second_chance_appears_once_a_neighbour_folds_away() {
        // The whole point of a live graph. 0 -> 1, 0 -> 2, 2 -> 1.
        // {0,1} has a side entry from 2 and cannot fold...
        let mut g = CollapseGraph::from_cfg(&[vec![1, 2], vec![], vec![1]]);
        assert!(g.check_single_entry(&[0, 1], 0).is_err());
        // ...but once {0,2} folds, the surviving node is 1's only predecessor,
        // so the region that was illegal a moment ago is now legal.
        g.collapse(&[0, 2], 0, vec![stmt("folded")]).unwrap();
        assert_eq!(g.check_single_entry(&[0, 1], 0), Ok(()));
        g.collapse(&[0, 1], 0, vec![stmt("whole")]).unwrap();
        assert_eq!(g.sole_live_node(), Some(0));
    }

    #[test]
    fn a_structured_loops_back_edge_is_retired_by_the_fold() {
        // 0 -> 1, 1 -> 1 (self loop), 1 -> 2.
        let mut g = CollapseGraph::from_cfg(&[vec![1], vec![1, 2], vec![]]);
        g.collapse(&[1], 1, vec![stmt("while")]).unwrap();
        assert!(
            !g.successors(1).contains(&1),
            "the back edge is expressed by the structured body, not the graph"
        );
        assert_eq!(g.successors(1), &[2]);
        assert_eq!(g.predecessors(1), &[0]);
    }

    #[test]
    fn folding_everything_reaches_a_single_node() {
        let mut g = triangle();
        g.collapse(&[0, 1], 0, vec![stmt("a")]).unwrap();
        assert!(g.sole_live_node().is_none());
        g.collapse(&[0, 2], 0, vec![stmt("b")]).unwrap();
        assert_eq!(g.sole_live_node(), Some(0));
        assert_eq!(
            g.node(0).unwrap().members.iter().collect::<Vec<_>>(),
            [0, 1, 2],
            "membership accumulates across folds"
        );
    }

    #[test]
    fn refuses_dead_and_malformed_regions() {
        let mut g = triangle();
        g.collapse(&[0, 1], 0, vec![stmt("a")]).unwrap();
        assert_eq!(
            g.check_single_entry(&[1], 1),
            Err(CollapseError::DeadMember(1))
        );
        assert_eq!(g.check_single_entry(&[], 0), Err(CollapseError::EmptyRegion));
        assert_eq!(
            g.check_single_entry(&[0], 2),
            Err(CollapseError::EntryNotAMember(2))
        );
    }

    #[test]
    fn virtualizing_an_edge_does_not_shrink_the_graph() {
        let mut g = triangle();
        assert!(g.virtualize_edge(0, 2));
        assert_eq!(g.live_count(), 3, "a conceded jump folds nothing");
        assert_eq!(g.successors(0), &[1]);
        assert_eq!(g.predecessors(2), &[1]);
        assert!(!g.virtualize_edge(0, 2), "already gone");
    }
}
