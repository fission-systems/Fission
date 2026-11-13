//! LLVM keeps assembler-local `.L` labels in a RISC-V symbol table, thousands of
//! them in a real binary. Each became a "function", and one sitting at a
//! function's entry took the function's name (locals precede globals in an ELF
//! symbol table), so `classify` was listed as `.L0 ` and `main` vanished.

use serde_json::Value;
use std::process::Command;

#[test]
fn local_labels_are_not_functions_and_do_not_rename_one() {
    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../fission-sleigh/testdata/riscv64_llvm_local_labels.elf"
    );
    let output = Command::new(env!("CARGO_BIN_EXE_fission_cli"))
        .args(["list", fixture, "--json", "--no-db"])
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .output()
        .expect("run fission_cli");
    assert!(output.status.success(), "{output:?}");
    let functions: Value = serde_json::from_slice(&output.stdout).expect("list json");
    let mut names: Vec<&str> = functions
        .as_array()
        .expect("function list")
        .iter()
        .filter_map(|f| f["name"].as_str())
        .collect();
    names.sort_unstable();
    assert_eq!(names, ["classify", "pick", "sum_to"]);
}
