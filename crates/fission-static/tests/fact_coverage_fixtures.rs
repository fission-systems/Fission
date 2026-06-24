//! Checked-in CFG fact coverage fixtures (Ghidra-free oracle).

use std::fs;
use std::path::{Path, PathBuf};

use fission_loader::loader::LoadedBinary;
use fission_static::analysis::control_flow_facts::{
    FunctionControlFlowFacts, control_flow_facts_for,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct FactsFixture {
    name: String,
    binary: String,
    addr: String,
    facts: FunctionControlFlowFacts,
}

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../benchmark/cfg_facts/fixtures")
}

fn parse_addr(value: &str) -> u64 {
    u64::from_str_radix(value.trim_start_matches("0x"), 16).expect("fixture addr")
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn function_max_bytes(binary: &LoadedBinary, entry: u64) -> usize {
    if let Some(func) = binary.function_at_exact(entry) {
        if func.size > 0 {
            return func.size as usize;
        }
    }
    let mut next = entry.saturating_add(256 * 1024);
    for info in &binary.functions {
        if info.address > entry && info.address < next {
            next = info.address;
        }
    }
    next.saturating_sub(entry) as usize
}

fn load_fixtures() -> Vec<FactsFixture> {
    let mut fixtures: Vec<FactsFixture> = Vec::new();
    for entry in fs::read_dir(fixture_dir()).expect("read fixture dir") {
        let entry = entry.expect("fixture dir entry");
        if entry.path().extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        fixtures.push(
            serde_json::from_str(&fs::read_to_string(entry.path()).expect("read fixture"))
                .expect("parse fixture"),
        );
    }
    fixtures.sort_by(|left, right| left.name.cmp(&right.name));
    fixtures
}

#[test]
fn fact_coverage_fixtures_match_control_flow_facts() {
    for fixture in load_fixtures() {
        let binary_path = repo_root().join(&fixture.binary);
        let binary = LoadedBinary::from_file(&binary_path)
            .unwrap_or_else(|err| panic!("{}: load binary: {err}", fixture.name));
        let entry = parse_addr(&fixture.addr);
        let facts = control_flow_facts_for(&binary);
        let actual = facts.facts_for_function(&binary, entry, function_max_bytes(&binary, entry));

        assert_eq!(
            actual.function_address, fixture.facts.function_address,
            "{}: function_address",
            fixture.name
        );
        assert_eq!(
            actual.labels, fixture.facts.labels,
            "{}: labels",
            fixture.name
        );
        assert_eq!(
            actual.flow_edges, fixture.facts.flow_edges,
            "{}: flow_edges",
            fixture.name
        );
        assert_eq!(
            actual.indirect_targets, fixture.facts.indirect_targets,
            "{}: indirect_targets",
            fixture.name
        );
        assert_eq!(
            actual.noreturn_callsites, fixture.facts.noreturn_callsites,
            "{}: noreturn_callsites",
            fixture.name
        );
    }
}
