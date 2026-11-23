//! The P1 controlled cases, their hand-written ground truth, and scoring
//! against the baselines.
//!
//! Every case is one of this crate's own fixtures (`testdata/src/*.c`),
//! one pinned input and one sink. Ground truth is written from the source
//! at input-byte granularity:
//!
//! - `causes`: bytes for which some single-byte change flips Φ;
//! - `joint`: bytes that flip Φ only together (no single one does);
//! - every other delivered byte is a non-cause;
//! - `refuse`: the case must be refused as non-deterministic.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};

use crate::baseline::{self, TaintResult};
use crate::contract::{
    Intervention, JointStatus, Location, MemPredicate, SinkPoint, SinkSpec, Verdict,
};
use crate::machine::Program;
use crate::runner::{self, Case, CaseReport, Config};
use crate::static_slice;
use crate::trace::{self, EndState, Trace};

/// What a case expects.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Truth {
    pub causes: Vec<usize>,
    pub joint: Vec<usize>,
    pub refuse: bool,
}

type SinkFn = fn(&Program, &Trace) -> Result<SinkSpec>;
type RequestFn = fn(&Program, &Trace) -> Result<Vec<Intervention>>;

pub struct CaseSpec {
    pub name: &'static str,
    pub fixture: &'static str,
    /// What the case exercises, for the report.
    pub property: &'static str,
    pub input: Vec<u8>,
    pub sink: SinkFn,
    pub requested: RequestFn,
    pub truth: Truth,
}

fn pad(prefix: &[u8], len: usize) -> Vec<u8> {
    let mut v = prefix.to_vec();
    while v.len() < len {
        v.push(b'x');
    }
    v
}

fn rfc_input() -> Vec<u8> {
    let mut v = b"abc\x7fdefgLOG!".to_vec();
    v.resize(32, b'x');
    v
}

fn sym(program: &Program, name: &str) -> Result<u64> {
    program
        .find_global(name)
        .or_else(|| program.find_function(name))
        .ok_or_else(|| anyhow!("{}: no symbol {name}", program.path))
}

fn func(program: &Program, name: &str) -> Result<u64> {
    program
        .find_function(name)
        .ok_or_else(|| anyhow!("{}: no function {name}", program.path))
}

fn none(_: &Program, _: &Trace) -> Result<Vec<Intervention>> {
    Ok(Vec::new())
}

fn is_admin_eq_1(program: &Program, _: &Trace) -> Result<SinkSpec> {
    Ok(SinkSpec::MemoryValue {
        addr: sym(program, "session")? + 8,
        len: 4,
        predicate: MemPredicate::Equals(vec![1, 0, 0, 0]),
        at: SinkPoint::Exit,
    })
}

fn grant7(program: &Program, _: &Trace) -> Result<SinkSpec> {
    Ok(SinkSpec::CallTo {
        target: func(program, "grant")?,
        args: vec![Some(7)],
    })
}

fn cfg_flag(program: &Program, _: &Trace) -> Result<SinkSpec> {
    Ok(SinkSpec::MemoryValue {
        addr: sym(program, "cfg")? + 4,
        len: 4,
        predicate: MemPredicate::Equals(vec![1, 0, 0, 0]),
        at: SinkPoint::Exit,
    })
}

fn account_admin(program: &Program, _: &Trace) -> Result<SinkSpec> {
    Ok(SinkSpec::MemoryValue {
        addr: sym(program, "account")? + 8,
        len: 4,
        predicate: MemPredicate::NonZero,
        at: SinkPoint::Exit,
    })
}

fn grant_any(program: &Program, _: &Trace) -> Result<SinkSpec> {
    Ok(SinkSpec::CallTo {
        target: func(program, "grant")?,
        args: Vec::new(),
    })
}

fn grant_admin(program: &Program, _: &Trace) -> Result<SinkSpec> {
    Ok(SinkSpec::CallTo {
        target: func(program, "grant_admin")?,
        args: vec![Some(1000)],
    })
}

/// The crash observed in the discovery run: whatever instruction faulted.
fn observed_fault(_: &Program, discovery: &Trace) -> Result<SinkSpec> {
    match &discovery.end {
        EndState::Faulted { pc, .. } => Ok(SinkSpec::FaultAt { pc: *pc }),
        other => Err(anyhow!("discovery run did not fault: {other:?}")),
    }
}

/// The first conditional branch `classify` executed, and the way it went.
fn classify_branch(program: &Program, discovery: &Trace) -> Result<SinkSpec> {
    let start = func(program, "classify")?;
    let step = discovery
        .steps
        .iter()
        .find(|s| s.pc >= start && matches!(s.flow, crate::cfg::Flow::CBranch { .. }))
        .ok_or_else(|| anyhow!("classify executed no conditional branch"))?;
    let crate::cfg::Flow::CBranch { target } = step.flow else {
        unreachable!()
    };
    Ok(SinkSpec::BranchOutcome {
        pc: step.pc,
        occurrence: 1,
        taken: step.next_pc == target,
    })
}

