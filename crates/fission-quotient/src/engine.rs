//! `WBCQ_Synthesize`: summarize, factor, learn, build, refute, repeat.

use std::path::Path;

use anyhow::{Context, Result};
use fission_dir::PcodeFoundation;
use fission_loader::loader::LoadedBinary;

use crate::boundary::{AbiModel, QuotientBoundary, QuotientBudget};
use crate::certify::{Refutation, probe_words, refute};
use crate::expr::render;
use crate::learn::{Hypothesis, Learned, Learner};
use crate::oracle::{EmulatorOracle, Exit, MembershipOracle, Symbol};
use crate::report::{
    ApartnessWitness, Assumption, AssumptionKind, CertifiedQuotient, Evidence, FoundationRef,
    MODEL_ID, Metrics, QuotientStatus, SCHEMA_VERSION, SlotReport, SlotRole, SlotValue,
    StateReport, TransitionReport, UpdateReport,
};
use crate::srqa::{Built, SlotRoles, Srqa, build, factor, minimal_input, slot_value};
use crate::summary::{SliceSummary, summarize};

/// Computes a certified quotient of one P-code foundation.
pub trait QuotientEngine {
    fn synthesize(
        &mut self,
        foundation: &PcodeFoundation,
        budget: QuotientBudget,
    ) -> Result<CertifiedQuotient>;
}

/// The white-box engine: P-code summaries for guards and refutation, a
/// membership oracle for ground truth.
pub struct Wbcq<O: MembershipOracle> {
    pub abi: AbiModel,
    pub boundary: QuotientBoundary,
    pub oracle: O,
}

