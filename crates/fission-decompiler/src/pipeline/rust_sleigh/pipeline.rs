use crate::rust_sleigh::bounds::{clamp_to_available_execution, next_function_distance};
use crate::rust_sleigh::decode::{decode_rust_sleigh_pcode, pcode_op_count};
use crate::rust_sleigh::probe::probe_wrapper_contraction;
use crate::rust_sleigh::render_finish::{
    finish_rust_sleigh_render, should_retry_with_strict_indirect_stop,
};
use crate::rust_sleigh::{
    RustSleighDecompileConfig, RustSleighDecompileResult, RustSleighPipelineEvidence,
};
use fission_core::cancellation::{
    CancellationReason, CancellationToken, check_current_cancellation, current_cancellation_token,
    enter_cancellation_scope, is_current_cancelled,
};
use fission_loader::loader::LoadedBinary;
use fission_static::analysis::decomp::facts::FactStore;
use std::time::{Duration, Instant};

pub(crate) fn format_cancellation_error(
    reason: CancellationReason,
    stage: &str,
    timeout_ms: Option<u64>,
) -> String {
    match reason {
        CancellationReason::TimedOut => match timeout_ms.filter(|ms| *ms > 0) {
            Some(ms) => {
                format!("nir_timeout: Fission NIR pipeline timed out after {ms}ms ({stage})")
            }
            None => format!("nir_timeout: Fission NIR pipeline timed out ({stage})"),
        },
        CancellationReason::Cancelled => {
            format!("nir_cancelled: Fission NIR pipeline cancelled ({stage})")
        }
    }
}

fn check_pipeline_cancellation(stage: &str, timeout_ms: Option<u64>) -> Result<(), String> {
    check_current_cancellation()
        .map_err(|reason| format_cancellation_error(reason, stage, timeout_ms))
}

/// How many bytes and instructions to decode for the function at
/// `entry_address`.
fn decode_bounds(
    binary: &LoadedBinary,
    entry_address: u64,
    function_size: usize,
    config: &RustSleighDecompileConfig,
    max_function_size: Option<u32>,
    max_instructions: Option<u32>,
) -> (usize, usize) {
    // Absent a caller-supplied limit the limit is the *cap*, not the default
    // size. `default_decode_bytes` answers "how much to read when the
    // function's size is unknown" (that is `fallback_default_bytes`, below);
    // using it here answered "how much to read at most", which is a different
    // question with a 4x smaller answer. Every function over 16 KB was
    // silently cut to 16 KB -- `openssh` `process_config_line_depth` is
    // 19,586 bytes, and its jump table's first target lands at 0x1dae6, past
    // a window that ended at 0x1cf1d, so the table was read, believed, and
    // then rejected entry by entry.
    let max_bytes_limit = max_function_size
        .and_then(|v| usize::try_from(v).ok())
        .unwrap_or(config.decode_max_bytes_cap)
        .max(1)
        .min(config.decode_max_bytes_cap.max(1));
    let fallback_default_bytes = config.default_decode_bytes.max(1).min(max_bytes_limit);

    let max_bytes = if function_size > 0 {
        function_size.min(max_bytes_limit)
    } else if config.use_next_function_distance_if_unknown {
        next_function_distance(binary, entry_address)
            .map(|dist| dist.min(max_bytes_limit))
            .unwrap_or(fallback_default_bytes)
    } else {
        fallback_default_bytes
    }
    .max(1);
    let max_bytes = clamp_to_available_execution(binary, entry_address, max_bytes);

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

    (max_bytes, instruction_limit)
}

/// A function as the decompiler lifts it, before any normalization.
#[derive(Debug, Clone)]
pub struct LiftedFunction {
    pub entry_address: u64,
    pub pcode: crate::PcodeFunction,
    /// Decoder context at the entry; `None` is the language's default.
    pub context_override: Option<fission_sleigh::runtime::PackedContextOverride>,
}

/// Lift the function containing `address` exactly as a default decompile would,
/// and stop there. For read-only views that must agree with the decompiler
/// about which instructions the function has.
pub fn lift_function_pcode(binary: &LoadedBinary, address: u64) -> Result<LiftedFunction, String> {
    let config = RustSleighDecompileConfig::cli_defaults();
    let function = binary.function_at(address);
    let entry_address = function.map(|f| f.address).unwrap_or(address);
    let function_size = binary
        .function_at(entry_address)
        .map(|f| usize::try_from(f.size).unwrap_or(0))
        .unwrap_or(0);
    let (max_bytes, instruction_limit) =
        decode_bounds(binary, entry_address, function_size, &config, None, None);
    let pcode = crate::rust_sleigh::decode_rust_sleigh_pcode(
        binary,
        "lifted_function",
        entry_address,
        max_bytes,
        instruction_limit,
        config.continue_past_indirect_branch,
        config.retry_on_decode_error,
    )?;
    Ok(LiftedFunction {
        entry_address,
        pcode,
        context_override: crate::rust_sleigh::decode::entry_context_override(binary, entry_address),
    })
}

pub fn decompile_with_rust_sleigh(
    binary: &LoadedBinary,
    address: u64,
    name: &str,
    config: &RustSleighDecompileConfig,
    max_function_size: Option<u32>,
    max_instructions: Option<u32>,
) -> Result<RustSleighDecompileResult, String> {
    let facts = FactStore::from_binary_without_signature_matches(binary);
    decompile_with_rust_sleigh_with_facts(
        binary,
        &facts,
        address,
        name,
        config,
        max_function_size,
        max_instructions,
    )
}

