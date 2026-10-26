//! Contract-mode supervisor: re-executes the CLI, captures its output, and
//! writes the `--contract v1` envelope and exit code.

use super::errors::{self, ErrorKind};
use super::{CHILD_ENV, CONTRACT_ID, ERROR_RECORD_PREFIX};
use serde_json::{Map, Value, json};
use std::ffi::OsString;
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Instant;

const EXCERPT_CHARS: usize = 600;

/// True inside the re-executed child of a contract-mode run.
pub fn is_child() -> bool {
    std::env::var_os(CHILD_ENV).is_some()
}

/// In the child, report a fatal error as one machine-readable stderr line the
/// supervisor turns into the envelope's `error` object.
pub fn emit_child_error(error: &anyhow::Error) {
    let record = json!({
        "code": errors::classify_error(error),
        "message": format!("{error:#}"),
    });
    eprintln!("{ERROR_RECORD_PREFIX}{record}");
}

struct Request {
    /// Arguments for the child, without the program name and without the
    /// supervisor-only flags.
    child_args: Vec<OsString>,
    fail_on_partial: bool,
}

enum Parsed {
    NotRequested,
    Request(Request),
    Invalid(String),
}

/// Strip `--contract <v>`, `--contract=<v>` and `--fail-on-partial`; honour
/// `FISSION_CONTRACT` when the flag is absent.
fn parse_request(args: &[OsString], env_contract: Option<String>) -> Parsed {
    let mut child_args = Vec::new();
    let mut contract = env_contract;
    let mut fail_on_partial = false;
    let mut flag_seen = false;
    let mut iter = args.iter().skip(1);
    while let Some(arg) = iter.next() {
        match arg.to_str() {
            Some("--contract") => {
                flag_seen = true;
                match iter.next().and_then(|value| value.to_str()) {
                    Some(value) => contract = Some(value.to_string()),
                    None => return Parsed::Invalid("--contract needs a value".to_string()),
                }
            }
            Some(text) if text.starts_with("--contract=") => {
                flag_seen = true;
                contract = Some(text["--contract=".len()..].to_string());
            }
            Some("--fail-on-partial") => fail_on_partial = true,
            _ => child_args.push(arg.clone()),
        }
    }
    let _ = flag_seen;
    match contract.as_deref() {
        None => Parsed::NotRequested,
        Some("v1") => Parsed::Request(Request {
            child_args,
            fail_on_partial,
        }),
        Some(other) => Parsed::Invalid(format!(
            "unsupported contract version {other:?}; only \"v1\" exists"
        )),
    }
}

fn command_name(child_args: &[OsString]) -> String {
    child_args
        .iter()
        .filter_map(|arg| arg.to_str())
        .find(|arg| crate::cli::args::CANONICAL_SUBCOMMANDS.contains(arg))
        .unwrap_or("unknown")
        .to_string()
}

fn wants_help(child_args: &[OsString]) -> bool {
    child_args
        .iter()
        .filter_map(|arg| arg.to_str())
        .any(|arg| matches!(arg, "--help" | "-h" | "--version" | "-V" | "help"))
}

fn has_json_flag(child_args: &[OsString]) -> bool {
    child_args
        .iter()
        .filter_map(|arg| arg.to_str())
        .any(|arg| arg == "--json" || arg == "-j")
}

/// Run the supervisor when contract mode is requested. `None` means "not
/// requested": the caller continues with the normal CLI.
pub fn maybe_supervise() -> Option<i32> {
    if is_child() {
        return None;
    }
    let args: Vec<OsString> = std::env::args_os().collect();
    let env_contract = std::env::var("FISSION_CONTRACT")
        .ok()
        .filter(|value| !value.is_empty());
    let start = Instant::now();
    match parse_request(&args, env_contract) {
        Parsed::NotRequested => None,
        Parsed::Invalid(message) => Some(fail(
            "unknown",
            "usage_error",
            &message,
            Value::Null,
            None,
            start,
        )),
        Parsed::Request(request) => Some(supervise(request, start)),
    }
}