impl Wbcq<EmulatorOracle> {
    /// Engine over a binary on disk, with the real emulator as oracle.
    pub fn for_binary(
        path: &Path,
        boundary: QuotientBoundary,
        max_instructions: u64,
    ) -> Result<(Self, PcodeFoundation)> {
        let binary = LoadedBinary::from_file(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let pcode = fission_decompiler::disasm::raw_pcode_function(&binary, boundary.entry)
            .map_err(|error| anyhow::anyhow!("lifting {:#x}: {error}", boundary.entry))?;
        let foundation = PcodeFoundation::new(boundary.entry, pcode)
            .map_err(|error| anyhow::anyhow!("invalid P-code: {error}"))?;
        let abi = AbiModel::for_binary(&binary)?;
        let oracle = EmulatorOracle::new(path, &boundary, max_instructions)?;
        Ok((
            Self {
                abi,
                boundary,
                oracle,
            },
            foundation,
        ))
    }
}

enum Replay {
    Agrees,
    /// The result or the exit differs: the learner can see it.
    Visible,
    /// Only a data slot differs.
    DataOnly,
}

impl<O: MembershipOracle> QuotientEngine for Wbcq<O> {
    fn synthesize(
        &mut self,
        foundation: &PcodeFoundation,
        budget: QuotientBudget,
    ) -> Result<CertifiedQuotient> {
        let raw = foundation.pcode().as_raw();
        let identity = foundation.identity();
        let mut report = CertifiedQuotient {
            schema_version: SCHEMA_VERSION,
            foundation: FoundationRef {
                function_address: identity.function_address,
                block_count: identity.block_count,
                op_count: identity.op_count,
                digest: format!("{:016x}", identity.digest),
            },
            boundary: self.boundary.clone(),
            budget,
            status: QuotientStatus::UnsupportedSemantics {
                reason: String::new(),
            },
            slots: Vec::new(),
            states: Vec::new(),
            transitions: Vec::new(),
            apartness_witnesses: Vec::new(),
            assumptions: assumptions(&self.boundary, &self.abi, &budget),
            evidence: Evidence::default(),
            metrics: Metrics {
                pcode_blocks: raw.blocks.len(),
                pcode_edges: raw.blocks.iter().map(|b| b.successors.len()).sum(),
                ..Metrics::default()
            },
        };

        let summary = match summarize(raw, &self.abi, &self.boundary, &budget) {
            Ok(summary) => summary,
            Err(error) => {
                report.status = QuotientStatus::UnsupportedSemantics {
                    reason: error.to_string(),
                };
                return Ok(report);
            }
        };
        let roles = factor(&summary);
        let big = self.abi.big_endian;
        let initial = self.boundary.context.initial.clone();
        let initial_slots = summary
            .slots
            .iter()
            .map(|slot| slot_value(&initial, slot.offset, slot.size, big))
            .collect::<Vec<_>>();
        report.slots = summary
            .slots
            .iter()
            .enumerate()
            .map(|(index, slot)| SlotReport {
                name: slot.name(),
                offset: slot.offset,
                size: slot.size,
                role: if roles.control.contains(&index) {
                    SlotRole::Control
                } else {
                    SlotRole::Data
                },
                initial: initial_slots[index],
            })
            .collect();
        report.metrics.feasible_paths = summary.paths.len();
        report.metrics.pruned_branches = summary.pruned.clone();
        report.evidence.solver_unknowns = summary.solver_unknowns;

        // One representative per path: the least input that can take it.
        let mut alphabet: Vec<Symbol> = summary
            .paths
            .iter()
            .filter_map(|path| {
                minimal_input(&path.guard, &summary.inputs, &budget)
                    .ok()
                    .flatten()
            })
            .collect();
        alphabet.sort();
        alphabet.dedup();
        if alphabet.is_empty() {
            alphabet.push(vec![0; summary.inputs.len()]);
        }
        let mut learner = Learner::new(alphabet, initial);
        let data_initial = roles
            .data
            .iter()
            .map(|slot| initial_slots[*slot])
            .collect::<Vec<_>>();

        let status = loop {
            if report.evidence.rounds >= budget.max_rounds {
                break QuotientStatus::UnsupportedSemantics {
                    reason: format!("no stable quotient within {} rounds", budget.max_rounds),
                };
            }
            report.evidence.rounds += 1;
            if learner.alphabet.len() > budget.max_alphabet {
                break QuotientStatus::PartialFrontier {
                    max_states_hit: learner.basis().len(),
                    unmodeled_edges: 0,
                };
            }
            let hypothesis = match learner.learn(&mut self.oracle, budget.max_states)? {
                Learned::Hypothesis(hypothesis) => hypothesis,
                Learned::Partial { states, frontier } => {
                    break QuotientStatus::PartialFrontier {
                        max_states_hit: states,
                        unmodeled_edges: frontier,
                    };
                }
            };
            let srqa = match build(&summary, &roles, &hypothesis, &learner, big, &budget) {
                Built::Srqa(srqa) => srqa,
                Built::NeedSymbol(symbol) => {
                    learner.intern(&symbol);
                    continue;
                }
                Built::FanOut(symbols) => {
                    // Let the learner see where the fan-out leads; if it has
                    // already seen all of it, the transition cannot be
                    // written down within the split budget.
                    let known = learner.alphabet.len();
                    for symbol in &symbols {
                        learner.intern(symbol);
                    }
                    if learner.alphabet.len() > known {
                        continue;
                    }
                    // The symbols are known but the hypothesis has not yet
                    // told their targets apart: test it on longer words.
                    if contradicts(&mut learner, &mut self.oracle, &hypothesis, &budget)? {
                        continue;
                    }
                    break QuotientStatus::PartialFrontier {
                        max_states_hit: learner.basis().len(),
                        unmodeled_edges: 1,
                    };
                }
                Built::Unsupported(reason) => {
                    break QuotientStatus::UnsupportedSemantics { reason };
                }
            };

            report.evidence.refutation_queries += 1;
            let refutation = refute(
                &summary,
                &roles,
                &srqa,
                &initial_slots,
                budget.horizon,
                &budget,
            );
            let (candidate, from_solver) = match refutation {
                Refutation::Proved => {
                    self.finish(&mut report, &summary, &roles, &learner, &hypothesis, &srqa)?;
                    break QuotientStatus::ProvedUnderModel {
                        horizon: budget.horizon,
                        model_id: MODEL_ID.to_string(),
                    };
                }
                Refutation::Candidate(word) => (Some(word), true),
                Refutation::Unknown => {
                    report.evidence.solver_unknowns += 1;
                    let ports = self
                        .boundary
                        .inputs
                        .iter()
                        .map(|p| p.bits)
                        .collect::<Vec<_>>();
                    let mut found = None;
                    for word in
                        probe_words(&learner.alphabet, &ports, budget.probes, budget.horizon)
                    {
                        report.evidence.probes_run += 1;
                        let verdict = self.replay(&summary, &roles, &srqa, &data_initial, &word)?;
                        if !matches!(verdict, Replay::Agrees) {
                            found = Some(word);
                            break;
                        }
                    }
                    (found, false)
                }
            };
            let Some(word) = candidate else {
                self.finish(&mut report, &summary, &roles, &learner, &hypothesis, &srqa)?;
                break QuotientStatus::EmpiricallyConsistent {
                    horizon: budget.horizon,
                    probes_run: report.evidence.probes_run,
                    solver_unknowns: report.evidence.solver_unknowns,
                };
            };
            match self.replay(&summary, &roles, &srqa, &data_initial, &word)? {
                Replay::Visible => {
                    // Into the tree; the next round's consistency check turns
                    // it into a split.
                    let indices = word.iter().map(|s| learner.intern(s)).collect::<Vec<_>>();
                    learner.query(&mut self.oracle, &indices)?;
                }
                Replay::DataOnly => {
                    self.finish(&mut report, &summary, &roles, &learner, &hypothesis, &srqa)?;
                    break QuotientStatus::Refuted {
                        counterexample_hex: hex_word(&word),
                    };
                }
                Replay::Agrees if from_solver => {
                    break QuotientStatus::UnsupportedSemantics {
                        reason: format!(
                            "the solver's distinguishing word {} does not reproduce on the \
                             emulator: the P-code model and the machine disagree",
                            hex_word(&word)
                        ),
                    };
                }
                Replay::Agrees => unreachable!("a probe is only returned when it disagrees"),
            }
        };

        report.status = status;
        report.evidence.membership_queries = learner.queries();
        report.evidence.emulator_steps = self.oracle.steps();
        report.evidence.alphabet = learner.alphabet.clone();
        if matches!(report.status, QuotientStatus::PartialFrontier { .. }) {
            report.states = learner
                .basis()
                .iter()
                .enumerate()
                .map(|(id, node)| StateReport {
                    id,
                    access_word: learner.symbols(&learner.access(*node)),
                    control_valuation: control_valuation(
                        &summary,
                        &roles,
                        learner.context(*node),
                        big,
                    ),
                })
                .collect();
            report.metrics.quotient_states = report.states.len();
        }
        if report
            .apartness_witnesses
            .iter()
            .any(|witness| !witness.verified_by_emulator)
        {
            report.status = QuotientStatus::UnsupportedSemantics {
                reason: "an apartness witness did not reproduce: the slice is not deterministic"
                    .to_string(),
            };
        }
        Ok(report)
    }
}

impl<O: MembershipOracle> Wbcq<O> {
    /// Run `word` on the oracle and on the automaton and say how they differ.
    fn replay(
        &mut self,
        summary: &SliceSummary,
        roles: &SlotRoles,
        srqa: &Srqa,
        data_initial: &[u64],
        word: &[Symbol],
    ) -> Result<Replay> {
        let real = self.oracle.run(word)?;
        let Some(predicted) = srqa.run(summary, roles, data_initial.to_vec(), word) else {
            return Ok(Replay::Visible);
        };
        let mut data_differs = false;
        for (record, step) in real.iter().zip(&predicted) {
            if record.observation.exit != Exit::Returned || record.observation.value != step.output
            {
                return Ok(Replay::Visible);
            }
            for (slot, value) in roles.data.iter().zip(&step.data) {
                let slot = &summary.slots[*slot];
                let actual =
                    slot_value(&record.context, slot.offset, slot.size, self.abi.big_endian);
                if actual != *value {
                    data_differs = true;
                }
            }
        }
        Ok(if data_differs {
            Replay::DataOnly
        } else {
            Replay::Agrees
        })
    }

