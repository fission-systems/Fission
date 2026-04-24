use anyhow::Result;

use crate::compiler::{CompiledOpTpl, CompiledTemplateSource};
use crate::runtime::{RuntimeExecutionDetails, RuntimeSleighError};

use super::RuntimeConstructState;

pub trait RuntimeTemplateExecutor {
    fn emit_op_template(&mut self, state: &RuntimeConstructState, op: &CompiledOpTpl)
        -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::{
        CompiledConstTpl, CompiledConstructTplKind, CompiledConstructorTemplate, CompiledOpTpl,
        CompiledOpTplOpcode, CompiledTemplateSource, CompiledVarnodeTpl,
    };
    use crate::runtime::spine::RuntimeMatchTrace;

    struct NoopExecutor;

    impl RuntimeTemplateExecutor for NoopExecutor {
        fn emit_op_template(
            &mut self,
            _state: &RuntimeConstructState,
            _op: &CompiledOpTpl,
        ) -> Result<()> {
            Ok(())
        }
    }

    fn empty_trace() -> RuntimeMatchTrace {
        RuntimeMatchTrace {
            root_bucket: "test".to_string(),
            probes: Vec::new(),
            leaf_constructor_indexes: Vec::new(),
        }
    }

    #[test]
    fn spec_derived_template_rejects_compatibility_varnode() {
        let state = RuntimeConstructState {
            construct_tpl_kind: CompiledConstructTplKind::Mov,
            constructor_template: CompiledConstructorTemplate {
                handles: Vec::new(),
                decode_steps: Vec::new(),
                semantic_ops: Vec::new(),
                op_templates: vec![CompiledOpTpl {
                    opcode: CompiledOpTplOpcode::Copy,
                    output: Some(CompiledVarnodeTpl::Handle { operand_index: 0 }),
                    inputs: vec![CompiledVarnodeTpl::Const(CompiledConstTpl::Integer {
                        value: 1,
                        size: 1,
                    })],
                    label: None,
                }],
                template_source: CompiledTemplateSource::SpecDerived,
            },
            construct_nodes: Vec::new(),
            handles: Vec::new(),
            operands: Vec::new(),
            condition_code: None,
            length: 1,
            match_trace: empty_trace(),
        };

        let err = RuntimeTemplateEvaluator::new(&mut NoopExecutor)
            .emit("test-language", &state)
            .expect_err("SpecDerived must reject compatibility varnodes");
        let rendered = err.to_string();
        assert!(rendered.contains("UnsupportedPcodeTemplate"));
        assert!(rendered.contains("spec_derived_construct_tpl_contains_compatibility_varnode"));
    }

    #[test]
    fn evaluator_source_has_no_compatibility_emit_hook() {
        let source = include_str!("template.rs");
        assert!(!source.contains(concat!("emit_", "compatibility_op")));
    }
}

pub struct RuntimeTemplateEvaluator<'a, E> {
    emitter: &'a mut E,
}

impl<'a, E> RuntimeTemplateEvaluator<'a, E>
where
    E: RuntimeTemplateExecutor,
{
    pub fn new(emitter: &'a mut E) -> Self {
        Self { emitter }
    }

    pub fn emit(
        &mut self,
        language: &str,
        state: &RuntimeConstructState,
    ) -> Result<RuntimeExecutionDetails> {
        match state.constructor_template.template_source {
            CompiledTemplateSource::SpecDerived => {
                if state.constructor_template.op_templates.is_empty() {
                    return Err(RuntimeSleighError::UnsupportedPcodeTemplate {
                        language: language.to_string(),
                        reason: "spec_derived_construct_tpl_has_no_ops".to_string(),
                    }
                    .into());
                }
                if !state
                    .constructor_template
                    .op_templates
                    .iter()
                    .all(CompiledOpTpl::uses_only_ghidra_template_shapes)
                {
                    return Err(RuntimeSleighError::UnsupportedPcodeTemplate {
                        language: language.to_string(),
                        reason: "spec_derived_construct_tpl_contains_compatibility_varnode"
                            .to_string(),
                    }
                    .into());
                }
                for op in &state.constructor_template.op_templates {
                    self.emitter.emit_op_template(state, op)?;
                }
                Ok(RuntimeExecutionDetails {
                    compat_emitter_used: false,
                    template_source: Some(state.constructor_template.template_source),
                })
            }
            CompiledTemplateSource::CompatibilityLowered => {
                Err(RuntimeSleighError::UnsupportedPcodeTemplate {
                    language: language.to_string(),
                    reason: "compatibility_lowered_template_not_canonical".to_string(),
                }
                .into())
            }
        }
    }
}
