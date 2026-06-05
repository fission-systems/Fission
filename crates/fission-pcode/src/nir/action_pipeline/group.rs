//! Action groups and fixpoint repeat semantics.

use super::budget::hir_shape;
use super::concept::GhidraActionConcept;
use super::pass::{Pass, PassCtx, PassOutcome, run_pass_logged};
use super::super::types::HirFunction;
use std::time::Instant;

#[derive(Debug, Clone, Copy)]
pub(crate) enum Repeat {
    Once,
    UntilStable { max_rounds: usize },
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Gate {
    Always,
    NotLargeFunction,
    Custom(fn(&HirFunction) -> bool),
}

impl Gate {
    pub(crate) fn allows(self, func: &HirFunction) -> bool {
        match self {
            Gate::Always => true,
            Gate::NotLargeFunction => !super::budget::is_large_hir_function(func),
            Gate::Custom(f) => f(func),
        }
    }
}

pub(crate) struct ActionGroup {
    pub(crate) name: &'static str,
    pub(crate) concept: GhidraActionConcept,
    pub(crate) gate: Gate,
    pub(crate) repeat: Repeat,
    pub(crate) passes: Vec<Box<dyn Pass>>,
}

impl ActionGroup {
    pub(crate) fn new(name: &'static str, concept: GhidraActionConcept) -> Self {
        Self {
            name,
            concept,
            gate: Gate::Always,
            repeat: Repeat::Once,
            passes: Vec::new(),
        }
    }

    pub(crate) fn with_gate(mut self, gate: Gate) -> Self {
        self.gate = gate;
        self
    }

    pub(crate) fn with_repeat(mut self, repeat: Repeat) -> Self {
        self.repeat = repeat;
        self
    }

    pub(crate) fn pass(mut self, pass: Box<dyn Pass>) -> Self {
        self.passes.push(pass);
        self
    }

    pub(crate) fn passes(mut self, passes: Vec<Box<dyn Pass>>) -> Self {
        self.passes.extend(passes);
        self
    }

    pub(crate) fn run(&self, ctx: &mut PassCtx<'_>) -> bool {
        if !self.gate.allows(ctx.func) {
            return false;
        }

        let mut group_changed = false;
        match self.repeat {
            Repeat::Once => {
                group_changed = self.run_passes_once(ctx);
            }
            Repeat::UntilStable { max_rounds } => {
                for round in 0..max_rounds {
                    let round_start = if ctx.perf {
                        Some(Instant::now())
                    } else {
                        None
                    };
                    let (before_stmts, before_locals) = if ctx.perf {
                        hir_shape(ctx.func)
                    } else {
                        (0, 0)
                    };
                    let round_changed = self.run_passes_once(ctx);
                    group_changed |= round_changed;

                    if ctx.diag {
                        eprintln!(
                            "[DIAG] action-group: {} round={} changed={}",
                            self.name,
                            round + 1,
                            round_changed,
                        );
                    }

                    if let Some(start) = round_start {
                        let (after_stmts, after_locals) = hir_shape(ctx.func);
                        eprintln!(
                            "[PERF] action-group-round: group={} round={} changed={} elapsed_ms={:.3} stmts={}=>{} locals={}=>{}",
                            self.name,
                            round + 1,
                            round_changed,
                            start.elapsed().as_secs_f64() * 1000.0,
                            before_stmts,
                            after_stmts,
                            before_locals,
                            after_locals,
                        );
                    }

                    if !round_changed {
                        break;
                    }
                }
            }
        }

        ctx.record_stage(self.concept);
        group_changed
    }

    fn run_passes_once(&self, ctx: &mut PassCtx<'_>) -> bool {
        let mut changed = false;
        for pass in &self.passes {
            changed |= run_pass_logged(ctx, pass.as_ref()).changed();
        }
        changed
    }
}
