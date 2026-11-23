//! Quotients of the step functions in `testdata/quotient_fixture.S` (and its
//! AArch64 twin), with the real emulator as the membership oracle.

use std::path::PathBuf;

use fission_loader::loader::LoadedBinary;
use fission_quotient::calls::CalleeClass;
use fission_quotient::report::{SlotRole, TransitionReport};
use fission_quotient::{
    CertifiedQuotient, ContextRegion, Exit, InputPort, QuotientBoundary, QuotientBudget,
    QuotientEngine, QuotientStatus, Wbcq,
};

const X86_64: &str = "testdata/quotient_fixture.elf";
const AARCH64: &str = "testdata/quotient_fixture_aarch64.elf";

fn fixture(file: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(file)
}

fn address_of(file: &str, name: &str) -> u64 {
    let binary = LoadedBinary::from_file(&fixture(file)).expect("load fixture");
    let inner = binary.inner();
    let index = *inner
        .function_name_index
        .get(name)
        .unwrap_or_else(|| panic!("{name} in the fixture's symbols"));
    inner.functions[index].address
}

/// `int step(struct ctx *ctx, unsigned cmd)` with an 8-byte zeroed context.
fn quotient_in(file: &str, name: &str, budget: QuotientBudget) -> CertifiedQuotient {
    let boundary = QuotientBoundary {
        entry: address_of(file, name),
        inputs: vec![InputPort {
            name: "cmd".to_string(),
            arg_index: 1,
            bits: 32,
        }],
        context: ContextRegion {
            arg_index: 0,
            initial: vec![0; 8],
        },
        output_bits: 32,
    };
    let (mut engine, foundation) =
        Wbcq::for_binary(&fixture(file), boundary, 100_000).expect("engine over the fixture");
    engine
        .synthesize(&foundation, budget)
        .expect("synthesis runs")
}

fn quotient_with(name: &str, budget: QuotientBudget) -> CertifiedQuotient {
    quotient_in(X86_64, name, budget)
}

fn quotient(name: &str) -> CertifiedQuotient {
    quotient_with(name, QuotientBudget::default())
}

fn transition<'a>(report: &'a CertifiedQuotient, from: usize, guard: &str) -> &'a TransitionReport {
    report
        .transitions
        .iter()
        .find(|t| t.from == from && t.guard == guard)
        .unwrap_or_else(|| {
            panic!(
                "no transition from {from} on {guard}: {:#?}",
                report.transitions
            )
        })
}

/// The flattened dispatcher collapses to the three protocol states, with the
/// opaque predicate's dead side refuted rather than kept, the accumulator
/// left as a data register, and every pair of states told apart by a word
/// the emulator reproduces.
#[test]
fn a_flattened_dispatcher_collapses_to_its_protocol_states() {
    let report = quotient("auth_step");
    // Proved for H, then deepened to the Moore bound: 3 states + K_max 32.
    assert_eq!(
        report.status,
        QuotientStatus::ProvedUnderModel {
            horizon: 35,
            model_id: "separated-registers-frame-context.v1".to_string(),
        }
    );

    let roles = report
        .slots
        .iter()
        .map(|slot| (slot.offset, slot.role))
        .collect::<Vec<_>>();
    assert_eq!(roles, [(0, SlotRole::Control), (4, SlotRole::Data)]);

    assert_eq!(report.states.len(), 3);
    let modes = report
        .states
        .iter()
        .map(|state| state.control_valuation[0].value)
        .collect::<Vec<_>>();
    assert_eq!(modes, [0, 1, 2], "IDLE, CHALLENGE, AUTH_OK");
    assert_eq!(report.states[1].access_word, [vec![0x5a]]);
    assert_eq!(report.states[2].access_word, [vec![0x5a], vec![0xa5]]);

    assert_eq!(report.transitions.len(), 5);
    assert_eq!(transition(&report, 0, "(cmd[7:0] == 0x5a)").to, 1);
    assert_eq!(transition(&report, 0, "(cmd[7:0] != 0x5a)").to, 0);
    let accept = transition(&report, 1, "(cmd[7:0] == 0xa5)");
    assert_eq!((accept.to, accept.output.as_str()), (2, "1"));
    assert_eq!(transition(&report, 1, "(cmd[7:0] != 0xa5)").to, 0);
    assert_eq!(transition(&report, 2, "true").to, 2);
    for t in &report.transitions {
        assert_eq!(t.updates.len(), 1);
        assert_eq!(t.updates[0].value, "(ctx[0x4:4] + cmd)");
    }

    // The `test dl, 1; jne` of the opaque predicate: its taken side is dead.
    assert_eq!(report.metrics.pruned_branches.len(), 1);
    assert!(report.metrics.pruned_branches[0].taken);
    assert!(report.metrics.pcode_blocks > report.states.len());

    assert_eq!(report.apartness_witnesses.len(), 3);
    for witness in &report.apartness_witnesses {
        assert!(witness.verified_by_emulator);
        assert_ne!(witness.obs_a, witness.obs_b);
    }
}

