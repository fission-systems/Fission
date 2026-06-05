use std::fs;
use std::path::{Path, PathBuf};

use fission_loader::loader::LoadedBinary;
use fission_pcode::cfg::AddressCfgSnapshot;
use fission_static::analysis::control_flow_facts::{decode_memory_context_for, function_max_bytes};
use fission_sleigh::runtime::{
    build_instruction_cfg_snapshot, DecodeContract, InstructionCfgHints, RuntimeSleighFrontend,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct CfgFixture {
    name: String,
    binary: String,
    function_address: u64,
    fission_model: String,
    expect_full_match: bool,
    snapshot: AddressCfgSnapshot,
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

fn fixture_dir() -> PathBuf {
    repo_root().join("benchmark/cfg_parity/fixtures")
}

fn lift_function_snapshot(
    binary_path: &Path,
    entry_address: u64,
    model: &str,
) -> AddressCfgSnapshot {
    let binary = LoadedBinary::from_file(binary_path).expect("load binary");
    let load_spec = binary.load_spec().expect("load spec");
    let frontends =
        RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(load_spec).expect("frontends");
    let frontend = frontends.first().expect("frontend candidate");
    let address_state = frontend.normalize_low_bit_code_address(entry_address);
    let decode_entry_address = address_state.address;
    let max_bytes = function_max_bytes(&binary, decode_entry_address, 256 * 1024);
    let bytes = binary
        .view_bytes(decode_entry_address, max_bytes)
        .expect("view bytes");
    let memory_context = decode_memory_context_for(&binary, decode_entry_address, max_bytes);
    let lifted = frontend
        .lift_raw_pcode_function_with_context_and_memory_context(
            bytes,
            decode_entry_address,
            DecodeContract::decomp_function(512),
            &memory_context,
            address_state.context_override,
        )
        .expect("lift function");

    match model {
        "pcode_instruction_cfg" => {
            let ops = lifted
                .function
                .blocks
                .iter()
                .flat_map(|block| block.ops.iter().cloned())
                .collect::<Vec<_>>();
            let cfg_hints = InstructionCfgHints::from_memory_context(&memory_context);
            build_instruction_cfg_snapshot(
                decode_entry_address,
                &lifted.reachable_instruction_addresses,
                &lifted.instruction_lengths,
                &ops,
                &lifted.indirect_targets,
                &lifted.inferred_indirect_edges,
                &cfg_hints,
                false,
            )
        }
        "pcode_structuring" => AddressCfgSnapshot::from_pcode_structuring(&lifted.function),
        "pcode_cfg_builder" => {
            AddressCfgSnapshot::from_pcode_cfg_builder(&lifted.function).expect("cfg builder export")
        }
        other => panic!("unsupported fixture model {other}"),
    }
}

fn load_fixtures() -> Vec<CfgFixture> {
    let mut fixtures = Vec::new();
    for entry in fs::read_dir(fixture_dir()).expect("read fixture dir") {
        let entry = entry.expect("fixture dir entry");
        if entry.path().extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let fixture: CfgFixture =
            serde_json::from_str(&fs::read_to_string(entry.path()).expect("read fixture"))
                .expect("parse fixture");
        if fixture.expect_full_match {
            fixtures.push(fixture);
        }
    }
    fixtures.sort_by(|left, right| left.name.cmp(&right.name));
    fixtures
}

#[test]
fn cfg_parity_matches_ghidra_fixtures() {
    let fixtures = load_fixtures();
    assert!(
        !fixtures.is_empty(),
        "expected at least one fixture with expect_full_match=true"
    );

    for fixture in fixtures {
        let binary_path = repo_root().join(&fixture.binary);
        let actual = lift_function_snapshot(
            &binary_path,
            fixture.function_address,
            &fixture.fission_model,
        );
        assert_eq!(
            actual.block_starts, fixture.snapshot.block_starts,
            "{} block_starts",
            fixture.name
        );
        assert_eq!(actual.edges, fixture.snapshot.edges, "{} edges", fixture.name);
        assert_eq!(
            actual.exit_blocks, fixture.snapshot.exit_blocks,
            "{} exit_blocks",
            fixture.name
        );
        assert_eq!(
            actual.function_address, fixture.snapshot.function_address,
            "{} function_address",
            fixture.name
        );
    }
}