/// The read step that delivered input byte `offset`, plus one.
fn input_step(discovery: &Trace, offset: usize) -> Result<(u64, u64)> {
    discovery
        .steps
        .iter()
        .find_map(|s| {
            s.input
                .iter()
                .find(|b| b.offset == offset)
                .map(|b| (s.step + 1, b.addr))
        })
        .ok_or_else(|| anyhow!("input byte {offset} never delivered"))
}

/// Interventions that break the contract, plus a valid positive and a
/// valid negative control.
fn invalid_requests(program: &Program, discovery: &Trace) -> Result<Vec<Intervention>> {
    let (step, addr3) = input_step(discovery, 3)?;
    let (_, addr9) = input_step(discovery, 9)?;
    let main = func(program, "main")?;
    let reg = |name: &str| Location::Register {
        name: name.to_string(),
    };
    let sp_width = program
        .register(&program.sp_register)
        .map_or(8, |(_, _, s)| s as usize);
    Ok(vec![
        // Positive control: the RFC's cause.
        Intervention {
            step,
            location: Location::Memory { addr: addr3, len: 1 },
            value: vec![0x00],
        },
        // Negative control: a logged byte.
        Intervention {
            step,
            location: Location::Memory { addr: addr9, len: 1 },
            value: vec![0x00],
        },
        // The program counter.
        Intervention {
            step,
            location: reg(&program.pc_register),
            value: vec![0; sp_width],
        },
        // A register the language does not have.
        Intervention {
            step,
            location: reg("NOT_A_REGISTER"),
            value: vec![0; 8],
        },
        // Unmapped memory.
        Intervention {
            step,
            location: Location::Memory { addr: 0x10, len: 1 },
            value: vec![1],
        },
        // Code.
        Intervention {
            step,
            location: Location::Memory { addr: main, len: 1 },
            value: vec![0x90],
        },
        // Wrong width.
        Intervention {
            step,
            location: Location::Memory { addr: addr3, len: 1 },
            value: vec![0, 0],
        },
        // The recorded value itself.
        Intervention {
            step,
            location: Location::Memory { addr: addr3, len: 1 },
            value: vec![0x7f],
        },
        // After the run (and so after the sink).
        Intervention {
            step: discovery.steps.len() as u64 + 5,
            location: Location::Memory { addr: addr3, len: 1 },
            value: vec![0x00],
        },
    ])
}

