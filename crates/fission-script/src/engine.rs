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

/// What a run is allowed to reach beyond the binary's inventory.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScriptOptions {
    /// Launch the binary under the emulator and hand the script a `machine`.
    ///
    /// Off by default: launching costs a real program load, and a script that
    /// only reads the inventory should not pay for an emulator it never
    /// touches. It is also the difference between a script that inspects a
    /// file and one that runs it.
    pub machine: bool,
}

pub fn run_script(
    binary: &LoadedBinary,
    script_source: &str,
    script_path_display: &str,
    limits: ScriptLimits,
) -> ScriptRunResult {
    run_script_with(
        binary,
        script_source,
        script_path_display,
        limits,
        ScriptOptions::default(),
    )
}

pub fn run_script_with(
    binary: &LoadedBinary,
    script_source: &str,
    script_path_display: &str,
    limits: ScriptLimits,
    options: ScriptOptions,
) -> ScriptRunResult {
    let _ = options;
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

    host::register_helpers(&mut engine);

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

    // A live machine, if the caller asked for one. Launching costs a real
    // program load, so it happens only when requested -- a script that reads
    // the inventory should not pay for an emulator it never touches.
    #[cfg(feature = "emulator")]
    let machine = if options.machine {
        match crate::api::machine::MachineHost::launch(&binary.path) {
            Ok(machine) => Some(machine),
            Err(message) => return ScriptRunResult::error_compile(message, &limits),
        }
    } else {
        None
    };
    #[cfg(feature = "emulator")]
    crate::api::machine::register(&mut engine);

    let handle = thread::spawn(move || {
        let mut scope = rhai::Scope::new();
        scope.push("binary", host_bin);
        #[cfg(feature = "emulator")]
        if let Some(machine) = machine {
            scope.push("machine", machine);
        }
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
