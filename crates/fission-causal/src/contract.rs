//! The CSIC v1 contract: what a sink is, what may be intervened on, what a
//! replay is pinned to, and what an experiment may conclude.
//!
//! Everything here is data. The runner (`crate::runner`) produces it and the
//! evaluation harness serialises it; nothing in this module executes code.
//!
//! The verdicts are deliberately weaker than "cause". A
//! [`Verdict::CounterfactualWitness`] says one thing: in this recorded
//! experiment (same binary, same input stream, same environment stubs, same
//! initial state), setting one machine location to one other value at one
//! step changed the sink predicate, and a fresh replay from the initial state
//! agreed. It is not a Halpern-Pearl actual-cause claim: no contingency set
//! was searched and nothing about minimality is asserted. Likewise
//! [`Verdict::NotACause`] holds only for the values that were tried.

use serde::{Deserialize, Serialize};

/// Contract version carried by every report.
pub const CONTRACT_VERSION: &str = "csic/v1";

/// The sink predicate Φ.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SinkSpec {
    /// The run stops on a memory fault raised by the instruction at `pc`.
    FaultAt { pc: u64 },
    /// Some entry to `target` happens with every `Some` argument equal to the
    /// given value. Arguments are read through the architecture's calling
    /// convention at the entry instruction.
    CallTo { target: u64, args: Vec<Option<u64>> },
    /// The `occurrence`-th (1-based) execution of the conditional branch at
    /// `pc` goes to its branch target (`taken == true`) rather than falling
    /// through. Φ is false if that execution never happens.
    BranchOutcome {
        pc: u64,
        occurrence: u32,
        taken: bool,
    },
    /// `len` bytes at `addr` satisfy `predicate` at `at`.
    MemoryValue {
        addr: u64,
        len: u32,
        predicate: MemPredicate,
        at: SinkPoint,
    },
}

/// When a [`SinkSpec::MemoryValue`] is read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SinkPoint {
    /// After the program has exited (or otherwise stopped without a budget
    /// overrun).
    Exit,
    /// After exactly `step` retired instructions.
    Step { step: u64 },
    /// Just before the `occurrence`-th (1-based) execution of `pc`.
    PcHit { pc: u64, occurrence: u32 },
}

/// What a [`SinkSpec::MemoryValue`] requires of the bytes it reads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "bytes", rename_all = "snake_case")]
pub enum MemPredicate {
    Equals(Vec<u8>),
    NonZero,
}

/// A machine location an intervention may set.
///
/// Registers are named the way the language's register map names them, so
/// the same contract holds on every ISA. Memory is the guest's RAM space.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Location {
    Register { name: String },
    Memory { addr: u64, len: u32 },
}

impl Location {
    pub fn width(&self, register_width: impl Fn(&str) -> Option<u32>) -> Option<u32> {
        match self {
            Self::Register { name } => register_width(name),
            Self::Memory { len, .. } => Some(*len),
        }
    }

    pub fn render(&self) -> String {
        match self {
            Self::Register { name } => name.clone(),
            Self::Memory { addr, len } => format!("mem[0x{addr:x}+{len}]"),
        }
    }
}

/// `do(location := value)` right before the instruction at `step` executes,
/// i.e. once exactly `step` instructions have retired. `value` is
/// little-endian for registers and in memory order for RAM, and must be
/// exactly as wide as the location.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Intervention {
    pub step: u64,
    pub location: Location,
    pub value: Vec<u8>,
}

/// Why an intervention is outside the contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InvalidReason {
    /// The register is not in this language's register map.
    UnknownRegister { name: String },
    /// Setting the program counter changes which program runs, not the
    /// state it runs on.
    ProgramCounter,
    /// The memory range is not mapped at that step.
    Unmapped { addr: u64 },
    /// The memory range is executable: writing it patches code.
    CodePage { addr: u64 },
    /// `value` is not as wide as the location.
    WidthMismatch { expected: u32, got: u32 },
    /// `value` equals what the location already holds at that step, so the
    /// "intervention" is the recorded run itself.
    NoOp,
    /// The step is at or after the sink event (or past the recorded run), so
    /// the intervention cannot precede what it is meant to explain.
    NotBeforeSink { step: u64, sink_step: u64 },
}

/// Pins a replay: everything a rerun must reproduce to be the same
/// experiment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplayFingerprint {
    pub contract: String,
    /// BLAKE3 of the binary file.
    pub binary_blake3: String,
    /// Sleigh language id the emulator ran.
    pub language: String,
    /// BLAKE3 of the input stream (guest stdin), and its length.
    pub input_blake3: String,
    pub input_len: usize,
    /// Guest argv and environment as the loader set them up.
    pub argv: Vec<String>,
    pub envp: Vec<String>,
    /// BLAKE3 over the initial register file and every mapped page, after
    /// image setup and before the first instruction.
    pub initial_state_blake3: String,
    /// Environment-derived values the run consumed, by syscall name, with
    /// the emulator stub's answer standing in for the host's. See
    /// [`EnvSourceStatus`].
    pub environment_sources: Vec<EnvSource>,
    /// The engine and mode the counterfactual replays used.
    pub engine: String,
}

