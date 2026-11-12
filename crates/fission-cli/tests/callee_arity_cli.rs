//! A callee's own body says how many arguments it takes; a call must not pass
//! more. The fixture is stripped, so no name or debug record says so: the only
//! evidence is the callee's code.

use serde_json::Value;
use std::process::Command;

fn decompile_caller() -> String {
    let fixture = format!(
        "{}/../fission-sleigh/testdata/x64_stale_arg_register.elf",
        env!("CARGO_MANIFEST_DIR")
    );
    let output = Command::new(env!("CARGO_BIN_EXE_fission_cli"))
        .args([
            "decomp",
            &fixture,
            "--addr",
            "0x1001240",
            "--json",
            "--no-db",
        ])
        .env("FISSION_BENCHMARK_NO_CACHE", "1")
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .output()
        .expect("run fission_cli decomp");
    assert!(output.status.success(), "{output:?}");
    let body: Value = serde_json::from_slice(&output.stdout).expect("json");
    body[0]["code"].as_str().expect("code").to_string()
}

/// Arguments of the first call to `callee` in `code`, split on top-level commas.
fn call_args(code: &str, callee: &str) -> Vec<String> {
    // Past the `extern ... callee();` declaration, into the caller's body.
    let body = &code[code.find('{').expect("function body")..];
    let start = body.find(&format!("{callee}(")).expect("call present") + callee.len() + 1;
    let mut depth = 1;
    let mut args = vec![String::new()];
    for c in body[start..].chars() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            ',' if depth == 1 => {
                args.push(String::new());
                continue;
            }
            _ => {}
        }
        args.last_mut().expect("arg").push(c);
    }
    args.retain(|arg| !arg.trim().is_empty());
    args
}

#[test]
fn a_one_parameter_callee_is_not_called_with_a_leftover_second_register() {
    let code = decompile_caller();
    assert_eq!(
        call_args(&code, "sub_1001220").len(),
        1,
        "the callee reads only its first argument register: {code}"
    );
}

#[test]
fn a_two_parameter_callee_keeps_both_arguments() {
    let code = decompile_caller();
    assert_eq!(
        call_args(&code, "sub_1001200").len(),
        2,
        "the callee reads both argument registers: {code}"
    );
}
