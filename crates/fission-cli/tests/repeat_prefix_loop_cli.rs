//! A `rep`-prefixed string instruction is a loop with an exit. musl's
//! `__init_libc` zeroes its auxv table with `rep stosq` and then walks envp
//! and auxv; `memset` and `memcpy` finish their bulk work with `rep stosq` /
//! `rep movsq`. When the loop lost its exit edge, everything after it was
//! unreachable and dropped; when the loop body's definitions were attributed
//! to the exit test, the destination pointer stopped advancing.
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

fn decompile_all(fixture: &str) -> HashMap<String, String> {
    let binary = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../fission-loader/testdata")
        .join(fixture);
    let output = Command::new(env!("CARGO_BIN_EXE_fission_cli"))
        .args(["--no-db", "decomp"])
        .arg(&binary)
        .args(["--all", "--json"])
        .output()
        .expect("run the CLI");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let functions: Vec<serde_json::Value> =
        serde_json::from_slice(&output.stdout).expect("decomp --all --json is a JSON array");
    functions
        .into_iter()
        .filter_map(|function| {
            Some((
                function.get("name")?.as_str()?.to_string(),
                function.get("code")?.as_str()?.to_string(),
            ))
        })
        .collect()
}

fn count(code: &str, needle: &str) -> usize {
    code.matches(needle).count()
}

#[test]
fn code_after_a_repeat_prefix_loop_survives() {
    let functions = decompile_all("x64_dyn_split_debug_test.elf");
    let init = &functions["__init_libc"];
    // The stores and calls that follow `rep stosq` in the source, each once.
    for store in [
        "__hwcap = ",
        "__sysinfo = ",
        "program_invocation_name = ",
        "__progname = ",
    ] {
        assert!(count(init, store) >= 1, "missing `{store}`:\n{init}");
    }
    for call in ["static_init_tls(", "dummy1("] {
        assert!(count(init, call) >= 1, "missing call `{call}`:\n{init}");
    }
    // The `rep stosq` loop is a conditional loop, not `while (1)`.
    assert_eq!(count(init, "while (1) {\n        if ("), 0, "{init}");

    // memset's `rep stosq` loop is its last: from the loop's label to the
    // end of the function there must be a way out of it.
    let memset = &functions["memset"];
    let body = memset
        .trim_end()
        .strip_suffix('}')
        .unwrap_or(memset)
        .trim_end();
    let last_label = body
        .lines()
        .rev()
        .find(|line| line.trim().starts_with("block_") && line.trim().ends_with(':'));
    let from_last_label = last_label
        .and_then(|label| body.rfind(label))
        .map_or(body, |at| &body[at..]);
    assert!(
        from_last_label.contains("return"),
        "memset's last loop has no exit:\n{memset}"
    );
}

#[test]
fn a_repeat_prefix_loop_advances_its_pointers() {
    let functions = decompile_all("x64_dyn_split_debug_test.elf");
    // `rep stosq` writes through rdi and steps it by 8 (or -8 with DF set);
    // the step must be in the loop, or every iteration stores to one slot.
    let init = &functions["__init_libc"];
    assert_eq!(count(init, " + 8 - 16 * df"), 1, "{init}");
    // `rep movsq` steps both rsi and rdi.
    let memcpy = &functions["memcpy"];
    assert_eq!(
        count(memcpy, " + 8 - 16 * df") + count(memcpy, "-(16 * df)) + 8"),
        2,
        "{memcpy}"
    );
}
