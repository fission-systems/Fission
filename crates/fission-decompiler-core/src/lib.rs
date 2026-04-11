use fission_loader::loader::LoadedBinary;
use fission_pcode::{NirBuildStats, NirHintStats, NirRenderOptions, PcodeFunction, Varnode};
use fission_sleigh::lifter::{LiftDecodeContract, SleighLifter};

pub use fission_static::analysis::decomp::{NirEngineMode, NirSelection};

#[derive(Debug, Clone)]
pub struct RustSleighDecompileConfig {
    pub decode_max_bytes_cap: usize,
    pub default_decode_bytes: usize,
    pub instruction_budget_cap: usize,
    pub instruction_budget_default: usize,
    pub continue_past_indirect_branch: bool,
    pub retry_on_decode_error: bool,
    pub use_next_function_distance_if_unknown: bool,
    pub nir_mode: NirEngineMode,
    pub nir_timeout_ms: Option<u64>,
    pub pe_x64_only: bool,
    pub conservative_irreducible_fallback: bool,
}

impl RustSleighDecompileConfig {
    /// Default Rust-Sleigh + NIR pipeline configuration.
    ///
    /// Used by both [`crate::decompile_with_rust_sleigh`] call sites (CLI and desktop) so lift/decode
    /// limits match for the same binary and address.
    pub fn cli_defaults() -> Self {
        Self {
            decode_max_bytes_cap: 0x10000,
            default_decode_bytes: 0x4000,
            instruction_budget_cap: 4096,
            instruction_budget_default: 512,
            continue_past_indirect_branch: true,
            retry_on_decode_error: true,
            use_next_function_distance_if_unknown: true,
            nir_mode: NirEngineMode::Nir,
            nir_timeout_ms: None,
            pe_x64_only: false,
            conservative_irreducible_fallback: true,
        }
    }
}

impl Default for RustSleighDecompileConfig {
    fn default() -> Self {
        Self::cli_defaults()
    }
}

#[derive(Debug, Clone)]
pub struct RustSleighDecompileResult {
    pub code: String,
    pub fell_back: bool,
    pub fallback_reason: Option<String>,
    pub build_stats: Option<NirBuildStats>,
    pub hint_stats: Option<NirHintStats>,
}

fn sleigh_language_for_arch_spec(arch_spec: &str) -> Option<&'static str> {
    if arch_spec.starts_with("AARCH64:LE:64") && arch_spec.contains("AppleSilicon") {
        return Some("AARCH64_AppleSilicon");
    }
    if arch_spec.starts_with("AARCH64:LE:64") {
        return Some("AARCH64");
    }
    if arch_spec.starts_with("AARCH64:BE:64") {
        return Some("AARCH64BE");
    }
    if arch_spec.starts_with("x86:LE:64") {
        return Some("x86-64");
    }
    if arch_spec.starts_with("x86:LE:32") || arch_spec.starts_with("x86:LE:16") {
        return Some("x86");
    }
    None
}

fn extract_safe_bytes_from_decode_error(err: &str, func_addr: u64) -> Option<usize> {
    let marker = "decode failed at 0x";
    let idx = err.find(marker)?;
    let hex_start = idx + marker.len();
    let hex_end = err[hex_start..]
        .find(|c: char| !c.is_ascii_hexdigit())
        .map(|i| hex_start + i)
        .unwrap_or(err.len());
    let fail_addr = u64::from_str_radix(&err[hex_start..hex_end], 16).ok()?;
    let safe = fail_addr.checked_sub(func_addr)? as usize;
    if safe == 0 { None } else { Some(safe) }
}

fn decode_rust_sleigh_pcode(
    binary: &LoadedBinary,
    name: &str,
    entry_address: u64,
    max_bytes: usize,
    instruction_limit: usize,
    continue_past_indirect_branch: bool,
    retry_on_decode_error: bool,
) -> Result<PcodeFunction, String> {
    let bytes = binary.view_bytes(entry_address, max_bytes).ok_or_else(|| {
        format!("rust_sleigh: unable to read bytes at 0x{entry_address:x} for {name}")
    })?;

    let language = sleigh_language_for_arch_spec(&binary.arch_spec)
        .ok_or_else(|| format!("rust_sleigh: unsupported arch_spec '{}'", binary.arch_spec))?;

    let lifter =
        SleighLifter::new_for_language(language).map_err(|e| format!("rust_sleigh: {e:#}"))?;

    let lift_contract = if continue_past_indirect_branch {
        LiftDecodeContract::decomp_function(instruction_limit)
    } else {
        LiftDecodeContract::strict_function(instruction_limit)
    };
    let result =
        lifter.lift_raw_pcode_function_with_decode_contract(&bytes, entry_address, lift_contract);
    match result {
        Ok(lifted) => Ok(lifted.function),
        Err(first_err) => {
            if retry_on_decode_error {
                let err_str = format!("{first_err:#}");
                if let Some(safe) = extract_safe_bytes_from_decode_error(&err_str, entry_address) {
                    if safe > 0 && safe < bytes.len() {
                        if let Ok(retry) = lifter.lift_raw_pcode_function_with_decode_contract(
                            &bytes[..safe],
                            entry_address,
                            lift_contract,
                        ) {
                            return Ok(retry.function);
                        }
                    }
                }
            }
            Err(format!(
                "rust_sleigh: function lift failed for {name} at 0x{entry_address:x}: {first_err:#}"
            ))
        }
    }
}

fn format_varnode_for_pcode(vn: &Varnode) -> String {
    if vn.is_constant {
        format!("const(0x{:x}:{})", vn.constant_val as u64, vn.size)
    } else {
        format!(
            "v(space={},off=0x{:x},size={})",
            vn.space_id, vn.offset, vn.size
        )
    }
}

