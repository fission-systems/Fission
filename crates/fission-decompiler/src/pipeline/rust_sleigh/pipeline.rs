use crate::rust_sleigh::bounds::{clamp_to_available_execution, next_function_distance};
use crate::rust_sleigh::decode::{decode_rust_sleigh_pcode, pcode_op_count};
use crate::rust_sleigh::probe::probe_wrapper_contraction;
use crate::rust_sleigh::render_finish::{
    finish_rust_sleigh_render, should_retry_with_strict_indirect_stop,
};
use crate::rust_sleigh::{
    RustSleighDecompileConfig, RustSleighDecompileResult, RustSleighPipelineEvidence,
};
use fission_loader::loader::LoadedBinary;
use std::time::Instant;

pub fn decompile_with_rust_sleigh(
    binary: &LoadedBinary,
    address: u64,
    name: &str,
    config: &RustSleighDecompileConfig,
    max_function_size: Option<u32>,
    max_instructions: Option<u32>,
) -> Result<RustSleighDecompileResult, String> {
    let start = Instant::now();
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

    let mut evidence = RustSleighPipelineEvidence::new(entry_address, max_bytes, instruction_limit);
    evidence.wrapper_probe_attempted = config.enable_wrapper_contraction_probe;

    if let Some(mut summary_result) =
        probe_wrapper_contraction(binary, entry_address, name, function_size, config, start)?
    {
        evidence.wrapper_probe_matched = true;
        evidence
            .pipeline_stage_status
            .insert("wrapper_probe".into(), "matched_contracted_summary".into());
        summary_result.evidence = evidence;
        return Ok(summary_result);
    }

    let (pcode, diag, userops) = match decode_rust_sleigh_pcode(
        binary,
        name,
        entry_address,
        max_bytes,
        instruction_limit,
        config.continue_past_indirect_branch,
        config.retry_on_decode_error,
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

    evidence.decode_attempt_count = diag.attempts;
    evidence.decode_stop_reason = diag.stop_reason.clone();
    evidence.template_source_counts = diag.template_source_counts.clone();
    evidence.raw_pcode_op_count = Some(pcode_op_count(&pcode));
    evidence.observe_pcode(&pcode);
    evidence
        .pipeline_stage_status
        .insert("decode".into(), "ok".into());

    match finish_rust_sleigh_render(binary, entry_address, name, config, &pcode, userops.clone(), &mut evidence) {
        Ok(result) => Ok(result),
        Err(err)
            if config.continue_past_indirect_branch
                && should_retry_with_strict_indirect_stop(&err) =>
        {
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
