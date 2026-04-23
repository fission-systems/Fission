use anyhow::Result;

use crate::compiler::{CompiledArithmeticOpcode, CompiledSemanticOp};

use super::RuntimeConstructState;

pub trait RuntimeSemanticEmitter {
    fn emit_return(&mut self) -> Result<()>;
    fn emit_call(&mut self, state: &RuntimeConstructState) -> Result<()>;
    fn emit_jump(&mut self, state: &RuntimeConstructState) -> Result<()>;
    fn emit_conditional_jump(&mut self, state: &RuntimeConstructState) -> Result<()>;
    fn emit_copy_template(&mut self, state: &RuntimeConstructState) -> Result<()>;
    fn emit_address_template(&mut self, state: &RuntimeConstructState) -> Result<()>;
    fn emit_stack_store_template(&mut self, state: &RuntimeConstructState) -> Result<()>;
    fn emit_stack_load_template(&mut self, state: &RuntimeConstructState) -> Result<()>;
    fn emit_frame_teardown_template(&mut self) -> Result<()>;
    fn emit_binary(
        &mut self,
        state: &RuntimeConstructState,
        opcode: CompiledArithmeticOpcode,
    ) -> Result<()>;
    fn emit_compare(&mut self, state: &RuntimeConstructState, bitwise: bool) -> Result<()>;
    fn emit_extend(&mut self, state: &RuntimeConstructState, signed: bool) -> Result<()>;
    fn emit_setcc(&mut self, state: &RuntimeConstructState) -> Result<()>;
    fn emit_accumulator_extend(
        &mut self,
        state: &RuntimeConstructState,
        src_size: u32,
        dst_size: u32,
    ) -> Result<()>;
}

pub struct RuntimeTemplateEvaluator<'a, E> {
    emitter: &'a mut E,
}

impl<'a, E> RuntimeTemplateEvaluator<'a, E>
where
    E: RuntimeSemanticEmitter,
{
    pub fn new(emitter: &'a mut E) -> Self {
        Self { emitter }
    }

    pub fn emit(&mut self, state: &RuntimeConstructState) -> Result<()> {
        for op in &state.constructor_template.semantic_ops {
            match op {
                CompiledSemanticOp::Nop => {}
                CompiledSemanticOp::Return => self.emitter.emit_return()?,
                CompiledSemanticOp::Call => self.emitter.emit_call(state)?,
                CompiledSemanticOp::Jump => self.emitter.emit_jump(state)?,
                CompiledSemanticOp::ConditionalJump => self.emitter.emit_conditional_jump(state)?,
                CompiledSemanticOp::Copy => self.emitter.emit_copy_template(state)?,
                CompiledSemanticOp::AddressOf => self.emitter.emit_address_template(state)?,
                CompiledSemanticOp::StackStore => self.emitter.emit_stack_store_template(state)?,
                CompiledSemanticOp::StackLoad => self.emitter.emit_stack_load_template(state)?,
                CompiledSemanticOp::FrameTeardown => self.emitter.emit_frame_teardown_template()?,
                CompiledSemanticOp::Binary { opcode } => {
                    self.emitter.emit_binary(state, *opcode)?
                }
                CompiledSemanticOp::Compare { bitwise } => {
                    self.emitter.emit_compare(state, *bitwise)?
                }
                CompiledSemanticOp::Extend { signed } => {
                    self.emitter.emit_extend(state, *signed)?
                }
                CompiledSemanticOp::SetCc => self.emitter.emit_setcc(state)?,
                CompiledSemanticOp::AccumulatorExtend { src_size, dst_size } => self
                    .emitter
                    .emit_accumulator_extend(state, *src_size, *dst_size)?,
            }
        }
        Ok(())
    }
}
