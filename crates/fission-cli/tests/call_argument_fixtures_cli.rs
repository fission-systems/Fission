//! Call arguments and return values on the repository's fixture binaries,
//! checked against their disassembly.
use std::path::PathBuf;
use std::process::Command;

fn decompile(crate_dir: &str, fixture: &str, address: &str) -> String {
    let binary = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(crate_dir)
        .join("testdata")
        .join(fixture);
    let output = Command::new(env!("CARGO_BIN_EXE_fission_cli"))
        .args(["--no-db", "decomp"])
        .arg(&binary)
        .args(["--addr", address, "--json", "--timeout-ms", "0"])
        .output()
        .expect("run the CLI");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("decomp --json is JSON");
    let record = if value.is_array() {
        value[0].clone()
    } else {
        value
    };
    record["code"].as_str().expect("code").to_string()
}

/// Top-level arguments of every call to `callee` in `code`.
fn call_arguments(code: &str, callee: &str) -> Vec<Vec<String>> {
    let mut calls = Vec::new();
    let needle = format!("{callee}(");
    for (start, _) in code.match_indices(&needle) {
        let before = &code[..start];
        if before.ends_with(|c: char| c.is_alphanumeric() || c == '_')
            || before.trim_end().ends_with("extern unsigned long long")
            || code[..start]
                .lines()
                .last()
                .is_some_and(|line| line.contains("extern"))
        {
            continue;
        }
        let mut depth = 1;
        let mut args = Vec::new();
        let mut current = String::new();
        for c in code[start + needle.len()..].chars() {
            match c {
                '(' => {
                    depth += 1;
                    current.push(c);
                }
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                    current.push(c);
                }
                ',' if depth == 1 => args.push(std::mem::take(&mut current).trim().to_string()),
                _ => current.push(c),
            }
        }
        if !current.trim().is_empty() {
            args.push(current.trim().to_string());
        }
        calls.push(args);
    }
    calls
}

#[test]
fn fwritex_keeps_its_three_declared_arguments() {
    // fmt_fp 0x40185f: `sub rcx, rdi; mov rdx, r13; mov [rsp], r10;
    // mov rsi, rcx; call __fwritex` -- rdi is a merge of three writes.
    // `__fwritex(const unsigned char *, size_t, FILE *)` takes three
    // arguments; the spill of r10 is not a fourth, and the unnamed merge of
    // rdi must not drop the other two or pass rdi's entry value.
    let code = decompile("fission-loader", "x64_dyn_split_debug_test.elf", "0x400900");
    let calls = call_arguments(&code, "__fwritex");
    assert!(!calls.is_empty(), "{code}");
    for args in &calls {
        assert_eq!(args.len(), 3, "__fwritex({}):\n{code}", args.join(", "));
        assert_ne!(args[0], "param_1", "rdi's entry value:\n{code}");
    }
}

#[test]
fn conditional_overwrite_condition_reads_no_flag_variables() {
    // static_init_tls: `lea rdi, builtin_tls; cmp size, 336; jbe 1f; ...
    // mov rdi, rax; 1: call __copy_tls` -- the argument is a select on the
    // branch's condition, which must not make CF/ZF variables.
    let code = decompile("fission-loader", "x64_dyn_split_debug_test.elf", "0x402dc0");
    assert!(code.contains("__copy_tls("), "{code}");
    assert!(
        !code.contains("cf = ") && !code.contains("zf = "),
        "flag assignments:\n{code}"
    );
}

#[test]
fn aarch64_float_helper_does_not_return_an_integer_call_result() {
    // 0x102fe5c ends `fcvt h0, s0; ret`: it returns in v0. x0 there holds
    // the result of the integer call `bl 0x102c010`, so `return x0` passes
    // that call's result as the function's float return value.
    let code = decompile("fission-emulator", "aarch64_control_flow.elf", "0x102fe5c");
    assert!(!code.contains("return x0;"), "{code}");
}
