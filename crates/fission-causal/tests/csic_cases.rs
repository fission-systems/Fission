//! CSIC on its own fixtures: each test pairs a positive control (a cause
//! the runner must find, or a valid intervention it must accept) with an
//! adversarial negative control (a byte a baseline wrongly blames, an
//! intervention it must refuse, an experiment it must not attribute).
//!
//! Budgets are small so the tests stay fast in a debug build; the full
//! evaluation is `examples/csic_eval.rs`.

use fission_causal::contract::{
    CandidateOrigin, InvalidReason, JointSearch, JointStatus, Location, Minimality,
    SinkObservation, Verdict,
};
use fission_causal::eval::{self, CaseSpec};
use fission_causal::runner::{Case, CaseReport, Config, InternalPolicy, ModelOutcome, run_case};
use fission_causal::trace::{self, EndState};
use fission_causal::{MemPredicate, Program, SinkPoint, SinkSpec};

fn spec(name: &str) -> CaseSpec {
    eval::cases()
        .into_iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("no case {name}"))
}

fn small() -> Config {
    Config {
        max_candidates: 6,
        values_per_candidate: 3,
        max_oracles: 400,
        max_pairs: 8,
        ..Config::default()
    }
}

/// Load a case's program, find its sink and requests from a discovery run,
/// and run CSIC with `config`.
fn run(name: &str, config: &Config) -> (CaseReport, Program) {
    let spec = spec(name);
    let path = eval::fixture_dir().join(spec.fixture);
    let program = Program::load(&path.to_string_lossy(), &spec.input).expect("load");
    let discovery = trace::record(
        &program,
        &SinkSpec::MemoryValue {
            addr: 0,
            len: 1,
            predicate: MemPredicate::NonZero,
            at: SinkPoint::Exit,
        },
        &[],
        config.max_steps,
    )
    .expect("discovery");
    let sink = (spec.sink)(&program, &discovery).expect("sink");
    let requested = (spec.requested)(&program, &discovery).expect("requests");
    let requested_sets = spec
        .sets
        .iter()
        .map(|set| {
            set.iter()
                .map(|&(offset, value)| {
                    let (step, addr) = discovery
                        .steps
                        .iter()
                        .find_map(|s| {
                            s.input
                                .iter()
                                .find(|b| b.offset == offset)
                                .map(|b| (s.step + 1, b.addr))
                        })
                        .expect("delivered");
                    fission_causal::Intervention {
                        step,
                        location: Location::Memory { addr, len: 1 },
                        value: vec![value],
                    }
                })
                .collect()
        })
        .collect();
    let report = run_case(
        &Case {
            name: name.to_string(),
            program: &program,
            sink,
            requested,
            requested_sets,
        },
        config,
    )
    .expect("run");
    (report, program)
}

fn witness_offsets(report: &CaseReport) -> Vec<usize> {
    report.witness_inputs.clone()
}

fn requested(report: &CaseReport, index: usize) -> &Verdict {
    &report.requested[index].verdict
}

#[test]
fn the_rfc_case_reproduces_natively_observable_state() {
    // Positive control for everything below: the emulator's recording of
    // the RFC fixture ends the way the native run does (exit 1, "ADMIN").
    let spec = spec("rfc_auth_x64_O0");
    let path = eval::fixture_dir().join(spec.fixture);
    let program = Program::load(&path.to_string_lossy(), &spec.input).expect("load");
    let sink = SinkSpec::MemoryValue {
        addr: program.find_global("session").expect("session") + 8,
        len: 4,
        predicate: MemPredicate::Equals(vec![1, 0, 0, 0]),
        at: SinkPoint::Exit,
    };
    let recorded = trace::record(&program, &sink, &[], 100_000).expect("record");
    assert_eq!(recorded.end, EndState::Exited { code: Some(1) });
    assert_eq!(recorded.stdout, b"ADMIN\n");
    assert!(matches!(
        recorded.observation,
        SinkObservation::Holds { .. }
    ));
    // Input bytes were attributed to offsets in delivery order.
    let offsets: Vec<usize> = recorded.inputs.iter().map(|b| b.offset).collect();
    assert_eq!(offsets, (0..32).collect::<Vec<_>>());
}