#[test]
fn the_report_is_byte_for_byte_deterministic() {
    let first = quotient("auth_step").to_json().unwrap();
    let second = quotient("auth_step").to_json().unwrap();
    assert_eq!(first, second);
}

/// Mode alternates 0, 3, 0, ... and reaches the result, but both values
/// give the same result: two control valuations, one state.
#[test]
fn observably_equal_control_valuations_share_a_state() {
    let report = quotient("toggle_step");
    assert!(matches!(
        report.status,
        QuotientStatus::ProvedUnderModel { .. }
    ));
    assert_eq!(report.slots.len(), 1);
    assert_eq!(report.slots[0].role, SlotRole::Control);
    assert_eq!(report.states.len(), 1);
    assert_eq!(report.transitions.len(), 1);
    assert_eq!(report.transitions[0].output, "1");
}

/// Merging the two toggle valuations is right for the result and wrong for
/// the accumulator, whose update differs between them: the quotient must
/// say it is refuted, with a word the emulator confirms.
#[test]
fn a_data_effect_the_control_states_cannot_express_is_refuted() {
    let report = quotient("split_step");
    let QuotientStatus::Refuted { counterexample_hex } = &report.status else {
        panic!("expected a refutation, got {:?}", report.status);
    };
    assert!(!counterexample_hex.is_empty());
    let roles = report
        .slots
        .iter()
        .map(|slot| slot.role)
        .collect::<Vec<_>>();
    assert_eq!(roles, [SlotRole::Control, SlotRole::Data]);
}

/// A counter whose value is the result has as many states as values. The
/// learner must stop at the budget and report a lower bound, not a small
/// wrong machine.
#[test]
fn a_counter_exhausts_the_state_budget_and_says_so() {
    let budget = QuotientBudget {
        max_states: 8,
        ..QuotientBudget::default()
    };
    let report = quotient_with("counter_step", budget);
    let QuotientStatus::PartialFrontier { max_states_hit, .. } = report.status else {
        panic!("expected a partial frontier, got {:?}", report.status);
    };
    assert!(max_states_hit > 8);
    assert!(report.transitions.is_empty());
}

#[test]
fn effects_outside_the_boundary_are_unsupported() {
    for (name, needle) in [
        ("clock_step", "user-defined operation"),
        // An unknown callee handed the context may write it: a call is
        // modelled, but this one stays outside the model.
        (
            "calling_step",
            "a call passes an address of the context to toggle_step",
        ),
    ] {
        let report = quotient(name);
        let QuotientStatus::UnsupportedSemantics { reason } = &report.status else {
            panic!("{name}: expected unsupported, got {:?}", report.status);
        };
        assert!(reason.contains(needle), "{name}: {reason}");
        assert!(report.states.is_empty() && report.transitions.is_empty());
    }
}

/// The context pointer and the command survive a round trip through the
/// stack frame, as unoptimised code spills them.
#[test]
fn values_spilled_to_the_frame_keep_their_meaning() {
    let report = quotient("latch_step");
    assert!(matches!(
        report.status,
        QuotientStatus::ProvedUnderModel { .. }
    ));
    assert_eq!(report.states.len(), 2);
    assert_eq!(transition(&report, 0, "((cmd & 1) == 1)").to, 1);
    assert_eq!(transition(&report, 1, "true").output, "1");
}

/// A refutation the solver cannot finish is never a proof: with no conflicts
/// allowed, the same latch can only be reported as empirically consistent.
#[test]
fn an_undecided_refutation_is_evidence_not_proof() {
    let budget = QuotientBudget {
        conflict_budget: Some(0),
        ..QuotientBudget::default()
    };
    let report = quotient_with("latch_step", budget);
    let QuotientStatus::EmpiricallyConsistent {
        probes_run,
        solver_unknowns,
        ..
    } = report.status
    else {
        panic!("an undecided query must not certify: {:?}", report.status);
    };
    assert!(probes_run > 0 && solver_unknowns > 0);
    // The states are still real: each pair has a witness the emulator ran.
    assert_eq!(report.states.len(), 2);
    assert!(
        report
            .apartness_witnesses
            .iter()
            .all(|w| w.verified_by_emulator)
    );
}

/// The shape of a quotient with the ISA's spelling of guards taken out.
fn skeleton(report: &CertifiedQuotient) -> Vec<(usize, String, usize)> {
    let mut edges = report
        .transitions
        .iter()
        .map(|t| (t.from, t.output.clone(), t.to))
        .collect::<Vec<_>>();
    edges.sort();
    edges
}

