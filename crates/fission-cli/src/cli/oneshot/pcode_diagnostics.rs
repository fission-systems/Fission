use super::debug_decomp::debug_decomp_bundle_json;
use fission_decompiler::{RustSleighDecompileConfig, decompile_with_rust_sleigh};
use fission_loader::loader::LoadedBinary;
use serde_json::Value;
use std::io;

pub(super) struct PcodeDiagnosticRun {
    pub bundle: Value,
    pub build_stats: Option<Value>,
    pub hint_stats: Option<Value>,
    pub pipeline: Value,
}

pub(super) fn run_pcode_diagnostic(
    binary: &LoadedBinary,
    addr: u64,
    max_bytes: usize,
    instruction_limit: usize,
    strict_indirect_stop: bool,
) -> io::Result<PcodeDiagnosticRun> {
    let func = binary.function_at(addr).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("no function found at or containing 0x{addr:x}"),
        )
    })?;

    let mut config = RustSleighDecompileConfig::cli_defaults();
    config.continue_past_indirect_branch = !strict_indirect_stop;

    let result = decompile_with_rust_sleigh(
        binary,
        func.address,
        &func.name,
        &config,
        Some(clamp_usize_to_u32(max_bytes)),
        Some(clamp_usize_to_u32(instruction_limit)),
    )
    .map_err(|err| io::Error::other(format!("Rust-Sleigh decompile failed: {err}")))?;

    let build_stats = result
        .build_stats
        .as_ref()
        .map(serde_json::to_value)
        .transpose()
        .map_err(|err| io::Error::other(format!("NirBuildStats serialization failed: {err}")))?;
    let hint_stats = result
        .hint_stats
        .as_ref()
        .map(serde_json::to_value)
        .transpose()
        .map_err(|err| io::Error::other(format!("NirHintStats serialization failed: {err}")))?;
    let pipeline = serde_json::to_value(&result.evidence).map_err(|err| {
        io::Error::other(format!("pipeline evidence serialization failed: {err}"))
    })?;

    let bundle = debug_decomp_bundle_json(
        binary,
        Some(addr),
        func,
        result.build_stats.as_ref(),
        result.hint_stats.as_ref(),
        Some(&result.evidence),
        None,
        false,
        result.build_stats.is_none(),
    );

    Ok(PcodeDiagnosticRun {
        bundle,
        build_stats,
        hint_stats,
        pipeline,
    })
}

pub(super) fn write_json(value: &Value) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    let output = serde_json::to_string_pretty(value)
        .map_err(|err| io::Error::other(format!("JSON serialization failed: {err}")))?;
    use std::io::Write;
    writeln!(stdout, "{output}")?;
    Ok(())
}

pub(super) fn json_scalar(value: &Value) -> String {
    value
        .as_str()
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| value.to_string())
}

fn clamp_usize_to_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX).max(1)
}
