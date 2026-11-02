#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FunctionDiscoveryReport {
    pub decoded_instruction_count: usize,
    pub call_target_count: usize,
    pub jump_target_count: usize,
    pub accepted_function_count: usize,
    pub unsupported_runtime: bool,
}
