//! Action groups and fixpoint repeat semantics.

use super::budget::hir_shape;
use super::concept::GhidraActionConcept;
use super::pass::{Pass, PassCtx, PassOutcome, run_pass_logged};
use crate::ir::{PreHirFunction, validate_prehir_function};
use fission_midend_core::ir::{StructuralValidationReport, ValidationMode};
use std::time::Instant;

#[derive(Debug, Clone, Copy)]
pub enum Repeat {
    Once,
    UntilStable { max_rounds: usize },
}

#[derive(Debug, Clone, Copy)]
pub enum Gate {
    Always,
    NotLargeFunction,
    Custom(fn(&PreHirFunction) -> bool),
}

impl Gate {
    pub fn allows(self, func: &PreHirFunction) -> bool {
        match self {
            Gate::Always => true,
            Gate::NotLargeFunction => !super::budget::is_large_hir_function(func),
            Gate::Custom(f) => f(func),
        }
    }
}

pub struct ActionGroup {
    pub name: &'static str,
    pub concept: GhidraActionConcept,
    pub gate: Gate,
    pub repeat: Repeat,
    pub passes: Vec<Box<dyn Pass>>,
}

impl ActionGroup {
    pub fn new(name: &'static str, concept: GhidraActionConcept) -> Self {
        Self {
            name,
            concept,
            gate: Gate::Always,
            repeat: Repeat::Once,
            passes: Vec::new(),
        }
    }

    pub fn with_gate(mut self, gate: Gate) -> Self {
        self.gate = gate;
        self
    }

    pub fn with_repeat(mut self, repeat: Repeat) -> Self {
        self.repeat = repeat;
        self
    }

    pub fn pass(mut self, pass: Box<dyn Pass>) -> Self {
        self.passes.push(pass);
        self
    }

    pub fn passes(mut self, passes: Vec<Box<dyn Pass>>) -> Self {
        self.passes.extend(passes);
        self
    }

    pub fn run(&self, ctx: &mut PassCtx<'_, '_>) -> bool {
        self.run_with_mode(ctx, ValidationMode::Off, &mut 0)
            .unwrap_or(false)
    }

    pub fn run_validated(
        &self,
        ctx: &mut PassCtx<'_, '_>,
        mode: ValidationMode,
        revision: &mut u64,
    ) -> Result<bool, StructuralValidationReport> {
        self.run_with_mode(ctx, mode, revision)
    }

    fn run_with_mode(
        &self,
        ctx: &mut PassCtx<'_, '_>,
        mode: ValidationMode,
        revision: &mut u64,
    ) -> Result<bool, StructuralValidationReport> {
        if !self.gate.allows(ctx.func) {
            return Ok(false);
        }

        let mut group_changed = false;
        match self.repeat {
            Repeat::Once => {
                group_changed = self.run_passes_once(ctx, mode, revision)?;
            }
            Repeat::UntilStable { max_rounds } => {
                for round in 0..max_rounds {
                    let round_start = if ctx.perf { Some(Instant::now()) } else { None };
                    let (before_stmts, before_locals) = if ctx.perf {
                        hir_shape(ctx.func)
                    } else {
                        (0, 0)
                    };
                    let round_changed = self.run_passes_once(ctx, mode, revision)?;
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
        Ok(group_changed)
    }

    fn run_passes_once(
        &self,
        ctx: &mut PassCtx<'_, '_>,
        mode: ValidationMode,
        revision: &mut u64,
    ) -> Result<bool, StructuralValidationReport> {
        let mut changed = false;
        for pass in &self.passes {
            let contract = pass.contract();
            if mode == ValidationMode::VerifyEach && !contract.required.is_empty() {
                let pre_label = format!("pre:{}", pass.name());
                validate_prehir_function(ctx.func, contract.required, &pre_label, *revision)?;
            }

            let pass_changed = run_pass_logged(ctx, pass.as_ref()).changed();
            changed |= pass_changed;
            *revision += 1;

            if mode == ValidationMode::VerifyEach && !contract.guaranteed.is_empty() {
                validate_prehir_function(ctx.func, contract.guaranteed, pass.name(), *revision)?;
            }
        }
        Ok(changed)
    }
}
