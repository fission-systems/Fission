use fission_pcode::{IndirectControlClassification, NirBuildStats};

pub(crate) fn canonical_indirect_classification(
    stats: Option<&NirBuildStats>,
) -> IndirectControlClassification {
    IndirectControlClassification::from_stats_only(stats)
}
