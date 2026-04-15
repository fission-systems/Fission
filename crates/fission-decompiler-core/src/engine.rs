use fission_loader::loader::LoadedBinary;
use fission_pcode::{NirRenderOptions, PcodeFunction};
use fission_static::analysis::decomp::facts::FactStore;

pub use crate::recovery::{PreviewRoutingDecision, PreviewSelection};
pub use crate::routing::{
    auto_nir_admission_eligible, auto_nir_eligible, native_failure_routing_decision,
    rescue_nir_output, rescue_nir_output_with_facts, select_nir_output,
    select_nir_output_from_pcode, select_nir_output_from_pcode_with_facts,
    select_nir_output_with_facts,
};
pub use crate::taxonomy::{
    classified_nir_error, classify_native_failure_kind, classify_nir_failure,
    classify_nir_failure_refined, fallback_reason_with_kind,
};
pub use crate::types::{
    NirEngineMode, NirRoutingDecision, NirRoutingResolver, NirSelection, NirSource, NirSurfaceKind,
    NirWorkerRequest, NirWorkerResponse, PreviewEngineMode, PreviewRoutingResolver, PreviewSource,
    PreviewSurfaceKind, PreviewWorkerRequest, PreviewWorkerResponse,
};
pub use crate::worker::{execute_nir_worker, execute_preview_worker};

pub fn nir_fallback_reason_with_kind(kind: &str, detail: impl AsRef<str>) -> String {
    fallback_reason_with_kind(kind, detail)
}

pub fn auto_mlil_eligible(binary: &LoadedBinary, pcode: &PcodeFunction) -> bool {
    auto_nir_admission_eligible(binary, pcode)
}

pub fn select_preview_output<S: NirSource>(
    source: &mut S,
    binary: &LoadedBinary,
    address: u64,
    name: &str,
    mode: NirEngineMode,
    timeout_ms: Option<u64>,
) -> Result<NirSelection, String> {
    select_nir_output(source, binary, address, name, mode, timeout_ms)
}

pub fn select_preview_output_with_facts<S: NirSource>(
    source: &mut S,
    binary: &LoadedBinary,
    fact_store: &FactStore,
    address: u64,
    name: &str,
    mode: NirEngineMode,
    timeout_ms: Option<u64>,
) -> Result<NirSelection, String> {
    select_nir_output_with_facts(source, binary, fact_store, address, name, mode, timeout_ms)
}

pub fn select_preview_output_from_pcode(
    pcode: &PcodeFunction,
    binary: &LoadedBinary,
    address: u64,
    name: &str,
    mode: NirEngineMode,
    timeout_ms: Option<u64>,
    options: NirRenderOptions,
) -> Result<NirSelection, String> {
    select_nir_output_from_pcode(pcode, binary, address, name, mode, timeout_ms, options)
}

pub fn select_preview_output_from_pcode_with_facts(
    pcode: &PcodeFunction,
    binary: &LoadedBinary,
    fact_store: &FactStore,
    address: u64,
    name: &str,
    mode: NirEngineMode,
    timeout_ms: Option<u64>,
    options: NirRenderOptions,
) -> Result<NirSelection, String> {
    select_nir_output_from_pcode_with_facts(
        pcode, binary, fact_store, address, name, mode, timeout_ms, options,
    )
}

pub fn rescue_preview_output<S: NirSource>(
    source: &mut S,
    binary: &LoadedBinary,
    address: u64,
    name: &str,
    error: &str,
    timeout_ms: Option<u64>,
) -> Result<Option<NirSelection>, String> {
    rescue_nir_output(source, binary, address, name, error, timeout_ms)
}

