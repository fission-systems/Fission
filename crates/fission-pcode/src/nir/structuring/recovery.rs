use super::cleanup::cleanup_redundant_labels;
use super::*;

impl<'a> PreviewBuilder<'a> {
    pub(crate) fn try_recover_region_linearized_body(
        &mut self,
        start_idx: usize,
        err: &MlilPreviewError,
        targeted: &HashSet<u64>,
        emitted_labels: &mut HashSet<u64>,
    ) -> Result<Option<(Vec<HirStmt>, usize)>, MlilPreviewError> {
        if !self.options.region_linearize_structuring || err.structuring_failure_kind().is_none() {
            return Ok(None);
        }

        let Some(exit) = self.linear_exit(start_idx)? else {
            return Ok(None);
        };
        let Some((mut body, skip_to)) = self.lower_linear_body(start_idx, exit)? else {
            return Ok(None);
        };
        if skip_to <= start_idx {
            return Ok(None);
        }

        let block_key = self.block_target_key(start_idx);
        if (start_idx == 0 || targeted.contains(&block_key)) && emitted_labels.insert(block_key) {
            body.insert(0, HirStmt::Label(block_label(block_key)));
        }

        Ok(Some((cleanup_redundant_labels(body), skip_to)))
    }
}
