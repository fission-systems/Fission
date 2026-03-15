use fission_loader::loader::LoadedBinary;
use fission_pcode::{
    MlilPreviewOptions, PcodeFunction, PcodeOpcode, PcodeOptimizer, PcodeOptimizerConfig,
    PreviewCallParamRule, PreviewTypeContext, render_mlil_preview_with_context,
};
use fission_signatures::WIN_API_DB;
use fission_signatures::win_types::WindowsStructures;
use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewEngineMode {
    Legacy,
    MlilPreview,
    Auto,
}

impl PreviewEngineMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            PreviewEngineMode::Legacy => "legacy",
            PreviewEngineMode::MlilPreview => "mlil_preview",
            PreviewEngineMode::Auto => "auto",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewSelection {
    pub preview_code: Option<String>,
    pub engine_used: PreviewEngineMode,
    pub fell_back: bool,
    pub fallback_reason: Option<String>,
}

fn is_type_failure_for_preview_rescue(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.contains("duplicate variablepiece")
        || lower.contains("ptrsub")
        || lower.contains("non structured pointer type")
        || lower.contains("struct")
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
    pcode
        .blocks
        .iter()
        .flat_map(|block| block.ops.iter())
        .any(|op| matches!(op.opcode, PcodeOpcode::CallInd | PcodeOpcode::BranchInd))
}

pub fn auto_mlil_eligible(binary: &LoadedBinary, pcode: &PcodeFunction) -> bool {
    binary.is_64bit
        && binary.format.eq_ignore_ascii_case("PE")
        && pcode.blocks.len() <= 12
        && pcode_total_ops(pcode) <= 600
        && !contains_indirect_control_flow(pcode)
        && max_multiequal_fanin(pcode) <= 4
}

fn sanitize_preview_symbol_name(name: &str) -> String {
    let mut sanitized = name.trim().to_string();
    if let Some((_, tail)) = sanitized.rsplit_once('!') {
        sanitized = tail.trim().to_string();
    }
    if let Some(stripped) = sanitized.strip_prefix("__imp_") {
        sanitized = stripped.trim().to_string();
    }
    for suffix in [" [import]", " [export]"] {
        if let Some(stripped) = sanitized.strip_suffix(suffix) {
            sanitized = stripped.trim_end().to_string();
        }
    }
    sanitized
}

fn build_preview_type_context(binary: &LoadedBinary) -> PreviewTypeContext {
    let structures = WindowsStructures::new();
    let mut call_targets = HashMap::new();
    for func in &binary.functions {
        if func.address == 0 || func.name.is_empty() {
            continue;
        }
        call_targets
            .entry(func.address)
            .or_insert_with(|| sanitize_preview_symbol_name(&func.name));
    }
    for (addr, name) in &binary.inner().iat_symbols {
        if *addr == 0 || name.is_empty() {
            continue;
        }
        call_targets
            .entry(*addr)
            .or_insert_with(|| sanitize_preview_symbol_name(name));
    }
    for (addr, name) in &binary.inner().global_symbols {
        if *addr == 0 || name.is_empty() {
            continue;
        }
        call_targets
            .entry(*addr)
            .or_insert_with(|| sanitize_preview_symbol_name(name));
    }

    let mut call_param_rules = Vec::new();
    for sig in WIN_API_DB.iter() {
        for (arg_index, param) in sig.params.iter().enumerate() {
            let Some(struct_name) = resolve_preview_struct_name(&param.type_name, &structures)
            else {
                continue;
            };
            let Some(struct_def) = structures.get(&struct_name) else {
                continue;
            };
            if struct_def.size_64 == 0 {
                continue;
            }
            call_param_rules.push(PreviewCallParamRule {
                callee_name: sig.name.clone(),
                arg_index,
                pointer_alias: param.type_name.clone(),
                pointee_alias: struct_name,
                pointer_size: 8,
                pointee_sizes: vec![struct_def.size_64 as u32],
            });
        }
    }

    PreviewTypeContext {
        call_targets,
        call_param_rules,
    }
}

fn resolve_preview_struct_name(type_name: &str, structures: &WindowsStructures) -> Option<String> {
    if type_name.contains('*') {
        return None;
    }
    for prefix in ["LP", "P"] {
        let Some(candidate) = type_name.strip_prefix(prefix) else {
            continue;
        };
        if structures.get(candidate).is_some() {
            return Some(candidate.to_string());
        }
    }
    None
}

