use crate::HashMap;
use crate::HashSet;
use crate::regions::{RegionKind, RegionProof};
use fission_midend_core::ir::MlilPreviewError;
use fission_midend_core::ir::*;
use fission_midend_prehir::PreHirStmt;

pub type StructureNodeId = usize;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct BlockOwnership {
    blocks: Vec<usize>,
}

impl BlockOwnership {
    pub fn from_members(members: impl IntoIterator<Item = usize>) -> Self {
        let mut blocks = members.into_iter().collect::<Vec<_>>();
        blocks.sort_unstable();
        blocks.dedup();
        Self { blocks }
    }

    pub fn single(block: usize) -> Self {
        Self {
            blocks: vec![block],
        }
    }

    pub fn contains(&self, block: usize) -> bool {
        self.blocks.binary_search(&block).is_ok()
    }

    pub fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.blocks.iter().copied()
    }

    pub fn extend(&mut self, members: impl IntoIterator<Item = usize>) {
        self.blocks.extend(members);
        self.blocks.sort_unstable();
        self.blocks.dedup();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructureNodeKind {
    Basic,
    Copy,
    Goto,
    MultiGoto,
    Condition,
    If,
    WhileDo,
    DoWhile,
    Region(RegionKind),
    Switch,
    InfLoop,
    Sequence,
    Unstructured,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructureEdgeFlags {
    Plain,
    Goto,
    Loop,
    Irreducible,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StructureEdge {
    pub from: StructureNodeId,
    pub to: StructureNodeId,
    pub flags: StructureEdgeFlags,
}

#[derive(Debug, Clone)]
pub struct StructureNode {
    pub id: StructureNodeId,
    pub kind: StructureNodeKind,
    pub skip_to: usize,
    pub statements: Vec<PreHirStmt>,
    pub proof: Option<RegionProof>,
    pub ownership: BlockOwnership,
}

impl StructureNode {
    pub fn region(id: StructureNodeId, stmt: PreHirStmt, skip_to: usize, proof: RegionProof) -> Self {
        let ownership = BlockOwnership::from_members(proof.members.iter().copied());
        Self {
            id,
            kind: StructureNodeKind::Region(proof.kind),
            skip_to,
            statements: vec![stmt],
            proof: Some(proof),
            ownership,
        }
    }

    pub fn region_body(
        id: StructureNodeId,
        statements: Vec<PreHirStmt>,
        skip_to: usize,
        proof: RegionProof,
    ) -> Self {
        let ownership = BlockOwnership::from_members(proof.members.iter().copied());
        Self {
            id,
            kind: StructureNodeKind::Region(proof.kind),
            skip_to,
            statements,
            proof: Some(proof),
            ownership,
        }
    }

    pub fn basic(id: StructureNodeId, statements: Vec<PreHirStmt>, block: usize) -> Self {
        Self {
            id,
            kind: StructureNodeKind::Basic,
            skip_to: block + 1,
            statements,
            proof: None,
            ownership: BlockOwnership::single(block),
        }
    }

    pub fn unstructured(id: StructureNodeId, statements: Vec<PreHirStmt>, block: usize) -> Self {
        Self {
            id,
            kind: StructureNodeKind::Unstructured,
            skip_to: block + 1,
            statements,
            proof: None,
            ownership: BlockOwnership::single(block),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DuplicateBlockOwnership {
    pub block: usize,
    pub existing_owner: StructureNodeId,
    pub attempted_owner: StructureNodeId,
}

#[derive(Debug, Default, Clone)]
pub struct StructureGraph {
    nodes: Vec<StructureNode>,
    edges: Vec<StructureEdge>,
    owner_by_block: HashMap<usize, StructureNodeId>,
}

impl StructureGraph {
    pub fn next_node_id(&self) -> StructureNodeId {
        self.nodes.len()
    }

    pub fn push(
        &mut self,
        node: StructureNode,
    ) -> Result<StructureNodeId, DuplicateBlockOwnership> {
        let id = node.id;
        for block in node.ownership.iter() {
            if let Some(&existing_owner) = self.owner_by_block.get(&block) {
                return Err(DuplicateBlockOwnership {
                    block,
                    existing_owner,
                    attempted_owner: id,
                });
            }
        }
        for block in node.ownership.iter() {
            self.owner_by_block.insert(block, id);
        }
        self.nodes.push(node);
        Ok(id)
    }

    pub fn push_edge(
        &mut self,
        from: StructureNodeId,
        to: StructureNodeId,
        flags: StructureEdgeFlags,
    ) {
        self.edges.push(StructureEdge { from, to, flags });
    }

    pub fn nodes(&self) -> &[StructureNode] {
        &self.nodes
    }

    pub fn into_nodes(self) -> Vec<StructureNode> {
        self.nodes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::regions::{RegionKind, RegionProof};

    #[test]
    fn region_node_owns_proven_members() {
        let proof = RegionProof::structured(RegionKind::Loop, 2, 6, None);
        let node = StructureNode::region(0, PreHirStmt::Block(vec![].into()), 6, proof);

        assert_eq!(node.ownership.iter().collect::<Vec<_>>(), vec![2, 3, 4, 5]);
        assert!(node.ownership.contains(4));
        assert!(!node.ownership.contains(6));
    }

    #[test]
    fn graph_rejects_duplicate_block_claims() {
        let mut graph = StructureGraph::default();
        graph
            .push(StructureNode::basic(0, vec![], 3))
            .expect("first owner");
        let conflict = graph
            .push(StructureNode::unstructured(1, vec![], 3))
            .expect_err("duplicate claim must fail");

        assert_eq!(
            conflict,
            DuplicateBlockOwnership {
                block: 3,
                existing_owner: 0,
                attempted_owner: 1,
            }
        );
    }
}

pub fn surface_structure_graph(graph: StructureGraph) -> Vec<PreHirStmt> {
    graph
        .into_nodes()
        .into_iter()
        .flat_map(|node| {
            if let Some(proof) = node.proof.as_ref() {
                debug_assert!(
                    matches!(node.kind, StructureNodeKind::Region(kind) if kind == proof.kind)
                );
                debug_assert_eq!(proof.follow, Some(node.skip_to));
            }
            node.statements.into_iter()
        })
        .collect()
}

pub fn capture_structuring_failure<T>(
    result: Result<Option<T>, MlilPreviewError>,
    last_structuring_failure: &mut Option<MlilPreviewError>,
) -> Result<Option<T>, MlilPreviewError> {
    match result {
        Ok(result) => Ok(result),
        Err(err) if err.structuring_failure_kind().is_some() => {
            *last_structuring_failure = Some(err);
            Ok(None)
        }
        Err(MlilPreviewError::UnsupportedControlFlow)
        | Err(MlilPreviewError::UnsupportedCfgBranchTarget) => Ok(None),
        Err(err) => Err(err),
    }
}
