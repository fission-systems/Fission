//! Addresses cross between the static views and the machine.
//!
//! `binary.functions()` / `binary.sections()` / `binary.image_base()` give an
//! address as the string `"0x401000"`, as the rest of the tool writes one;
//! `machine.bp`, `machine.read`, `machine.pc()` and friends take and return an
//! integer. Rhai does not coerce between them, so without a way back from the
//! string a script could not pass an address it found statically to the
//! machine, or compare it with `machine.pc()`. `hex(n)` goes integer -> string;
//! `addr(s)` is the way back.
use fission_loader::loader::LoadedBinary;
use fission_script::{
    ScriptLimits, ScriptOptions, ScriptRunResult, ScriptRunStatus, run_script_with,
};

fn run(source: &str) -> ScriptRunResult {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../fission-emulator/testdata/win_x64_write.exe"
    );
    let binary = LoadedBinary::from_file(path).expect("load the fixture");
    run_script_with(
        &binary,
        source,
        "test.rhai",
        ScriptLimits::default(),
        ScriptOptions { machine: false },
    )
}

fn data(result: &ScriptRunResult) -> serde_json::Value {
    assert_eq!(
        result.status,
        ScriptRunStatus::Ok,
        "{:?}",
        result.diagnostics
    );
    result.findings[0].data.clone().expect("finding data")
}

#[test]
fn addr_reads_a_hex_string_and_a_decimal_string() {
    let result =
        run(r#"emit(#{ kind: "t", a: addr("0x401000"), b: addr("4198400"), c: addr("0X10") });"#);
    let d = data(&result);
    assert_eq!(d["a"], serde_json::json!(0x401000));
    assert_eq!(d["b"], serde_json::json!(4_198_400));
    assert_eq!(d["c"], serde_json::json!(0x10));
}

#[test]
fn addr_is_the_inverse_of_hex() {
    let result = run(r#"
        let n = 0x140001040;
        emit(#{ kind: "t", back: addr(hex(n)), same: addr(hex(n)) == n });
        "#);
    let d = data(&result);
    assert_eq!(d["same"], serde_json::json!(true), "{d}");
}

/// The case the issue describes: an address found in the static view is an
/// integer a script can compute with and hand to the machine.
#[test]
fn a_static_view_address_becomes_an_integer() {
    let result = run(r#"
        let base = addr(binary.image_base());
        let first = binary.functions()[0];
        let fa = addr(first.address);
        emit(#{ kind: "t", offset_is_nonneg: fa >= base, delta_ok: (fa - base) >= 0 });
        "#);
    let d = data(&result);
    assert_eq!(d["offset_is_nonneg"], serde_json::json!(true), "{d}");
}

/// Not an address: a script error, not a silent zero that would set a
/// breakpoint at address 0.
#[test]
fn addr_refuses_what_is_not_an_address() {
    for bad in ["", "0x", "zz", "0xzz", "-1", "12 34"] {
        let result = run(&format!(r#"emit(#{{ kind: "t", a: addr("{bad}") }});"#));
        assert_ne!(
            result.status,
            ScriptRunStatus::Ok,
            "addr({bad:?}) must be an error, got {:?}",
            result.findings
        );
    }
}
