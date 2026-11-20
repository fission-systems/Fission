//! The shared initial state, on a fixture checked into this crate
//! (`testdata/ground_truth_fixture.c`).
//!
//! Every run draws arguments, registers and memory from one function of the
//! run number, and both the real machine code and the decompiled tree read
//! it. These pin the parts a check relies on: a pointer argument reads the
//! same bytes on both sides wherever it points, a pointer chain read from
//! that memory ends, the ABI's argument extension is honoured, and a global
//! reads the image.

use fission_dir::report::VerifyOutcome;
use fission_dir::{EmulatorHarness, Grounding, RunPlan, check_ground_truth_with, decompile_one};
use fission_loader::loader::{FunctionInfo, LoadedBinary};
use fission_static::analysis::decomp::facts::FactStore;
use std::path::PathBuf;

const RUNS: usize = 64;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/ground_truth_fixture.elf")
}

/// Check the fixture function `name` and return the outcome.
fn check(name: &str) -> VerifyOutcome {
    let path = fixture();
    let binary = LoadedBinary::from_file(&path).expect("load fixture");
    let address = {
        let inner = binary.inner();
        let index = *inner
            .function_name_index
            .get(name)
            .unwrap_or_else(|| panic!("{name} in the fixture's symbols"));
        inner.functions[index].address
    };
    let facts = FactStore::from_binary(&binary);
    let func = FunctionInfo {
        name: name.to_string(),
        address,
        ..Default::default()
    };
    let pair = decompile_one(&binary, &facts, &func).expect("decompile");
    let grounding =
        Grounding::for_function(&binary, address, &pair.globals, pair.frame_offsets.clone());
    let mut harness = EmulatorHarness::build(&path, Some(200_000)).expect("harness");
    check_ground_truth_with(
        &mut harness,
        address,
        &pair.prehir,
        &pair.hir,
        &RunPlan {
            runs: RUNS,
            ..RunPlan::default()
        },
        &grounding,
    )
}

fn assert_agrees(name: &str, at_least: usize) {
    match check(name) {
        VerifyOutcome::Equivalent { checked } => assert!(
            checked >= at_least,
            "{name}: only {checked} of {RUNS} runs compared"
        ),
        other => panic!("{name}: expected agreement, got {other:?}"),
    }
}

/// `p[0] + p[3]` with `p` a random word: both sides read the same bytes
/// there. A run whose pointer lands in the top page is skipped (that page
/// cannot be mapped, and faults as a non-canonical address would).
#[test]
fn memory_behind_a_pointer_argument_is_the_same_on_both_sides() {
    assert_agrees("first_plus_fourth", RUNS - 4);
}

/// A linked list walked through random memory: the edge words (null among
/// them) end it on both sides at the same node.
#[test]
fn a_pointer_chain_through_random_memory_ends_the_same_way() {
    assert_agrees("list_len", RUNS / 2);
}

/// `widen(signed char)` and `pick(bool, int)` agree on every run. The
/// decompiled tree types both first parameters as `int`, so this does not
/// exercise the narrow-argument extension itself -- that is
/// `initial_state::tests::arguments_keep_the_extension_the_abi_promises` --
/// only that random register words in those slots do not split the sides.
#[test]
fn narrow_and_bool_parameters_agree() {
    assert_agrees("widen", RUNS);
    assert_agrees("pick", RUNS);
}

/// A writable global reads the image on both sides.
#[test]
fn a_global_reads_the_program_image() {
    assert_agrees("read_counter", RUNS);
}

/// The stack canary: the decompiled tree declares `long long fs_offset;` and
/// reads `*(fs_offset + 40)` without assigning it -- the FS segment base at
/// entry. Bound to the machine's register, both sides read the same canary,
/// and the check at the epilogue passes on both.
#[test]
fn a_register_the_tree_declares_but_never_assigns_reads_its_entry_value() {
    assert_agrees("guarded_sum", RUNS);
}