#[test]
fn requested_interventions_split_into_witness_non_cause_and_invalid() {
    let config = Config {
        max_candidates: 0,
        max_pairs: 0,
        ..small()
    };
    let (report, program) = run("invalid_interventions", &config);
    assert!(report.refusal.is_none(), "{:?}", report.refusal);
    // Positive control: req_buf[3] := 0 un-grants admin.
    assert!(
        matches!(requested(&report, 0), Verdict::CounterfactualWitness { witness } if witness.fresh_replay_agrees),
        "{:?}",
        requested(&report, 0)
    );
    assert_eq!(report.requested[0].input_offset, Some(3));
    // Adversarial negative control: the logged byte 9 is not a cause.
    assert!(matches!(requested(&report, 1), Verdict::NotACause { .. }));
    assert_eq!(report.requested[1].input_offset, Some(9));
    let invalid = |i: usize| match requested(&report, i) {
        Verdict::InvalidIntervention { reason } => reason.clone(),
        other => panic!("request {i}: expected invalid, got {other:?}"),
    };
    assert_eq!(invalid(2), InvalidReason::ProgramCounter);
    assert!(matches!(invalid(3), InvalidReason::UnknownRegister { .. }));
    assert_eq!(invalid(4), InvalidReason::Unmapped { addr: 0x10 });
    assert!(matches!(invalid(5), InvalidReason::CodePage { .. }));
    assert_eq!(
        invalid(6),
        InvalidReason::WidthMismatch {
            expected: 1,
            got: 2
        }
    );
    assert_eq!(invalid(7), InvalidReason::NoOp);
    assert!(matches!(invalid(8), InvalidReason::NotBeforeSink { .. }));
    let _ = program;
}

#[test]
fn the_rfc_cause_is_found_and_logged_bytes_are_not_blamed() {
    let (report, _) = run("rfc_auth_x64_O0", &small());
    assert!(report.refusal.is_none(), "{:?}", report.refusal);
    // Positive control: the RFC's byte is a witness.
    assert!(
        witness_offsets(&report).contains(&3),
        "{:?}",
        report.witness_inputs
    );
    // Negative control: no logged byte (8..11) is a witness.
    assert!(
        !witness_offsets(&report).iter().any(|o| (8..12).contains(o)),
        "{:?}",
        report.witness_inputs
    );
    // The primary tier is exactly the slice's input bytes, and with an
    // input witness found nothing else was tried.
    let inputs: Vec<usize> = report
        .candidates
        .iter()
        .filter(|c| c.origin == CandidateOrigin::InputByte)
        .filter_map(|c| c.input_offset)
        .collect();
    assert_eq!(inputs.len(), report.counts.slice_input_bytes);
    assert_eq!(report.counts.tested_outside_slice, 0);
    assert_eq!(report.counts.tested_internal, 0);
    assert_eq!(report.counts.oracle_executions_internal, 0);
}

#[test]
fn the_rfc_cause_holds_on_aarch64_too() {
    let (report, _) = run("rfc_auth_a64_O0", &small());
    assert!(report.refusal.is_none(), "{:?}", report.refusal);
    assert!(
        witness_offsets(&report).contains(&3),
        "{:?}",
        report.witness_inputs
    );
    assert!(!witness_offsets(&report).iter().any(|o| (8..12).contains(o)));
}

#[test]
fn a_clock_dependent_grant_is_refused_and_a_clock_free_one_is_not() {
    let (time, _) = run("nondet_time", &small());
    let refusal = time.refusal.as_deref().unwrap_or("");
    assert!(
        refusal.contains("clock_gettime") || refusal.contains("time"),
        "{refusal}"
    );
    assert_eq!(time.joint, JointStatus::Refused);
    assert!(time.candidates.is_empty());
    // Positive control: the same shape of program without the clock read is
    // attributed (joint_and's grant also depends on req[0]).
    let (clean, _) = run("joint_and", &small());
    assert!(clean.refusal.is_none(), "{:?}", clean.refusal);
    assert!(witness_offsets(&clean).contains(&0));
}

#[test]
fn entropy_dependent_grant_is_refused() {
    let (report, _) = run("nondet_random", &small());
    let refusal = report.refusal.as_deref().unwrap_or("");
    assert!(refusal.contains("getrandom"), "{refusal}");
}

