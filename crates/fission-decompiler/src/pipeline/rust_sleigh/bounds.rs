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
    // `function_after` compares raw addresses, so on ARM a Thumb-tagged
    // (odd) address for the SAME function we're already decoding --
    // typically its own ELF-entry/symbol-table record -- can sort as "the
    // next function" immediately after our Thumb-bit-stripped
    // `extent_start`, one byte later. Normalize the candidate's address the
    // same way `extent_start` already is before treating it as a real
    // subsequent boundary; a normalized match means it's this function, not
    // the next one, so keep looking past it.
    let mut candidate_address = entry_address;
    loop {
        let next = binary.function_after(candidate_address)?;
        let next_extent_start = execution_extent_start(binary, next.address);
        if next_extent_start == extent_start {
            candidate_address = next.address;
            continue;
        }
        let dist = next_extent_start.saturating_sub(extent_start) as usize;
        return (dist > 0).then_some(dist);
    }
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

    #[test]
    fn next_function_distance_skips_own_thumb_tagged_entry_point() {
        use fission_core::architecture::{
            BinaryLoadSpec, CompilerSpecId, GhidraLanguageId, LanguageCompilerSpecPair,
        };
        use fission_loader::loader::types::{DataBuffer, FunctionInfo, LoadedBinaryBuilder};

        let load_spec = BinaryLoadSpec {
            format: "ELF".to_string(),
            image_base: 0x8000000,
            pair: LanguageCompilerSpecPair {
                language_id: GhidraLanguageId("ARM:LE:32:v8".to_string()),
                compiler_spec_id: CompilerSpecId("default".to_string()),
            },
            preferred: true,
            source: "test".to_string(),
        };

        // `_start` at 0x800035d is the Thumb-tagged (odd) form of the SAME
        // entry point as the function we're asking about at the
        // Thumb-bit-stripped 0x800035c -- not a real subsequent function.
        // `func_after` at 0x8000500 is a genuinely later, different function.
        let binary =
            LoadedBinaryBuilder::new("test.elf".to_string(), DataBuffer::Heap(vec![0u8; 0x1000]))
                .load_spec(load_spec)
                .entry_point(0x800035d)
                .image_base(0x8000000)
                .is_64bit(false)
                .add_function(FunctionInfo {
                    name: "_start".to_string(),
                    address: 0x800035d,
                    origin: Some("elf-entry".to_string()),
                    ..Default::default()
                })
                .add_function(FunctionInfo {
                    name: "func_after".to_string(),
                    address: 0x8000500,
                    ..Default::default()
                })
                .build()
                .expect("build");

        assert_eq!(
            next_function_distance(&binary, 0x800035c),
            Some(0x8000500 - 0x800035c)
        );
    }
}
