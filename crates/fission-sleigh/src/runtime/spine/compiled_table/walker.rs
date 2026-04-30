// ParserWalker-style operand binding for the compiled table. Split into `walker/*.rs`.
include!("walker/bind.rs");
include!("walker/structs.rs");
// Keep the full `CompiledParserWalker` impl in one include (see `template_eval` — splitting
// a single `impl` across multiple `include!` files can trigger rustc delimiter errors).
include!("walker/impl_walker.rs");
