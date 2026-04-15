use super::*;

impl<'a> PreviewBuilder<'a> {
    pub(crate) fn capture_structuring_failure<T>(
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
}

pub(crate) fn surface_structure_graph(graph: StructureGraph) -> Vec<HirStmt> {
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
