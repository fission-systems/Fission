//! `sandbox` reports what its dynamic analyses found, as data: the inputs a
//! symbolic exploration solved for each path, the machine at a time-travel
//! step, and a snapshot written to disk. Runs only checked-in or generated
//! fixtures -- never a corpus binary.
//!
//! `x64_concolic_branch_sys.elf` (source: `fission-emulator/testdata/src/
//! build_concolic_sys.py`) reads one byte of stdin at entry 0x400078, exits 0
//! from 0x4000a2 when it is `A`, and from 0x4000ab otherwise.
use serde_json::Value;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::time::{Duration, Instant};

const CONCOLIC: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../fission-emulator/testdata/x64_concolic_branch_sys.elf"
);

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fission_cli"))
        .args(args)
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .output()
        .expect("run fission_cli")
}

fn report(args: &[&str]) -> Value {
    let out = run(args);
    assert!(out.status.success(), "{out:?}");
    serde_json::from_slice(&out.stdout).expect("sandbox json")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("fission-sandbox-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

/// Given `B`, exploration toward the success branch solves the byte that
/// takes it. Without `--find` nothing is "found".
#[test]
fn sym_explore_solves_the_input_that_reaches_find() {
    let body = report(&[
        "sandbox",
        CONCOLIC,
        "--stdin-mock",
        "B",
        "--sym-explore",
        "--find",
        "0x4000a2",
        "--avoid",
        "0x4000ab",
        "--json",
    ]);
    let symbolic = &body["symbolic"];
    let found = symbolic["found"].as_array().expect("found");
    assert_eq!(found.len(), 1, "{symbolic}");
    assert_eq!(found[0]["pc"], 0x4000a2);
    let inputs = found[0]["inputs"].as_array().expect("solved inputs");
    assert_eq!(inputs.len(), 1, "{symbolic}");
    assert!(inputs[0]["name"].as_str().unwrap().starts_with("stdin_"));
    assert_eq!(inputs[0]["value"], u64::from(b'A'));

    let unguided = report(&[
        "sandbox",
        CONCOLIC,
        "--stdin-mock",
        "B",
        "--sym-explore",
        "--json",
    ]);
    assert_eq!(unguided["symbolic"]["found"], serde_json::json!([]));
    // Both ways out are still explored, each with an input that leads there.
    let ended = unguided["symbolic"]["deadended"].as_array().unwrap();
    assert_eq!(ended.len(), 2, "{unguided}");
    let values: Vec<u64> = ended
        .iter()
        .map(|s| s["inputs"][0]["value"].as_u64().unwrap())
        .collect();
    assert!(values.contains(&u64::from(b'A')), "{values:?}");
    assert!(values.iter().any(|v| *v != u64::from(b'A')), "{values:?}");
}

/// After the seek the report holds the machine at that step: `read`
/// returned 1 in RAX, and the bytes asked for are the entry's `sub rsp,0x10`.
#[test]
fn ttd_seek_reports_registers_and_memory_at_the_step() {
    let body = report(&[
        "sandbox",
        CONCOLIC,
        "--stdin-mock",
        "A",
        "--ttd-record",
        "1",
        "--ttd-seek",
        "8",
        "--ttd-read",
        "0x400078:4",
        "--json",
    ]);
    let state = &body["ttd_state"];
    assert_eq!(state["step"], 8, "{state}");
    assert_eq!(state["registers"]["RAX"], 1, "{state}");
    assert_eq!(state["memory"][0]["bytes"], "4883ec10", "{state}");
    // Not asked for, not reported.
    let plain = report(&["sandbox", CONCOLIC, "--stdin-mock", "A", "--json"]);
    assert!(plain.get("ttd_state").is_none());
}

#[test]
fn a_snapshot_is_written_and_restores() {
    let path = scratch("entry.snapshot");
    let path = path.to_str().unwrap();
    let out = run(&[
        "sandbox",
        CONCOLIC,
        "--stdin-mock",
        "A",
        "--snapshot-at",
        "0x400078",
        "--snapshot-out",
        path,
    ]);
    assert!(out.status.success(), "{out:?}");
    assert!(std::fs::metadata(path).unwrap().len() > 0);

    let restored = report(&[
        "sandbox",
        CONCOLIC,
        "--stdin-mock",
        "A",
        "--restore-snapshot",
        path,
        "--json",
    ]);
    assert_eq!(restored["exit_code"], 0, "{restored}");

    // An address the run never reaches leaves nothing to write, and says so.
    let missed = run(&[
        "sandbox",
        CONCOLIC,
        "--stdin-mock",
        "A",
        "--snapshot-at",
        "0x400099",
        "--snapshot-out",
        scratch("never.snapshot").to_str().unwrap(),
    ]);
    assert!(!missed.status.success());
    assert!(String::from_utf8_lossy(&missed.stderr).contains("never reached"));
}

/// The interpreter compiles no translation blocks; the JIT does.
#[test]
fn interp_runs_without_the_jit() {
    let jit = report(&["sandbox", CONCOLIC, "--stdin-mock", "A", "--json"]);
    let interp = report(&[
        "sandbox",
        CONCOLIC,
        "--stdin-mock",
        "A",
        "--interp",
        "--json",
    ]);
    assert!(
        jit["metrics"]["tbs_compiled"].as_u64().unwrap() > 0,
        "{jit}"
    );
    assert_eq!(interp["metrics"]["tbs_compiled"], 0, "{interp}");
    assert_eq!(interp["exit_code"], 0);
}

/// `jmp $` never ends on its own; the wall-clock limit ends it.
#[test]
fn timeout_ms_ends_a_run_that_never_would() {
    let spin = scratch("spin.elf");
    std::fs::write(&spin, elf_x64(&[0xeb, 0xfe])).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_fission_cli"))
        .args([
            "sandbox",
            spin.to_str().unwrap(),
            "--timeout-ms",
            "300",
            "--json",
        ])
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success(), "{status:?}");
            break;
        }
        if started.elapsed() > Duration::from_secs(120) {
            child.kill().ok();
            panic!("--timeout-ms 300 did not end the run");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// A minimal static x86-64 ELF: one RX segment at 0x400000, `code` at the
/// entry 0x400078 -- the same layout as `build_concolic_sys.py`.
fn elf_x64(code: &[u8]) -> Vec<u8> {
    let entry: u64 = 0x400078;
    let mut out = vec![0u8; 0x78];
    out[0..4].copy_from_slice(b"\x7fELF");
    out[4] = 2;
    out[5] = 1;
    out[6] = 1;
    out[16..18].copy_from_slice(&2u16.to_le_bytes());
    out[18..20].copy_from_slice(&0x3eu16.to_le_bytes());
    out[20..24].copy_from_slice(&1u32.to_le_bytes());
    out[24..32].copy_from_slice(&entry.to_le_bytes());
    out[32..40].copy_from_slice(&64u64.to_le_bytes());
    out[52..54].copy_from_slice(&64u16.to_le_bytes());
    out[54..56].copy_from_slice(&56u16.to_le_bytes());
    out[56..58].copy_from_slice(&1u16.to_le_bytes());
    let size = (0x78 + code.len()) as u64;
    let ph = &mut out[64..120];
    ph[0..4].copy_from_slice(&1u32.to_le_bytes());
    ph[4..8].copy_from_slice(&5u32.to_le_bytes());
    ph[16..24].copy_from_slice(&0x400000u64.to_le_bytes());
    ph[24..32].copy_from_slice(&0x400000u64.to_le_bytes());
    ph[32..40].copy_from_slice(&size.to_le_bytes());
    ph[40..48].copy_from_slice(&size.to_le_bytes());
    ph[48..56].copy_from_slice(&0x1000u64.to_le_bytes());
    out.extend_from_slice(code);
    out
}
