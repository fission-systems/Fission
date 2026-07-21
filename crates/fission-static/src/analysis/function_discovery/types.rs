#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionDiscoveryProfile {
    /// Collect direct call targets only.
    Conservative,
    /// Everything Conservative does, plus reference/signature-driven gap
    /// scanners: data-section pointer references, and Ghidra's static XML
    /// prologue-pattern DB (`scan_data_references`, `scan_ghidra_patterns`
    /// in `discover.rs`), and shared-return/tail-call recovery.
    Balanced,
    /// Everything Balanced does, plus `scan_dynamic_prologues` -- a
    /// self-referential prologue-fingerprint scanner (Ghidra's Aggressive
    /// Instruction Finder scorecard item) that trusts the binary's *own*
    /// recurring prologue shapes rather than a hardcoded signature DB.
    /// Strictly riskier than Balanced's scanners, same as in Ghidra.
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