    /// Fill in states, transitions and re-verified witnesses.
    fn finish(
        &mut self,
        report: &mut CertifiedQuotient,
        summary: &SliceSummary,
        roles: &SlotRoles,
        learner: &Learner,
        hypothesis: &Hypothesis,
        srqa: &Srqa,
    ) -> Result<()> {
        let names = summary.names(&self.boundary);
        let big = self.abi.big_endian;
        report.states = hypothesis
            .states
            .iter()
            .enumerate()
            .map(|(id, node)| StateReport {
                id,
                access_word: learner.symbols(&learner.access(*node)),
                control_valuation: control_valuation(summary, roles, learner.context(*node), big),
            })
            .collect();
        report.transitions = srqa
            .transitions
            .iter()
            .map(|t| TransitionReport {
                from: t.from,
                guard: render(&t.guard, &names),
                output: render(&t.output, &names),
                updates: roles
                    .data
                    .iter()
                    .zip(&t.updates)
                    .map(|(slot, value)| UpdateReport {
                        slot: summary.slots[*slot].name(),
                        value: render(value, &names),
                    })
                    .collect(),
                to: t.to,
            })
            .collect();
        report.metrics.quotient_states = report.states.len();
        report.metrics.quotient_transitions = report.transitions.len();

        // Every pair of states, told apart again on the emulator from
        // scratch rather than read back from the tree.
        let mut witnesses = Vec::new();
        for a in 0..hypothesis.states.len() {
            for b in a + 1..hypothesis.states.len() {
                let (node_a, node_b) = (hypothesis.states[a], hypothesis.states[b]);
                let suffix = learner
                    .witness(node_a, node_b)
                    .expect("basis states are pairwise apart");
                let observe = |oracle: &mut O, node| -> Result<Vec<_>> {
                    let access = learner.access(node);
                    let mut word = learner.symbols(&access);
                    word.extend(learner.symbols(&suffix));
                    let records = oracle.run(&word)?;
                    Ok(records[access.len()..]
                        .iter()
                        .map(|record| record.observation)
                        .collect())
                };
                let obs_a = observe(&mut self.oracle, node_a)?;
                let obs_b = observe(&mut self.oracle, node_b)?;
                witnesses.push(ApartnessWitness {
                    state_a: a,
                    state_b: b,
                    input_sequence: learner.symbols(&suffix),
                    verified_by_emulator: obs_a != obs_b,
                    obs_a,
                    obs_b,
                });
            }
        }
        report.apartness_witnesses = witnesses;
        Ok(())
    }
}

/// Run deterministic words over the alphabet and keep the first one the
/// hypothesis predicts wrongly (it is now in the tree, where the learner's
/// consistency check will split on it).
fn contradicts(
    learner: &mut Learner,
    oracle: &mut dyn MembershipOracle,
    hypothesis: &Hypothesis,
    budget: &QuotientBudget,
) -> Result<bool> {
    let symbols = learner.alphabet.len() as u64;
    let mut seed = 0x2545_f491_4f6c_dd1du64;
    for _ in 0..budget.probes {
        let word = (0..budget.horizon.max(2))
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                (seed % symbols) as usize
            })
            .collect::<Vec<_>>();
        let observed = learner.query(oracle, &word)?;
        if observed != hypothesis.run_from(0, &word).0 {
            return Ok(true);
        }
    }
    Ok(false)
}

