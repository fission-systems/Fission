use fission_loader::loader::LoadedBinary;
use fission_pcode::{MlilPreviewOptions, PcodeFunction, PcodeOpcode, PcodeOptimizer, PcodeOptimizerConfig, render_mlil_preview};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewEngineMode {
    Legacy,
    MlilPreview,
    Auto,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewSelection {
    pub preview_code: Option<String>,
    pub engine_used: PreviewEngineMode,
    pub fell_back: bool,
    pub fallback_reason: Option<String>,
}

pub trait PreviewSource {
    fn get_pcode_json(&mut self, address: u64) -> fission_core::Result<String>;
}

impl PreviewSource for fission_ffi::DecompilerNative {
    fn get_pcode_json(&mut self, address: u64) -> fission_core::Result<String> {
        self.get_pcode(address)
    }
}

impl PreviewSource for crate::analysis::decomp::CachingDecompiler {
    fn get_pcode_json(&mut self, address: u64) -> fission_core::Result<String> {
        self.inner_mut().get_pcode(address)
    }
}

fn pcode_total_ops(pcode: &PcodeFunction) -> usize {
    pcode.blocks.iter().map(|block| block.ops.len()).sum()
}

fn max_multiequal_fanin(pcode: &PcodeFunction) -> usize {
    pcode
        .blocks
        .iter()
        .flat_map(|block| block.ops.iter())
        .filter(|op| op.opcode == PcodeOpcode::MultiEqual)
        .map(|op| op.inputs.len())
        .max()
        .unwrap_or(0)
}

fn contains_indirect_control_flow(pcode: &PcodeFunction) -> bool {
    pcode.blocks.iter().flat_map(|block| block.ops.iter()).any(|op| {
        matches!(op.opcode, PcodeOpcode::CallInd | PcodeOpcode::BranchInd)
    })
}

pub fn auto_mlil_eligible(binary: &LoadedBinary, pcode: &PcodeFunction) -> bool {
    binary.is_64bit
        && binary.format.eq_ignore_ascii_case("PE")
        && pcode.blocks.len() <= 8
        && pcode_total_ops(pcode) <= 400
        && !contains_indirect_control_flow(pcode)
        && max_multiequal_fanin(pcode) <= 4
}

fn render_preview_from_json(
    pcode_json: &str,
    binary: &LoadedBinary,
    address: u64,
    name: &str,
    enforce_auto_gate: bool,
) -> Result<Option<String>, String> {
    let mut pcode = PcodeFunction::from_json(pcode_json)
        .map_err(|e| format!("mlil-preview pcode parse failed: {e}"))?;
    if enforce_auto_gate && !auto_mlil_eligible(binary, &pcode) {
        return Ok(None);
    }
    let mut optimizer = PcodeOptimizer::new(PcodeOptimizerConfig::default());
    let _ = optimizer.optimize(&mut pcode);
    let options = MlilPreviewOptions::from_loaded_binary(binary);
    render_mlil_preview(&pcode, name, address, &options)
        .map(Some)
        .map_err(|e| format!("mlil-preview unavailable: {e}"))
}

pub fn select_preview_output<S: PreviewSource>(
    source: &mut S,
    binary: &LoadedBinary,
    address: u64,
    name: &str,
    mode: PreviewEngineMode,
) -> Result<PreviewSelection, String> {
    match mode {
        PreviewEngineMode::Legacy => Ok(PreviewSelection {
            preview_code: None,
            engine_used: PreviewEngineMode::Legacy,
            fell_back: false,
            fallback_reason: None,
        }),
        PreviewEngineMode::MlilPreview => {
            let pcode_json = source
                .get_pcode_json(address)
                .map_err(|e| e.to_string())?;
            match render_preview_from_json(&pcode_json, binary, address, name, false) {
                Ok(Some(code)) => Ok(PreviewSelection {
                    preview_code: Some(code),
                    engine_used: PreviewEngineMode::MlilPreview,
                    fell_back: false,
                    fallback_reason: None,
                }),
                Ok(None) => Ok(PreviewSelection {
                    preview_code: None,
                    engine_used: PreviewEngineMode::Legacy,
                    fell_back: true,
                    fallback_reason: Some(
                        "mlil-preview skipped: function not supported by preview builder"
                            .to_string(),
                    ),
                }),
                Err(err) => Ok(PreviewSelection {
                    preview_code: None,
                    engine_used: PreviewEngineMode::Legacy,
                    fell_back: true,
                    fallback_reason: Some(err),
                }),
            }
        }
        PreviewEngineMode::Auto => {
            let pcode_json = source
                .get_pcode_json(address)
                .map_err(|e| e.to_string())?;
            match render_preview_from_json(&pcode_json, binary, address, name, true) {
                Ok(Some(code)) => Ok(PreviewSelection {
                    preview_code: Some(code),
                    engine_used: PreviewEngineMode::MlilPreview,
                    fell_back: false,
                    fallback_reason: None,
                }),
                Ok(None) => Ok(PreviewSelection {
                    preview_code: None,
                    engine_used: PreviewEngineMode::Legacy,
                    fell_back: false,
                    fallback_reason: None,
                }),
                Err(err) => Ok(PreviewSelection {
                    preview_code: None,
                    engine_used: PreviewEngineMode::Legacy,
                    fell_back: true,
                    fallback_reason: Some(err),
                }),
            }
        }
    }
}