pub fn decompile_with_rust_sleigh_with_facts(
    binary: &LoadedBinary,
    facts: &FactStore,
    address: u64,
    name: &str,
    config: &RustSleighDecompileConfig,
    max_function_size: Option<u32>,
    max_instructions: Option<u32>,
) -> Result<RustSleighDecompileResult, String> {
    let start = Instant::now();
    let stage_timeout = config.nir_timeout_ms.filter(|&ms| ms > 0);
    let timeout_duration = stage_timeout.map(Duration::from_millis);
    let request_token = match current_cancellation_token() {
        Some(parent) => parent.child(timeout_duration),
        None => CancellationToken::with_deadline(timeout_duration.map(|d| start + d)),
    };
    let _cancel_guard = enter_cancellation_scope(request_token);
    check_pipeline_cancellation("before_setup", stage_timeout)?;

    let entry_address = binary
        .function_at(address)
        .map(|f| f.address)
        .unwrap_or(address);
    let function_size = binary
        .function_at(entry_address)
        .map(|f| usize::try_from(f.size).unwrap_or(0))
        .unwrap_or(0);

    let (max_bytes, instruction_limit) = decode_bounds(
        binary,
        entry_address,
        function_size,
        config,
        max_function_size,
        max_instructions,
    );

    let mut evidence = RustSleighPipelineEvidence::new(entry_address, max_bytes, instruction_limit);
    evidence.wrapper_probe_attempted = config.enable_wrapper_contraction_probe;

    check_pipeline_cancellation("before_wrapper_probe", stage_timeout)?;
    if let Some(mut summary_result) =
        probe_wrapper_contraction(binary, entry_address, name, function_size, config, start)?
    {
        check_pipeline_cancellation("after_wrapper_probe", stage_timeout)?;
        evidence.wrapper_probe_matched = true;
        evidence
            .pipeline_stage_status
            .insert("wrapper_probe".into(), "matched_contracted_summary".into());
        summary_result.evidence = evidence;
        return Ok(summary_result);
    }

    // Only offered when our own contract kind will match
    // (`continue_past_indirect_branch` => `DecodeContract::decomp_function`,
    // same as FID matching uses) -- see `FactStore::get_cached_decoded_
    // function`'s doc comment for why a `strict_function` request must not
    // reuse this cache.
    check_pipeline_cancellation("before_decode", stage_timeout)?;
    let cached_decoded = config
        .continue_past_indirect_branch
        .then(|| facts.get_cached_decoded_function(entry_address))
        .flatten();
    let (pcode, diag, userops) = match decode_rust_sleigh_pcode(
        binary,
        name,
        entry_address,
        max_bytes,
        instruction_limit,
        config.continue_past_indirect_branch,
        config.retry_on_decode_error,
        cached_decoded,
    ) {
        Ok(ok) => ok,
        Err(fail) => {
            evidence.decode_attempt_count = fail.diag.attempts;
            evidence.decode_stop_reason = fail.diag.stop_reason.clone();
            evidence
                .pipeline_stage_status
                .insert("decode".into(), "failed".into());
            return Err(fail.message);
        }
    };
    check_pipeline_cancellation("after_decode", stage_timeout)?;

    evidence.decode_attempt_count = diag.attempts;
    evidence.decode_stop_reason = diag.stop_reason.clone();
    evidence.template_source_counts = diag.template_source_counts.clone();
    evidence.raw_pcode_op_count = Some(pcode_op_count(&pcode));
    evidence.observe_pcode(&pcode);
    evidence
        .pipeline_stage_status
        .insert("decode".into(), "ok".into());

    match finish_rust_sleigh_render(
        binary,
        facts,
        entry_address,
        name,
        config,
        &pcode,
        userops.clone(),
        &mut evidence,
    ) {
        Ok(result) => {
            check_pipeline_cancellation("after_nir_render", stage_timeout)?;
            Ok(result)
        }
        Err(err)
            if config.continue_past_indirect_branch
                && !is_current_cancelled()
                && !err.contains("nir_timeout")
                && !err.contains("nir_cancelled")
                && should_retry_with_strict_indirect_stop(&err) =>
        {
            check_pipeline_cancellation("before_strict_retry", stage_timeout)?;
            evidence.strict_indirect_retry_attempted = true;
            evidence
                .pipeline_stage_status
                .insert("nir_render".into(), "retry_strict_decode".into());

            let (strict_pcode, diag2, strict_userops) = match decode_rust_sleigh_pcode(
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
                None,
            ) {
                Ok(ok) => ok,
                Err(fail) => {
                    evidence.decode_attempt_count =
                        diag.attempts.saturating_add(fail.diag.attempts);
                    evidence.decode_stop_reason = fail.diag.stop_reason.clone();
                    evidence
                        .pipeline_stage_status
                        .insert("decode_strict_retry".into(), "failed".into());
                    return Err(fail.message);
                }
            };

            evidence.decode_attempt_count = diag.attempts.saturating_add(diag2.attempts);
            evidence.decode_stop_reason = diag2.stop_reason.clone();
            evidence.template_source_counts = diag2.template_source_counts.clone();
            evidence.raw_pcode_op_count = Some(pcode_op_count(&strict_pcode));
            evidence.observe_pcode(&strict_pcode);
            evidence
                .pipeline_stage_status
                .insert("decode_strict_retry".into(), "ok".into());

            finish_rust_sleigh_render(
                binary,
                facts,
                entry_address,
                name,
                config,
                &strict_pcode,
                strict_userops,
                &mut evidence,
            )
        }
        Err(err) => Err(err),
    }
}