fn render_pcode_text(name: &str, pcode: &PcodeFunction) -> String {
    let mut out = String::new();
    out.push_str(&format!("// rust_sleigh direct pcode output: {name}\n"));
    for block in &pcode.blocks {
        out.push_str(&format!(
            "block_{} @ 0x{:x}\n",
            block.index, block.start_address
        ));
        for op in &block.ops {
            let out_vn = op
                .output
                .as_ref()
                .map(format_varnode_for_pcode)
                .unwrap_or_else(|| "-".to_string());
            let in_vn = op
                .inputs
                .iter()
                .map(format_varnode_for_pcode)
                .collect::<Vec<_>>()
                .join(", ");
            out.push_str(&format!(
                "  [{:04}] 0x{:x} {:?}  {} <- {}\n",
                op.seq_num, op.address, op.opcode, out_vn, in_vn
            ));
        }
    }
    out
}

fn should_retry_with_strict_indirect_stop(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.contains("unsupported pcode pattern")
        || lower.contains("nir_unsupported")
        || lower.contains("unsupported opcode")
}

fn finish_rust_sleigh_render(
    binary: &LoadedBinary,
    entry_address: u64,
    name: &str,
    config: &RustSleighDecompileConfig,
    pcode: &PcodeFunction,
) -> Result<RustSleighDecompileResult, String> {
    let mut options = NirRenderOptions::from_loaded_binary(binary);
    options.pe_x64_only = config.pe_x64_only;
    options.conservative_irreducible_fallback = config.conservative_irreducible_fallback;

    let selection = select_nir_output_from_prebuilt_pcode(
        pcode,
        binary,
        entry_address,
        name,
        config.nir_mode,
        config.nir_timeout_ms,
        options,
    )
    .map_err(|e| format!("rust_sleigh routing failed: {e}"))?;

    if let Some(code) = selection.nir_code {
        return Ok(RustSleighDecompileResult {
            code,
            fell_back: selection.fell_back,
            fallback_reason: selection.fallback_reason,
            build_stats: selection.build_stats,
            hint_stats: selection.hint_stats,
        });
    }

    let fallback_reason = selection.fallback_reason.unwrap_or_else(|| {
        "nir skipped: function not supported by Fission NIR builder".to_string()
    });
    let lower = fallback_reason.to_ascii_lowercase();
    let is_unsupported_arch = lower.contains("unsupported architecture in mlil-preview")
        || matches!(
            selection.fallback_kind_refined,
            Some("preview_architecture_unsupported")
        );
    if is_unsupported_arch {
        return Ok(RustSleighDecompileResult {
            code: render_pcode_text(name, pcode),
            fell_back: true,
            fallback_reason: Some("nir_unsupported_arch:pcode_dump".to_string()),
            build_stats: None,
            hint_stats: None,
        });
    }

    Err(format!("rust_sleigh render failed: {fallback_reason}"))
}

pub fn decompile_with_rust_sleigh(
    binary: &LoadedBinary,
    address: u64,
    name: &str,
    config: &RustSleighDecompileConfig,
    max_function_size: Option<u32>,
    max_instructions: Option<u32>,
) -> Result<RustSleighDecompileResult, String> {
    let entry_address = binary
        .function_at(address)
        .map(|f| f.address)
        .unwrap_or(address);
    let function_size = binary
        .function_at(entry_address)
        .map(|f| usize::try_from(f.size).unwrap_or(0))
        .unwrap_or(0);

    let max_bytes_limit = max_function_size
        .and_then(|v| usize::try_from(v).ok())
        .unwrap_or(config.default_decode_bytes)
        .max(1)
        .min(config.decode_max_bytes_cap.max(1));
    let fallback_default_bytes = config.default_decode_bytes.max(1).min(max_bytes_limit);

    let max_bytes = if function_size > 0 {
        function_size.min(max_bytes_limit)
    } else if config.use_next_function_distance_if_unknown {
        binary
            .function_after(entry_address)
            .and_then(|next| {
                let dist = next.address.saturating_sub(entry_address) as usize;
                if dist > 0 {
                    Some(dist.min(max_bytes_limit))
                } else {
                    None
                }
            })
            .unwrap_or(fallback_default_bytes)
    } else {
        fallback_default_bytes
    }
    .max(1);

    let default_instruction_limit = if config.continue_past_indirect_branch {
        config
            .instruction_budget_default
            .max(max_bytes.min(config.instruction_budget_cap.max(1)))
    } else {
        config.instruction_budget_default
    };
    let instruction_limit = max_instructions
        .and_then(|v| usize::try_from(v).ok())
        .unwrap_or(default_instruction_limit)
        .max(1)
        .min(config.instruction_budget_cap.max(1));

    let pcode = decode_rust_sleigh_pcode(
        binary,
        name,
        entry_address,
        max_bytes,
        instruction_limit,
        config.continue_past_indirect_branch,
        config.retry_on_decode_error,
    )?;
    match finish_rust_sleigh_render(binary, entry_address, name, config, &pcode) {
        Ok(result) => Ok(result),
        Err(err)
            if config.continue_past_indirect_branch
                && should_retry_with_strict_indirect_stop(&err) =>
        {
            let strict_pcode = decode_rust_sleigh_pcode(
                binary,
                name,
                entry_address,
                max_bytes,
                config
                    .instruction_budget_default
                    .max(1)
                    .min(config.instruction_budget_cap.max(1)),
                false,
                config.retry_on_decode_error,
            )?;
            finish_rust_sleigh_render(binary, entry_address, name, config, &strict_pcode)
        }
        Err(err) => Err(err),
    }
}

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

#[cfg(test)]
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