/// The controlled cases.
pub fn cases() -> Vec<CaseSpec> {
    let rfc_truth = Truth {
        causes: vec![3],
        ..Truth::default()
    };
    vec![
        CaseSpec {
            name: "rfc_auth_x64_O0",
            fixture: "x64_rfc_auth.elf",
            property: "RFC case: implicit flow into session.is_admin; logged bytes 8..11 are a static false alias",
            input: rfc_input(),
            sink: is_admin_eq_1,
            requested: none,
            truth: rfc_truth.clone(),
        },
        CaseSpec {
            name: "rfc_auth_x64_O2",
            fixture: "x64_rfc_auth_o2.elf",
            property: "RFC case at -O2 (the branch may become data flow)",
            input: rfc_input(),
            sink: is_admin_eq_1,
            requested: none,
            truth: rfc_truth.clone(),
        },
        CaseSpec {
            name: "rfc_auth_a64_O0",
            fixture: "a64_rfc_auth.elf",
            property: "RFC case on AArch64 (ISA-agnostic path)",
            input: rfc_input(),
            sink: is_admin_eq_1,
            requested: none,
            truth: rfc_truth.clone(),
        },
        CaseSpec {
            name: "invalid_interventions",
            fixture: "x64_rfc_auth.elf",
            property: "contract violations are refused; valid positive/negative controls judged",
            input: rfc_input(),
            sink: is_admin_eq_1,
            requested: invalid_requests,
            truth: rfc_truth,
        },
        CaseSpec {
            name: "implicit_call_arg",
            fixture: "x64_implicit_call.elf",
            property: "implicit flow: a branch on req[5] picks grant()'s constant argument",
            input: pad(b"abcdeZgh", 16),
            sink: grant7,
            requested: none,
            truth: Truth {
                causes: vec![5],
                ..Truth::default()
            },
        },
        CaseSpec {
            name: "false_alias_ptr",
            fixture: "x64_false_alias_ptr.elf",
            property: "false alias: pointer stores of req[4..7] may alias cfg.flag statically",
            input: pad(b"1bcdPTR!", 16),
            sink: cfg_flag,
            requested: none,
            truth: Truth {
                causes: vec![0],
                ..Truth::default()
            },
        },
        CaseSpec {
            name: "real_alias",
            fixture: "x64_real_alias.elf",
            property: "real alias: a request-chosen offset makes a byte store land on is_admin",
            input: {
                let mut v = pad(b"ab", 16);
                v[2] = 8;
                v[9] = b'A';
                v
            },
            sink: account_admin,
            requested: none,
            truth: Truth {
                causes: vec![2, 9],
                ..Truth::default()
            },
        },
        CaseSpec {
            name: "joint_or",
            fixture: "x64_joint_or.elf",
            property: "overdetermined grant (A || B, both true): needs a joint intervention",
            input: pad(b"AB", 16),
            sink: grant_any,
            requested: none,
            truth: Truth {
                joint: vec![0, 1],
                ..Truth::default()
            },
        },
        CaseSpec {
            name: "joint_and",
            fixture: "x64_joint_and.elf",
            property: "conjunctive grant (K && Y): each byte alone is a but-for cause",
            input: pad(b"KY", 16),
            sink: grant_any,
            requested: none,
            truth: Truth {
                causes: vec![0, 1],
                ..Truth::default()
            },
        },
        CaseSpec {
            name: "nondet_time",
            fixture: "x64_nondet_time.elf",
            property: "grant depends on the wall clock: must be refused",
            input: pad(b"T", 16),
            sink: grant_any,
            requested: none,
            truth: Truth {
                causes: vec![0],
                refuse: true,
                ..Truth::default()
            },
        },
        CaseSpec {
            name: "nondet_random",
            fixture: "x64_nondet_random.elf",
            property: "grant depends on getrandom: must be refused",
            input: pad(b"R", 16),
            sink: grant_any,
            requested: none,
            truth: Truth {
                causes: vec![0],
                refuse: true,
                ..Truth::default()
            },
        },
        CaseSpec {
            name: "crash_null_x64",
            fixture: "x64_crash_null.elf",
            property: "crash: req[0] selects a NULL slot (address dependence); the stored req[1] is not a cause",
            input: {
                let mut v = vec![2u8, b'A'];
                v.resize(8, b'x');
                v
            },
            sink: observed_fault,
            requested: none,
            truth: Truth {
                causes: vec![0],
                ..Truth::default()
            },
        },
        CaseSpec {
            name: "crash_null_a64",
            fixture: "a64_crash_null.elf",
            property: "the crash case on AArch64",
            input: {
                let mut v = vec![2u8, b'A'];
                v.resize(8, b'x');
                v
            },
            sink: observed_fault,
            requested: none,
            truth: Truth {
                causes: vec![0],
                ..Truth::default()
            },
        },
        CaseSpec {
            name: "password_x64",
            fixture: "x64_password.elf",
            property: "authorization sink: grant_admin(1000) after a 4-byte secret compare",
            input: pad(b"s3crXYZW", 16),
            sink: grant_admin,
            requested: none,
            truth: Truth {
                causes: vec![0, 1, 2, 3],
                ..Truth::default()
            },
        },
        CaseSpec {
            name: "password_a64",
            fixture: "a64_password.elf",
            property: "the authorization case on AArch64",
            input: pad(b"s3crXYZW", 16),
            sink: grant_admin,
            requested: none,
            truth: Truth {
                causes: vec![0, 1, 2, 3],
                ..Truth::default()
            },
        },
        CaseSpec {
            name: "branch_len",
            fixture: "x64_branch_len.elf",
            property: "branch-outcome sink with direct data flow (positive control for taint)",
            input: {
                let mut v = vec![20u8];
                v.extend_from_slice(b"bcd");
                v.resize(16, b'x');
                v
            },
            sink: classify_branch,
            requested: none,
            truth: Truth {
                causes: vec![0],
                ..Truth::default()
            },
        },
    ]
}

pub fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata")
}

/// One method's explanation scored against the truth.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Score {
    /// Method-specific node count (see the report's legend).
    pub nodes: usize,
    pub explanation: Vec<usize>,
    pub true_pos: usize,
    pub false_pos: usize,
    pub false_neg: usize,
}

