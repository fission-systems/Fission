//! The `--json` output contract: the *shape* of every command's JSON -- which
//! keys exist, what type each holds, how they nest -- is pinned, the values
//! are not.
//!
//! The rule is the one an agent reading the output relies on: **adding a key
//! is allowed; removing a key, renaming one, or changing its type fails.**
//! Each test runs one command against a checked-in fixture, reduces the output
//! to its shape, and checks it against `tests/json_contracts/<name>.json`.
//!
//! How shapes are read:
//! - A value is `null`, `bool`, `number`, `string`, `{"array": shape}`,
//!   `{"object": {key: shape}}`, or `{"map": shape}`.
//! - An object whose keys are all numbers or hex addresses (`syscalls` by
//!   number, `names` by address) is data, not schema: it becomes a `map`, and
//!   only its values' shape is pinned.
//! - The elements of an array are merged: an object element's pinned keys are
//!   the keys *every* element has. An empty array pins nothing about its
//!   elements.
//! - `null` in either the pinned shape or the new output matches anything:
//!   an optional value that happens to be absent is not a type change.
//! - Decompiler telemetry (`TELEMETRY_KEYS`: `NirBuildStats` and its
//!   relatives) is pinned as "an object", not key by key. Those counters are
//!   internal and change with the decompiler; `NirBuildStats` is their
//!   contract (AGENTS.md), not this file.
//!
//! **Updating a pinned shape** after a deliberate contract change (say why in
//! the commit, and bump that command's `schema_version` if it has one):
//!
//! ```text
//! FISSION_UPDATE_JSON_CONTRACTS=1 cargo nextest run -p fission-cli --test json_contract_cli
//! ```
//!
//! That rewrites every file under `tests/json_contracts/` from the current
//! output; review the diff -- a removed line is a broken consumer.
//!
//! Every test also runs a positive control: it deletes one pinned key from the
//! real output and checks that the comparison then fails, so a checker that
//! accepts everything cannot pass.

use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};
use std::process::Command;

const A64: &str = "crates/fission-sleigh/testdata/a64_csel_select.so";
const GROUND_TRUTH: &str = "crates/fission-dir/testdata/ground_truth_fixture.elf";
const CONCOLIC: &str = "crates/fission-emulator/testdata/x64_concolic_branch_sys.elf";
const DWARF5: &str = "crates/fission-loader/testdata/x64_dwarf5_strx.elf";
const PDB_EXE: &str = "crates/fission-loader/testdata/x64_pdb_struct_test.exe";
const WIN_EXE: &str = "crates/fission-emulator/testdata/win_x64_write.exe";

fn repo_root() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

fn contracts_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/json_contracts"))
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("fission-contract-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run_json(args: &[String]) -> Value {
    let out = Command::new(env!("CARGO_BIN_EXE_fission_cli"))
        .args(args)
        .env("FISSION_BENCHMARK_NO_CACHE", "1")
        .current_dir(repo_root())
        .output()
        .expect("run fission_cli");
    assert!(
        out.status.success(),
        "`fission_cli {}` failed:\n{}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "`fission_cli {}` did not print one JSON document ({e}):\n{}",
            args.join(" "),
            String::from_utf8_lossy(&out.stdout)
        )
    })
}

// ── Shapes ──────────────────────────────────────────────────────────────────

/// Objects whose keys are internal counters rather than a promise to readers.
const TELEMETRY_KEYS: &[&str] = &[
    "preview_build_stats",
    "preview_hint_stats",
    "nir_build_stats",
];

fn is_data_key(key: &str) -> bool {
    let hex = key.strip_prefix("0x").unwrap_or(key);
    !hex.is_empty()
        && (key.chars().all(|c| c.is_ascii_digit())
            || (key.starts_with("0x") && hex.chars().all(|c| c.is_ascii_hexdigit())))
}

