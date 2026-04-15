use crate::{
    DecompileEngineMode, DecompileRoutingDecision, DecompileSelection, PostProcessOptions,
};
use fission_loader::loader::LoadedBinary;
use fission_pcode::{NirBuildStats, NirHintStats, NirRenderOptions, PcodeFunction};
use fission_static::analysis::decomp::facts::FactStore;

#[derive(Debug, Clone)]
pub struct DecompileRequest<'a> {
    pub binary: &'a LoadedBinary,
    pub fact_store: Option<&'a FactStore>,
    pub function_address: u64,
    pub function_name: Option<&'a str>,
    pub engine_mode: DecompileEngineMode,
    pub timeout_ms: Option<u64>,
    pub render_options: Option<NirRenderOptions>,
    pub postprocess_options: Option<PostProcessOptions>,
}

impl<'a> DecompileRequest<'a> {
    pub fn resolved_name(&self) -> String {
        self.function_name
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| {
                self.binary
                    .function_at(self.function_address)
                    .map(|func| func.name.clone())
                    .unwrap_or_else(|| format!("sub_{:x}", self.function_address))
            })
    }

    pub fn resolved_render_options(&self) -> NirRenderOptions {
        self.render_options
            .clone()
            .unwrap_or_else(|| NirRenderOptions::from_loaded_binary(self.binary))
    }
}

#[derive(Debug, Clone)]
pub struct DecompileResult {
    pub code: Option<String>,
    pub selection: DecompileSelection,
    pub routing: DecompileRoutingDecision,
    pub build_stats: Option<NirBuildStats>,
    pub hint_stats: Option<NirHintStats>,
    pub postprocess_applied: bool,
}

impl DecompileResult {
    pub fn from_selection(selection: DecompileSelection, postprocess_applied: bool) -> Self {
        let routing = selection.routing_decision();
        let build_stats = selection.build_stats.clone();
        let hint_stats = selection.hint_stats.clone();
        let code = selection.nir_code.clone();
        Self {
            code,
            selection,
            routing,
            build_stats,
            hint_stats,
            postprocess_applied,
        }
    }
}

pub fn decompile_prebuilt_pcode(
    pcode: &PcodeFunction,
    request: &DecompileRequest<'_>,
) -> Result<DecompileResult, String> {
    let selection = if let Some(fact_store) = request.fact_store {
        crate::select_nir_output_from_pcode_with_facts(
            pcode,
            request.binary,
            fact_store,
            request.function_address,
            &request.resolved_name(),
            request.engine_mode,
            request.timeout_ms,
            request.resolved_render_options(),
        )?
    } else {
        crate::select_nir_output_from_pcode(
            pcode,
            request.binary,
            request.function_address,
            &request.resolved_name(),
            request.engine_mode,
            request.timeout_ms,
            request.resolved_render_options(),
        )?
    };

    Ok(DecompileResult::from_selection(selection, false))
}
