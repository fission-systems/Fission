//! Configure Rhai [`Engine`] for sandboxed scripts.

use crate::error::ScriptError;
use crate::limits::ScriptLimits;
use rhai::Engine;

pub fn configure_engine(engine: &mut Engine, limits: &ScriptLimits) -> Result<(), ScriptError> {
    engine.set_max_operations(limits.max_operations);
    engine.set_max_expr_depths(64, 64);
    engine.set_max_string_size(256 * 1024);
    Ok(())
}

pub fn new_engine_for_compile_check() -> Engine {
    let mut engine = Engine::new();
    let limits = ScriptLimits::default();
    let _ = configure_engine(&mut engine, &limits);
    engine
}
