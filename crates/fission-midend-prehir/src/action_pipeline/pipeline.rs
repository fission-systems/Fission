//! Top-level pipeline orchestrator.

use super::concept::GhidraActionConcept;
use super::group::ActionGroup;
use super::pass::{PassCtx, record_last_pipeline_validation_error};
use crate::ir::validate_prehir_function;
use fission_midend_core::ir::{IrInvariantSet, StructuralValidationReport, ValidationMode};
use std::time::Instant;

pub struct Pipeline {
    pub name: &'static str,
    pub groups: Vec<ActionGroup>,
}

impl Pipeline {
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            groups: Vec::new(),
        }
    }

    pub fn group(mut self, group: ActionGroup) -> Self {
        self.groups.push(group);
        self
    }

    pub fn run(&self, ctx: &mut PassCtx<'_, '_>) {
        if ValidationMode::from_env() == ValidationMode::VerifyEach {
            if let Err(report) = self.run_validated(ctx, ValidationMode::VerifyEach) {
                record_last_pipeline_validation_error(report);
            }
            return;
        }
        let _ = self.run_validated(ctx, ValidationMode::Off);
    }

    /// Execute the pipeline under an explicit [`ValidationMode`], returning the
    /// final IR revision on success or the first [`StructuralValidationReport`]
    /// that halted execution.
    pub fn run_validated(
        &self,
        ctx: &mut PassCtx<'_, '_>,
        mode: ValidationMode,
    ) -> Result<u64, StructuralValidationReport> {
        let total_start = if ctx.perf { Some(Instant::now()) } else { None };
        let mut revision = 0u64;

        if mode == ValidationMode::StageBoundary {
            let entry_label = format!("stage:{}:entry", self.name);
            validate_prehir_function(
                ctx.func,
                IrInvariantSet::PREHIR_STAGE,
                &entry_label,
                revision,
            )?;
        }

        if ctx.diag {
            eprintln!("[DIAG] pipeline start: {} fn={}", self.name, ctx.func.name);
        }

        for group in &self.groups {
            group.run_validated(ctx, mode, &mut revision)?;
        }

        if mode == ValidationMode::StageBoundary {
            let exit_label = format!("stage:{}:exit", self.name);
            validate_prehir_function(
                ctx.func,
                IrInvariantSet::PREHIR_STAGE,
                &exit_label,
                revision,
            )?;
        }

        if let Some(start) = total_start {
            eprintln!(
                "[PERF] pipeline total: name={} fn={} elapsed_ms={:.3}",
                self.name,
                ctx.func.name,
                start.elapsed().as_secs_f64() * 1000.0,
            );
        }

        if ctx.diag {
            eprintln!("[DIAG] pipeline done: {} fn={}", self.name, ctx.func.name);
        }

        Ok(revision)
    }

    pub fn group_names(&self) -> Vec<&'static str> {
        self.groups.iter().map(|group| group.name).collect()
    }
}

pub fn group(name: &'static str, concept: GhidraActionConcept) -> ActionGroup {
    ActionGroup::new(name, concept)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action_pipeline::pass::fn_pass_with_contract;
    use crate::action_pipeline::{Repeat, pass::fn_pass};
    use crate::ir::{PreHirBinding, PreHirExpr, PreHirFunction, PreHirLValue, PreHirStmt};
    use fission_midend_core::ir::{IrInvariant, NirType, PassContract};

    fn i32_ty() -> NirType {
        NirType::Int {
            bits: 32,
            signed: true,
        }
    }

    #[test]
    fn verify_each_localizes_first_failing_pass_and_stops_execution() {
        fn valid_step(func: &mut PreHirFunction) -> bool {
            func.body.push(PreHirStmt::Assign {
                lhs: PreHirLValue::Var("x".to_string()),
                rhs: PreHirExpr::Const(
                    1,
                    NirType::Int {
                        bits: 32,
                        signed: true,
                    },
                ),
            });
            true
        }

        fn corrupting_step(func: &mut PreHirFunction) -> bool {
            func.body.push(PreHirStmt::Break);
            true
        }

        fn third_step(func: &mut PreHirFunction) -> bool {
            func.body.push(PreHirStmt::Goto("never_runs".to_string()));
            true
        }

        let pipeline = Pipeline::new("test_verify_each").group(
            group("g", GhidraActionConcept::Normalize)
                .with_repeat(Repeat::Once)
                .pass(fn_pass_with_contract(
                    "pass_1_valid",
                    GhidraActionConcept::Normalize,
                    PassContract::default_prehir(),
                    valid_step,
                ))
                .pass(fn_pass(
                    "pass_2_corrupts",
                    GhidraActionConcept::Normalize,
                    corrupting_step,
                ))
                .pass(fn_pass(
                    "pass_3_should_not_run",
                    GhidraActionConcept::Normalize,
                    third_step,
                )),
        );

        let mut func = PreHirFunction {
            name: "sample".to_string(),
            locals: vec![PreHirBinding {
                name: "x".to_string(),
                ty: i32_ty(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            }],
            return_type: i32_ty(),
            ..Default::default()
        };
        let mut ctx = PassCtx {
            func: &mut func,
            perf: false,
            diag: false,
            stats: None,
            decomp_facts: None,
        };

        let report = pipeline
            .run_validated(&mut ctx, ValidationMode::VerifyEach)
            .unwrap_err();
        assert_eq!(report.pass, "pass_2_corrupts");
        assert_eq!(report.revision, 2);
        assert_eq!(
            report.violations[0].invariant,
            IrInvariant::StructuredControlExits
        );
        assert!(!report.proves_semantic_equivalence());
        assert!(
            !func
                .body
                .iter()
                .any(|s| matches!(s, PreHirStmt::Goto(lbl) if lbl == "never_runs")),
            "execution should stop immediately after pass_2_corrupts"
        );
    }
}