/// The same dispatcher written for AArch64 -- a link register instead of a
/// pushed return address, `tbnz` instead of `test; jne` for the opaque
/// predicate -- has the same quotient: nothing in the engine is x86's.
#[test]
fn the_same_protocol_on_aarch64_has_the_same_quotient() {
    let x86 = quotient("auth_step");
    let arm = quotient_in(AARCH64, "auth_step", QuotientBudget::default());
    assert_eq!(arm.status, x86.status);
    assert_eq!(skeleton(&arm), skeleton(&x86));
    let access = |r: &CertifiedQuotient| {
        r.states
            .iter()
            .map(|s| s.access_word.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(access(&arm), access(&x86));
    assert_eq!(arm.metrics.pruned_branches.len(), 1);
    assert!(
        arm.apartness_witnesses
            .iter()
            .all(|w| w.verified_by_emulator)
    );
}

/// `cmove` lifts to a branch that lands on the next instruction, in the
/// middle of a block; the summary must follow it there.
#[test]
fn a_branch_into_the_middle_of_a_block_is_followed() {
    let report = quotient("select_step");
    assert!(
        matches!(report.status, QuotientStatus::ProvedUnderModel { .. }),
        "{:?}",
        report.status
    );
    assert_eq!(report.states.len(), 2);
    assert_eq!(transition(&report, 0, "(cmd == 7)").to, 1);
}

fn assert_proved(report: &CertifiedQuotient) {
    assert!(
        matches!(report.status, QuotientStatus::ProvedUnderModel { .. }),
        "{:?}",
        report.status
    );
    assert!(
        report
            .apartness_witnesses
            .iter()
            .all(|w| w.verified_by_emulator)
    );
}

/// A call is an effect, not a refusal. The classifier's answer is the
/// environment's input `ret0` -- every symbol carries the command and it --
/// and the call with its one declared argument is part of what each step
/// shows. Its two run states are told apart only through what it returns.
#[test]
fn a_classifier_call_is_an_observed_effect_with_an_environment_chosen_return() {
    let report = quotient("digits_step");
    assert_proved(&report);
    assert_eq!(report.states.len(), 2);
    let [call] = report.calls.as_slice() else {
        panic!("one call site: {:?}", report.calls);
    };
    assert_eq!(call.callee.as_deref(), Some("isdigit"));
    assert_eq!(call.class, CalleeClass::Library);
    assert!(!call.tail);
    // `int isdigit(int)`: the first argument register only.
    assert!(call.arg_bits[0] >= 32);
    assert!(call.arg_bits[1..].iter().all(|bits| *bits == 0));
    assert!(
        report
            .evidence
            .alphabet
            .iter()
            .all(|symbol| symbol.len() == 2)
    );
    assert!(
        report
            .transitions
            .iter()
            .all(|t| t.calls == ["isdigit(zext64(cmd))"] && t.exit == Exit::Returned)
    );
    assert!(report.transitions.iter().any(|t| t.guard.contains("ret0")));
    assert!(
        report
            .assumptions
            .iter()
            .any(|a| a.description.contains("chosen by the environment"))
    );
    assert!(
        report.apartness_witnesses.iter().all(|w| w
            .obs_a
            .iter()
            .chain(&w.obs_b)
            .all(|o| o.calls.len() == 1))
    );
}

/// A callee the static no-return analysis proves never returns ends the
/// step, and the machine halts there: a sink state that only says so.
#[test]
fn a_call_that_does_not_return_halts_the_machine() {
    let report = quotient("guarded_step");
    assert_proved(&report);
    assert_eq!(report.states.len(), 3);
    assert_eq!(report.calls.len(), 1);
    assert_eq!(report.calls[0].class, CalleeClass::NoReturn);
    let aborting = transition(&report, 1, "(cmd == 0)");
    assert_eq!(aborting.exit, Exit::NoReturn);
    assert_eq!(aborting.calls, ["abort()"]);
    let sink = transition(&report, aborting.to, "true");
    assert_eq!((sink.exit, sink.to), (Exit::Halted, aborting.to));
}

/// A jump out of the slice is a call whose return is the step's: the result
/// is whatever the callee returns, and the argument it is given tells the
/// states apart.
#[test]
fn a_tail_call_returns_what_the_callee_returns() {
    let report = quotient("tail_step");
    assert_proved(&report);
    assert_eq!(report.states.len(), 2);
    assert!(report.calls[0].tail);
    assert!(report.transitions.iter().all(|t| t.output == "ret0[31:0]"));
    assert_eq!(transition(&report, 0, "true").calls, ["isdigit(1)"]);
    assert_eq!(transition(&report, 1, "true").calls, ["isdigit(0)"]);
}

/// The `cmov`'s branch lands on a `nop`, an instruction with no P-code: it
/// does nothing and falls through, so the branch is followed to the next
/// instruction rather than taken for a jump out of the slice.
#[test]
fn a_branch_to_an_instruction_without_pcode_falls_through_it() {
    let report = quotient("padded_select_step");
    assert_proved(&report);
    assert_eq!(report.states.len(), 2);
    assert_eq!(transition(&report, 0, "(cmd == 7)").to, 1);
}
