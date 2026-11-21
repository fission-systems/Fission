//! `decomp --json` carries what the code declares and where its locals live as
//! data -- the globals it names, the stack slots whose frame offset is proven,
//! and the callees it calls through a declared prototype -- and `verify --json`
//! carries each tier's result as fields beside the original strings.
//!
//! The fixture's source is `crates/fission-dir/testdata/ground_truth_fixture.c`.
use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;

fn fixture() -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../fission-dir/testdata/ground_truth_fixture.elf")
        .to_string_lossy()
        .into_owned()
}

fn cli(args: &[&str]) -> Vec<u8> {
    let output = Command::new(env!("CARGO_BIN_EXE_fission_cli"))
        .args(args)
        .env("FISSION_BENCHMARK_NO_CACHE", "1")
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .output()
        .expect("run fission_cli");
    assert!(output.status.success(), "{output:?}");
    output.stdout
}

fn address_of(name: &str) -> String {
    let functions: Value =
        serde_json::from_slice(&cli(&["list", &fixture(), "--json", "--no-db"])).expect("list");
    functions
        .as_array()
        .expect("function list")
        .iter()
        .find(|f| f["name"] == name)
        .and_then(|f| f["address"].as_str())
        .unwrap_or_else(|| panic!("{name} missing from the fixture"))
        .to_string()
}

fn decomp(name: &str, extra: &[&str]) -> Value {
    let address = address_of(name);
    let fixture = fixture();
    let mut args = extra.to_vec();
    args.extend(["decomp", &fixture, "--addr", &address, "--json"]);
    let body: Value = serde_json::from_slice(&cli(&args)).expect("decomp json");
    body[0].clone()
}

/// `read_counter` returns `counter + 1`: one global, named, typed and placed.
#[test]
fn decomp_json_lists_the_globals_the_code_declares() {
    let body = decomp("read_counter", &["--no-db"]);
    let globals = body["globals"].as_array().expect("globals array");
    let counter = globals
        .iter()
        .find(|g| g["name"] == "counter")
        .unwrap_or_else(|| panic!("no `counter` in {globals:?}"));
    assert!(
        counter["address"]
            .as_str()
            .is_some_and(|a| a.starts_with("0x"))
    );
    assert!(
        !counter["type"].as_str().unwrap_or("").is_empty(),
        "{counter}"
    );
    // The declaration is in the code, so the list is not inventing it.
    assert!(body["code"].as_str().unwrap().contains("counter"));
}

/// `pick_local` keeps a four-element volatile array on the stack, so it has
/// at least four slots below the frame address, listed in address order.
#[test]
fn decomp_json_lists_the_stack_frame() {
    let body = decomp("pick_local", &["--no-db"]);
    let frame = body["frame"].as_array().expect("frame array");
    assert!(frame.len() >= 4, "{frame:?}");
    let offsets: Vec<i64> = frame
        .iter()
        .map(|slot| slot["cfa_offset"].as_i64().expect("offset"))
        .collect();
    assert!(offsets.iter().all(|o| *o < 0), "{offsets:?}");
    assert!(offsets.windows(2).all(|w| w[0] <= w[1]), "{offsets:?}");
    assert!(frame.iter().all(|slot| slot["name"].is_string()));
}

/// A callee is listed only once someone declared its prototype: without the
/// signature `note` is a recovered call, with it a declared one.
#[test]
fn decomp_json_lists_callees_with_a_declared_prototype() {
    let without = decomp("reload_after_call", &["--no-db"]);
    assert_eq!(without["declared_callees"], serde_json::json!([]));

    let dir = std::env::temp_dir().join(format!("fission-layout-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let db = dir.join("gt.fission.json");
    let db = db.to_str().unwrap();
    let note = address_of("note");
    cli(&["--db", db, "db", &fixture(), "sig", &note, "void (long x)"]);
    let with = decomp("reload_after_call", &["--db", db]);
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(with["declared_callees"], serde_json::json!(["note"]));
}

/// The single-function JSON keeps each tier as the string it always was and
/// adds the same result as fields under `tiers`, with the seed that drew the
/// ground-truth runs.
#[test]
fn verify_json_carries_tier_fields_and_the_seed() {
    let address = address_of("pick");
    let body: Value = serde_json::from_slice(&cli(&[
        "verify",
        &fixture(),
        "--addr",
        &address,
        "--name",
        "pick",
        "--json",
        "--tier",
        "ground-truth",
        "--seed",
        "42",
    ]))
    .expect("verify json");
    assert_eq!(body["seed"], 42);
    assert!(body["ground_truth"].is_string(), "{body}");
    let tier = &body["tiers"]["ground_truth"];
    assert_eq!(tier["category"], "equivalent", "{body}");
    assert!(tier["checked"].as_u64().is_some_and(|n| n > 0), "{body}");
}