#[test]
fn an_overdetermined_grant_is_reported_as_joint_not_as_no_cause() {
    let (or, _) = run("joint_or", &small());
    assert!(or.refusal.is_none());
    // Negative control: neither byte alone is a witness.
    assert!(or.witness_inputs.is_empty(), "{:?}", or.witness_inputs);
    // Positive: the pair is. The recorded run never read the second byte
    // (`||` short-circuits); the solver reached it by negating the first
    // branch and searching the replayed model's own path.
    match &or.joint {
        JointStatus::JointWitness {
            input_offsets,
            found_by,
            minimality,
            contingencies,
            ..
        } => {
            let mut offsets: Vec<usize> = input_offsets.iter().flatten().copied().collect();
            offsets.sort_unstable();
            assert_eq!(offsets, vec![0, 1]);
            assert!(
                matches!(found_by, JointSearch::Solver { .. }),
                "{found_by:?}"
            );
            assert!(
                matches!(minimality, Minimality::MinimalWithinBudget { .. }),
                "{minimality:?}"
            );
            // Each byte is a witness under the other's contingency.
            assert_eq!(contingencies.len(), 2);
            assert!(contingencies.iter().all(|c| c.holds), "{contingencies:?}");
        }
        other => panic!("expected a joint witness, got {other:?}"),
    }
    // Contrast: a conjunction has single-byte witnesses and needs no pair.
    let (and, _) = run("joint_and", &small());
    let mut w = and.witness_inputs.clone();
    w.sort_unstable();
    assert_eq!(w, vec![0, 1]);
    assert_eq!(and.joint, JointStatus::NotNeeded);
}

#[test]
fn a_crash_is_caused_by_the_selector_not_by_the_stored_value() {
    let (report, _) = run("crash_null_x64", &small());
    assert!(report.refusal.is_none(), "{:?}", report.refusal);
    assert!(matches!(report.sink, SinkSpec::FaultAt { .. }));
    assert!(
        report.witness_inputs.contains(&0),
        "{:?}",
        report.witness_inputs
    );
    assert!(
        !report.witness_inputs.contains(&1),
        "{:?}",
        report.witness_inputs
    );
}

#[test]
fn a_real_alias_is_a_cause_through_both_index_and_value() {
    let (report, _) = run("real_alias", &small());
    let mut w = report.witness_inputs.clone();
    w.sort_unstable();
    assert_eq!(w, vec![2, 9]);
}

#[test]
fn reports_are_deterministic() {
    let config = Config {
        max_candidates: 4,
        values_per_candidate: 2,
        ..small()
    };
    let (a, _) = run("implicit_call_arg", &config);
    let (b, _) = run("implicit_call_arg", &config);
    // Positive control: the report is not trivially empty.
    assert!(a.witness_inputs.contains(&5), "{:?}", a.witness_inputs);
    assert_eq!(a.deterministic_json(), b.deterministic_json());
}

#[test]
fn every_witness_names_where_it_intervened() {
    let (report, _) = run("password_x64", &small());
    let witnesses: Vec<_> = report
        .candidates
        .iter()
        .filter_map(|c| match &c.verdict {
            Verdict::CounterfactualWitness { witness } => Some((c, witness)),
            _ => None,
        })
        .collect();
    assert!(!witnesses.is_empty());
    for (c, w) in witnesses {
        assert_eq!(w.step, c.step);
        assert_eq!(w.location, c.location);
        assert_ne!(w.original_bytes, w.intervened_bytes);
        assert_ne!(w.original_sink.holds(), w.counterfactual_sink.holds());
        assert!(w.fresh_replay_agrees);
        if let Location::Memory { len, .. } = w.location {
            assert_eq!(w.intervened_bytes.len(), len as usize);
        }
    }
}

