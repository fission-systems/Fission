use super::nir_taxonomy::{
    classified_nir_error, classify_native_failure_kind, extract_fallback_kind,
    extract_refined_fallback_kind, fallback_reason_with_kind,
};
use fission_pcode::{NirBuildStats, NirHintStats, NirRenderOptions, NirTypeContext};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NirEngineMode {
    Legacy,
    Nir,
    Auto,
}

impl NirEngineMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            NirEngineMode::Legacy => "legacy",
            NirEngineMode::Nir => "nir",
            NirEngineMode::Auto => "auto",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct NirSelection {
    pub nir_code: Option<String>,
    pub build_stats: Option<NirBuildStats>,
    pub hint_stats: Option<NirHintStats>,
    pub engine_used: NirEngineMode,
    pub fell_back: bool,
    pub fallback_reason: Option<String>,
    pub fallback_kind: Option<&'static str>,
    pub fallback_kind_refined: Option<&'static str>,
    pub nir_surface: Option<NirSurfaceKind>,
    pub recovery_strategy_attempted: Option<&'static str>,
    pub recovery_strategy_applied: Option<&'static str>,
    pub recovery_outcome: Option<&'static str>,
    pub recovery_source_signature: Option<String>,
    pub recovery_structuring_mode: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NirRoutingDecision {
    pub engine_used: NirEngineMode,
    pub fell_back: bool,
    pub fallback_reason: Option<String>,
    pub fallback_kind: Option<&'static str>,
    pub fallback_kind_refined: Option<&'static str>,
    pub nir_surface: Option<NirSurfaceKind>,
    pub recovery_strategy_attempted: Option<&'static str>,
    pub recovery_strategy_applied: Option<&'static str>,
    pub recovery_outcome: Option<&'static str>,
    pub recovery_source_signature: Option<String>,
    pub recovery_structuring_mode: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NirSurfaceKind {
    Structured,
    Unstructured,
}

pub struct NirRoutingResolver;

impl NirRoutingResolver {
    pub fn from_selection(selection: &NirSelection) -> NirRoutingDecision {
        NirRoutingDecision {
            engine_used: selection.engine_used,
            fell_back: selection.fell_back,
            fallback_reason: selection.fallback_reason.clone(),
            fallback_kind: selection.fallback_kind,
            fallback_kind_refined: selection.fallback_kind_refined,
            nir_surface: selection.nir_surface,
            recovery_strategy_attempted: selection.recovery_strategy_attempted,
            recovery_strategy_applied: selection.recovery_strategy_applied,
            recovery_outcome: selection.recovery_outcome,
            recovery_source_signature: selection.recovery_source_signature.clone(),
            recovery_structuring_mode: selection.recovery_structuring_mode,
        }
    }

    pub fn legacy_mode() -> NirSelection {
        NirSelection {
            nir_code: None,
            build_stats: None,
            hint_stats: None,
            engine_used: NirEngineMode::Legacy,
            fell_back: false,
            fallback_reason: None,
            fallback_kind: None,
            fallback_kind_refined: None,
            nir_surface: None,
            recovery_strategy_attempted: None,
            recovery_strategy_applied: None,
            recovery_outcome: None,
            recovery_source_signature: None,
            recovery_structuring_mode: None,
        }
    }

    pub fn nir_success(
        code: String,
        build_stats: Option<NirBuildStats>,
        hint_stats: Option<NirHintStats>,
        fell_back: bool,
        fallback_reason: Option<String>,
    ) -> NirSelection {
        NirSelection {
            nir_surface: Some(classify_nir_surface(&code)),
            nir_code: Some(code),
            build_stats,
            hint_stats,
            engine_used: NirEngineMode::Nir,
            fell_back,
            fallback_kind: extract_fallback_kind(fallback_reason.as_deref()),
            fallback_kind_refined: extract_refined_fallback_kind(fallback_reason.as_deref()),
            fallback_reason,
            recovery_strategy_attempted: None,
            recovery_strategy_applied: None,
            recovery_outcome: None,
            recovery_source_signature: None,
            recovery_structuring_mode: None,
        }
    }

    pub fn nir_success_with_recovery(
        code: String,
        build_stats: Option<NirBuildStats>,
        hint_stats: Option<NirHintStats>,
        attempted: &'static str,
        applied: &'static str,
        outcome: &'static str,
        source_signature: Option<String>,
        structuring_mode: &'static str,
    ) -> NirSelection {
        NirSelection {
            nir_surface: Some(classify_nir_surface(&code)),
            nir_code: Some(code),
            build_stats,
            hint_stats,
            engine_used: NirEngineMode::Nir,
            fell_back: false,
            fallback_reason: None,
            fallback_kind: None,
            fallback_kind_refined: None,
            recovery_strategy_attempted: Some(attempted),
            recovery_strategy_applied: Some(applied),
            recovery_outcome: Some(outcome),
            recovery_source_signature: source_signature,
            recovery_structuring_mode: Some(structuring_mode),
        }
    }

    pub fn nir_fallback(reason: impl AsRef<str>) -> NirSelection {
        let fallback_reason = classified_nir_error(reason.as_ref());
        NirSelection {
            nir_code: None,
            build_stats: None,
            hint_stats: None,
            engine_used: NirEngineMode::Legacy,
            fell_back: true,
            fallback_kind: extract_fallback_kind(Some(fallback_reason.as_str())),
            fallback_kind_refined: extract_refined_fallback_kind(Some(fallback_reason.as_str())),
            fallback_reason: Some(fallback_reason),
            nir_surface: None,
            recovery_strategy_attempted: None,
            recovery_strategy_applied: None,
            recovery_outcome: None,
            recovery_source_signature: None,
            recovery_structuring_mode: None,
        }
    }

    pub fn nir_fallback_with_recovery(
        reason: impl AsRef<str>,
        attempted: &'static str,
        applied: Option<&'static str>,
        outcome: &'static str,
        source_signature: Option<String>,
        structuring_mode: Option<&'static str>,
    ) -> NirSelection {
        let fallback_reason = classified_nir_error(reason.as_ref());
        NirSelection {
            nir_code: None,
            build_stats: None,
            hint_stats: None,
            engine_used: NirEngineMode::Legacy,
            fell_back: true,
            fallback_kind: extract_fallback_kind(Some(fallback_reason.as_str())),
            fallback_kind_refined: extract_refined_fallback_kind(Some(fallback_reason.as_str())),
            fallback_reason: Some(fallback_reason),
            nir_surface: None,
            recovery_strategy_attempted: Some(attempted),
            recovery_strategy_applied: applied,
            recovery_outcome: Some(outcome),
            recovery_source_signature: source_signature,
            recovery_structuring_mode: structuring_mode,
        }
    }

    pub fn native_failure(error: &str) -> NirRoutingDecision {
        let kind = classify_native_failure_kind(error);
        NirRoutingDecision {
            engine_used: NirEngineMode::Legacy,
            fell_back: true,
            fallback_reason: Some(fallback_reason_with_kind(kind, error)),
            fallback_kind: Some(kind),
            fallback_kind_refined: None,
            nir_surface: None,
            recovery_strategy_attempted: None,
            recovery_strategy_applied: None,
            recovery_outcome: None,
            recovery_source_signature: None,
            recovery_structuring_mode: None,
        }
    }
}

impl NirSelection {
    pub fn routing_decision(&self) -> NirRoutingDecision {
        NirRoutingResolver::from_selection(self)
    }
}

pub(crate) fn classify_nir_surface(code: &str) -> NirSurfaceKind {
    let has_goto = code.contains("goto ");
    let has_label = code.lines().any(|line| {
        let trimmed = line.trim();
        trimmed.ends_with(':') && !trimmed.starts_with("case ") && !trimmed.starts_with("default:")
    });
    if has_goto || has_label {
        NirSurfaceKind::Unstructured
    } else {
        NirSurfaceKind::Structured
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NirWorkerRequest {
    pub pcode_json: String,
    pub address: u64,
    pub name: String,
    pub options: NirRenderOptions,
    pub type_context: NirTypeContext,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NirWorkerResponse {
    pub success: bool,
    pub code: Option<String>,
    pub build_stats: Option<NirBuildStats>,
    pub hint_stats: Option<NirHintStats>,
    pub error: Option<String>,
}

pub trait NirSource {
    fn get_pcode_json(&mut self, address: u64) -> fission_core::Result<String>;
}

#[cfg(feature = "native_decomp")]
impl NirSource for fission_ffi::DecompilerNative {
    fn get_pcode_json(&mut self, address: u64) -> fission_core::Result<String> {
        self.get_pcode(address)
    }
}

#[cfg(feature = "native_decomp")]
impl NirSource for crate::analysis::decomp::CachingDecompiler {
    fn get_pcode_json(&mut self, address: u64) -> fission_core::Result<String> {
        self.inner_mut().get_pcode(address)
    }
}

pub type PreviewEngineMode = NirEngineMode;
pub type PreviewSelection = NirSelection;
pub type PreviewRoutingDecision = NirRoutingDecision;
pub type PreviewRoutingResolver = NirRoutingResolver;
pub type PreviewSurfaceKind = NirSurfaceKind;
pub type PreviewWorkerRequest = NirWorkerRequest;
pub type PreviewWorkerResponse = NirWorkerResponse;
pub use NirSource as PreviewSource;

#[cfg(test)]
pub(crate) fn sanitize_preview_symbol_name(name: &str) -> String {
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
