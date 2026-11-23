#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FunctionDiscoveryReport {
    pub decoded_instruction_count: usize,
    pub call_target_count: usize,
    pub jump_target_count: usize,
    pub accepted_function_count: usize,
    pub unsupported_runtime: bool,
}

/// A function start a scan found, with the mode it was found in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FunctionStartCandidate {
    pub(super) address: u64,
    /// Found as the architecture's alternate code mode (Thumb): a pattern set
    /// it, or the pointer to it carried the ABI's mode bit.
    pub(super) low_bit_mode: bool,
}
