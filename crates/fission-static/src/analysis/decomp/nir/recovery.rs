use super::nir_render::{
    render_nir_from_json_with_type_context, render_nir_from_pcode_with_type_context_and_options,
};
use super::nir_taxonomy::{classify_nir_failure_refined, structuring_failure_signature};
use super::nir_types::{NirRoutingResolver, NirSelection};
use fission_loader::loader::LoadedBinary;
use fission_pcode::{
    NirBuildStats, NirRenderOptions, NirTypeContext, PcodeFunction, RecoveryMode,
    structuring_outcome_for_signature, take_last_nir_build_stats,
};

const RECOVERY_STRATEGY_LINEAR_STRUCTURING_RETRY: &str = "linearized_structuring_retry";

fn merge_optional_build_stats(
    primary: Option<NirBuildStats>,
    secondary: Option<NirBuildStats>,
) -> Option<NirBuildStats> {
    match (primary, secondary) {
        (Some(mut primary), Some(secondary)) => {
            primary.merge_assign(&secondary);
            Some(primary)
        }
        (None, Some(secondary)) => Some(secondary),
        (primary, None) => primary,
    }
}

pub(crate) fn is_type_failure_for_nir_rescue(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.contains("duplicate variablepiece")
        || lower.contains("ptrsub")
        || lower.contains("non structured pointer type")
        || lower.contains("struct")
}

pub(crate) fn try_structuring_recovery(
    pcode_json: &str,
    binary: &LoadedBinary,
    address: u64,
    name: &str,
    timeout_ms: Option<u64>,
    type_context: NirTypeContext,
    error: &str,
) -> Result<Option<NirSelection>, String> {
    if classify_nir_failure_refined(error) != "nir_structuring_failure" {
        return Ok(None);
    }
    let Some(signature) = structuring_failure_signature(error) else {
        return Ok(None);
    };
    let Some(outcome) = structuring_outcome_for_signature(signature) else {
        return Ok(None);
    };

    let region_retry = render_nir_from_json_with_type_context(
        pcode_json,
        binary,
        address,
        name,
        false,
        timeout_ms,
        type_context.clone(),
        true,
        false,
    );
    let region_retry_build_stats = match region_retry {
        Ok(Some((code, build_stats, hint_stats))) => {
            return Ok(Some(NirRoutingResolver::nir_success_with_recovery(
                code,
                build_stats,
                hint_stats,
                RECOVERY_STRATEGY_LINEAR_STRUCTURING_RETRY,
                RECOVERY_STRATEGY_LINEAR_STRUCTURING_RETRY,
                "recovered",
                Some(signature.to_string()),
                "region_linearized",
                outcome.reason_family.as_str(),
                outcome.retryable,
            )));
        }
        Ok(None) | Err(_) => take_last_nir_build_stats(),
    };

    match render_nir_from_json_with_type_context(
        pcode_json,
        binary,
        address,
        name,
        false,
        timeout_ms,
        type_context,
        false,
        true,
    ) {
        Ok(Some((code, build_stats, hint_stats))) => {
            let merged_build_stats =
                merge_optional_build_stats(build_stats, region_retry_build_stats);
            let mode = match outcome.mode {
                RecoveryMode::Structured => "structured",
                RecoveryMode::RegionLinearized => "region_linearized",
                RecoveryMode::ForcedLinear => "forced_linear",
            };
            Ok(Some(NirRoutingResolver::nir_success_with_recovery(
                code,
                merged_build_stats,
                hint_stats,
                RECOVERY_STRATEGY_LINEAR_STRUCTURING_RETRY,
                RECOVERY_STRATEGY_LINEAR_STRUCTURING_RETRY,
                "recovered",
                Some(signature.to_string()),
                mode,
                outcome.reason_family.as_str(),
                outcome.retryable,
            )))
        }
        Ok(None) => Ok(Some(NirRoutingResolver::nir_fallback_with_recovery(
            error,
            RECOVERY_STRATEGY_LINEAR_STRUCTURING_RETRY,
            None,
            "retry_skipped",
            Some(signature.to_string()),
            Some("forced_linear"),
            Some(outcome.reason_family.as_str()),
            Some(outcome.retryable),
        ))),
        Err(retry_err) => Ok(Some(NirRoutingResolver::nir_fallback_with_recovery(
            format!("{error}; recovery failed: {retry_err}"),
            RECOVERY_STRATEGY_LINEAR_STRUCTURING_RETRY,
            None,
            "retry_failed",
            Some(signature.to_string()),
            Some("forced_linear"),
            Some(outcome.reason_family.as_str()),
            Some(outcome.retryable),
        ))),
    }
}

