//! Membership queries: run an input word from the initial context and say
//! what each step observably did.

use std::path::Path;

use anyhow::{Context, Result, bail};
use fission_dir::{CallOutcome, EmulatorHarness};
use serde::Serialize;

use crate::boundary::QuotientBoundary;
use crate::expr::mask;

/// One input symbol: a value per input port.
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
}

/// What one step showed the outside world.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub struct Observation {
    pub exit: Exit,
    /// The result, masked to the boundary's width; 0 unless `exit` is
    /// `Returned`.
    pub value: u64,
}

impl Observation {
    #[must_use]
    pub fn returned(value: u64) -> Self {
        Self {
            exit: Exit::Returned,
            value,
        }
    }

    #[must_use]
    pub fn halted() -> Self {
        Self {
            exit: Exit::Halted,
            value: 0,
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
}

/// Membership through the real emulator: each step is one call of the
/// slice's machine code, with the context in a mapped buffer that persists
/// between the steps of a word and is reset between words.
pub struct EmulatorOracle {
    harness: EmulatorHarness,
    boundary: QuotientBoundary,
    context_address: u64,
    arg_count: usize,
    steps: u64,
}

impl EmulatorOracle {
    pub fn new(
        binary_path: &Path,
        boundary: &QuotientBoundary,
        max_instructions: u64,
    ) -> Result<Self> {
        let mut harness = EmulatorHarness::build(binary_path, Some(max_instructions))?;
        let context_address = harness
            .install_scratch(&boundary.context.initial)
            .context("failed to map the context buffer")?;
        let arg_count = boundary
            .inputs
            .iter()
            .map(|port| port.arg_index + 1)
            .chain([boundary.context.arg_index + 1])
            .max()
            .unwrap_or(0);
        Ok(Self {
            harness,
            boundary: boundary.clone(),
            context_address,
            arg_count,
            steps: 0,
        })
    }
}

impl MembershipOracle for EmulatorOracle {
    fn run(&mut self, word: &[Symbol]) -> Result<Vec<StepRecord>> {
        let mut context = self.boundary.context.initial.clone();
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
            if symbol.len() != self.boundary.inputs.len() {
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
            let observation = match self.harness.call(self.boundary.entry, &args)? {
                CallOutcome::Returned(value) => {
                    Observation::returned(value & mask(self.boundary.output_bits))
                }
                CallOutcome::HitBudget => Observation {
                    exit: Exit::Budget,
                    value: 0,
                },
                CallOutcome::ProcessExited | CallOutcome::Other(_) => Observation {
                    exit: Exit::Fault,
                    value: 0,
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
}