fn render_preview_from_json(
    pcode_json: &str,
    binary: &LoadedBinary,
    address: u64,
    name: &str,
    enforce_auto_gate: bool,
) -> Result<Option<String>, String> {
    if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
        let _ = std::fs::write(format!("/tmp/fission_preview_{address:x}.json"), pcode_json);
    }
    let mut pcode = PcodeFunction::from_json(pcode_json)
        .map_err(|e| format!("mlil-preview pcode parse failed: {e}"))?;
    if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
        let mut debug_dump = String::new();
        debug_dump.push_str(&format!(
            "[mlil-preview] function=0x{address:x} blocks={} ops={}\n",
            pcode.blocks.len(),
            pcode.blocks.iter().map(|b| b.ops.len()).sum::<usize>()
        ));
        eprintln!(
            "[mlil-preview] function=0x{address:x} blocks={} ops={}",
            pcode.blocks.len(),
            pcode.blocks.iter().map(|b| b.ops.len()).sum::<usize>()
        );
        for block in &pcode.blocks {
            let term = block
                .ops
                .last()
                .map(|op| format!("{:?}@0x{:x}", op.opcode, op.address))
                .unwrap_or_else(|| "<none>".to_string());
            debug_dump.push_str(&format!(
                "[mlil-preview] block 0x{:x} ops={} term={}\n",
                block.start_address,
                block.ops.len(),
                term
            ));
            eprintln!(
                "[mlil-preview] block 0x{:x} ops={} term={}",
                block.start_address,
                block.ops.len(),
                term
            );
        }
        let _ = std::fs::write(format!("/tmp/fission_preview_{address:x}.log"), debug_dump);
    }
    if enforce_auto_gate && !auto_mlil_eligible(binary, &pcode) {
        return Ok(None);
    }
    if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(format!("/tmp/fission_preview_{address:x}.log"))
            .and_then(|mut f| {
                std::io::Write::write_all(&mut f, b"[mlil-preview] stage=before_optimize\n")
            });
    }
    let mut optimizer = PcodeOptimizer::new(PcodeOptimizerConfig::default());
    let optimize_result = catch_unwind(AssertUnwindSafe(|| optimizer.optimize(&mut pcode)));
    if optimize_result.is_err() && std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(format!("/tmp/fission_preview_{address:x}.log"))
            .and_then(|mut f| {
                std::io::Write::write_all(
                    &mut f,
                    b"[mlil-preview] stage=optimize_panicked_using_raw_pcode\n",
                )
            });
    }
    if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(format!("/tmp/fission_preview_{address:x}.log"))
            .and_then(|mut f| {
                std::io::Write::write_all(&mut f, b"[mlil-preview] stage=after_optimize\n")
            });
    }
    let options = MlilPreviewOptions::from_loaded_binary(binary);
    let type_context = build_preview_type_context(binary);
    if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(format!("/tmp/fission_preview_{address:x}.log"))
            .and_then(|mut f| {
                std::io::Write::write_all(&mut f, b"[mlil-preview] stage=before_render\n")
            });
    }
    match render_mlil_preview_with_context(&pcode, name, address, &options, Some(&type_context)) {
        Ok(code) => {
            if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
                let _ = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(format!("/tmp/fission_preview_{address:x}.log"))
                    .and_then(|mut f| {
                        std::io::Write::write_all(&mut f, b"[mlil-preview] stage=render_ok\n")
                    });
            }
            Ok(Some(code))
        }
        Err(err) => {
            if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
                let _ = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(format!("/tmp/fission_preview_{address:x}.log"))
                    .and_then(|mut f| {
                        std::io::Write::write_all(
                            &mut f,
                            format!("[mlil-preview] stage=render_error err={err}\n").as_bytes(),
                        )
                    });
            }
            Err(format!("mlil-preview unavailable: {err}"))
        }
    }
}

fn classify_preview_failure(reason: &str) -> &'static str {
    let lower = reason.to_ascii_lowercase();
    if lower.contains("unsupported architecture") || lower.contains("supports pe x64 only") {
        "unsupported_arch"
    } else if lower.contains("unsupported branch target") {
        "unsupported_cfg_branch_target"
    } else if lower.contains("unsupported region shape") {
        "unsupported_cfg_region_shape"
    } else if lower.contains("unsupported phi join") {
        "unsupported_cfg_phi_join"
    } else if lower.contains("unsupported indirect call region") {
        "unsupported_cfg_indirect_call_region"
    } else if lower.contains("unsupported control flow") {
        "unsupported_cfg"
    } else if lower.contains("multiequal") {
        "unsupported_expr_multiequal"
    } else if lower.contains("unsupported address materialization") {
        "unsupported_expr_address_materialization"
    } else if lower.contains("unsupported indirect value source") {
        "unsupported_expr_indirect_value_source"
    } else if lower.contains("unsupported piece/subpiece shape") {
        "unsupported_expr_piece_shape"
    } else if lower.contains("unsupported ptr arithmetic shape") {
        "unsupported_expr_ptr_arithmetic"
    } else if lower.contains("unsupported memory-backed varnode") {
        "unsupported_expr_memory_backed_varnode"
    } else if lower.contains("value lowering failed on varnode") {
        "unsupported_expr_varnode_lowering"
    } else if lower.contains("loop") || lower.contains("dowhile") || lower.contains("while") {
        "unsupported_loop_shape"
    } else if lower.contains("switch") {
        "unsupported_switch_shape"
    } else if lower.contains("ptr") || lower.contains("load") || lower.contains("store") {
        "unsupported_memory_pattern"
    } else if lower.contains("multiequal") || lower.contains("phi") {
        "unsupported_phi_merge"
    } else if lower.contains("call") {
        "unsupported_call_boundary"
    } else {
        "unsupported_expr"
    }
}

