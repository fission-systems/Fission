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