pub(crate) fn try_structuring_recovery_from_pcode(
    pcode: &PcodeFunction,
    binary: &LoadedBinary,
    address: u64,
    name: &str,
    timeout_ms: Option<u64>,
    type_context: NirTypeContext,
    base_options: NirRenderOptions,
    error: &str,
) -> Result<Option<NirSelection>, String> {
    if classify_nir_failure_refined(error) != "nir_structuring_failure" {
        return Ok(None);
    }
    let Some(signature) = structuring_failure_signature(error) else {
        return Ok(None);
    };
    let Some(outcome) = structuring_outcome_for_signature(signature) else {
        return Ok(None);
    };

    let region_retry = render_nir_from_pcode_with_type_context_and_options(
        pcode,
        binary,
        address,
        name,
        false,
        timeout_ms,
        type_context.clone(),
        base_options.clone(),
        true,
        false,
    );
    let region_retry_build_stats = match region_retry {
        Ok(Some((code, build_stats, hint_stats))) => {
            return Ok(Some(NirRoutingResolver::nir_success_with_recovery(
                code,
                build_stats,
                hint_stats,
                RECOVERY_STRATEGY_LINEAR_STRUCTURING_RETRY,
                RECOVERY_STRATEGY_LINEAR_STRUCTURING_RETRY,
                "recovered",
                Some(signature.to_string()),
                "region_linearized",
                outcome.reason_family.as_str(),
                outcome.retryable,
            )));
        }
        Ok(None) | Err(_) => take_last_nir_build_stats(),
    };

    match render_nir_from_pcode_with_type_context_and_options(
        pcode,
        binary,
        address,
        name,
        false,
        timeout_ms,
        type_context,
        base_options,
        false,
        true,
    ) {
        Ok(Some((code, build_stats, hint_stats))) => {
            let merged_build_stats =
                merge_optional_build_stats(build_stats, region_retry_build_stats);
            let mode = match outcome.mode {
                RecoveryMode::Structured => "structured",
                RecoveryMode::RegionLinearized => "region_linearized",
                RecoveryMode::ForcedLinear => "forced_linear",
            };
            Ok(Some(NirRoutingResolver::nir_success_with_recovery(
                code,
                merged_build_stats,
                hint_stats,
                RECOVERY_STRATEGY_LINEAR_STRUCTURING_RETRY,
                RECOVERY_STRATEGY_LINEAR_STRUCTURING_RETRY,
                "recovered",
                Some(signature.to_string()),
                mode,
                outcome.reason_family.as_str(),
                outcome.retryable,
            )))
        }
        Ok(None) => Ok(Some(NirRoutingResolver::nir_fallback_with_recovery(
            error,
            RECOVERY_STRATEGY_LINEAR_STRUCTURING_RETRY,
            None,
            "retry_skipped",
            Some(signature.to_string()),
            Some("forced_linear"),
            Some(outcome.reason_family.as_str()),
            Some(outcome.retryable),
        ))),
        Err(retry_err) => Ok(Some(NirRoutingResolver::nir_fallback_with_recovery(
            format!("{error}; recovery failed: {retry_err}"),
            RECOVERY_STRATEGY_LINEAR_STRUCTURING_RETRY,
            None,
            "retry_failed",
            Some(signature.to_string()),
            Some("forced_linear"),
            Some(outcome.reason_family.as_str()),
            Some(outcome.retryable),
        ))),
    }
}

pub use super::nir_types::{PreviewRoutingDecision, PreviewSelection};
