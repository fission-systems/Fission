//! Rust-native embedded scripting over binary inventory (read-only).

mod api;
mod engine;
mod error;
mod host;
mod limits;
mod result;
mod sandbox;

pub use engine::{check_script, run_script};
pub use error::ScriptError;
pub use limits::ScriptLimits;
pub use result::{
    LimitsEcho, SCHEMA_VERSION, ScriptDiagnostic, ScriptFinding, ScriptMeta, ScriptRunResult,
    ScriptRunStatus,
};

use fission_loader::loader::LoadedBinary;
use fission_static::analysis::{FunctionDiscoveryProfile, discover_functions_with_runtime};

/// Run SLEIGH-backed function discovery (`Balanced`) so script `binary.functions()` aligns with automated CLI workflows.
pub fn prepare_binary_for_script(mut binary: LoadedBinary) -> LoadedBinary {
    let _ = discover_functions_with_runtime(&mut binary, FunctionDiscoveryProfile::Balanced);
    binary
}

#[cfg(test)]
mod tests {
    use super::*;
    use fission_core::common::types::FunctionInfo;
    use fission_loader::loader::{DataBuffer, LoadedBinaryBuilder};

    fn test_binary() -> LoadedBinary {
        LoadedBinaryBuilder::new("fixture.bin".into(), DataBuffer::Heap(vec![0_u8; 8]))
            .format("TEST")
            .image_base(0x1000)
            .is_64bit(false)
            .add_function(FunctionInfo {
                name: "main".into(),
                address: 0x1000,
                size: 32,
                is_export: true,
                is_import: false,
                external_library: None,
                ..Default::default()
            })
            .add_function(FunctionInfo {
                name: "puts".into(),
                address: 0x2000,
                size: 8,
                is_export: false,
                is_import: true,
                external_library: Some("libc.so".into()),
                ..Default::default()
            })
            .build()
            .expect("fixture binary")
    }

    #[test]
    fn check_script_accepts_valid_rhai() {
        assert!(check_script("let x = 1;").is_ok());
    }

    #[test]
    fn check_script_rejects_invalid_syntax() {
        assert!(check_script("let x = ").is_err());
    }

    #[test]
    fn run_script_emit_records_findings() {
        let bin = prepare_binary_for_script(test_binary());
        let src = r#"emit(#{"kind": "note", "message": "hello"});"#;
        let out = run_script(&bin, src, "inline.rhai", ScriptLimits::default());
        assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
        assert_eq!(out.findings.len(), 1);
        assert_eq!(out.findings[0].kind, "note");
        assert_eq!(out.findings[0].message.as_deref(), Some("hello"));
    }

    #[test]
    fn run_script_respects_max_findings() {
        let bin = prepare_binary_for_script(test_binary());
        let limits = ScriptLimits {
            max_findings: 2,
            max_runtime_ms: 2000,
            ..ScriptLimits::default()
        };
        let src = r#"
            for i in 0..10 {
                emit(#{"kind": "x"});
            }
        "#;
        let out = run_script(&bin, src, "limits.rhai", limits);
        assert_eq!(out.findings.len(), 2);
        assert!(
            out.diagnostics
                .iter()
                .any(|d| d.message.contains("max_findings")),
            "{:?}",
            out.diagnostics
        );
    }

    #[test]
    fn run_script_respects_max_operations() {
        let bin = prepare_binary_for_script(test_binary());
        let limits = ScriptLimits {
            max_operations: 800,
            max_runtime_ms: 5000,
            ..ScriptLimits::default()
        };
        let src = r#"
            let x = 0;
            while x < 100000 {
                x = x + 1;
            }
        "#;
        let out = run_script(&bin, src, "spin.rhai", limits);
        assert!(
            out.diagnostics.iter().any(|d| {
                let m = d.message.to_lowercase();
                m.contains("operations") || m.contains("maximum") || m.contains("limit")
            }),
            "{:?}",
            out.diagnostics
        );
    }
}
