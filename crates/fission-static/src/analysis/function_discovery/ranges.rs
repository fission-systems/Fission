use fission_loader::LoadedBinary;

pub(crate) fn runtime_load_spec_for(
    binary: &LoadedBinary,
) -> Option<&fission_loader::loader::BinaryLoadSpec> {
    binary.load_spec()
}

pub(crate) fn executable_ranges(binary: &LoadedBinary) -> Vec<(u64, u64)> {
    binary
        .sections
        .iter()
        .filter(|section| section.is_executable)
        .map(|section| {
            (
                section.virtual_address,
                section.virtual_address.saturating_add(section.virtual_size),
            )
        })
        .collect()
}

pub(crate) fn is_in_executable_ranges(target: u64, ranges: &[(u64, u64)]) -> bool {
    ranges
        .iter()
        .any(|&(start, end)| target >= start && target < end)
}
