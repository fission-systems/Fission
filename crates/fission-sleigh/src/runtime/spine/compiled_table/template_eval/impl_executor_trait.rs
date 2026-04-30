impl RuntimeTemplateExecutor for CompiledTableEmitter<'_> {
    fn emit_op_template(
        &mut self,
        state: &RuntimeConstructState,
        op: &CompiledOpTpl,
    ) -> Result<()> {
        CompiledTableEmitter::emit_op_template(self, state, op)
    }
}