fn classified_preview_error(reason: &str) -> String {
    format!("{}: {}", classify_preview_failure(reason), reason)
}

pub fn select_preview_output<S: PreviewSource>(
    source: &mut S,
    binary: &LoadedBinary,
    address: u64,
    name: &str,
    mode: PreviewEngineMode,
) -> Result<PreviewSelection, String> {
    let diag = std::env::var_os("FISSION_PREVIEW_DIAG").is_some();
    match mode {
        PreviewEngineMode::Legacy => Ok(PreviewSelection {
            preview_code: None,
            engine_used: PreviewEngineMode::Legacy,
            fell_back: false,
            fallback_reason: None,
        }),
        PreviewEngineMode::MlilPreview => {
            let pcode_start = Instant::now();
            if diag {
                eprintln!("[PREVIEW-DIAG] get_pcode start: fn=0x{address:x} mode=mlil_preview");
            }
            let pcode_json = source.get_pcode_json(address).map_err(|e| e.to_string())?;
            if diag {
                eprintln!(
                    "[PREVIEW-DIAG] get_pcode done: fn=0x{address:x} mode=mlil_preview elapsed_ms={:.1}",
                    pcode_start.elapsed().as_secs_f64() * 1000.0
                );
            }
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
                    fallback_reason: Some(classified_preview_error(
                        "mlil-preview skipped: function not supported by preview builder",
                    )),
                }),
                Err(err) => Ok(PreviewSelection {
                    preview_code: None,
                    engine_used: PreviewEngineMode::Legacy,
                    fell_back: true,
                    fallback_reason: Some(classified_preview_error(&err)),
                }),
            }
        }
        PreviewEngineMode::Auto => {
            let pcode_start = Instant::now();
            if diag {
                eprintln!("[PREVIEW-DIAG] get_pcode start: fn=0x{address:x} mode=auto");
            }
            let pcode_json = source.get_pcode_json(address).map_err(|e| e.to_string())?;
            if diag {
                eprintln!(
                    "[PREVIEW-DIAG] get_pcode done: fn=0x{address:x} mode=auto elapsed_ms={:.1}",
                    pcode_start.elapsed().as_secs_f64() * 1000.0
                );
            }
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
                    fallback_reason: Some(classified_preview_error(&err)),
                }),
            }
        }
    }
}

pub fn rescue_preview_output<S: PreviewSource>(
    source: &mut S,
    binary: &LoadedBinary,
    address: u64,
    name: &str,
    error: &str,
) -> Result<Option<PreviewSelection>, String> {
    if !is_type_failure_for_preview_rescue(error) {
        return Ok(None);
    }

    let diag = std::env::var_os("FISSION_PREVIEW_DIAG").is_some();
    let pcode_start = Instant::now();
    if diag {
        eprintln!("[PREVIEW-DIAG] get_pcode start: fn=0x{address:x} mode=rescue");
    }
    let pcode_json = source.get_pcode_json(address).map_err(|e| e.to_string())?;
    if diag {
        eprintln!(
            "[PREVIEW-DIAG] get_pcode done: fn=0x{address:x} mode=rescue elapsed_ms={:.1}",
            pcode_start.elapsed().as_secs_f64() * 1000.0
        );
    }
    match render_preview_from_json(&pcode_json, binary, address, name, false) {
        Ok(Some(code)) => Ok(Some(PreviewSelection {
            preview_code: Some(code),
            engine_used: PreviewEngineMode::MlilPreview,
            fell_back: true,
            fallback_reason: Some(format!(
                "legacy type failure rescued by mlil-preview: {error}"
            )),
        })),
        Ok(None) => Ok(None),
        Err(_) => Ok(None),
    }
}
