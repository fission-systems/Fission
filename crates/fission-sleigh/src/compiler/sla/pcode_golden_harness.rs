//! Golden parity harness contract: **Ghidra `PcodeEmit` output vs Fission compiled-table lift**.
//!
//! This module does **not** call Ghidra or the JVM. It defines the shape of fixtures and
//! comparison steps so parity work stays algorithmic and dependency-free:
//!
//! 1. **Inputs (Fission-owned):** `CompiledFrontend` (from packaged `.sla` + lowering),
//!    instruction bytes, virtual PC, optional `FlowEmitOptions` from the runtime compiled-table path.
//! 2. **Expected sequence (reference artifact):** ordered steps produced offline from the
//!    same inputs against Ghidra 12.x `PcodeEmit` / listing export — stored as JSON or
//!    similar next to benchmarks (not checked into this file).
//! 3. **Actual sequence:** `decode_and_lift_with_details` or the spine path under test,
//!    normalized to the same step schema (opcode, output space/index/size, mnemonic string).
//!
//! ## Fixture families (from parity audit)
//!
//! - **FlowOverride** — Ghidra `dumpFlowOverride` (`PcodeEmit.java`); Fission fail-closed unless `None`.
//! - **Overlay cross-build** — Ghidra `OverlayAddressSpace.getOverlayAddress` in `appendCrossBuild`.
//! - **Delay slot / INDIRECT** — Ghidra `delaySlot`; Fission `DelaySlotIndirect` + `try_bind_runtime_state_at`.
//! - **Cross-build** — Ghidra `appendCrossBuild` vs Fission `CrossBuild` + named template section.
//!
//! ## How to wire a test
//!
//! - Set `FISSION_PCODE_GOLDEN_DIR` to a directory of JSON fixtures (schema TBD per consumer).
//! - Add `#[ignore]` integration tests that load fixtures and assert step-wise equality.
//! - Until fixtures exist, only the contract tests below run in CI.

/// One lifted p-code step for golden comparison (minimal stable surface).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoldenPcodeStep {
    pub opcode_mnemonic: &'static str,
    pub output_space_id: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::GoldenPcodeStep;

    #[test]
    fn golden_step_schema_is_stable_for_serialization() {
        let step = GoldenPcodeStep {
            opcode_mnemonic: "COPY",
            output_space_id: Some(0),
        };
        assert_eq!(step.opcode_mnemonic, "COPY");
    }

    #[test]
    #[ignore = "set FISSION_PCODE_GOLDEN_DIR and add JSON fixtures to enable"]
    fn golden_fixture_dir_placeholder() {
        let _ = std::env::var("FISSION_PCODE_GOLDEN_DIR").expect("golden dir");
    }
}
