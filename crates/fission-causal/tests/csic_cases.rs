//! CSIC on its own fixtures: each test pairs a positive control (a cause
//! the runner must find, or a valid intervention it must accept) with an
//! adversarial negative control (a byte a baseline wrongly blames, an
//! intervention it must refuse, an experiment it must not attribute).
//!
//! Budgets are small so the tests stay fast in a debug build; the full
//! evaluation is `examples/csic_eval.rs`.

use fission_causal::contract::{
    CandidateOrigin, InvalidReason, JointStatus, Location, SinkObservation, Verdict,
};
use fission_causal::eval::{self, CaseSpec};
use fission_causal::runner::{Case, CaseReport, Config, run_case};
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
    let report = run_case(
        &Case {
            name: name.to_string(),
            program: &program,
            sink,
            requested,
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
    // The dynamic slice's input candidates came first and in offset order.
    let inputs: Vec<usize> = report
        .candidates
        .iter()
        .filter(|c| c.origin == CandidateOrigin::InputByte)
        .filter_map(|c| c.input_offset)
        .collect();
    let mut sorted = inputs.clone();
    sorted.sort_unstable();
    assert_eq!(inputs, sorted);
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
    // Positive: the pair is, and the second byte came from the
    // counterfactual run's slice (the recorded run never read it).
    match &or.joint {
        JointStatus::JointWitness { input_offsets, .. } => {
            let mut offsets: Vec<usize> = input_offsets.iter().flatten().copied().collect();
            offsets.sort_unstable();
            assert_eq!(offsets, vec![0, 1]);
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
