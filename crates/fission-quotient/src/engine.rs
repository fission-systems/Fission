//! `WBCQ_Synthesize`: summarize, factor, learn, build, refute, repeat.

use std::path::Path;

use anyhow::{Context, Result};
use fission_dir::PcodeFoundation;
use fission_loader::loader::LoadedBinary;

use crate::boundary::{AbiModel, QuotientBoundary, QuotientBudget};
use crate::calls::{CallFacts, CallModel, CalleeClass};
use crate::certify::{Refutation, probe_words, refute};
use crate::expr::{render, width};
use crate::learn::{Hypothesis, Learned, Learner};
use crate::oracle::{EmulatorOracle, MembershipOracle, Symbol};
use crate::report::{
    ApartnessWitness, Assumption, AssumptionKind, CertifiedQuotient, Evidence, FoundationRef,
    MODEL_ID, MODEL_ID_WITH_REGIONS, Metrics, QuotientStatus, SCHEMA_VERSION, SlotReport, SlotRole,
    SlotValue, StateReport, TransitionReport, UpdateReport,
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
    /// What is known about each call the slice makes.
    pub calls: CallFacts,
    /// `[start, end)` of each addressable section of the image: where an
    /// absolute address may name global memory (a loaded header is not a
    /// section).
    pub sections: Vec<(u64, u64)>,
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
        let calls = CallFacts::for_function(&binary, boundary.entry, foundation.pcode().as_raw());
        let sections = binary
            .sections
            .iter()
            .filter(|section| section.is_addressable() && section.virtual_size > 0)
            .map(|section| {
                (
                    section.virtual_address,
                    section.virtual_address + section.virtual_size,
                )
            })
            .collect();
        let oracle = EmulatorOracle::new(path, &boundary, max_instructions)?;
        Ok((
            Self {
                abi,
                boundary,
                oracle,
                calls,
                sections,
            },
            foundation,
        ))
    }
}

/// Where a distinguishing word came from.
#[derive(Clone, Copy)]
enum Source {
    /// The solver, asked about results only.
    ResultsQuery,
    /// The solver, asked about data once the results were proved.
    DataQuery,
    /// A concrete probe, after the solver could not decide.
    Probe,
}

enum Replay {
    Agrees,
    /// The result or the exit differs: the learner can see it.
    Visible,
    /// Only a data slot differs.
    DataOnly,
}

/// The automaton behind a report, for replaying words against it.
pub struct QuotientModel {
    summary: SliceSummary,
    roles: SlotRoles,
    srqa: Srqa,
    data_initial: Vec<u64>,
    big_endian: bool,
}

/// How a replayed word compared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayVerdict {
    Agrees,
    /// The result or the exit differs.
    ResultDiffers,
    /// Only a data field differs.
    DataDiffers,
}

impl QuotientModel {
    /// Compare what the machine did on `word` (`records`, from an oracle)
    /// with what the automaton predicts.
    #[must_use]
    pub fn compare(&self, word: &[Symbol], records: &[crate::oracle::StepRecord]) -> ReplayVerdict {
        let Some(predicted) =
            self.srqa
                .run(&self.summary, &self.roles, self.data_initial.clone(), word)
        else {
            return ReplayVerdict::ResultDiffers;
        };
        let mut data_differs = false;
        for (record, step) in records.iter().zip(&predicted) {
            if record.observation != step.observation() {
                return ReplayVerdict::ResultDiffers;
            }
            for (slot, value) in self.roles.data.iter().zip(&step.data) {
                let slot = &self.summary.slots[*slot];
                if slot_value(&record.context, slot.offset, slot.size, self.big_endian) != *value {
                    data_differs = true;
                }
            }
        }
        if data_differs {
            ReplayVerdict::DataDiffers
        } else {
            ReplayVerdict::Agrees
        }
    }
}

impl QuotientModel {
    /// How the slice's calls are observed; an oracle replaying words against
    /// the automaton must observe them the same way.
    #[must_use]
    pub fn calls(&self) -> &CallModel {
        &self.summary.calls
    }

    /// Input ports per symbol: the boundary's, then one per call.
    #[must_use]
    pub fn ports(&self) -> usize {
        self.summary.inputs.len()
    }
}

impl<O: MembershipOracle> QuotientEngine for Wbcq<O> {
    fn synthesize(
        &mut self,
        foundation: &PcodeFoundation,
        budget: QuotientBudget,
    ) -> Result<CertifiedQuotient> {
        Ok(self.synthesize_with_model(foundation, budget)?.0)
    }
}

