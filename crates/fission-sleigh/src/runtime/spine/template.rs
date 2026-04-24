use anyhow::Result;

use crate::compiler::{CompiledOpTpl, CompiledSemanticOp};
use crate::runtime::RuntimeExecutionDetails;

use super::RuntimeConstructState;

pub trait RuntimeTemplateExecutor {
    fn emit_op_template(&mut self, state: &RuntimeConstructState, op: &CompiledOpTpl)
        -> Result<()>;
    fn emit_compatibility_op(
        &mut self,
        state: &RuntimeConstructState,
        op: &CompiledSemanticOp,
    ) -> Result<()>;
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

    pub fn emit(&mut self, state: &RuntimeConstructState) -> Result<RuntimeExecutionDetails> {
        if !state.constructor_template.op_templates.is_empty() {
            for op in &state.constructor_template.op_templates {
                self.emitter.emit_op_template(state, op)?;
            }
            return Ok(RuntimeExecutionDetails {
                compat_emitter_used: false,
                template_source: Some(state.constructor_template.template_source),
            });
        }

        for op in &state.constructor_template.semantic_ops {
            if matches!(op, CompiledSemanticOp::Nop) {
                continue;
            }
            self.emitter.emit_compatibility_op(state, op)?;
        }
        Ok(RuntimeExecutionDetails {
            compat_emitter_used: true,
            template_source: Some(state.constructor_template.template_source),
        })
    }
}
