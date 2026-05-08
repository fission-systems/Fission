//! Compile and evaluate Rhai scripts against a [`LoadedBinary`].

use crate::error::ScriptError;
use crate::host::{self, BinaryHost};
use crate::limits::ScriptLimits;
use crate::result::{
    LimitsEcho, SCHEMA_VERSION, ScriptDiagnostic, ScriptRunResult, ScriptRunStatus,
};
use crate::sandbox;
use fission_loader::loader::LoadedBinary;
use rhai::{Dynamic, Engine};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub fn check_script(source: &str) -> Result<(), ScriptError> {
    let engine = sandbox::new_engine_for_compile_check();
    engine
        .compile(source)
        .map_err(|e| ScriptError::Compile(e.to_string()))?;
    Ok(())
}

pub fn run_script(
    binary: &LoadedBinary,
    script_source: &str,
    script_path_display: &str,
    limits: ScriptLimits,
) -> ScriptRunResult {
    let meta = host::script_meta(script_path_display.to_string());

    let findings = Arc::new(Mutex::new(Vec::new()));
    let halted = Arc::new(Mutex::new(None::<String>));

    let mut engine = Engine::new();
    if let Err(e) = sandbox::configure_engine(&mut engine, &limits) {
        return ScriptRunResult {
            schema_version: SCHEMA_VERSION,
            tool: "fission-script",
            script: Some(meta),
            status: ScriptRunStatus::Error,
            findings: Vec::new(),
            diagnostics: vec![ScriptDiagnostic {
                severity: "error".into(),
                message: e.to_string(),
                span: None,
            }],
            limits: LimitsEcho::from_limits(&limits),
        };
    }

    if let Err(e) = host::register_emit(
        &mut engine,
        findings.clone(),
        limits.clone(),
        halted.clone(),
    ) {
        return ScriptRunResult {
            schema_version: SCHEMA_VERSION,
            tool: "fission-script",
            script: Some(meta),
            status: ScriptRunStatus::Error,
            findings: Vec::new(),
            diagnostics: vec![ScriptDiagnostic {
                severity: "error".into(),
                message: e.to_string(),
                span: None,
            }],
            limits: LimitsEcho::from_limits(&limits),
        };
    }

    let bin = Arc::new(binary.clone());
    if let Err(e) = host::register_binary(&mut engine) {
        return ScriptRunResult {
            schema_version: SCHEMA_VERSION,
            tool: "fission-script",
            script: Some(meta),
            status: ScriptRunStatus::Error,
            findings: Vec::new(),
            diagnostics: vec![ScriptDiagnostic {
                severity: "error".into(),
                message: e.to_string(),
                span: None,
            }],
            limits: LimitsEcho::from_limits(&limits),
        };
    }

    let ast = match engine.compile(script_source) {
        Ok(a) => a,
        Err(e) => return ScriptRunResult::error_compile(e.to_string(), &limits),
    };

    let host_bin = BinaryHost(bin);
    let deadline = Duration::from_millis(limits.max_runtime_ms.max(1));

    let handle = thread::spawn(move || {
        let mut scope = rhai::Scope::new();
        scope.push("binary", host_bin);
        engine.eval_ast_with_scope::<Dynamic>(&mut scope, &ast)
    });

    let start = Instant::now();
    loop {
        if start.elapsed() > deadline {
            let mut result = ScriptRunResult::timeout(&limits);
            result.script = Some(meta);
            return result;
        }
        if handle.is_finished() {
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }

    let mut diagnostics = Vec::new();
    let eval_result = handle.join();

    match eval_result {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => diagnostics.push(ScriptDiagnostic {
            severity: "error".into(),
            message: e.to_string(),
            span: None,
        }),
        Err(_) => diagnostics.push(ScriptDiagnostic {
            severity: "error".into(),
            message: "script panicked".into(),
            span: None,
        }),
    }

    if let Ok(g) = halted.lock() {
        if let Some(msg) = g.as_ref() {
            diagnostics.push(ScriptDiagnostic {
                severity: "error".into(),
                message: msg.clone(),
                span: None,
            });
        }
    }

    let findings_vec = findings.lock().map(|g| g.clone()).unwrap_or_default();

    let status = if diagnostics.is_empty() {
        ScriptRunStatus::Ok
    } else {
        ScriptRunStatus::Error
    };

    ScriptRunResult {
        schema_version: SCHEMA_VERSION,
        tool: "fission-script",
        script: Some(meta),
        status,
        findings: findings_vec,
        diagnostics,
        limits: LimitsEcho::from_limits(&limits),
    }
}