impl<O: MembershipOracle> Wbcq<O> {
    /// [`QuotientEngine::synthesize`], also returning the automaton when one
    /// was built (any status but partial and unsupported).
    pub fn synthesize_with_model(
        &mut self,
        foundation: &PcodeFoundation,
        budget: QuotientBudget,
    ) -> Result<(CertifiedQuotient, Option<QuotientModel>)> {
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
            calls: Vec::new(),
            assumptions: assumptions(&self.boundary, &self.abi, &budget),
            evidence: Evidence::default(),
            metrics: Metrics {
                pcode_blocks: raw.blocks.len(),
                pcode_edges: raw.blocks.iter().map(|b| b.successors.len()).sum(),
                ..Metrics::default()
            },
        };

        let oracle = &self.oracle;
        let sections = &self.sections;
        let image = |address: u64, len: usize| {
            let end = address.checked_add(len as u64)?;
            sections
                .iter()
                .any(|(start, stop)| *start <= address && end <= *stop)
                .then(|| oracle.initial_bytes(address, len))
                .flatten()
        };
        let summary = match summarize(raw, &self.abi, &self.boundary, &budget, &self.calls, &image)
        {
            Ok(summary) => summary,
            Err(error) => {
                report.status = QuotientStatus::UnsupportedSemantics {
                    reason: error.to_string(),
                };
                return Ok((report, None));
            }
        };
        self.oracle.observe_calls(&summary.calls);
        // The pointer regions the summary used -- declared, then discovered --
        // are part of the boundary the quotient is relative to, and the
        // oracle gives each one its allocation.
        self.boundary.regions.clone_from(&summary.regions);
        let initial = self.oracle.place_regions(&self.boundary)?;
        report.boundary = self.boundary.clone();
        report.calls = summary.calls.sites.values().cloned().collect();
        report
            .assumptions
            .extend(call_assumptions(&summary.calls, &self.abi));
        report
            .assumptions
            .extend(region_assumptions(&self.boundary, &budget));
        let roles = factor(&summary);
        let big = self.abi.big_endian;
        let layout = self.boundary.layout();
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
                region: slot.region,
                offset: slot.offset - slot.region.map_or(0, |region| layout.bases[region]),
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
        // The control-flow graph a path-sensitive constant propagation with
        // dead-branch pruning leaves: the blocks and edges some feasible
        // path of one step uses.
        let mut used_blocks = std::collections::BTreeSet::new();
        let mut used_edges = std::collections::BTreeSet::new();
        for path in &summary.paths {
            used_blocks.extend(path.blocks.iter().copied());
            used_edges.extend(path.blocks.windows(2).map(|pair| (pair[0], pair[1])));
        }
        report.metrics.feasible_blocks = used_blocks.len();
        report.metrics.feasible_edges = used_edges.len();
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

        let mut built: Option<Srqa> = None;
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

