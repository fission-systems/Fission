use crate::StructuringFailureKind;

pub(crate) fn extract_fallback_kind(reason: Option<&str>) -> Option<&'static str> {
    let reason = reason?;
    let prefix = reason.split(':').next()?.trim().to_ascii_lowercase();
    match prefix.as_str() {
        "nir_timeout" | "preview_timeout" => Some("nir_timeout"),
        "nir_unsupported" | "preview_unsupported" => Some("nir_unsupported"),
        "native_pcode_failure" => Some("native_pcode_failure"),
        "legacy_fallback" => Some("legacy_fallback"),
        "assembly_fallback" => Some("assembly_fallback"),
        _ => None,
    }
}

pub(crate) fn extract_refined_fallback_kind(reason: Option<&str>) -> Option<&'static str> {
    let reason = reason?;
    match extract_fallback_kind(Some(reason)) {
        Some("nir_timeout") | Some("nir_unsupported") => Some(classify_nir_failure_refined(reason)),
        _ => None,
    }
}

pub(crate) fn structuring_failure_signature(reason: &str) -> Option<&'static str> {
    let lower = reason.to_ascii_lowercase();
    if lower.contains("unsupported_cfg_region_shape") || lower.contains("unsupported region shape")
    {
        return Some(StructuringFailureKind::RegionShape.preview_block_signature());
    }
    if lower.contains("unsupported_cfg_phi_join") || lower.contains("unsupported phi join") {
        return Some(StructuringFailureKind::PhiJoin.preview_block_signature());
    }
    if lower.contains("unsupported_cfg_indirect_call_region")
        || lower.contains("unsupported indirect call region")
    {
        return Some(StructuringFailureKind::IndirectCallRegion.preview_block_signature());
    }
    None
}

pub fn classify_nir_failure_refined(reason: &str) -> &'static str {
    let lower = reason.to_ascii_lowercase();
    if lower.contains("nir_timeout")
        || lower.contains("preview_timeout")
        || lower.contains("worker timed out")
    {
        return "nir_timeout";
    }
    if lower.contains("unsupported architecture")
        || lower.contains("currently supports pe x64 only")
    {
        return "nir_architecture_unsupported";
    }
    if lower.contains("unsupported format")
        || lower.contains("format mismatch")
        || lower.contains("pe-only mismatch")
        || lower.contains("only supports pe")
    {
        return "nir_format_unsupported";
    }
    if lower.contains("worker spawn failed")
        || lower.contains("stdin unavailable")
        || lower.contains("stdin write failed")
        || lower.contains("stdout read failed")
        || lower.contains("wait failed")
        || lower.contains("without json response")
        || lower.contains("response parse failed")
    {
        return "nir_worker_failure";
    }
    if structuring_failure_signature(reason).is_some() {
        return "nir_structuring_failure";
    }
    if lower.contains("not a function (orphan block detected)") {
        return "nir_orphan_block";
    }
    if lower.contains("unsupported control flow") || lower.contains("unsupported branch target") {
        return "nir_unsupported_cfg";
    }
    if lower.contains("structuring") {
        return "nir_structuring_failure";
    }
    if lower.contains("pcode parse failed")
        || lower.contains("unsupported architecture")
        || lower.contains("value lowering failed")
        || lower.contains("unsupported expr")
        || lower.contains("unsupported varnode")
        || lower.contains("unsupported address materialization")
        || lower.contains("piece/subpiece")
        || lower.contains("unsupported ptr arithmetic")
        || lower.contains("unsupported memory-backed varnode")
        || lower.contains("unsupported pattern")
    {
        return "nir_parse_or_lowering_failure";
    }
    "nir_non_success_unknown"
}

pub fn classify_nir_failure(reason: &str) -> &'static str {
    match classify_nir_failure_refined(reason) {
        "nir_timeout" => "nir_timeout",
        _ => "nir_unsupported",
    }
}

pub fn classified_nir_error(reason: &str) -> String {
    fallback_reason_with_kind(classify_nir_failure(reason), reason)
}

pub fn fallback_reason_with_kind(kind: &str, detail: impl AsRef<str>) -> String {
    format!("{kind}: {}", detail.as_ref())
}

pub fn classify_native_failure_kind(error: &str) -> &'static str {
    let lower = error.to_ascii_lowercase();
    if lower.contains("nir_timeout") || lower.contains("preview_timeout") {
        "nir_timeout"
    } else if lower.contains("could not find op at target address")
        || lower.contains("ghidra lowlevelerror")
    {
        "native_pcode_failure"
    } else {
        "legacy_fallback"
    }
}

pub fn nir_fallback_reason_with_kind(kind: &str, detail: impl AsRef<str>) -> String {
    fallback_reason_with_kind(kind, detail)
}
