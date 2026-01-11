//! Safe Rust Wrapper for Pcode FFI
//!
//! This module provides safe, idiomatic Rust interfaces that wrap
//! the unsafe C FFI functions.

use fission_core::{FissionError, Result};
use fission_pcode::{PcodeFunction, PcodeOptimizer, PcodeOptimizerConfig};

/// Safe wrapper for pcode optimization
///
/// This function provides a safe Rust interface to the FFI layer,
/// handling all serialization and error checking.
pub fn optimize_pcode_safe(
    pcode: &mut PcodeFunction,
    config: Option<PcodeOptimizerConfig>,
) -> Result<usize> {
    let cfg = config.unwrap_or_default();
    let mut optimizer = PcodeOptimizer::new(cfg);
    Ok(optimizer.optimize(pcode))
}

/// Optimize pcode from JSON string (safe wrapper)
pub fn optimize_pcode_from_json(json: &str) -> Result<String> {
    // Parse
    let mut pcode = PcodeFunction::from_json(json)
        .map_err(|e| FissionError::Other(format!("JSON parse error: {}", e)))?;

    // Optimize
    let config = PcodeOptimizerConfig::default();
    let mut optimizer = PcodeOptimizer::new(config);
    optimizer.optimize(&mut pcode);

    // Serialize
    serde_json::to_string(&pcode)
        .map_err(|e| FissionError::Other(format!("JSON serialize error: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_optimize_safe() {
        let json = r#"{"blocks":[{"index":0,"start_addr":"0x1000","ops":[]}]}"#;
        let result = optimize_pcode_from_json(json);
        assert!(result.is_ok());
    }
}