/// The shape of a JSON value (see the module docs).
fn shape(value: &Value) -> Value {
    match value {
        Value::Null => json!("null"),
        Value::Bool(_) => json!("bool"),
        Value::Number(_) => json!("number"),
        Value::String(_) => json!("string"),
        Value::Array(items) => json!({ "array": merged(items.iter().map(shape)) }),
        Value::Object(map) if !map.is_empty() && map.keys().all(|k| is_data_key(k)) => {
            json!({ "map": merged(map.values().map(shape)) })
        }
        Value::Object(map) => json!({
            "object": map
                .iter()
                .map(|(k, v)| {
                    let pinned = if TELEMETRY_KEYS.contains(&k.as_str()) && v.is_object() {
                        json!({ "object": {} })
                    } else {
                        shape(v)
                    };
                    (k.clone(), pinned)
                })
                .collect::<Map<_, _>>()
        }),
    }
}

/// One shape for several values: an object keeps the keys all of them have.
fn merged(shapes: impl Iterator<Item = Value>) -> Value {
    shapes.reduce(merge).unwrap_or_else(|| json!("any"))
}

fn merge(a: Value, b: Value) -> Value {
    match (a, b) {
        (a, b) if a == b => a,
        (Value::String(s), other) | (other, Value::String(s)) if s == "null" || s == "any" => other,
        (Value::Object(a), Value::Object(b)) => {
            if let (Some(Value::Object(x)), Some(Value::Object(y))) =
                (a.get("object"), b.get("object"))
            {
                let keys: Map<String, Value> = x
                    .iter()
                    .filter_map(|(k, v)| y.get(k).map(|w| (k.clone(), merge(v.clone(), w.clone()))))
                    .collect();
                return json!({ "object": keys });
            }
            for kind in ["array", "map"] {
                if let (Some(x), Some(y)) = (a.get(kind), b.get(kind)) {
                    return json!({ kind: merge(x.clone(), y.clone()) });
                }
            }
            // An object in one element and a map in another: the values are data.
            json!("any")
        }
        _ => json!("any"),
    }
}

/// Where `actual` breaks the contract `pinned` describes: every pinned key
/// still present, with a compatible type.
fn violations(pinned: &Value, actual: &Value, path: &str, out: &mut Vec<String>) {
    let lenient = |s: &Value| matches!(s.as_str(), Some("null" | "any"));
    if lenient(pinned) || lenient(actual) {
        return;
    }
    match (pinned, actual) {
        (Value::String(p), Value::String(a)) => {
            if p != a {
                out.push(format!("{path}: type changed from {p} to {a}"));
            }
        }
        (Value::Object(p), Value::Object(a)) => {
            if let Some(Value::Object(keys)) = p.get("object") {
                let Some(Value::Object(actual_keys)) = a.get("object") else {
                    out.push(format!("{path}: was an object, now {}", kind_of(actual)));
                    return;
                };
                for (key, sub) in keys {
                    match actual_keys.get(key) {
                        Some(next) => violations(sub, next, &format!("{path}/{key}"), out),
                        None => out.push(format!("{path}/{key}: removed")),
                    }
                }
            } else if let Some(elem) = p.get("array") {
                match a.get("array") {
                    Some(next) => violations(elem, next, &format!("{path}/*"), out),
                    None => out.push(format!("{path}: was an array, now {}", kind_of(actual))),
                }
            } else if let Some(value) = p.get("map") {
                match (a.get("map"), a.get("object")) {
                    (Some(next), _) => violations(value, next, &format!("{path}/*"), out),
                    (None, Some(Value::Object(fields))) => {
                        for (key, next) in fields {
                            violations(value, next, &format!("{path}/{key}"), out);
                        }
                    }
                    _ => out.push(format!("{path}: was a map, now {}", kind_of(actual))),
                }
            }
        }
        _ => out.push(format!(
            "{path}: was {}, now {}",
            kind_of(pinned),
            kind_of(actual)
        )),
    }
}

