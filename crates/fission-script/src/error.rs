//! Script crate errors.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ScriptError {
    #[error("failed to compile script: {0}")]
    Compile(String),

    #[error("runtime error: {0}")]
    Runtime(String),

    #[error("configuration error: {0}")]
    Config(String),
}
