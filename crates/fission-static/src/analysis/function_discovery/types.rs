#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionDiscoveryProfile {
    /// Collect direct call targets only.
    Conservative,
    /// Collect direct call targets only; reserved for future analyzer budgets.
    Balanced,
    /// Collect direct call and branch targets.
    Aggressive,
}

impl Default for FunctionDiscoveryProfile {
    fn default() -> Self {
        Self::Conservative
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FunctionDiscoveryReport {
    pub decoded_instruction_count: usize,
    pub call_target_count: usize,
    pub jump_target_count: usize,
    pub accepted_function_count: usize,
    pub unsupported_runtime: bool,
}