fn kind_of(shape: &Value) -> String {
    match shape {
        Value::String(s) => s.clone(),
        Value::Object(o) => o.keys().next().cloned().unwrap_or_default(),
        _ => "?".into(),
    }
}

/// Delete the first key the pinned shape requires, wherever it occurs in
/// `value`. Returns the path of the deleted key, or `None` if the shape pins
/// no key at all (which a contract must, or the control would prove nothing).
fn drop_a_pinned_key(pinned: &Value, value: &mut Value) -> Option<String> {
    fn first_key_path(shape: &Value) -> Option<Vec<String>> {
        let obj = shape.as_object()?;
        if let Some(Value::Object(keys)) = obj.get("object") {
            let (key, _) = keys.iter().next()?;
            return Some(vec![key.clone()]);
        }
        let inner = obj.get("array").or_else(|| obj.get("map"))?;
        let mut rest = first_key_path(inner)?;
        rest.insert(0, "*".into());
        Some(rest)
    }
    fn remove(value: &mut Value, path: &[String]) -> bool {
        match (path, value) {
            ([last], Value::Object(map)) if last != "*" => map.remove(last).is_some(),
            ([head, rest @ ..], Value::Array(items)) if head == "*" => items
                .iter_mut()
                .fold(false, |hit, item| remove(item, rest) || hit),
            ([head, rest @ ..], Value::Object(map)) if head == "*" => map
                .values_mut()
                .fold(false, |hit, item| remove(item, rest) || hit),
            _ => false,
        }
    }
    let path = first_key_path(pinned)?;
    remove(value, &path).then(|| path.join("/"))
}

fn check_contract(name: &str, args: &[String]) {
    let output = run_json(args);
    let actual = shape(&output);
    let file = contracts_dir().join(format!("{name}.json"));

    if std::env::var_os("FISSION_UPDATE_JSON_CONTRACTS").is_some() {
        std::fs::create_dir_all(contracts_dir()).unwrap();
        let pinned = json!({ "command": args, "shape": actual });
        std::fs::write(&file, serde_json::to_string_pretty(&pinned).unwrap() + "\n").unwrap();
        return;
    }

    let pinned: Value =
        serde_json::from_str(&std::fs::read_to_string(&file).unwrap_or_else(|_| {
            panic!(
                "no pinned shape at {}; generate it with FISSION_UPDATE_JSON_CONTRACTS=1",
                file.display()
            )
        }))
        .expect("pinned shape is JSON");
    let pinned_shape = &pinned["shape"];

    let mut broken = Vec::new();
    violations(pinned_shape, &actual, "", &mut broken);
    assert!(
        broken.is_empty(),
        "`fission_cli {}` broke its JSON contract ({}):\n  {}\n\
         If the change is deliberate, regenerate with FISSION_UPDATE_JSON_CONTRACTS=1.",
        args.join(" "),
        relative(&file),
        broken.join("\n  ")
    );

    // Positive control: the same output with one pinned key deleted fails.
    let mut damaged = output.clone();
    let dropped = drop_a_pinned_key(pinned_shape, &mut damaged)
        .unwrap_or_else(|| panic!("{name}: the pinned shape pins no key to test with"));
    let mut caught = Vec::new();
    violations(pinned_shape, &shape(&damaged), "", &mut caught);
    assert!(
        caught.iter().any(|v| v.contains("removed")),
        "{name}: deleting `{dropped}` went unnoticed"
    );
}

fn relative(path: &Path) -> String {
    path.strip_prefix(repo_root())
        .unwrap_or(path)
        .display()
        .to_string()
}

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

macro_rules! contract {
    ($name:ident, [$($arg:expr),* $(,)?]) => {
        #[test]
        fn $name() {
            check_contract(stringify!($name), &args(&[$($arg),*]));
        }
    };
}

// ── Static views ────────────────────────────────────────────────────────────

