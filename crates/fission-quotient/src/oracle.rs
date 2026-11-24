//! Membership queries: run an input word from the initial context and say
//! what each step observably did.

use std::path::Path;

use anyhow::{Context, Result, bail};
use fission_dir::{CallOutcome, EmulatorHarness, MachineFlow, StubbedEnd};
use fission_loader::loader::LoadedBinary;
use serde::Serialize;

use crate::boundary::{AbiModel, QuotientBoundary};
use crate::calls::CallModel;
use crate::expr::mask;

/// One input symbol: a value per input port -- the boundary's ports, then
/// what each call of the step returns, in call order.
pub type Symbol = Vec<u64>;

/// How one step ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Exit {
    Returned,
    /// Stopped for a reason other than returning: a fault, an exit.
    Fault,
    /// Did not return within the instruction budget.
    Budget,
    /// An earlier step of the word did not return; nothing ran.
    Halted,
    /// Ended in a call proven not to return; no later step runs.
    NoReturn,
    /// The model's own: an access the P-code summary cannot keep inside its
    /// pointer region. The machine never shows it, so a quotient in which
    /// it can happen is unsupported, never built.
    LeftRegion,
}

/// One call a step made: where to, with which observed arguments.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub struct CallEvent {
    #[serde(serialize_with = "crate::report::hex_address")]
    pub target: u64,
    /// The argument registers the call site observes, in convention order,
    /// each masked to its observed width.
    pub args: Vec<u64>,
}

/// What one step showed the outside world.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub struct Observation {
    pub exit: Exit,
    /// The result, masked to the boundary's width; 0 unless `exit` is
    /// `Returned`.
    pub value: u64,
    /// The calls the step made, in order.
    pub calls: Vec<CallEvent>,
}

impl Observation {
    #[must_use]
    pub fn returned(value: u64) -> Self {
        Self {
            exit: Exit::Returned,
            value,
            calls: Vec::new(),
        }
    }

    #[must_use]
    pub fn halted() -> Self {
        Self {
            exit: Exit::Halted,
            value: 0,
            calls: Vec::new(),
        }
    }

    fn ended(exit: Exit) -> Self {
        Self {
            exit,
            value: 0,
            calls: Vec::new(),
        }
    }
}

/// One step of a membership query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepRecord {
    pub observation: Observation,
    /// The context bytes the step left behind.
    pub context: Vec<u8>,
}

/// The concrete ground truth the learner queries.
pub trait MembershipOracle {
    /// Run `word` from the initial context.
    fn run(&mut self, word: &[Symbol]) -> Result<Vec<StepRecord>>;

    /// Steps executed so far.
    fn steps(&self) -> u64;

    /// How the slice's calls are observed, once the summary has decided it.
    /// An oracle whose slices make no calls can ignore it.
    fn observe_calls(&mut self, _model: &CallModel) {}

    /// Give every pointer region of `boundary` an allocation of its own and
    /// return the memory a word starts from: the context, then each region,
    /// laid out as [`QuotientBoundary::layout`] says. `StepRecord::context`
    /// is that byte space too.
    fn place_regions(&mut self, boundary: &QuotientBoundary) -> Result<Vec<u8>> {
        Ok(boundary.initial_memory())
    }
}

/// Membership through the real emulator: each step is one call of the
/// slice's machine code, with the context in a mapped buffer that persists
/// between the steps of a word and is reset between words.
///
/// Calls the slice makes are stubbed (`EmulatorHarness::call_stubbed`, as
/// `verify --tier ground-truth` stubs them): the callee never runs, the
/// `n`th call of a step returns the symbol's `n`th value past the
/// boundary's ports (0 when the symbol has none), and the call is observed
/// through the [`CallModel`] the summary decided.
pub struct EmulatorOracle {
    harness: EmulatorHarness,
    boundary: QuotientBoundary,
    context_address: u64,
    /// The context and the pointer regions behind it, as one buffer, with
    /// each region's real address in its pointer field.
    memory: Vec<u8>,
    pointer_bytes: usize,
    big_endian: bool,
    arg_count: usize,
    steps: u64,
    /// The function's calls and tail jumps, when it has any and they could
    /// all be listed. `None` runs the machine code unstubbed.
    flow: Option<MachineFlow>,
    calls: CallModel,
}

impl EmulatorOracle {
    pub fn new(
        binary_path: &Path,
        boundary: &QuotientBoundary,
        max_instructions: u64,
    ) -> Result<Self> {
        let harness = EmulatorHarness::build(binary_path, Some(max_instructions))?;
        let loaded = LoadedBinary::from_file(binary_path).ok();
        let (pointer_bytes, big_endian) = match &loaded {
            Some(binary) => {
                let abi = AbiModel::for_binary(binary)?;
                ((abi.pointer_bits / 8) as usize, abi.big_endian)
            }
            None => (8, false),
        };
        let arg_count = boundary
            .inputs
            .iter()
            .map(|port| port.arg_index + 1)
            .chain([boundary.context.arg_index + 1])
            .max()
            .unwrap_or(0);
        // A function whose calls cannot all be listed is run unstubbed: the
        // summary refuses any call it makes, so a call that runs here only
        // ever shows up as a disagreement.
        let flow = loaded
            .as_ref()
            .and_then(|binary| fission_dir::call_sites(binary, boundary.entry).ok())
            .filter(|flow| !flow.calls.is_empty() || !flow.tail_jumps.is_empty());
        let mut oracle = Self {
            harness,
            boundary: boundary.clone(),
            context_address: 0,
            memory: Vec::new(),
            pointer_bytes,
            big_endian,
            arg_count,
            steps: 0,
            flow,
            calls: CallModel::default(),
        };
        oracle.lay_out(boundary)?;
        Ok(oracle)
    }