/// An environment value the run consumed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvSource {
    pub step: u64,
    pub syscall: String,
    pub class: EnvClass,
    pub status: EnvSourceStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvClass {
    /// Differs from one native run to the next (clock, entropy, pids,
    /// cycle counters, host files).
    NonDeterministic,
    /// Fixed per host (uid, uname); the emulator answers with a stub.
    HostStable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvSourceStatus {
    /// The sink's dynamic dependence slice reaches this source's output.
    InSinkSlice,
    /// Perturbing this source's output (a counterfactual on it) changed Φ.
    PerturbationChangesSink,
    /// Not in the slice, and no tried perturbation changed Φ. Flagged, not
    /// refused: the claim is conditioned on the stubbed value.
    OutsideSliceStable,
    /// Host-stable value; recorded, not tested.
    Recorded,
}

/// Φ evaluated on one run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SinkObservation {
    Holds {
        detail: String,
    },
    DoesNotHold {
        detail: String,
    },
    /// The run hit the instruction budget before Φ was decided.
    Undecided {
        detail: String,
    },
}

impl SinkObservation {
    pub fn holds(&self) -> Option<bool> {
        match self {
            Self::Holds { .. } => Some(true),
            Self::DoesNotHold { .. } => Some(false),
            Self::Undecided { .. } => None,
        }
    }
}

/// The verdict for one candidate (or one explicitly requested
/// intervention).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Verdict {
    /// One value at this location changed Φ, and a fresh replay from the
    /// initial state reproduced the change. Not an actual-cause claim.
    CounterfactualWitness {
        witness: InterventionWitness,
    },
    /// No tried value changed Φ. Holds for exactly these values, in this
    /// experiment.
    NotACause {
        tried: Vec<Vec<u8>>,
    },
    InvalidIntervention {
        reason: InvalidReason,
    },
    /// The fork replay and the fresh replay disagreed, or the sink depends
    /// on a non-deterministic environment source.
    NonDeterministic {
        reason: String,
    },
    /// The oracle-execution budget ran out before this candidate was
    /// decided, or a replay hit the instruction budget.
    BudgetBounded {
        detail: String,
    },
    /// The engine could not run the experiment (decode failure, an
    /// unsupported sink, ...).
    Unsupported {
        reason: String,
    },
}

impl Verdict {
    pub fn label(&self) -> &'static str {
        match self {
            Self::CounterfactualWitness { .. } => "counterfactual_witness",
            Self::NotACause { .. } => "not_a_cause",
            Self::InvalidIntervention { .. } => "invalid_intervention",
            Self::NonDeterministic { .. } => "non_deterministic",
            Self::BudgetBounded { .. } => "budget_bounded",
            Self::Unsupported { .. } => "unsupported",
        }
    }
}

/// RFC #1101's `InterventionWitness`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InterventionWitness {
    pub step: u64,
    /// The program counter at `step` in the recorded run.
    pub pc: u64,
    pub location: Location,
    pub original_bytes: Vec<u8>,
    pub intervened_bytes: Vec<u8>,
    pub original_sink: SinkObservation,
    pub counterfactual_sink: SinkObservation,
    /// BLAKE3 over the counterfactual run's end state (exit status, output,
    /// retired-instruction count), from the fresh replay.
    pub counterfactual_trace_blake3: String,
    /// The fork replay and the fresh replay agreed on Φ and on the end
    /// state.
    pub fresh_replay_agrees: bool,
}

/// Where a candidate came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateOrigin {
    /// An input byte, intervened on right after the read that delivered it.
    InputByte,
    /// A value the sink's backward dynamic data slice reads.
    DynamicData,
    /// A value a dynamically control-dependent branch reads (control
    /// dependence from static postdominators of the containing function).
    ControlDependence,
    /// The output of an environment syscall, tested to decide determinism.
    EnvironmentSource,
    /// Requested by the caller rather than generated.
    Requested,
}

/// One candidate and what the experiment concluded about it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateResult {
    pub step: u64,
    pub pc: u64,
    pub location: Location,
    pub origin: CandidateOrigin,
    /// The input-stream offset this location held at `step`, when it held
    /// exactly one unmodified input byte.
    pub input_offset: Option<usize>,
    pub verdict: Verdict,
}

/// What the case as a whole concluded about joint interventions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum JointStatus {
    /// At least one single input-byte witness exists; no joint search ran.
    NotNeeded,
    /// No single input-byte witness; this pair of interventions together
    /// changed Φ (verified by a fresh replay). The cause needs a joint
    /// intervention. Not a minimality claim beyond "no single member of the
    /// tried candidates sufficed".
    JointWitness {
        interventions: Vec<Intervention>,
        input_offsets: Vec<Option<usize>>,
    },
    /// No single input-byte witness and the bounded pair search found none.
    /// This is not "no cause": a larger set, or other values, may still
    /// change Φ.
    NoWitnessWithinBudget { pairs_tried: u64 },
    /// The case was refused before any search.
    Refused,
}
