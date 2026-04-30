pub(super) fn bind_instruction<'a>(
    compiled: &'a CompiledFrontend,
    strategy: RuntimeDecodeStrategy<'a>,
    ctx: &CompiledInstructionContext<'_>,
    selection: RuntimeSelection<'a>,
) -> Result<RuntimeConstructState> {
    constructor_matches(ctx, selection.constructor)?;
    CompiledParserWalker::new(compiled, strategy, ctx, selection)?.walk()
}