#[test]
fn a_check_whose_failing_arm_never_ran_needs_potential_dependence() {
    // Positive control: with potential dependence (the default) the four
    // secret-compare bytes reach the slice and each is a witness.
    let (with, _) = run("password_x64", &small());
    let mut w = with.witness_inputs.clone();
    w.sort_unstable();
    assert_eq!(w, vec![0, 1, 2, 3]);
    assert!(
        with.candidates
            .iter()
            .filter(|c| c.input_offset.is_some()
                && matches!(c.verdict, Verdict::CounterfactualWitness { .. }))
            .all(|c| c.origin == CandidateOrigin::InputByte)
    );
    // Negative control: a plain dynamic slice never sees them, because the
    // `ok = 0` store that would have depended on them never executed. The
    // explanation survives anyway, from the second tier (the delivered
    // bytes outside the slice): its recall is not the slice's.
    let plain = Config {
        potential_dependence: false,
        max_pairs: 0,
        ..small()
    };
    let (without, _) = run("password_x64", &plain);
    assert_eq!(without.counts.slice_input_bytes, 0);
    let mut w = without.witness_inputs.clone();
    w.sort_unstable();
    assert_eq!(w, vec![0, 1, 2, 3]);
    assert!(
        without
            .candidates
            .iter()
            .filter(|c| matches!(c.verdict, Verdict::CounterfactualWitness { .. }))
            .all(|c| c.origin == CandidateOrigin::InputByteOutsideSlice)
    );
}

#[test]
fn input_first_spends_no_oracles_on_internal_locations_once_an_input_witness_exists() {
    // Positive control: asked to, the runner still tests internal
    // locations, and they cost oracles.
    let always = Config {
        internal_locations: InternalPolicy::Always,
        ..small()
    };
    let (all, _) = run("branch_len", &always);
    assert!(all.counts.tested_internal > 0);
    assert!(all.counts.oracle_executions_internal > 0);
    // Default: the input witness ends the search before any internal
    // location, with the same explanation.
    let (first, _) = run("branch_len", &small());
    assert_eq!(first.counts.tested_internal, 0);
    assert_eq!(first.counts.oracle_executions_internal, 0);
    assert_eq!(first.witness_inputs, all.witness_inputs);
    assert_eq!(first.witness_inputs, vec![0]);
    assert!(first.counts.oracle_executions < all.counts.oracle_executions);
}

#[test]
fn a_three_way_and_needs_all_three_bytes_with_their_values() {
    let (report, _) = run("and3_x64", &small());
    assert!(report.refusal.is_none(), "{:?}", report.refusal);
    // No single byte, and no value-set pair, flips a request that gets
    // all three wrong.
    assert!(
        report.witness_inputs.is_empty(),
        "{:?}",
        report.witness_inputs
    );
    match &report.joint {
        JointStatus::JointWitness {
            input_offsets,
            interventions,
            minimality,
            contingencies,
            ..
        } => {
            let offsets: Vec<usize> = input_offsets.iter().flatten().copied().collect();
            assert_eq!(offsets, vec![0, 1, 2], "the decoy req[3] is not in it");
            let values: Vec<u8> = interventions.iter().map(|iv| iv.value[0]).collect();
            assert_eq!(values, b"KEY".to_vec());
            assert!(
                matches!(
                    minimality,
                    Minimality::MinimalWithinBudget {
                        subsets_checked: 3,
                        ..
                    }
                ),
                "{minimality:?}"
            );
            assert!(contingencies.iter().all(|c| c.holds), "{contingencies:?}");
        }
        other => panic!("expected a joint witness, got {other:?}"),
    }
    // Every model the solver produced was replayed before use: one
    // symbolic replay for the recorded input, one per new model.
    let replayed = report
        .solver
        .models
        .iter()
        .filter(|m| m.outcome != ModelOutcome::Duplicate)
        .count();
    assert!(replayed > 0);
    assert_eq!(report.solver.symbolic_replays as usize, replayed + 1);
    // The caller's controls: all three right changes Φ (positive); two of
    // three, or two plus the decoy, do not (adversarial).
    let sets: Vec<bool> = report
        .requested_sets
        .iter()
        .map(|s| s.changes_sink)
        .collect();
    assert_eq!(sets, vec![true, false, false]);
    assert!(report.requested_sets.iter().all(|s| s.replays_agree));
}

#[test]
fn an_xor_cause_is_two_singles_and_flipping_both_is_not_a_cause() {
    let (report, _) = run("xor_gate_x64_O0", &small());
    // Positive control: each byte alone is a witness.
    let mut w = report.witness_inputs.clone();
    w.sort_unstable();
    assert_eq!(w, vec![0, 1]);
    assert_eq!(report.joint, JointStatus::NotNeeded);
    // Adversarial: the union of two witnesses, each flipped the same way,
    // leaves Φ alone -- a joint intervention is not the union of singles.
    let sets: Vec<bool> = report
        .requested_sets
        .iter()
        .map(|s| s.changes_sink)
        .collect();
    assert_eq!(sets, vec![false, true]);
    // The decoy compared on the path is not a witness.
    assert!(!report.witness_inputs.contains(&2));
}

