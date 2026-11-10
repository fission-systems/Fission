//! `pcode-topology` accepts "an address inside the function" and must describe
//! the whole function, not the tail starting at that address.
use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;

fn fixture() -> PathBuf {
    PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../fission-emulator/testdata/x64_static_printf_malloc.elf"
    ))
}

fn json(command: &str, args: &[&str]) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_fission_cli"))
        .arg(command)
        .arg(fixture())
        .args(args)
        .arg("--json")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("one JSON stdout document")
}

#[test]
fn topology_from_an_interior_address_matches_the_one_from_the_entry() {
    // `main`-sized function that the string-navigation fixture test also uses.
    let entry = "0x1002090";
    let listing = json("disasm", &["--addr", entry, "--function"]);
    let interior = listing["instructions"][2]["address"]
        .as_str()
        .expect("third instruction address")
        .to_string();
    assert_ne!(interior, entry);

    let from_entry = json("pcode-topology", &["--addr", entry]);
    let from_interior = json("pcode-topology", &["--addr", &interior]);

    assert_eq!(
        from_interior["function"]["resolved_address"], from_entry["function"]["resolved_address"],
        "decode must start at the function entry, not at the requested address"
    );
    assert_eq!(
        from_interior["raw_pcode_topology"]["op_count"],
        from_entry["raw_pcode_topology"]["op_count"]
    );
    assert_eq!(
        from_interior["raw_pcode_topology"]["block_count"],
        from_entry["raw_pcode_topology"]["block_count"]
    );
}
