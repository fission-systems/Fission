use super::*;

pub(crate) type StructureNodeId = usize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StructureNodeKind {
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
pub(crate) enum StructureEdgeFlags {
    Plain,
    Goto,
    Loop,
    Irreducible,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StructureEdge {
    pub(crate) from: StructureNodeId,
    pub(crate) to: StructureNodeId,
    pub(crate) flags: StructureEdgeFlags,
}

#[derive(Debug, Clone)]
pub(crate) struct StructureNode {
    pub(crate) id: StructureNodeId,
    pub(crate) kind: StructureNodeKind,
    pub(crate) skip_to: usize,
    pub(crate) statements: Vec<HirStmt>,
    pub(crate) proof: Option<RegionProof>,
}

impl StructureNode {
    pub(crate) fn region(
        id: StructureNodeId,
        stmt: HirStmt,
        skip_to: usize,
        proof: RegionProof,
    ) -> Self {
        Self {
            id,
            kind: StructureNodeKind::Region(proof.kind),
            skip_to,
            statements: vec![stmt],
            proof: Some(proof),
        }
    }

    pub(crate) fn basic(id: StructureNodeId, statements: Vec<HirStmt>, skip_to: usize) -> Self {
        Self {
            id,
            kind: StructureNodeKind::Basic,
            skip_to,
            statements,
            proof: None,
        }
    }

    pub(crate) fn unstructured(
        id: StructureNodeId,
        statements: Vec<HirStmt>,
        skip_to: usize,
    ) -> Self {
        Self {
            id,
            kind: StructureNodeKind::Unstructured,
            skip_to,
            statements,
            proof: None,
        }
    }
}

#[derive(Debug, Default, Clone)]
pub(crate) struct StructureGraph {
    nodes: Vec<StructureNode>,
    edges: Vec<StructureEdge>,
}

impl StructureGraph {
    pub(crate) fn next_node_id(&self) -> StructureNodeId {
        self.nodes.len()
    }

    pub(crate) fn push(&mut self, node: StructureNode) -> StructureNodeId {
        let id = node.id;
        self.nodes.push(node);
        id
    }

    pub(crate) fn push_edge(
        &mut self,
        from: StructureNodeId,
        to: StructureNodeId,
        flags: StructureEdgeFlags,
    ) {
        self.edges.push(StructureEdge { from, to, flags });
    }

    pub(crate) fn nodes(&self) -> &[StructureNode] {
        &self.nodes
    }

    pub(crate) fn into_nodes(self) -> Vec<StructureNode> {
        self.nodes
    }
}
