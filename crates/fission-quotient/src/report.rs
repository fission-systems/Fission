//! The `quotient.v1` contract: what a quotient claims, kept apart from what
//! it assumes and from how strongly it was checked.

use serde::Serialize;

use crate::boundary::{QuotientBoundary, QuotientBudget};
use crate::oracle::{Observation, Symbol};
use crate::summary::PrunedBranch;

pub const SCHEMA_VERSION: &str = "quotient.v1";

/// Byte strings are written as one lowercase hex string.
pub(crate) fn hex_bytes<S: serde::Serializer>(
    value: &[u8],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(
        &value
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>(),
    )
}

/// Addresses are written as `0x`-prefixed hex strings, as every other
/// `fission_cli` JSON payload writes them.
pub(crate) fn hex_address<S: serde::Serializer>(
    value: &u64,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&format!("0x{value:x}"))
}

/// The memory model a `ProvedUnderModel` certificate is relative to.
pub const MODEL_ID: &str = "separated-registers-frame-context.v1";

/// How far the quotient's claim reaches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum QuotientStatus {
    /// The solver refuted every distinguishing word of length up to
    /// `horizon`, under the memory model `model_id`. The result check is
    /// deepened from the budget's horizon towards `states + K_max`, the
    /// length within which two machines of that size must differ;
    /// `horizon` is how deep it got. Data fields are checked to the
    /// budget's horizon.
    ProvedUnderModel { horizon: u32, model_id: String },
    /// The solver could not decide; `probes_run` concrete words found no
    /// difference. Evidence, not proof.
    EmpiricallyConsistent {
        horizon: u32,
        probes_run: u64,
        solver_unknowns: u32,
    },
    /// The state budget ran out: the slice has at least `max_states_hit`
    /// observably distinct states. The states listed are real; the machine
    /// is not complete.
    PartialFrontier {
        max_states_hit: usize,
        unmodeled_edges: usize,
    },
    /// A word, confirmed on the emulator, that the quotient gets wrong
    /// although the learner cannot see it (a data effect that does not
    /// factor through the control states).
    Refuted { counterexample_hex: String },
    /// The slice does something the model does not cover; no quotient.
    UnsupportedSemantics { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FoundationRef {
    #[serde(serialize_with = "hex_address")]
    pub function_address: u64,
    pub block_count: u32,
    pub op_count: u32,
    pub digest: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SlotRole {
    Control,
    Data,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SlotReport {
    pub name: String,
    pub offset: u32,
    pub size: u32,
    pub role: SlotRole,
    pub initial: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SlotValue {
    pub slot: String,
    pub value: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StateReport {
    pub id: usize,
    /// The input word that first reached this state from the initial one.
    pub access_word: Vec<Symbol>,
    /// The control slots after the access word, as the emulator left them.
    /// A state may stand for several valuations; this is the one that
    /// reached it first.
    pub control_valuation: Vec<SlotValue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UpdateReport {
    pub slot: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TransitionReport {
    pub from: usize,
    pub guard: String,
    pub output: String,
    pub updates: Vec<UpdateReport>,
    pub to: usize,
}

/// A concrete input sequence that tells two states apart, re-run on the
/// emulator from both states' access words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ApartnessWitness {
    pub state_a: usize,
    pub state_b: usize,
    pub input_sequence: Vec<Symbol>,
    pub obs_a: Vec<Observation>,
    pub obs_b: Vec<Observation>,
    pub verified_by_emulator: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AssumptionKind {
    Memory,
    CallingConvention,
    Environment,
    Bound,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Assumption {
    pub kind: AssumptionKind,
    pub description: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Evidence {
    pub rounds: usize,
    pub membership_queries: u64,
    pub emulator_steps: u64,
    pub refutation_queries: u32,
    pub solver_unknowns: u32,
    pub probes_run: u64,
    pub alphabet: Vec<Symbol>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Metrics {
    /// The slice's P-code control-flow graph.
    pub pcode_blocks: usize,
    pub pcode_edges: usize,
    /// Feasible paths through one step.
    pub feasible_paths: usize,
    /// Blocks and edges some feasible path uses: the graph left after
    /// path-sensitive constant propagation and dead-branch pruning.
    pub feasible_blocks: usize,
    pub feasible_edges: usize,
    /// Branch sides the solver proved dead for every input and state.
    pub pruned_branches: Vec<PrunedBranch>,
    pub quotient_states: usize,
    pub quotient_transitions: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CertifiedQuotient {
    pub schema_version: &'static str,
    pub foundation: FoundationRef,
    pub boundary: QuotientBoundary,
    pub budget: QuotientBudget,
    pub status: QuotientStatus,
    pub slots: Vec<SlotReport>,
    pub states: Vec<StateReport>,
    pub transitions: Vec<TransitionReport>,
    pub apartness_witnesses: Vec<ApartnessWitness>,
    pub assumptions: Vec<Assumption>,
    pub evidence: Evidence,
    pub metrics: Metrics,
}

impl CertifiedQuotient {
    /// Canonical JSON: field order is fixed by the types and every
    /// collection is built in a deterministic order.
    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }
}