fn control_valuation(
    summary: &SliceSummary,
    roles: &SlotRoles,
    context: &[u8],
    big_endian: bool,
) -> Vec<SlotValue> {
    roles
        .control
        .iter()
        .map(|slot| {
            let slot = &summary.slots[*slot];
            SlotValue {
                slot: slot.name(),
                value: slot_value(context, slot.offset, slot.size, big_endian),
            }
        })
        .collect()
}

fn hex_word(word: &[Symbol]) -> String {
    word.iter()
        .map(|symbol| {
            symbol
                .iter()
                .map(|value| format!("{value:x}"))
                .collect::<Vec<_>>()
                .join(",")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn assumptions(
    boundary: &QuotientBoundary,
    abi: &AbiModel,
    budget: &QuotientBudget,
) -> Vec<Assumption> {
    vec![
        Assumption {
            kind: AssumptionKind::Memory,
            description: format!(
                "the {}-byte context (argument {}) is the only state that persists between \
                 steps, and it aliases neither the stack frame nor the register file",
                boundary.context.initial.len(),
                boundary.context.arg_index
            ),
        },
        Assumption {
            kind: AssumptionKind::CallingConvention,
            description: format!(
                "arguments arrive in {} and the result in {}",
                abi.arguments
                    .iter()
                    .map(|slot| slot.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
                abi.return_value.name
            ),
        },
        Assumption {
            kind: AssumptionKind::Environment,
            description: "a step is deterministic in its context and inputs".to_string(),
        },
        Assumption {
            kind: AssumptionKind::Bound,
            description: format!(
                "equivalence is claimed for input words of at most {} steps",
                budget.horizon
            ),
        },
    ]
}
