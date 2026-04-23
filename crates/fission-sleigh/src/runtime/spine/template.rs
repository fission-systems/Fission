use anyhow::Result;

use crate::compiler::{CompiledArithmeticOpcode, CompiledSemanticOp};

use super::RuntimeConstructState;

pub trait RuntimeSemanticEmitter {
    fn emit_return(&mut self) -> Result<()>;
    fn emit_call(&mut self, state: &RuntimeConstructState) -> Result<()>;
    fn emit_jump(&mut self, state: &RuntimeConstructState) -> Result<()>;
    fn emit_conditional_jump(&mut self, state: &RuntimeConstructState) -> Result<()>;
    fn emit_move(&mut self, state: &RuntimeConstructState) -> Result<()>;
    fn emit_lea(&mut self, state: &RuntimeConstructState) -> Result<()>;
    fn emit_push(&mut self, state: &RuntimeConstructState) -> Result<()>;
    fn emit_pop(&mut self, state: &RuntimeConstructState) -> Result<()>;
    fn emit_leave(&mut self) -> Result<()>;
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
                CompiledSemanticOp::Move => self.emitter.emit_move(state)?,
                CompiledSemanticOp::Lea => self.emitter.emit_lea(state)?,
                CompiledSemanticOp::Push => self.emitter.emit_push(state)?,
                CompiledSemanticOp::Pop => self.emitter.emit_pop(state)?,
                CompiledSemanticOp::Leave => self.emitter.emit_leave()?,
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
