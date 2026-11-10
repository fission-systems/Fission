//! A jump-table switch is on the value that indexes the table, with the
//! table's slot numbers as its case values.
use std::path::PathBuf;
use std::process::Command;

fn decompile(function: &str) -> String {
    decompile_fixture("x64_static_switch_no_reloc.elf", function)
}

fn decompile_fixture(fixture: &str, function: &str) -> String {
    let binary = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../fission-sleigh/testdata")
        .join(fixture);
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

/// Position-independent code stores the table as offsets from its own base, so
/// the dispatch is `base + table[c - 43]`. The switch is on `c - 43`, with the
/// table's slot numbers as case values; naming the target a variable must not
/// leave the switch on that variable, and the case values must not carry the
/// `- 43` a second time.
#[test]
fn a_relative_table_switches_on_the_index_with_slot_numbers() {
    // See `x64_pie_relative_switch.c`: `switch (c)` over '+'..'0'.
    let code = decompile_fixture("x64_pie_relative_switch.elf", "0x1000");
    let selector = code
        .lines()
        .find_map(|line| line.trim().strip_prefix("switch ("))
        .and_then(|rest| rest.strip_suffix(") {"))
        .unwrap_or_else(|| panic!("no switch recovered:\n{code}"));
    assert!(
        code.contains(&format!("{selector} = rdi - 43;")),
        "the selector must be the index `c - 43`:\n{code}"
    );
    // Slot 1 is ',' (22), slot 5 is '0' (66).
    assert!(code.contains("case 1:\n            return 22;"), "{code}");
    assert!(code.contains("case 5:\n            return 66;"), "{code}");
    // The table load is the dispatch, not a value the switch reads.
    assert!(!code.contains("8192"), "{code}");
}
