//! Configure Rhai [`Engine`] for sandboxed scripts.

use crate::error::ScriptError;
use crate::limits::ScriptLimits;
use rhai::Engine;
use rhai::packages::{
    BasicArrayPackage, BasicMapPackage, BasicMathPackage, BitFieldPackage, CorePackage,
    LogicPackage, MoreStringPackage, Package,
};

pub fn configure_engine(engine: &mut Engine, limits: &ScriptLimits) -> Result<(), ScriptError> {
    engine.set_max_operations(limits.max_operations);
    engine.set_max_expr_depths(64, 64);
    engine.set_max_string_size(256 * 1024);
    // Scripts use host bindings and pure language helpers only.  No script
    // imports are part of the API, so fail closed even if a future package
    // adds a resolver-backed module by default.
    engine.set_max_modules(0);
    Ok(())
}

/// Construct the scripting engine without Rhai's default module resolver.
///
/// `Engine::new()` installs a [`FileModuleResolver`] and the standard package.
/// That is a poor default for scripts that are intentionally limited to
/// Fission's registered host bindings: an `import` could otherwise reach the
/// filesystem.  Keep the useful, deterministic language packages explicit and
/// leave I/O-oriented capabilities out of the engine entirely.
pub fn new_sandbox_engine() -> Engine {
    let mut engine = Engine::new_raw();
    CorePackage::new().register_into_engine(&mut engine);
    BitFieldPackage::new().register_into_engine(&mut engine);
    LogicPackage::new().register_into_engine(&mut engine);
    BasicMathPackage::new().register_into_engine(&mut engine);
    BasicArrayPackage::new().register_into_engine(&mut engine);
    BasicMapPackage::new().register_into_engine(&mut engine);
    MoreStringPackage::new().register_into_engine(&mut engine);
    engine
}

pub fn new_engine_for_compile_check() -> Engine {
    let mut engine = new_sandbox_engine();
    let limits = ScriptLimits::default();
    let _ = configure_engine(&mut engine, &limits);
    engine
}
