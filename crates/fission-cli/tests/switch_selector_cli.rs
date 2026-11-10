//! A jump-table switch is on the value that indexes the table, with the
//! table's slot numbers as its case values.
use std::path::PathBuf;
use std::process::Command;

fn decompile(function: &str) -> String {
    let binary = PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../fission-sleigh/testdata/x64_static_switch_no_reloc.elf"
    ));
    let output = Command::new(env!("CARGO_BIN_EXE_fission_cli"))
        .args(["decomp"])
        .arg(&binary)
        .args(["--addr", function, "--no-db"])
        .output()
        .expect("run the CLI");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn the_switch_is_on_the_table_index_not_on_the_loaded_target() {
    // `cmp edi, 3; ja default; jmp [table + rdi*8]` -- four slots.
    let code = decompile("0x1002030");
    assert!(
        code.contains("switch (param_1)"),
        "the selector must be the argument that indexes the table:\n{code}"
    );
    // A selector that is a variable nothing assigns reads garbage.
    assert!(!code.contains("switch (xVar"), "{code}");
}
