use fission_loader::loader::LoadedBinary;

fn execution_extent_start(binary: &LoadedBinary, entry_address: u64) -> u64 {
    execution_extent_start_for_language(binary.sleigh_language_id(), entry_address)
}

fn execution_extent_start_for_language(language_id: Option<&str>, entry_address: u64) -> u64 {
    if language_id.is_some_and(|id| id.starts_with("ARM:")) && entry_address & 1 == 1 {
        entry_address.saturating_sub(1)
    } else {
        entry_address
    }
}

pub fn next_function_distance(binary: &LoadedBinary, entry_address: u64) -> Option<usize> {
    let extent_start = execution_extent_start(binary, entry_address);
    binary.function_after(entry_address).and_then(|next| {
        let dist = next.address.saturating_sub(extent_start) as usize;
        (dist > 0).then_some(dist)
    })
}

pub fn clamp_to_available_execution(
    binary: &LoadedBinary,
    entry_address: u64,
    max_bytes: usize,
) -> usize {
    let extent_start = execution_extent_start(binary, entry_address);
    binary
        .available_execution_bytes(extent_start)
        .map(|available| max_bytes.min(available).max(1))
        .unwrap_or(max_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arm_low_bit_code_pointer_uses_aligned_extent_start() {
        assert_eq!(
            execution_extent_start_for_language(Some("ARM:LE:32:v8"), 0x100037),
            0x100036
        );
    }

    #[test]
    fn arm_aligned_code_pointer_keeps_extent_start() {
        assert_eq!(
            execution_extent_start_for_language(Some("ARM:LE:32:v8"), 0x100036),
            0x100036
        );
    }

    #[test]
    fn non_arm_odd_code_pointer_keeps_extent_start() {
        assert_eq!(
            execution_extent_start_for_language(Some("x86:LE:64:default"), 0x100037),
            0x100037
        );
    }
}