    /// Map the context and its regions as one buffer and write each
    /// region's address into its pointer field.
    fn lay_out(&mut self, boundary: &QuotientBoundary) -> Result<()> {
        let layout = boundary.layout();
        let address = self
            .harness
            .install_scratch(&vec![0; layout.len])
            .context("failed to map the context buffer")?;
        let mut memory = boundary.initial_memory();
        for (base, field) in layout.bases.iter().zip(&layout.fields) {
            let pointer = address + u64::from(*base);
            let bytes = if self.big_endian {
                pointer.to_be_bytes()[8 - self.pointer_bytes..].to_vec()
            } else {
                pointer.to_le_bytes()[..self.pointer_bytes].to_vec()
            };
            let field = *field as usize;
            memory[field..field + self.pointer_bytes].copy_from_slice(&bytes);
        }
        if self.harness.install_scratch(&memory)? != address {
            bail!("the context buffer moved while its regions were laid out");
        }
        self.boundary = boundary.clone();
        self.context_address = address;
        self.memory = memory;
        Ok(())
    }

    /// Run one step with calls stubbed: the `n`th call returns
    /// `returns[n]`, or 0 past its end.
    fn stubbed_step(
        &mut self,
        flow: &MachineFlow,
        args: &[u64],
        returns: &[u64],
    ) -> Result<Observation> {
        let result = |n: usize| returns.get(n).copied().unwrap_or(0);
        let run = self
            .harness
            .call_stubbed(self.boundary.entry, args, flow, &result)?;
        let calls = run
            .calls
            .iter()
            .map(|call| match self.calls.sites.get(&call.site) {
                Some(site) => CallEvent {
                    target: site.target,
                    args: site.observe(&call.args),
                },
                // A site the summary never saw: observed by its address,
                // with every argument register, so it cannot match.
                None => CallEvent {
                    target: call.site,
                    args: call.args.clone(),
                },
            })
            .collect();
        let mut observation = match run.end {
            StubbedEnd::Returned(value) => {
                Observation::returned(value & mask(self.boundary.output_bits))
            }
            StubbedEnd::NoReturnCall(_) => Observation::ended(Exit::NoReturn),
            StubbedEnd::Other(CallOutcome::HitBudget) => Observation::ended(Exit::Budget),
            StubbedEnd::Other(_) | StubbedEnd::LeftDecodedCode(_) => {
                Observation::ended(Exit::Fault)
            }
        };
        observation.calls = calls;
        Ok(observation)
    }
}

impl MembershipOracle for EmulatorOracle {
    fn run(&mut self, word: &[Symbol]) -> Result<Vec<StepRecord>> {
        let mut context = self.memory.clone();
        let mut halted = false;
        let mut records = Vec::with_capacity(word.len());
        for symbol in word {
            if halted {
                records.push(StepRecord {
                    observation: Observation::halted(),
                    context: context.clone(),
                });
                continue;
            }
            if symbol.len() < self.boundary.inputs.len() {
                bail!(
                    "a symbol of {} values for {} ports",
                    symbol.len(),
                    self.boundary.inputs.len()
                );
            }
            let address = self.harness.install_scratch(&context)?;
            if address != self.context_address {
                bail!("the context buffer moved between steps");
            }
            let mut args = vec![0u64; self.arg_count];
            args[self.boundary.context.arg_index] = address;
            for (port, value) in self.boundary.inputs.iter().zip(symbol) {
                args[port.arg_index] = value & mask(port.bits);
            }
            self.steps += 1;
            let observation = match self.flow.take() {
                Some(flow) => {
                    let returns = &symbol[self.boundary.inputs.len()..];
                    let observed = self.stubbed_step(&flow, &args, returns);
                    self.flow = Some(flow);
                    observed?
                }
                None => match self.harness.call(self.boundary.entry, &args)? {
                    CallOutcome::Returned(value) => {
                        Observation::returned(value & mask(self.boundary.output_bits))
                    }
                    CallOutcome::HitBudget => Observation::ended(Exit::Budget),
                    CallOutcome::ProcessExited | CallOutcome::Other(_) => {
                        Observation::ended(Exit::Fault)
                    }
                },
            };
            if observation.exit == Exit::Returned {
                context = self.harness.read_memory(address, context.len())?;
            } else {
                halted = true;
            }
            records.push(StepRecord {
                observation,
                context: context.clone(),
            });
        }
        Ok(records)
    }

    fn steps(&self) -> u64 {
        self.steps
    }

    fn observe_calls(&mut self, model: &CallModel) {
        self.calls = model.clone();
    }

    fn place_regions(&mut self, boundary: &QuotientBoundary) -> Result<Vec<u8>> {
        if *boundary != self.boundary {
            self.lay_out(boundary)?;
        }
        Ok(self.memory.clone())
    }
}