            // The results first: a data field can only be judged once the
            // control machine stands, or an incomplete machine would be
            // reported as data that does not factor.
            report.evidence.refutation_queries += 1;
            let mut source = Source::ResultsQuery;
            let mut refutation = refute(
                &summary,
                &roles,
                &srqa,
                &initial_slots,
                budget.horizon,
                &budget,
                false,
            );
            // Two machines of n and m states that differ do so on a word of
            // fewer than n + m steps (Moore). So once the results hold for
            // `H`, deepen towards `n + K_max`: a state told apart only by a
            // long word -- a protocol keyword, a counter -- must not be
            // merged because the horizon was short. The status reports the
            // deepest horizon proved.
            let mut proved_horizon = 0;
            if matches!(refutation, Refutation::Proved) {
                proved_horizon = budget.horizon;
                let bound = hypothesis.states.len() as u32 + budget.max_states as u32;
                let mut next = budget.horizon.max(1) * 2;
                while proved_horizon < bound {
                    let depth = next.min(bound);
                    report.evidence.refutation_queries += 1;
                    match refute(
                        &summary,
                        &roles,
                        &srqa,
                        &initial_slots,
                        depth,
                        &budget,
                        false,
                    ) {
                        Refutation::Proved => {
                            proved_horizon = depth;
                            next = depth * 2;
                        }
                        Refutation::Candidate(word) => {
                            refutation = Refutation::Candidate(word);
                            break;
                        }
                        Refutation::Unknown => break,
                    }
                }
            }
            if matches!(refutation, Refutation::Proved) {
                report.evidence.refutation_queries += 1;
                source = Source::DataQuery;
                refutation = refute(
                    &summary,
                    &roles,
                    &srqa,
                    &initial_slots,
                    budget.horizon,
                    &budget,
                    true,
                );
            }
            let candidate = match refutation {
                Refutation::Proved => {
                    self.finish(&mut report, &summary, &roles, &learner, &hypothesis, &srqa)?;
                    built = Some(srqa.clone());
                    break QuotientStatus::ProvedUnderModel {
                        horizon: proved_horizon,
                        model_id: if self.boundary.regions.is_empty() {
                            MODEL_ID
                        } else {
                            MODEL_ID_WITH_REGIONS
                        }
                        .to_string(),
                    };
                }
                Refutation::Candidate(word) => Some(word),
                Refutation::Unknown => {
                    report.evidence.solver_unknowns += 1;
                    source = Source::Probe;
                    let ports = summary.inputs.iter().map(width).collect::<Vec<_>>();
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
                    found
                }
            };
            let Some(word) = candidate else {
                self.finish(&mut report, &summary, &roles, &learner, &hypothesis, &srqa)?;
                built = Some(srqa.clone());
                break QuotientStatus::EmpiricallyConsistent {
                    horizon: budget.horizon,
                    probes_run: report.evidence.probes_run,
                    solver_unknowns: report.evidence.solver_unknowns,
                };
            };
            match (
                self.replay(&summary, &roles, &srqa, &data_initial, &word)?,
                source,
            ) {
                (Replay::Visible, _) => {
                    // Into the tree; the next round's consistency check turns
                    // it into a split.
                    let indices = word.iter().map(|s| learner.intern(s)).collect::<Vec<_>>();
                    learner.query(&mut self.oracle, &indices)?;
                }
                (Replay::DataOnly, Source::DataQuery | Source::Probe) => {
                    self.finish(&mut report, &summary, &roles, &learner, &hypothesis, &srqa)?;
                    built = Some(srqa.clone());
                    break QuotientStatus::Refuted {
                        counterexample_hex: hex_word(&word),
                    };
                }
                (Replay::DataOnly | Replay::Agrees, Source::ResultsQuery)
                | (Replay::Agrees, Source::DataQuery) => {
                    break QuotientStatus::UnsupportedSemantics {
                        reason: format!(
                            "the solver's distinguishing word {} does not reproduce on the \
                             emulator: the P-code model and the machine disagree",
                            hex_word(&word)
                        ),
                    };
                }
                (Replay::Agrees, Source::Probe) => {
                    unreachable!("a probe is only returned when it disagrees")
                }
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
            built = None;
        }
        let model = built.map(|srqa| QuotientModel {
            summary,
            roles,
            srqa,
            data_initial,
            big_endian: big,
        });
        Ok((report, model))
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
            if record.observation != step.observation() {
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
        let callee = |target: u64| {
            summary
                .calls
                .sites
                .values()
                .find(|site| site.target == target)
                .and_then(|site| site.callee.clone())
                .unwrap_or_else(|| format!("0x{target:x}"))
        };
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
                calls: t
                    .calls
                    .iter()
                    .map(|call| {
                        format!(
                            "{}({})",
                            callee(call.target),
                            call.args
                                .iter()
                                .map(|arg| render(arg, &names))
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    })
                    .collect(),
                exit: t.exit,
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
                        .map(|record| record.observation.clone())
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

/// What a quotient over pointer regions assumes about them.
fn region_assumptions(boundary: &QuotientBoundary, budget: &QuotientBudget) -> Vec<Assumption> {
    if boundary.regions.is_empty() {
        return Vec::new();
    }
    let names = (0..boundary.regions.len())
        .map(|index| boundary.region_name(index))
        .collect::<Vec<_>>();
    let mut out = Vec::new();
    let named = |pick: &dyn Fn(&crate::boundary::PointerRegion) -> bool| {
        boundary
            .regions
            .iter()
            .zip(&names)
            .filter(|(region, _)| pick(region))
            .map(|(_, name)| name.as_str())
            .collect::<Vec<_>>()
    };
    let globals = named(&|region| region.address.is_some());
    if !globals.is_empty() {
        out.push(Assumption {
            kind: AssumptionKind::Memory,
            description: format!(
                "global memory {} is state of this step function alone: it starts as the \
                 binary's image holds it, and nothing else -- no callee, no other thread, no \
                 signal handler -- reads or writes it between or during steps",
                globals.join(", ")
            ),
        });
    }
    let anchored = boundary
        .regions
        .iter()
        .filter_map(|region| region.register.as_deref())
        .collect::<Vec<_>>();
    if !anchored.is_empty() {
        out.push(Assumption {
            kind: AssumptionKind::Environment,
            description: format!(
                "{} point(s) to a block of its own (thread-local storage, such as the stack \
                 protector's canary) that starts zeroed and that only this step function \
                 writes; a callee leaves the register as it found it when the calling \
                 convention does not list it as changed by a call",
                anchored.join(", ")
            ),
        });
    }
    let pointees = named(&|region| region.is_pointee());
    if pointees.is_empty() {
        return out;
    }
    out.extend([
        Assumption {
            kind: AssumptionKind::Memory,
            description: format!(
                "{} each point to a region of their own that aliases nothing else: not the \
                 context, not the stack frame, not another region, and no callee holds an \
                 alias of it; the pointers are not null and never change",
                pointees.join(", ")
            ),
        },
        Assumption {
            kind: AssumptionKind::Bound,
            description: format!(
                "each region is exactly as long as its initial bytes ({}); an access the model \
                 cannot keep inside them stops the quotient",
                boundary
                    .regions
                    .iter()
                    .zip(&names)
                    .filter(|(region, _)| region.is_pointee())
                    .map(|(region, name)| format!("{name}: {} bytes", region.initial.len()))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        },
    ]);
    let discovered = boundary
        .regions
        .iter()
        .zip(&names)
        .filter(|(region, _)| region.discovered && region.is_pointee())
        .map(|(_, name)| name.as_str())
        .collect::<Vec<_>>();
    if !discovered.is_empty() {
        out.push(Assumption {
            kind: AssumptionKind::Environment,
            description: format!(
                "{} were found dereferenced by the slice, not declared: their regions start \
                 zeroed ({} bytes, grown to cover every constant offset read, at most {}), so the \
                 quotient holds for that initial memory only",
                discovered.join(", "),
                budget.region_bytes,
                budget.max_region_bytes
            ),
        });
    }
    out
}

/// What a quotient over a slice that makes calls assumes about its callees.
fn call_assumptions(model: &CallModel, abi: &AbiModel) -> Vec<Assumption> {
    if model.is_empty() {
        return Vec::new();
    }
    let name = |site: &crate::calls::CallSiteModel| {
        site.callee
            .clone()
            .unwrap_or_else(|| format!("0x{:x}", site.target))
    };
    let names_of = |class: CalleeClass| {
        let mut names = model
            .sites
            .values()
            .filter(|site| site.class == class)
            .map(name)
            .collect::<Vec<_>>();
        names.sort();
        names.dedup();
        names
    };
    let mut out = vec![
        Assumption {
            kind: AssumptionKind::Environment,
            description: "what a call returns is chosen by the environment: the nth call of a \
                          step returns input port retN, and the quotient holds for every value \
                          of it (callees are stubbed on both sides, never run)"
                .to_string(),
        },
        Assumption {
            kind: AssumptionKind::Memory,
            description: "a callee writes no memory the slice reads: no address of the context \
                          or of the frame is passed to it, and it holds no alias of the context \
                          of its own"
                .to_string(),
        },
        Assumption {
            kind: AssumptionKind::CallingConvention,
            description: format!(
                "a callee returns to its call's fall-through with {} and the stack pointer as \
                 it found them, pops only the return address its call pushed, and may leave \
                 anything in every other register",
                abi.preserved
                    .iter()
                    .map(|slot| slot.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        },
    ];
    let library = names_of(CalleeClass::Library);
    if !library.is_empty() {
        out.push(Assumption {
            kind: AssumptionKind::CallingConvention,
            description: format!(
                "{} take the parameters their prototypes in the signature library declare; \
                 exactly those are observed",
                library.join(", ")
            ),
        });
    }
    let unknown = names_of(CalleeClass::Unknown);
    if !unknown.is_empty() {
        out.push(Assumption {
            kind: AssumptionKind::CallingConvention,
            description: format!(
                "the arity of {} is not known: every argument register the boundary defines \
                 at the call is observed (a register the callee ignores can only split \
                 states, never merge them); arguments passed on the stack are not observed",
                unknown.join(", ")
            ),
        });
    }
    let no_return = names_of(CalleeClass::NoReturn);
    if !no_return.is_empty() {
        out.push(Assumption {
            kind: AssumptionKind::Environment,
            description: format!(
                "{} never return (static no-return analysis): a step that calls one ends \
                 there, and the machine halts",
                no_return.join(", ")
            ),
        });
    }
    let mut tails = model
        .sites
        .values()
        .filter(|site| site.tail)
        .map(name)
        .collect::<Vec<_>>();
    tails.sort();
    tails.dedup();
    if !tails.is_empty() {
        out.push(Assumption {
            kind: AssumptionKind::Environment,
            description: format!(
                "a jump to {} leaves the slice: the callee's return is the step's return",
                tails.join(", ")
            ),
        });
    }
    out
}
