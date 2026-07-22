use crate::regions::{RegionKind, RegionProof};
use fission_midend_core::ir::{MlilPreviewError};
use fission_midend_dir::{DirStmt};
use fission_midend_core::ir::*;
use crate::HashMap;
use crate::HashSet;

pub type StructureNodeId = usize;

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
    pub statements: Vec<DirStmt>,
    pub proof: Option<RegionProof>,
}

impl StructureNode {
    pub fn region(
        id: StructureNodeId,
        stmt: DirStmt,
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

    pub fn basic(id: StructureNodeId, statements: Vec<DirStmt>, skip_to: usize) -> Self {
        Self {
            id,
            kind: StructureNodeKind::Basic,
            skip_to,
            statements,
            proof: None,
        }
    }

    pub fn unstructured(
        id: StructureNodeId,
        statements: Vec<DirStmt>,
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
pub struct StructureGraph {
    nodes: Vec<StructureNode>,
    edges: Vec<StructureEdge>,
}

impl StructureGraph {
    pub fn next_node_id(&self) -> StructureNodeId {
        self.nodes.len()
    }

    pub fn push(&mut self, node: StructureNode) -> StructureNodeId {
        let id = node.id;
        self.nodes.push(node);
        id
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


pub fn surface_structure_graph(graph: StructureGraph) -> Vec<DirStmt> {
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