contract!(info, ["info", A64, "--json", "--no-db"]);
contract!(
    info_sections,
    ["info", A64, "--sections", "--json", "--no-db"]
);
contract!(
    info_exports,
    ["info", A64, "--exports", "--json", "--no-db"]
);
contract!(
    info_imports,
    ["info", WIN_EXE, "--imports", "--json", "--no-db"]
);
contract!(
    info_detections,
    ["info", A64, "--detections", "--json", "--no-db"]
);
contract!(
    info_identity,
    ["info", PDB_EXE, "--identity", "--json", "--no-db"]
);
contract!(list, ["list", A64, "--json", "--no-db"]);
contract!(strings, ["strings", A64, "--json", "--no-db"]);
contract!(xrefs, ["xrefs", A64, "--json", "--no-db"]);
contract!(callgraph, ["callgraph", A64, "--json", "--no-db"]);
contract!(
    hex,
    [
        "hex", A64, "--addr", "0x10414", "--count", "16", "--json", "--no-db"
    ]
);
contract!(
    search,
    ["search", A64, "--text", "sel", "--json", "--no-db"]
);
contract!(
    disasm_window,
    [
        "disasm", A64, "--addr", "0x10414", "-n", "4", "--json", "--no-db"
    ]
);
contract!(
    disasm_function,
    [
        "disasm",
        A64,
        "--addr",
        "0x10414",
        "--function",
        "--json",
        "--no-db"
    ]
);
contract!(
    disasm_with_source,
    [
        "disasm",
        DWARF5,
        "--addr",
        "0x201130",
        "--function",
        "--json",
        "--no-db"
    ]
);
contract!(
    whatis,
    ["whatis", DWARF5, "--addr", "0x201140", "--json", "--no-db"]
);
contract!(types, ["types", DWARF5, "--json", "--no-db"]);
contract!(
    inventory_program_metadata,
    ["inventory", "program-metadata", A64]
);

// ── Decompilation and IR ────────────────────────────────────────────────────

contract!(
    decomp,
    ["decomp", A64, "--addr", "0x10414", "--json", "--no-db"]
);
contract!(
    decomp_with_globals,
    [
        "decomp",
        GROUND_TRUTH,
        "--addr",
        "0x201500",
        "--json",
        "--no-db"
    ]
);
contract!(
    raw_pcode,
    ["raw-pcode", A64, "--addr", "0x10414", "--json", "--no-db"]
);
contract!(
    pcode_stages,
    [
        "pcode-stages",
        A64,
        "--addr",
        "0x10414",
        "--json",
        "--no-db"
    ]
);
contract!(
    nir_stats,
    ["nir-stats", A64, "--addr", "0x10414", "--json", "--no-db"]
);
contract!(
    pcode_topology,
    [
        "pcode-topology",
        A64,
        "--addr",
        "0x10414",
        "--json",
        "--no-db"
    ]
);

// ── Signatures and environment ──────────────────────────────────────────────

contract!(identify, ["identify", A64, "--json", "--no-db"]);
contract!(
    identify_miss,
    [
        "identify",
        GROUND_TRUTH,
        "--function",
        "0x201510",
        "--json",
        "--no-db"
    ]
);
contract!(similar, ["similar", A64, "--json", "--no-db"]);
contract!(api, ["api", "memcpy", "--json"]);
contract!(languages, ["languages", "--json"]);
contract!(resources_status, ["resources", "status", "--json"]);
contract!(schema_envelope, ["schema"]);
contract!(schema_error_codes, ["schema", "--error-codes"]);
contract!(schema_list, ["schema", "--list"]);
contract!(
    contract_envelope,
    ["--contract", "v1", "list", A64, "--json", "--no-db"]
);
contract!(
    debug_capabilities,
    ["debug", "--emulator", "capabilities", "--json"]
);

// ── Dynamic ─────────────────────────────────────────────────────────────────

