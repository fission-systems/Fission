//! A C++ landing pad reads registers the unwinder filled in. They are defined
//! on entering the pad, so the function does not grow a parameter for them.
use std::path::PathBuf;
use std::process::Command;

#[test]
fn a_function_with_catch_blocks_has_only_its_own_parameters() {
    // `int guarded(int n)` calls something that may throw and catches three
    // exception types (g++ -O0, dynamically linked).
    let binary = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../fission-loader/testdata/x64_cpp_eh_dyn.elf");
    let output = Command::new(env!("CARGO_BIN_EXE_fission_cli"))
        .args(["decomp"])
        .arg(&binary)
        .args(["--addr", "0x40125c", "--no-db"])
        .output()
        .expect("run the CLI");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let code = String::from_utf8_lossy(&output.stdout);
    assert!(code.contains("guarded"), "{code}");
    assert!(
        code.contains("int guarded(int n)"),
        "no parameter beyond `n` -- the pad's reads of the unwinder's registers are not inputs:\n{code}"
    );
}
