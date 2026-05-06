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
        CompiledConstTpl, CompiledConstructTplKind, CompiledConstructorTemplate,
        CompiledDisplayTemplate, CompiledOpTpl, CompiledOpTplOpcode, CompiledTemplateSource,
        CompiledVarnodeTpl,
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
            matched_leaf_pattern: None,
        }
    }

    #[test]
    fn spec_derived_template_rejects_compatibility_varnode() {
        let state = RuntimeConstructState {
            subtable_id: 0,
            constructor_id: 0,
            constructor_slot: 0,
            mnemonic: "mov".to_string(),
            construct_tpl_kind: CompiledConstructTplKind::Mov,
            constructor_template: CompiledConstructorTemplate {
                handles: Vec::new(),
                decode_steps: Vec::new(),
                num_labels: 0,
                result: None,
                ops: vec![CompiledOpTpl {
                    sla_raw_pcode_opcode: 0,
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
            display_template: CompiledDisplayTemplate::empty(),
            display_operands: Vec::new(),
            construct_nodes: Vec::new(),
            handles: Vec::new(),
            exported_handle: None,
            operands: Vec::new(),
            condition_code: None,
            absolute_offset: 0,
            relative_length: 1,
            length: 1,
            match_trace: empty_trace(),
            named_templates: Vec::new(),
            context_commits: Vec::new(),
            legacy_path_audit: Default::default(),
        };

        let err = RuntimeTemplateEvaluator::new(&mut NoopExecutor)
            .emit("test-language", &state)
            .expect_err("SpecDerived must reject compatibility varnodes");
        let rendered = err.to_string();
        assert!(rendered.contains("UnsupportedPcodeTemplate"));
        assert!(rendered.contains("spec_derived_construct_tpl_contains_"));
        assert!(rendered.contains("compatibility_handle_varnode"));
    }

    #[test]
    fn spec_derived_empty_template_is_zero_op_success() {
        let state = RuntimeConstructState {
            subtable_id: 0,
            constructor_id: 0,
            constructor_slot: 0,
            mnemonic: "nop".to_string(),
            construct_tpl_kind: CompiledConstructTplKind::Nop,
            constructor_template: CompiledConstructorTemplate {
                handles: Vec::new(),
                decode_steps: Vec::new(),
                num_labels: 0,
                result: None,
                ops: Vec::new(),
                template_source: CompiledTemplateSource::SpecDerived,
            },
            display_template: CompiledDisplayTemplate::empty(),
            display_operands: Vec::new(),
            construct_nodes: Vec::new(),
            handles: Vec::new(),
            exported_handle: None,
            operands: Vec::new(),
            condition_code: None,
            absolute_offset: 0,
            relative_length: 1,
            length: 1,
            match_trace: empty_trace(),
            named_templates: Vec::new(),
            context_commits: Vec::new(),
            legacy_path_audit: Default::default(),
        };

        let details = RuntimeTemplateEvaluator::new(&mut NoopExecutor)
            .emit("test-language", &state)
            .expect("SpecDerived empty templates are valid zero-op constructors");
        assert!(!details.compat_emitter_used);
        assert_eq!(
            details.template_source,
            Some(CompiledTemplateSource::SpecDerived)
        );
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
        if !matches!(
            state.constructor_template.template_source,
            CompiledTemplateSource::SpecDerived
        ) {
            return Err(RuntimeSleighError::UnsupportedPcodeTemplate {
                language: language.to_string(),
                reason: "non_spec_construct_tpl_not_canonical".to_string(),
            }
            .into());
        }
        if let Some(reason) = state.constructor_template.ghidra_template_shape_error() {
            return Err(RuntimeSleighError::UnsupportedPcodeTemplate {
                language: language.to_string(),
                reason: format!("spec_derived_construct_tpl_contains_{reason}"),
            }
            .into());
        }
        if std::env::var("FISSION_BUILD_DEBUG").is_ok() {
            eprintln!(
                "[template-emit] mnemonic={} template_source={:?} op_count={}",
                state.mnemonic,
                state.constructor_template.template_source,
                state.constructor_template.ops.len()
            );
            for (i, op) in state.constructor_template.ops.iter().enumerate() {
                eprintln!("  [op {}] {:?}", i, op.opcode);
            }
        }
        for op in &state.constructor_template.ops {
            self.emitter.emit_op_template(state, op)?;
        }
        Ok(RuntimeExecutionDetails {
            compat_emitter_used: false,
            template_source: Some(state.constructor_template.template_source),
            legacy_path_audit: state.legacy_path_audit,
        })
    }
}
