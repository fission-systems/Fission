//! Resource limits applied to Rhai evaluation.

/// Sandbox limits for script evaluation (operations, wall-clock time, findings).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScriptLimits {
    pub max_operations: u64,
    pub max_runtime_ms: u64,
    pub max_output_bytes: usize,
    pub max_findings: usize,
}

impl Default for ScriptLimits {
    fn default() -> Self {
        Self {
            max_operations: 1_000_000,
            max_runtime_ms: 500,
            max_output_bytes: 1_000_000,
            max_findings: 10_000,
        }
    }
}