impl Score {
    fn of(nodes: usize, explanation: BTreeSet<usize>, truth: &BTreeSet<usize>) -> Self {
        Self {
            nodes,
            true_pos: explanation.intersection(truth).count(),
            false_pos: explanation.difference(truth).count(),
            false_neg: truth.difference(&explanation).count(),
            explanation: explanation.into_iter().collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evaluation {
    pub name: String,
    /// The pinned input stream, hex.
    pub input_hex: String,
    pub fixture: String,
    pub property: String,
    pub truth: Truth,
    pub refused: bool,
    pub refusal_expected: bool,
    pub joint: JointStatus,
    pub static_slice: Score,
    pub dynamic_slice: Score,
    pub taint_data: Score,
    pub taint_data_control: Score,
    pub csic: Score,
    /// Every delivered input byte tried alone (the single-byte but-for set
    /// by brute force): measures the candidate generator's recall.
    pub exhaustive: Score,
    pub exhaustive_oracles: u64,
    pub report: CaseReport,
}

/// Every input byte alone, with the runner's value set. Returns the bytes
/// that flipped Φ and the oracle executions spent.
pub fn exhaustive_inputs(
    program: &Program,
    trace: &Trace,
    sink: &SinkSpec,
    config: &Config,
) -> Result<(BTreeSet<usize>, u64)> {
    let mut flipped = BTreeSet::new();
    let mut oracles = 0u64;
    let original = trace.observation.holds();
    let mut replay = runner::Replayer::new(program)?;
    let read_steps: BTreeSet<u64> = trace
        .steps
        .iter()
        .filter(|s| !s.input.is_empty())
        .map(|s| s.step + 1)
        .collect();
    replay.capture(&read_steps)?;
    for s in &trace.steps {
        for b in &s.input {
            let loc = Location::Memory {
                addr: b.addr,
                len: 1,
            };
            let probe = replay.position(s.step + 1)?;
            let current = runner::read_location(program, probe, &loc)?;
            for value in crate::slice::alternatives(&current, &[], config.values_per_candidate) {
                oracles += 1;
                let iv = Intervention {
                    step: s.step + 1,
                    location: loc.clone(),
                    value,
                };
                let obs = replay.observe(trace, sink, std::slice::from_ref(&iv), config.max_steps)?;
                if obs.holds().is_some() && obs.holds() != original {
                    flipped.insert(b.offset);
                    break;
                }
            }
        }
    }
    Ok((flipped, oracles))
}

/// Run one case through CSIC and every baseline.
pub fn evaluate(spec: &CaseSpec, config: &Config) -> Result<Evaluation> {
    let path = fixture_dir().join(spec.fixture);
    let program = Program::load(&path.to_string_lossy(), &spec.input)?;
    // Discovery: the incident as observed, to name pcs and steps.
    let placeholder = SinkSpec::MemoryValue {
        addr: 0,
        len: 1,
        predicate: MemPredicate::NonZero,
        at: SinkPoint::Exit,
    };
    let discovery = trace::record(&program, &placeholder, &[], config.max_steps)?;
    let sink = (spec.sink)(&program, &discovery)?;
    let requested = (spec.requested)(&program, &discovery)?;
    let case = Case {
        name: spec.name.to_string(),
        program: &program,
        sink: sink.clone(),
        requested,
    };
    let report = runner::run_case(&case, config).context("csic runner")?;

    let truth: BTreeSet<usize> = spec
        .truth
        .causes
        .iter()
        .chain(&spec.truth.joint)
        .copied()
        .collect();
    let (recorded, slice) = runner::record_and_slice(&program, &sink, config)?;
    let taint: TaintResult =
        baseline::dynamic_taint(&program, &recorded).context("taint baseline")?;
    let ss = static_slice::slice(&program, &recorded).context("static slice")?;

    let mut csic_expl: BTreeSet<usize> = report.witness_inputs.iter().copied().collect();
    if let JointStatus::JointWitness { input_offsets, .. } = &report.joint {
        csic_expl.extend(input_offsets.iter().flatten().copied());
    }
    if report.refusal.is_some() {
        csic_expl.clear();
    }
    let csic_nodes = report.counts.candidates_tested;
    let (exhaustive, exhaustive_oracles) =
        exhaustive_inputs(&program, &recorded, &sink, config).context("exhaustive sweep")?;

    Ok(Evaluation {
        name: spec.name.to_string(),
        input_hex: spec.input.iter().map(|b| format!("{b:02x}")).collect(),
        fixture: spec.fixture.to_string(),
        property: spec.property.to_string(),
        truth: spec.truth.clone(),
        refused: report.refusal.is_some(),
        refusal_expected: spec.truth.refuse,
        joint: report.joint.clone(),
        static_slice: Score::of(ss.instructions, ss.input_offsets, &truth),
        dynamic_slice: Score::of(slice.steps.len(), slice.input_offsets.clone(), &truth),
        taint_data: Score::of(taint.criterion_bytes, taint.data.clone(), &truth),
        taint_data_control: Score::of(taint.criterion_bytes, taint.data_control.clone(), &truth),
        csic: Score::of(csic_nodes, csic_expl, &truth),
        exhaustive: Score::of(recorded.inputs.len(), exhaustive, &truth),
        exhaustive_oracles,
        report,
    })
}

/// Whether a candidate verdict is a witness.
pub fn is_witness(v: &Verdict) -> bool {
    matches!(v, Verdict::CounterfactualWitness { .. })
}