fn supervise(request: Request, start: Instant) -> i32 {
    let command = command_name(&request.child_args);

    if wants_help(&request.child_args) {
        return passthrough(&request.child_args);
    }
    if !has_json_flag(&request.child_args) {
        return fail(
            &command,
            "json_required",
            "--contract v1 needs the command's own --json flag",
            Value::Null,
            None,
            start,
        );
    }

    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(error) => {
            return fail(
                &command,
                "internal_panic",
                &format!("cannot locate the fission_cli executable: {error}"),
                Value::Null,
                None,
                start,
            );
        }
    };
    let output = match Command::new(exe)
        .args(&request.child_args)
        .env(CHILD_ENV, "1")
        .stdin(Stdio::inherit())
        .output()
    {
        Ok(output) => output,
        Err(error) => {
            return fail(
                &command,
                "internal_panic",
                &format!("cannot start the fission_cli child process: {error}"),
                Value::Null,
                None,
                start,
            );
        }
    };

    let stderr_text = String::from_utf8_lossy(&output.stderr).into_owned();
    let (record, human_stderr) = split_stderr(&stderr_text);
    if !human_stderr.is_empty() {
        let mut stderr = std::io::stderr().lock();
        let _ = stderr.write_all(human_stderr.as_bytes());
    }
    let child_exit = output.status.code();

    if output.status.success() {
        let stdout_text = String::from_utf8_lossy(&output.stdout).into_owned();
        return match parse_stdout(&stdout_text) {
            Ok(data) => succeed(&command, data, request.fail_on_partial, start),
            Err(message) => fail(
                &command,
                "output_not_json",
                &message,
                json!({ "stdout_excerpt": excerpt_head(&stdout_text) }),
                child_exit,
                start,
            ),
        };
    }

    let (code, message) = match record {
        Some((code, message)) => (code, message),
        None if child_exit == Some(2) => ("usage_error", excerpt_tail(human_stderr.trim())),
        None if child_exit.is_none() || child_exit == Some(101) => (
            "internal_panic",
            format!(
                "terminated abnormally ({}): {}",
                output.status,
                excerpt_tail(human_stderr.trim())
            ),
        ),
        None => ("unclassified", excerpt_tail(human_stderr.trim())),
    };
    fail(&command, code, &message, Value::Null, child_exit, start)
}

/// Run the child with inherited stdio, for `--help` and `--version`.
fn passthrough(child_args: &[OsString]) -> i32 {
    let Ok(exe) = std::env::current_exe() else {
        return ErrorKind::Internal.exit_code();
    };
    match Command::new(exe)
        .args(child_args)
        .env(CHILD_ENV, "1")
        .status()
    {
        Ok(status) => status.code().unwrap_or(ErrorKind::Internal.exit_code()),
        Err(_) => ErrorKind::Internal.exit_code(),
    }
}

/// Separate the child's machine-readable error line from ordinary stderr.
fn split_stderr(text: &str) -> (Option<(&'static str, String)>, String) {
    let mut record = None;
    let mut human = String::new();
    for line in text.lines() {
        if let Some(json_text) = line.strip_prefix(ERROR_RECORD_PREFIX) {
            if let Ok(value) = serde_json::from_str::<Value>(json_text) {
                let code = value
                    .get("code")
                    .and_then(Value::as_str)
                    .and_then(errors::lookup)
                    .map_or("unclassified", |info| info.code);
                let message = value
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                record = Some((code, message));
                continue;
            }
        }
        human.push_str(line);
        human.push('\n');
    }
    (record, human)
}

/// Whole-document JSON first; otherwise accept newline-delimited JSON.
fn parse_stdout(text: &str) -> Result<Value, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("the command wrote nothing to stdout".to_string());
    }
    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        return Ok(value);
    }
    let rows: Result<Vec<Value>, _> = trimmed
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(serde_json::from_str::<Value>)
        .collect();
    match rows {
        Ok(rows) => Ok(Value::Array(rows)),
        Err(error) => Err(format!("stdout is not JSON: {error}")),
    }
}

