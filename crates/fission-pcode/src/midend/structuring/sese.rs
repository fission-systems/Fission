//! SESE discovery/structuring thin wrappers over midend free owners (ADR 0012).

use crate::midend::PreviewBuilder;
use crate::midend::ir::HirStmt;
use crate::midend::support::MlilPreviewError;

pub(crate) use fission_midend_structuring::{
    SeseRegion, SeseRegionTree, build_sese_tree, compute_rpo_map, find_sese_regions,
};

/// Main entrypoint for SESE region-based structuring (thin host wrap).
pub(crate) fn structure_cfg_via_sese(
    builder: &mut PreviewBuilder<'_>,
    total_nodes: usize,
) -> Result<Vec<HirStmt>, MlilPreviewError> {
    fission_midend_structuring::structure_cfg_via_sese(builder, total_nodes)
}