#[test]
fn a_sink_reached_only_in_a_signal_handler_is_caused_by_the_fault_selector() {
    let (report, program) = run("segv_grant_x64", &small());
    assert!(report.refusal.is_none(), "{:?}", report.refusal);
    // The recording ran through the delivered fault into the handler.
    let recorded = trace::record(&program, &report.sink, &[], 100_000).expect("record");
    assert_eq!(
        recorded.steps.iter().filter(|s| s.fault.is_some()).count(),
        1
    );
    assert!(matches!(
        recorded.observation,
        SinkObservation::Holds { .. }
    ));
    // Positive control: the selector is the cause. Adversarial: the stored
    // value flows into the faulting store but is not.
    assert_eq!(report.witness_inputs, vec![0]);
    let sets: Vec<bool> = report
        .requested_sets
        .iter()
        .map(|s| s.changes_sink)
        .collect();
    assert_eq!(sets, vec![true, false]);
}

#[test]
fn a_solver_that_cannot_decide_is_reported_as_unknown_not_as_no_cause() {
    // Positive control: with its default budget the solver finds the
    // conjunction's joint witness.
    let (found, _) = run("joint_and_deny", &small());
    assert!(
        matches!(found.joint, JointStatus::JointWitness { .. }),
        "{:?}",
        found.joint
    );
    // No conflicts allowed: every query that needs search is Unknown, and
    // with the pair search off that is what the case reports.
    let starved = Config {
        solver_conflicts: 0,
        max_pairs: 0,
        ..small()
    };
    let (unknown, _) = run("joint_and_deny", &starved);
    assert!(
        matches!(unknown.joint, JointStatus::SolverUnknown { .. }),
        "{:?}",
        unknown.joint
    );
    assert!(unknown.solver.unknown > 0);
}

#[test]
fn solver_backed_reports_are_deterministic() {
    let (a, _) = run("joint_and_deny", &small());
    let (b, _) = run("joint_and_deny", &small());
    // Positive control: the search produced models at all.
    assert!(!a.solver.models.is_empty());
    assert_eq!(a.deterministic_json(), b.deterministic_json());
}

#[test]
fn a_solver_model_the_replay_does_not_reproduce_is_rejected() {
    // Positive control: the reachable carry. The solver's model of the
    // stored word replays as predicted and is a verified joint witness
    // (one of the two minimal ones).
    let (carry, _) = run("carry", &small());
    match &carry.joint {
        JointStatus::JointWitness {
            input_offsets,
            found_by,
            ..
        } => {
            let offsets: Vec<usize> = input_offsets.iter().flatten().copied().collect();
            assert!(
                offsets == vec![0, 2] || offsets == vec![1, 2],
                "{offsets:?}"
            );
            assert!(matches!(found_by, JointSearch::Solver { .. }));
        }
        other => panic!("expected a joint witness, got {other:?}"),
    }
    assert!(
        carry
            .solver
            .models
            .iter()
            .all(|m| !matches!(m.outcome, ModelOutcome::Rejected { .. }))
    );
    // Adversarial: compared to 2, the high byte of a two-byte sum is out of
    // reach. The symbolic layer sees the whole sum where the program reads
    // one byte of it and proposes a model; the replay takes the other way,
    // the model is rejected, and the case says Unknown -- not "no cause"
    // and not a witness.
    let (impossible, _) = run("carry_impossible", &small());
    assert!(
        impossible
            .solver
            .models
            .iter()
            .any(|m| matches!(m.outcome, ModelOutcome::Rejected { .. })),
        "{:?}",
        impossible.solver.models
    );
    assert!(
        matches!(&impossible.joint, JointStatus::SolverUnknown { reason, .. } if reason.contains("did not reproduce")),
        "{:?}",
        impossible.joint
    );
    assert!(impossible.witness_inputs.is_empty());
    assert!(impossible.requested_sets.iter().all(|s| !s.changes_sink));
}
