//! fission-tui standalone binary entry point.
//! Primarily used for development/testing; launches the TUI chat interface directly.

use anyhow::{Context, Result};
use fission_ai::auth::{OAuthOptions, resolve_auth};
use fission_ai::provider::provider_kind_from_env;

#[tokio::main]
async fn main() -> Result<()> {
    let kind = provider_kind_from_env();
    let opts = OAuthOptions::default();
    let auth = resolve_auth(&opts)
        .await
        .context("failed to resolve auth")?;

    let system_prompt = "You are Fission AI, an expert assistant for binary analysis, \
             reverse engineering, and decompilation. Be concise and technically precise."
        .to_string();

    let pipeline = fission_ai::AiPipeline::build(kind, auth, Some(system_prompt), None, None)
        .context("failed to initialise AI pipeline")?;

    // Since we are inside a tokio runtime context, we use block_in_place to launch the TUI.
    tokio::task::block_in_place(|| fission_tui::run_tui(pipeline))
        .context("TUI exited with error")?;

    Ok(())
}
