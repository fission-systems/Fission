//! Memory on both sides of a ground-truth check, on the fixture checked into
//! this crate (`testdata/ground_truth_fixture.c`).
//!
//! A local whose address the body takes lives in the frame, at the slot the
//! machine uses: the decompiler places each stack slot relative to the
//! canonical frame address. A string literal is the bytes the decompiler
//! read it from.

use fission_dir::report::VerifyOutcome;
use fission_dir::{EmulatorHarness, Grounding, RunPlan, check_ground_truth_with, decompile_one};
use fission_loader::loader::{FunctionInfo, LoadedBinary};
use fission_static::analysis::decomp::facts::FactStore;
use std::path::PathBuf;

const RUNS: usize = 64;

fn check(name: &str) -> VerifyOutcome {
    check_in("ground_truth_fixture.elf", name)
}

fn check_in(fixture: &str, name: &str) -> VerifyOutcome {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("testdata")
        .join(fixture);
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
    let grounding = Grounding::for_function(&binary, address, &pair);
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

/// `int x = a * 3; bump(&x); return a + 1;` -- the call's argument is a
/// frame address, and the stubbed call is compared argument by argument, so
/// this agrees only when the tree's `&x` is the machine's.
#[test]
fn a_local_whose_address_a_call_receives_is_the_same_pointer_on_both_sides() {
    match check("give_local") {
        VerifyOutcome::Equivalent { checked } => assert_eq!(checked, RUNS),
        other => panic!("give_local: expected agreement, got {other:?}"),
    }
}

/// `"fixture text"[i & 7]`: the tree reads the literal's bytes where the
/// decompiler found them.
#[test]
fn a_string_literal_reads_the_bytes_it_was_read_from() {
    match check("literal_char") {
        VerifyOutcome::Equivalent { checked } => assert_eq!(checked, RUNS),
        other => panic!("literal_char: expected agreement, got {other:?}"),
    }
}

/// A local array indexed by the run's value, and a literal passed to a
/// call: both used to be refused (`AddressOfLocal`, "no address for
/// global"). Whatever the decompiled text gets right or wrong about them,
/// they are now checked. (`pass_literal` passes the wrong literal, but to a
/// callee whose prototype was recovered, so it is a suspect.)
#[test]
fn a_local_array_and_a_literal_argument_are_checked() {
    for name in ["pick_local", "pass_literal"] {
        let outcome = check(name);
        assert!(
            matches!(
                outcome,
                VerifyOutcome::Equivalent { .. }
                    | VerifyOutcome::Diverged(_)
                    | VerifyOutcome::Suspect(_)
            ),
            "{name}: expected a verdict, got {outcome:?}"
        );
    }
}

/// An `-O0` frame (`testdata/ground_truth_fixture_o0.c`): the buffer passed
/// to a call lives in the whole-frame backing, counted from the steady-state
/// stack pointer, while the frame pointer is the base the scalar slots are
/// named from. Placing every slot with one relation for the whole function
/// put the buffer `0x60` bytes off, and the call's argument differed.
#[test]
fn a_frame_with_two_bases_places_each_slot_by_its_own() {
    match check_in("ground_truth_fixture_o0.elf", "frame_buffer") {
        VerifyOutcome::Equivalent { checked } => assert_eq!(checked, RUNS),
        other => panic!("frame_buffer: expected agreement, got {other:?}"),
    }
}
