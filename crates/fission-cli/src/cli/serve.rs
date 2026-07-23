//! `fission serve` CLI entry point — delegates to the `fission-serve` crate.
//!
//! All HTTP server logic, session management, and handler implementation
//! lives in `fission-serve`. This module is only a thin dispatch shim.

use anyhow::Result;
use fission_serve::{ServeConfig, run_serve};

/// Start the fission HTTP API server.
/// Called from `oneshot/mod.rs` after CLI argument parsing.
pub async fn run_serve_cli(port: u16, host: String) -> Result<()> {
    let config = ServeConfig {
        port,
        host,
        ..ServeConfig::default()
    };
    run_serve(config).await
}