/// `(total, failed)` when `data` is a list of rows that report per-item `error`.
fn batch_counts(data: &Value) -> Option<(usize, usize)> {
    let rows = data.as_array()?;
    let reports_errors = rows
        .iter()
        .any(|row| row.as_object().is_some_and(|map| map.contains_key("error")));
    if !reports_errors {
        return None;
    }
    let failed = rows
        .iter()
        .filter(|row| match row.get("error") {
            None | Some(Value::Null) => false,
            Some(Value::String(text)) => !text.is_empty(),
            Some(_) => true,
        })
        .count();
    Some((rows.len(), failed))
}

fn succeed(command: &str, data: Value, fail_on_partial: bool, start: Instant) -> i32 {
    let counts = batch_counts(&data);
    let mut map = base(command, true, start);
    map.insert("schema_version".into(), json!(1));
    if let Some((total, failed)) = counts {
        map.insert(
            "summary".into(),
            json!({ "total": total, "succeeded": total - failed, "failed": failed }),
        );
        let outcome = if total > 0 && failed == total {
            Some(("all_items_failed", "every requested item failed"))
        } else if failed > 0 && fail_on_partial {
            Some(("partial_failure", "some requested items failed"))
        } else {
            None
        };
        if let Some((code, message)) = outcome {
            map.insert("ok".into(), json!(false));
            map.insert("data".into(), data);
            return finish_with_error(map, code, message, Value::Null, None);
        }
    }
    map.insert("data".into(), data);
    print_envelope(map);
    0
}

fn fail(
    command: &str,
    code: &str,
    message: &str,
    context: Value,
    child_exit: Option<i32>,
    start: Instant,
) -> i32 {
    let map = base(command, false, start);
    finish_with_error(map, code, message, context, child_exit)
}

fn finish_with_error(
    mut map: Map<String, Value>,
    code: &str,
    message: &str,
    mut context: Value,
    child_exit: Option<i32>,
) -> i32 {
    let info = errors::lookup(code).unwrap_or_else(|| {
        errors::lookup("unclassified").expect("registry contains `unclassified`")
    });
    if let Some(exit) = child_exit {
        if !context.is_object() {
            context = json!({});
        }
        context["child_exit"] = json!(exit);
    }
    let mut error = json!({
        "code": info.code,
        "kind": info.kind.as_str(),
        "message": message,
        "retryable": info.retryable,
    });
    if let Some(hint) = hint_for(info.code) {
        error["hint"] = json!(hint);
    }
    if !context.is_null() {
        error["context"] = context;
    }
    map.insert("error".into(), error);
    print_envelope(map);
    info.kind.exit_code()
}

fn hint_for(code: &str) -> Option<&'static str> {
    match code {
        "usage_error" => Some("Run the command with --help for its arguments."),
        "json_required" => Some("Add --json to the command."),
        "input_not_found" => {
            Some("Check the path; relative paths resolve against the current directory.")
        }
        "resource_root_missing" => Some("Set --resource-root or FISSION_RESOURCE_ROOT."),
        "function_timeout" => Some("Retry with a larger --timeout-ms."),
        _ => None,
    }
}

fn base(command: &str, ok: bool, start: Instant) -> Map<String, Value> {
    let mut map = Map::new();
    map.insert("contract".into(), json!(CONTRACT_ID));
    map.insert("command".into(), json!(command));
    map.insert("ok".into(), json!(ok));
    map.insert("warnings".into(), json!([]));
    map.insert(
        "tool".into(),
        json!({
            "name": "fission_cli",
            "version": env!("CARGO_PKG_VERSION"),
            "commit": option_env!("FISSION_GIT_COMMIT"),
        }),
    );
    map.insert(
        "elapsed_ms".into(),
        json!(u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)),
    );
    map
}

/// Write the envelope; a reader that closed the pipe is not an error, and the
/// exit code the caller returns is unaffected.
fn print_envelope(map: Map<String, Value>) {
    let text = Value::Object(map).to_string();
    let mut stdout = std::io::stdout().lock();
    let _ = writeln!(stdout, "{text}");
    let _ = stdout.flush();
}