pub fn rescue_preview_output_with_facts<S: NirSource>(
    source: &mut S,
    binary: &LoadedBinary,
    fact_store: &FactStore,
    address: u64,
    name: &str,
    error: &str,
    timeout_ms: Option<u64>,
) -> Result<Option<NirSelection>, String> {
    rescue_nir_output_with_facts(source, binary, fact_store, address, name, error, timeout_ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::sanitize_nir_symbol_name;
    use crate::render::{build_nir_type_context_from_facts, make_nir_request};
    use crate::worker::nir_worker_timeout_ms;
    use fission_core::common::types::FunctionInfo;
    use fission_loader::loader::types::{
        DwarfFunctionInfo, DwarfLocalVar, DwarfLocation, DwarfParamInfo,
    };
    use fission_loader::loader::{DataBuffer, LoadedBinaryBuilder};
    use fission_pcode::{NirRenderOptions, NirTypeContext, PcodeFunction, PreviewCallParamRule};
    use std::collections::HashMap;

    struct MockNirSource;

    impl NirSource for MockNirSource {
        fn get_pcode_json(&mut self, _address: u64) -> fission_core::Result<String> {
            Ok("{\"blocks\":[]}".to_string())
        }
    }

    #[test]
    fn preview_worker_request_roundtrip() {
        let request = NirWorkerRequest {
            pcode_json: "{\"blocks\":[]}".to_string(),
            address: 0x1234,
            name: "sub_1234".to_string(),
            options: NirRenderOptions {
                pe_x64_only: true,
                is_64bit: true,
                pointer_size: 8,
                format: "PE".to_string(),
                image_base: 0x140000000,
                sections: vec![(0x140001000, 0x140002000)],
                region_linearize_structuring: false,
                force_linear_structuring: false,
                conservative_irreducible_fallback: false,
                structuring_engine: StructuringEngineKind::LegacyScored,
                global_names: Default::default(),
                calling_convention: Default::default(),
            },
            type_context: NirTypeContext {
                call_targets: HashMap::from([(0x140001234, "MessageBoxW".to_string())]),
                call_target_refs: HashMap::new(),
                call_param_rules: vec![PreviewCallParamRule {
                    callee_address: None,
                    callee_name: "MessageBoxW".to_string(),
                    arg_index: 1,
                    pointer_alias: "LPCWSTR".to_string(),
                    pointee_alias: "WCHAR".to_string(),
                    pointer_size: 8,
                    pointee_sizes: vec![2],
                }],
                function_hints: None,
            },
        };

        let encoded = serde_json::to_string(&request).expect("serialize worker request");
        let decoded: NirWorkerRequest =
            serde_json::from_str(&encoded).expect("deserialize worker request");

        assert_eq!(decoded.address, request.address);
        assert_eq!(decoded.name, request.name);
        assert_eq!(decoded.options, request.options);
        assert_eq!(decoded.type_context, request.type_context);
    }

    #[test]
    fn preview_worker_timeout_clamps() {
        assert_eq!(nir_worker_timeout_ms(Some(500)), 1_000);
        assert_eq!(nir_worker_timeout_ms(Some(30_000)), 10_000);
    }

    #[test]
    fn native_failure_routing_uses_taxonomy() {
        let decision = native_failure_routing_decision("Could not find op at target address");
        assert_eq!(decision.engine_used, NirEngineMode::Legacy);
        assert!(decision.fell_back);
        assert_eq!(
            decision.fallback_reason.as_deref(),
            Some("native_pcode_failure: Could not find op at target address")
        );
    }

    #[test]
    fn preview_selection_exposes_routing_decision() {
        let selection = NirSelection {
            nir_code: None,
            build_stats: None,
            hint_stats: None,
            engine_used: NirEngineMode::Legacy,
            fell_back: true,
            fallback_reason: Some("preview_timeout: worker timed out".to_string()),
            fallback_kind: Some("preview_timeout"),
            fallback_kind_refined: Some("preview_timeout"),
            nir_surface: None,
            recovery_strategy_attempted: None,
            recovery_strategy_applied: None,
            recovery_outcome: None,
            recovery_source_signature: None,
            recovery_structuring_mode: None,
            recovery_reason_family: None,
            recovery_retryable: None,
        };
        let decision = selection.routing_decision();
        assert_eq!(decision.engine_used, NirEngineMode::Legacy);
        assert!(decision.fell_back);
        assert_eq!(decision.fallback_kind, Some("preview_timeout"));
        assert_eq!(decision.fallback_kind_refined, Some("preview_timeout"));
        assert_eq!(
            decision.fallback_reason.as_deref(),
            Some("preview_timeout: worker timed out")
        );
    }

    #[test]
    fn preview_failure_classifier_distinguishes_cfg_and_lowering_failures() {
        assert_eq!(
            classify_nir_failure_refined(
                "mlil-preview unavailable: unsupported branch target in mlil-preview"
            ),
            "nir_unsupported_cfg"
        );
        assert_eq!(
            classify_nir_failure_refined(
                "preview_structuring_failure[unsupported_cfg_region_shape]: unsupported region shape in mlil-preview"
            ),
            "nir_structuring_failure"
        );
        assert_eq!(
            classify_nir_failure_refined(
                "mlil-preview unavailable: value lowering failed on varnode: unsupported address materialization"
            ),
            "nir_parse_or_lowering_failure"
        );
        assert_eq!(
            classify_nir_failure_refined(
                "mlil-preview unavailable: unsupported architecture in mlil-preview"
            ),
            "nir_architecture_unsupported"
        );
        assert_eq!(
            classify_nir_failure_refined(
                "mlil-preview unavailable: unsupported format in mlil-preview"
            ),
            "nir_format_unsupported"
        );
        assert_eq!(
            classify_nir_failure_refined("mlil-preview worker response parse failed: bad json"),
            "nir_worker_failure"
        );
    }

    #[test]
    fn nir_fallback_exposes_refined_kind() {
        let selection = NirRoutingResolver::nir_fallback(
            "preview_structuring_failure[unsupported_cfg_phi_join]: unsupported phi join in mlil-preview",
        );
        assert_eq!(selection.fallback_kind, Some("nir_unsupported"));
        assert_eq!(
            selection.fallback_kind_refined,
            Some("nir_structuring_failure")
        );
    }

    #[test]
    fn nir_success_classifies_unstructured_surface() {
        let selection = NirRoutingResolver::nir_success(
            "label_1:\n  goto label_1;".to_string(),
            None,
            None,
            false,
            None,
        );
        assert_eq!(selection.nir_surface, Some(NirSurfaceKind::Unstructured));
        assert_eq!(
            selection.routing_decision().nir_surface,
            Some(NirSurfaceKind::Unstructured)
        );
    }

    #[test]
    fn fact_store_names_drive_preview_call_targets() {
        let binary = LoadedBinaryBuilder::new("sample.exe".to_string(), DataBuffer::Heap(vec![]))
            .format("PE")
            .is_64bit(true)
            .add_function(FunctionInfo {
                name: "sub_401000".to_string(),
                address: 0x401000,
                size: 0,
                is_export: false,
                is_import: false,
            })
            .build()
            .expect("build test binary");
        let mut facts = FactStore::from_binary(&binary);
        facts.ingest_name_fact(
            0x401000,
            "RenamedTarget".to_string(),
            fission_static::analysis::decomp::facts::FactProvenance::StrongFid,
        );

        let context = build_nir_type_context_from_facts(&binary, &facts, 0x401000);
        assert_eq!(
            context.call_targets.get(&0x401000).map(String::as_str),
            Some("RenamedTarget")
        );
    }

    #[test]
    fn preview_context_builder_preserves_call_param_rules() {
        let binary = LoadedBinaryBuilder::new("sample.exe".to_string(), DataBuffer::Heap(vec![]))
            .format("PE")
            .is_64bit(true)
            .build()
            .expect("build test binary");
        let facts = FactStore::from_binary(&binary);
        let context = build_nir_type_context_from_facts(&binary, &facts, 0);

        assert!(context.call_param_rules.iter().any(|rule| {
            rule.callee_name == "GetWindowRect"
                && !rule.pointer_alias.is_empty()
                && !rule.pointee_alias.is_empty()
                && !rule.pointee_sizes.is_empty()
        }));
    }

    #[test]
    fn make_nir_request_reuses_external_type_context() {
        let binary = LoadedBinaryBuilder::new("sample.exe".to_string(), DataBuffer::Heap(vec![]))
            .format("PE")
            .is_64bit(true)
            .build()
            .expect("build test binary");
        let type_context = NirTypeContext {
            call_targets: HashMap::from([(0x401000, "KnownName".to_string())]),
            call_target_refs: HashMap::new(),
            call_param_rules: vec![PreviewCallParamRule {
                callee_address: None,
                callee_name: "MessageBoxW".to_string(),
                arg_index: 1,
                pointer_alias: "LPCWSTR".to_string(),
                pointee_alias: "WCHAR".to_string(),
                pointer_size: 8,
                pointee_sizes: vec![2],
            }],
            function_hints: None,
        };

        let request = make_nir_request(
            "{}",
            0x401000,
            "sub_401000",
            NirRenderOptions::from_loaded_binary(&binary),
            type_context,
        );
        assert_eq!(
            request
                .type_context
                .call_targets
                .get(&0x401000)
                .map(String::as_str),
            Some("KnownName")
        );
    }

    #[test]
    fn sanitize_preview_symbol_name_strips_import_prefixes_and_suffixes() {
        assert_eq!(sanitize_nir_symbol_name("__imp_MessageBoxW"), "MessageBoxW");
        assert_eq!(sanitize_nir_symbol_name("foo [import]"), "foo");
    }

    #[test]
    fn select_nir_output_wrapper_keeps_legacy_mode_behavior() {
        let binary = LoadedBinaryBuilder::new("sample.exe".to_string(), DataBuffer::Heap(vec![]))
            .format("PE")
            .is_64bit(true)
            .build()
            .expect("build test binary");
        let mut source = MockNirSource;

        let selection = select_nir_output(
            &mut source,
            &binary,
            0x401000,
            "sub_401000",
            NirEngineMode::Legacy,
            None,
        )
        .expect("legacy preview selection");

        assert_eq!(selection.engine_used, NirEngineMode::Legacy);
        assert!(!selection.fell_back);
        assert!(selection.nir_code.is_none());
    }

    #[test]
    fn select_nir_output_from_pcode_wrapper_keeps_legacy_mode_behavior() {
        let binary = LoadedBinaryBuilder::new("sample.exe".to_string(), DataBuffer::Heap(vec![]))
            .format("PE")
            .is_64bit(true)
            .build()
            .expect("build test binary");
        let pcode = PcodeFunction { blocks: vec![] };

        let selection = select_nir_output_from_pcode(
            &pcode,
            &binary,
            0x401000,
            "sub_401000",
            NirEngineMode::Legacy,
            None,
            NirRenderOptions::from_loaded_binary(&binary),
        )
        .expect("legacy preview selection from pcode");

        assert_eq!(selection.engine_used, NirEngineMode::Legacy);
        assert!(!selection.fell_back);
        assert!(selection.nir_code.is_none());
    }

    #[test]
    fn select_nir_output_from_pcode_auto_gate_falls_back_for_non_pe_binary() {
        let binary = LoadedBinaryBuilder::new("sample.bin".to_string(), DataBuffer::Heap(vec![]))
            .format("ELF")
            .is_64bit(true)
            .build()
            .expect("build test binary");
        let pcode = PcodeFunction { blocks: vec![] };

        let selection = select_nir_output_from_pcode(
            &pcode,
            &binary,
            0x401000,
            "sub_401000",
            NirEngineMode::Auto,
            None,
            NirRenderOptions::from_loaded_binary(&binary),
        )
        .expect("auto preview selection from pcode");

        assert_eq!(selection.engine_used, NirEngineMode::Legacy);
        assert!(selection.fell_back);
        assert!(selection.nir_code.is_none());
    }

    #[test]
    fn rescue_nir_output_with_facts_ignores_non_type_failures() {
        let binary = LoadedBinaryBuilder::new("sample.exe".to_string(), DataBuffer::Heap(vec![]))
            .format("PE")
            .is_64bit(true)
            .build()
            .expect("build test binary");
        let facts = FactStore::from_binary(&binary);
        let mut source = MockNirSource;

        let selection = rescue_nir_output_with_facts(
            &mut source,
            &binary,
            &facts,
            0x401000,
            "sub_401000",
            "some unrelated error",
            None,
        )
        .expect("rescue helper");

        assert!(selection.is_none());
    }

    #[test]
    fn preview_request_carries_function_scoped_hints_from_dwarf_facts() {
        let mut binary =
            LoadedBinaryBuilder::new("sample.exe".to_string(), DataBuffer::Heap(vec![]))
                .format("PE")
                .is_64bit(true)
                .add_function(FunctionInfo {
                    name: "sub_401000".to_string(),
                    address: 0x401000,
                    size: 0,
                    is_export: false,
                    is_import: false,
                })
                .build()
                .expect("build test binary");
        binary.dwarf_functions.insert(
            0x401000,
            DwarfFunctionInfo {
                address: 0x401000,
                name: "KnownName".to_string(),
                return_type: Some("BOOL".to_string()),
                params: vec![DwarfParamInfo {
                    name: "hwnd".to_string(),
                    type_name: "HWND".to_string(),
                    location: DwarfLocation::Register("RCX".to_string()),
                }],
                local_vars: vec![DwarfLocalVar {
                    name: "rect".to_string(),
                    type_name: "RECT".to_string(),
                    location: DwarfLocation::StackOffset(-0x20),
                }],
            },
        );
        let facts = FactStore::from_binary(&binary);
        let type_context = build_nir_type_context_from_facts(&binary, &facts, 0x401000);
        let request = make_nir_request(
            "{}",
            0x401000,
            "sub_401000",
            NirRenderOptions::from_loaded_binary(&binary),
            type_context,
        );

        let hints = request
            .type_context
            .function_hints
            .as_ref()
            .expect("function-scoped preview hints");
        assert_eq!(hints.param_names, vec!["hwnd".to_string()]);
        assert_eq!(
            hints.stack_local_names.get(&-0x20).map(String::as_str),
            Some("rect")
        );
        assert_eq!(hints.return_type_name.as_deref(), Some("BOOL"));
    }
}
