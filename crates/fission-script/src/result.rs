//! JSON-serializable script results.

use crate::limits::ScriptLimits;
use serde::Serialize;

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScriptRunStatus {
    Ok,
    Error,
    Timeout,
}

#[derive(Clone, Debug, Serialize)]
pub struct ScriptMeta {
    pub path: String,
    pub engine: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct ScriptDiagnostic {
    pub severity: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ScriptFinding {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Serialize)]
pub struct LimitsEcho {
    pub max_operations: u64,
    pub max_runtime_ms: u64,
    pub max_findings: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct ScriptRunResult {
    pub schema_version: u32,
    pub tool: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub script: Option<ScriptMeta>,
    pub status: ScriptRunStatus,
    pub findings: Vec<ScriptFinding>,
    pub diagnostics: Vec<ScriptDiagnostic>,
    pub limits: LimitsEcho,
}

impl ScriptRunResult {
    pub fn error_compile(message: impl Into<String>, limits: &ScriptLimits) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            tool: "fission-script",
            script: None,
            status: ScriptRunStatus::Error,
            findings: Vec::new(),
            diagnostics: vec![ScriptDiagnostic {
                severity: "error".into(),
                message: message.into(),
                span: None,
            }],
            limits: LimitsEcho::from_limits(limits),
        }
    }

    pub fn error_runtime(message: impl Into<String>, limits: &ScriptLimits) -> Self {
        Self::error_compile(message, limits)
    }

    pub fn timeout(limits: &ScriptLimits) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            tool: "fission-script",
            script: None,
            status: ScriptRunStatus::Timeout,
            findings: Vec::new(),
            diagnostics: vec![ScriptDiagnostic {
                severity: "error".into(),
                message: "script exceeded wall-clock limit".into(),
                span: None,
            }],
            limits: LimitsEcho::from_limits(limits),
        }
    }
}

impl LimitsEcho {
    pub fn from_limits(l: &ScriptLimits) -> Self {
        Self {
            max_operations: l.max_operations,
            max_runtime_ms: l.max_runtime_ms,
            max_findings: l.max_findings,
        }
    }
}