fn excerpt_head(text: &str) -> String {
    text.chars().take(EXCERPT_CHARS).collect()
}

fn excerpt_tail(text: &str) -> String {
    let count = text.chars().count();
    if count <= EXCERPT_CHARS {
        text.to_string()
    } else {
        text.chars().skip(count - EXCERPT_CHARS).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    #[test]
    fn contract_flag_is_stripped_before_the_child_sees_it() {
        let parsed = parse_request(
            &os(&["fission_cli", "--contract", "v1", "list", "a.exe", "--json"]),
            None,
        );
        let Parsed::Request(request) = parsed else {
            panic!("expected a request")
        };
        assert_eq!(request.child_args, os(&["list", "a.exe", "--json"]));
        assert!(!request.fail_on_partial);
    }

    #[test]
    fn equals_form_and_fail_on_partial_are_handled() {
        let parsed = parse_request(
            &os(&[
                "fission_cli",
                "decomp",
                "--contract=v1",
                "--fail-on-partial",
                "a",
            ]),
            None,
        );
        let Parsed::Request(request) = parsed else {
            panic!("expected a request")
        };
        assert_eq!(request.child_args, os(&["decomp", "a"]));
        assert!(request.fail_on_partial);
    }

    #[test]
    fn environment_alone_requests_the_contract() {
        let parsed = parse_request(&os(&["fission_cli", "list", "a"]), Some("v1".into()));
        assert!(matches!(parsed, Parsed::Request(_)));
        let parsed = parse_request(&os(&["fission_cli", "list", "a"]), None);
        assert!(matches!(parsed, Parsed::NotRequested));
    }

    #[test]
    fn unknown_contract_version_is_invalid() {
        let parsed = parse_request(&os(&["fission_cli", "--contract", "v9"]), None);
        assert!(matches!(parsed, Parsed::Invalid(_)));
    }

    #[test]
    fn stderr_error_record_is_separated_from_human_text() {
        let text = format!(
            "[fission] note\n{ERROR_RECORD_PREFIX}{{\"code\":\"input_not_found\",\"message\":\"gone\"}}\n"
        );
        let (record, human) = split_stderr(&text);
        assert_eq!(record, Some(("input_not_found", "gone".to_string())));
        assert_eq!(human, "[fission] note\n");
    }

    #[test]
    fn unknown_record_code_falls_back_to_unclassified() {
        let text = format!("{ERROR_RECORD_PREFIX}{{\"code\":\"made_up\",\"message\":\"x\"}}\n");
        let (record, _) = split_stderr(&text);
        assert_eq!(record, Some(("unclassified", "x".to_string())));
    }

    #[test]
    fn batch_counts_treat_null_and_empty_errors_as_success() {
        let data = json!([
            { "name": "a", "error": null },
            { "name": "b", "error": "" },
            { "name": "c", "error": "boom" },
            { "name": "d" }
        ]);
        assert_eq!(batch_counts(&data), Some((4, 1)));
    }

    #[test]
    fn rows_without_error_fields_are_not_a_batch() {
        let data = json!([{ "name": "a" }, { "name": "b" }]);
        assert_eq!(batch_counts(&data), None);
        assert_eq!(batch_counts(&json!({ "name": "a" })), None);
    }

    #[test]
    fn ndjson_stdout_becomes_an_array() {
        let value = parse_stdout("{\"a\":1}\n{\"a\":2}\n").expect("ndjson parses");
        assert_eq!(value, json!([{ "a": 1 }, { "a": 2 }]));
        assert!(parse_stdout("not json").is_err());
        assert!(parse_stdout("  \n").is_err());
    }

    #[test]
    fn command_name_finds_the_subcommand_after_global_flags() {
        let args = os(&["--resource-root", "dir", "decomp", "a.exe"]);
        assert_eq!(command_name(&args), "decomp");
        assert_eq!(command_name(&os(&["a.exe"])), "unknown");
    }
}