contract!(
    verify,
    [
        "verify",
        GROUND_TRUTH,
        "--addr",
        "0x2014e0",
        "--name",
        "pick",
        "--json"
    ]
);
contract!(
    call,
    [
        "call",
        GROUND_TRUTH,
        "--addr",
        "0x2014f0",
        "--arg",
        "5",
        "--json"
    ]
);
contract!(
    call_stubbed,
    [
        "call",
        GROUND_TRUTH,
        "--addr",
        "0x201640",
        "--stub-calls",
        "0",
        "--json"
    ]
);
contract!(
    sandbox_explore,
    [
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
    ]
);
contract!(
    sandbox_ttd,
    [
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
        "--coverage",
        "--taint",
        "--json",
    ]
);
contract!(
    debug_session,
    [
        "debug",
        "--emulator",
        "session",
        CONCOLIC,
        "--record",
        "-c",
        "step",
        "-c",
        "regs",
        "-c",
        "back-step",
        "-c",
        "read 0x400078 --size 4",
        "--json",
    ]
);

/// A batch prints one JSON line per function; its shape is the line's.
#[test]
fn verify_batch() {
    let dir = scratch("verify-batch");
    let list = dir.join("addrs.txt");
    std::fs::write(&list, "0x2014e0\n").unwrap();
    check_contract(
        "verify_batch",
        &args(&[
            "verify",
            GROUND_TRUTH,
            "--addresses-file",
            list.to_str().unwrap(),
            "--tier",
            "concrete",
        ]),
    );
}

/// `db show` with one decision of every kind, so its maps are not empty.
#[test]
fn db_show() {
    let dir = scratch("db-show");
    let db = dir.join("fixture.fission.json");
    let db = db.to_str().unwrap();
    for edit in [
        vec!["name", "0x2014e0", "pick_value"],
        vec!["sig", "0x2014e0", "int (bool b, int x)"],
        vec!["note", "0x2014e0", "returns x or -x"],
        vec!["bp", "0x2014e0"],
        vec!["watch", "0x202698", "--size", "8"],
    ] {
        let mut argv = vec!["--db", db, "db", GROUND_TRUTH];
        argv.extend(edit);
        argv.push("--json");
        run_json(&args(&argv));
    }
    check_contract(
        "db_show",
        &args(&["--db", db, "db", GROUND_TRUTH, "show", "--json"]),
    );
}

// ── The checker itself ──────────────────────────────────────────────────────

#[test]
fn the_checker_allows_additions_and_rejects_removal_rename_and_retyping() {
    let pinned = shape(&json!({ "a": 1, "b": "x", "list": [{ "k": true }] }));
    let check = |value: Value| {
        let mut out = Vec::new();
        violations(&pinned, &shape(&value), "", &mut out);
        out
    };
    // Additions, and a value that happens to be null, are fine.
    assert!(
        check(json!({ "a": 2, "b": "y", "list": [{ "k": false, "new": 1 }], "extra": [] }))
            .is_empty()
    );
    assert!(check(json!({ "a": null, "b": "y", "list": [] })).is_empty());
    // Removal, rename and a type change are not.
    assert!(check(json!({ "b": "y", "list": [] }))[0].contains("/a: removed"));
    assert!(!check(json!({ "a": 1, "renamed_b": "y", "list": [] })).is_empty());
    assert!(check(json!({ "a": "1", "b": "y", "list": [] }))[0].contains("type changed"));
    assert!(!check(json!({ "a": 1, "b": "y", "list": [{ "renamed": true }] })).is_empty());
    // Data-keyed objects pin their values' shape, not their keys.
    let maps = shape(&json!({ "syscalls": { "0": 1, "60": 1 } }));
    let mut out = Vec::new();
    violations(
        &maps,
        &shape(&json!({ "syscalls": { "231": 1 } })),
        "",
        &mut out,
    );
    assert!(out.is_empty(), "{out:?}");
    violations(
        &maps,
        &shape(&json!({ "syscalls": { "231": "one" } })),
        "",
        &mut out,
    );
    assert!(!out.is_empty());
}
