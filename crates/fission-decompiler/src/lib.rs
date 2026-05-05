//! Fission Decompiler — orchestration layer (`fission-decompiler`) over canonical IR (`fission-pcode`).
//!
//! Downstream crates should prefer **`fission_decompiler::`** for IR types and pipelines; this crate
//! re-exports the full [`fission_pcode`] surface alongside routing, workers, and Rust-Sleigh glue.

#![allow(clippy::all)]

pub use fission_pcode::*;

pub mod adapters;
pub mod engine;
pub mod facts;
pub mod recovery;
pub mod render;
pub mod request;
pub mod routing;
pub mod taxonomy;
pub mod types;
pub mod worker;

pub mod rust_sleigh;

pub use adapters::{NativeDecompilerBackend, NativeDecompilerSource};
pub use fission_static::utils;

pub use engine::{
    NirEngineMode, NirRoutingDecision, NirRoutingResolver, NirSelection, NirSource, NirSurfaceKind,
    NirWorkerRequest, NirWorkerResponse, PreviewEngineMode, PreviewRoutingDecision,
    PreviewRoutingResolver, PreviewSelection, PreviewSource, PreviewSurfaceKind,
    PreviewWorkerRequest, PreviewWorkerResponse, auto_nir_admission_eligible, auto_nir_eligible,
    classified_nir_error, classify_native_failure_kind, classify_nir_failure,
    classify_nir_failure_refined, execute_nir_worker, execute_preview_worker,
    fallback_reason_with_kind, native_failure_routing_decision, nir_fallback_reason_with_kind,
    rescue_nir_output, rescue_nir_output_with_facts, select_nir_output,
    select_nir_output_from_pcode, select_nir_output_from_pcode_with_facts,
    select_nir_output_with_facts,
};
pub use request::{DecompileRequest, DecompileResult, decompile_prebuilt_pcode};

pub use rust_sleigh::{
    RustSleighDecompileConfig, RustSleighDecompileResult, RustSleighPipelineEvidence,
    decompile_with_rust_sleigh, select_nir_output_from_prebuilt_pcode,
};
pub(crate) use rust_sleigh::decode_rust_sleigh_pcode;

pub type DecompileEngineMode = NirEngineMode;
pub type DecompileSelection = NirSelection;
pub type DecompileRoutingDecision = NirRoutingDecision;
pub type WorkerRequest = NirWorkerRequest;
pub type WorkerResponse = NirWorkerResponse;

#[cfg(test)]
mod orchestration_tests {
    use super::*;
    use fission_loader::loader::{DataBuffer, LoadedBinaryBuilder};

    #[test]
    fn prebuilt_pcode_roundtrip_selection() {
        let binary = LoadedBinaryBuilder::new("sample.exe".to_string(), DataBuffer::Heap(vec![]))
            .format("PE")
            .is_64bit(true)
            .build()
            .expect("build test binary");
        let pcode = PcodeFunction { blocks: vec![] };
        let request = DecompileRequest {
            binary: &binary,
            fact_store: None,
            function_address: 0x401000,
            function_name: Some("sub_401000"),
            engine_mode: NirEngineMode::Legacy,
            timeout_ms: None,
            render_options: Some(NirRenderOptions::from_loaded_binary(&binary)),
        };
        let result = decompile_prebuilt_pcode(&pcode, &request).expect("prebuilt");
        assert!(result.code.is_none());
    }

    #[test]
    fn prebuilt_pcode_legacy_mode_is_passthrough() {
        let binary = LoadedBinaryBuilder::new("sample.exe".to_string(), DataBuffer::Heap(vec![]))
            .format("PE")
            .is_64bit(true)
            .build()
            .expect("build test binary");
        let pcode = PcodeFunction { blocks: vec![] };

        let selection = select_nir_output_from_prebuilt_pcode(
            &pcode,
            &binary,
            0x401000,
            "sub_401000",
            NirEngineMode::Legacy,
            None,
            NirRenderOptions::from_loaded_binary(&binary),
        )
        .expect("legacy mode selection");

        assert_eq!(selection.engine_used, NirEngineMode::Legacy);
        assert!(!selection.fell_back);
        assert!(selection.nir_code.is_none());
    }
}
