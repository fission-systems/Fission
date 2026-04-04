use fission_loader::loader::LoadedBinary;
use fission_pcode::{NirRenderOptions, PcodeFunction};

#[cfg(feature = "native_decomp")]
pub use fission_static::analysis::decomp::{NirEngineMode, NirSelection};

#[cfg(feature = "native_decomp")]
pub fn select_nir_output_from_prebuilt_pcode(
    pcode: &PcodeFunction,
    binary: &LoadedBinary,
    address: u64,
    name: &str,
    mode: NirEngineMode,
    timeout_ms: Option<u64>,
    options: NirRenderOptions,
) -> Result<NirSelection, String> {
    let fact_store = fission_static::analysis::decomp::FactStore::from_binary(binary);
    fission_static::analysis::decomp::select_nir_output_from_pcode_with_facts(
        pcode,
        binary,
        &fact_store,
        address,
        name,
        mode,
        timeout_ms,
        options,
    )
}

#[cfg(all(test, feature = "native_decomp"))]
mod tests {
    use super::*;
    use fission_loader::loader::{DataBuffer, LoadedBinaryBuilder};

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
