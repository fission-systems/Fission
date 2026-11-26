//! Intel HEX, Motorola S-record and UF2 files name no processor. Given
//! `--language` they load as the image their records describe; without it
//! they are refused with the way out. Fixtures are built here, record by
//! record, so each test says what bytes it holds.
use serde_json::Value;
use std::path::PathBuf;
use std::process::{Command, Output};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fission_cli"))
        .args(args)
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .output()
        .expect("run fission_cli")
}

/// `mov eax, [esp+4]; add eax, eax; ret`
const X86_DOUBLE: [u8; 7] = [0x8b, 0x44, 0x24, 0x04, 0x01, 0xc0, 0xc3];

fn hex_digits(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

fn ihex(address: u16, record_type: u8, payload: &[u8]) -> String {
    let mut bytes = vec![
        payload.len() as u8,
        (address >> 8) as u8,
        address as u8,
        record_type,
    ];
    bytes.extend_from_slice(payload);
    let sum = bytes.iter().fold(0u8, |acc, b| acc.wrapping_add(*b));
    bytes.push(sum.wrapping_neg());
    format!(":{}", hex_digits(&bytes))
}

fn srec(kind: u8, address: u32, payload: &[u8]) -> String {
    let mut body = address.to_be_bytes().to_vec();
    body.extend_from_slice(payload);
    let mut bytes = vec![(body.len() + 1) as u8];
    bytes.extend_from_slice(&body);
    let sum = bytes.iter().fold(0u8, |acc, b| acc.wrapping_add(*b));
    bytes.push(!sum);
    format!("S{kind}{}", hex_digits(&bytes))
}

/// One file per test, so tests in one process never share a path.
fn write_fixture(test: &str, name: &str, records: &[String]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("fission-fw-{}-{test}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, records.join("\n") + "\n").unwrap();
    path
}

/// `X86_DOUBLE` at 0x1000 with a start-address record pointing at it.
fn x86_hex(test: &str) -> PathBuf {
    write_fixture(
        test,
        "fw.hex",
        &[
            ihex(0x1000, 0x00, &X86_DOUBLE),
            ihex(0, 0x05, &0x1000u32.to_be_bytes()),
            ihex(0, 0x01, &[]),
        ],
    )
}

#[test]
fn intel_hex_disassembles_and_decompiles_under_a_language() {
    let file = x86_hex("intel_hex_disassembles_and_decompiles_under_a_language");
    let path = file.to_str().unwrap();

    let list = run(&[
        "--language",
        "x86:LE:32:default",
        "list",
        path,
        "--json",
        "--no-db",
    ]);
    assert!(list.status.success(), "{list:?}");
    let functions: Value = serde_json::from_slice(&list.stdout).expect("json");
    let listed = functions.to_string();
    assert!(listed.contains("entry"), "{listed}");

    let disasm = run(&[
        "--language",
        "x86:LE:32:default",
        "disasm",
        path,
        "--addr",
        "0x1000",
        "-n",
        "3",
        "--json",
        "--no-db",
    ]);
    assert!(disasm.status.success(), "{disasm:?}");
    let rows: Value = serde_json::from_slice(&disasm.stdout).expect("json");
    assert_eq!(rows[0]["address"], "0x1000");
    assert_eq!(rows[1]["instruction"], "add EAX,EAX");
    assert_eq!(rows[2]["instruction"], "ret");

    let decomp = run(&[
        "--language",
        "x86:LE:32:default",
        "decomp",
        path,
        "--addr",
        "0x1000",
        "--no-db",
    ]);
    assert!(decomp.status.success(), "{decomp:?}");
    let code = String::from_utf8_lossy(&decomp.stdout);
    assert!(code.contains("<< 1"), "{code}");
}

#[test]
fn motorola_srecord_loads_a_thumb_image_at_its_address() {
    // adds r0, r0, r1; bx lr -- start record carries the Thumb bit.
    let file = write_fixture(
        "motorola_srecord_loads_a_thumb_image_at_its_address",
        "fw.srec",
        &[
            srec(3, 0x0800_0000, &[0x40, 0x18, 0x70, 0x47]),
            srec(7, 0x0800_0001, &[]),
        ],
    );
    let out = run(&[
        "--language",
        "ARM:LE:32:Cortex",
        "disasm",
        file.to_str().unwrap(),
        "--addr",
        "0x8000000",
        "-n",
        "2",
        "--json",
        "--no-db",
    ]);
    assert!(out.status.success(), "{out:?}");
    let rows: Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(rows[0]["instruction"], "adds r0,r0,r1");
    assert_eq!(rows[1]["instruction"], "bx lr");
}

#[test]
fn without_a_language_a_record_file_is_refused_with_the_way_out() {
    let file = x86_hex("without_a_language_a_record_file_is_refused_with_the_way_out");
    let out = run(&["list", file.to_str().unwrap(), "--no-db"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("LoadSpecNotFound"), "{stderr}");
    assert!(stderr.contains("--language"), "{stderr}");
}

#[test]
fn patch_refuses_a_decoded_record_file() {
    let file = x86_hex("patch_refuses_a_decoded_record_file");
    let output = file.with_file_name("patched.hex");
    let out = run(&[
        "--language",
        "x86:LE:32:default",
        "patch",
        file.to_str().unwrap(),
        "--addr",
        "0x1000",
        "--nop",
        "1",
        "--output",
        output.to_str().unwrap(),
        "--no-db",
    ]);
    assert!(!out.status.success(), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("decoded into a different image"),
        "{out:?}"
    );
    assert!(
        !output.exists(),
        "no flat image may be written in its place"
    );
}

/// One UF2 block (512 bytes, little-endian fields) for an RP2040 flash
/// address, with the family-ID flag set.
fn uf2_block(target: u32, payload: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; 512];
    let fields = [
        0x0A32_4655u32,
        0x9E5D_5157,
        0x0000_2000,
        target,
        payload.len() as u32,
        0,
        1,
        0xe48b_ff56,
    ];
    for (index, value) in fields.iter().enumerate() {
        out[index * 4..index * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    out[32..32 + payload.len()].copy_from_slice(payload);
    out[508..].copy_from_slice(&0x0AB1_6F30u32.to_le_bytes());
    out
}

fn write_uf2(test: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("fission-fw-{}-{test}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("fw.uf2");
    // adds r0, r0, r1; bx lr
    std::fs::write(&path, uf2_block(0x1000_0100, &[0x40, 0x18, 0x70, 0x47])).unwrap();
    path
}

#[test]
fn uf2_decompiles_under_a_language_at_a_thumb_address() {
    let file = write_uf2("uf2_decompiles_under_a_language_at_a_thumb_address");
    let path = file.to_str().unwrap();
    let disasm = run(&[
        "--language",
        "ARM:LE:32:Cortex",
        "disasm",
        path,
        "--addr",
        "0x10000101",
        "-n",
        "2",
        "--json",
        "--no-db",
    ]);
    assert!(disasm.status.success(), "{disasm:?}");
    let rows: Value = serde_json::from_slice(&disasm.stdout).expect("json");
    assert_eq!(rows[0]["instruction"], "adds r0,r0,r1");

    let decomp = run(&[
        "--language",
        "ARM:LE:32:Cortex",
        "decomp",
        path,
        "--addr",
        "0x10000101",
        "--no-db",
    ]);
    assert!(decomp.status.success(), "{decomp:?}");
    let code = String::from_utf8_lossy(&decomp.stdout);
    assert!(code.contains("param_1 + param_2"), "{code}");
}

#[test]
fn uf2_without_a_language_names_its_family() {
    let file = write_uf2("uf2_without_a_language_names_its_family");
    let out = run(&["info", file.to_str().unwrap(), "--no-db"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("RP2040"), "{stderr}");
    assert!(stderr.contains("--language"), "{stderr}");
}

#[test]
fn info_reports_no_entry_when_the_file_states_none() {
    let file = write_fixture(
        "info_reports_no_entry_when_the_file_states_none",
        "noentry.hex",
        &[ihex(0x1000, 0x00, &X86_DOUBLE), ihex(0, 0x01, &[])],
    );
    let path = file.to_str().unwrap();
    let args = ["--language", "x86:LE:32:default", "info", path, "--no-db"];
    let json = run(&[&args[..], &["--json"]].concat());
    assert!(json.status.success(), "{json:?}");
    let info: Value = serde_json::from_slice(&json.stdout).expect("json");
    assert_eq!(info["entry"], "0x0");
    assert_eq!(info["entry_declared"], false);
    assert_eq!(info["image_base"], "0x1000");
    assert_eq!(info["functions"], 0);
    let text = run(&args);
    let text = String::from_utf8_lossy(&text.stdout);
    let entry = text
        .lines()
        .find(|line| line.contains("Entry:"))
        .expect("entry row");
    assert!(entry.contains("none"), "{entry}");

    let declared = x86_hex("info_reports_no_entry_when_the_file_states_none_declared");
    let json = run(&[
        "--language",
        "x86:LE:32:default",
        "info",
        declared.to_str().unwrap(),
        "--json",
        "--no-db",
    ]);
    let info: Value = serde_json::from_slice(&json.stdout).expect("json");
    assert_eq!(info["entry"], "0x1000");
    assert!(info.get("entry_declared").is_none(), "{info}");
}

#[test]
fn a_multibyte_srecord_line_is_an_error_not_a_crash() {
    let file = write_fixture(
        "a_multibyte_srecord_line_is_an_error_not_a_crash",
        "bad.srec",
        &["S0030000FC".to_string(), "S\u{20ac}0000".to_string()],
    );
    let out = run(&["info", file.to_str().unwrap(), "--no-db"]);
    assert!(!out.status.success());
    assert_ne!(out.status.code(), Some(101), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("MalformedHeader"),
        "{out:?}"
    );
}
