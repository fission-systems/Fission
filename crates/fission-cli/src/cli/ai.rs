//! `fission_cli ai` subcommand handler.
//!
//! Routes `ai login / status / logout / chat` to the appropriate
//! fission-ai and fission-tui entry points.

use anyhow::{Context, Result};
use fission_ai::auth::{OAuthOptions, ResolvedAuth, codex_oauth, resolve_auth, token_store::TokenStore};
use fission_ai::provider::{ProviderKind, provider_kind_from_env};

use crate::cli::AiInvocation;

/// Async entry point for all `ai` subcommands.
pub async fn run_ai(inv: AiInvocation) -> Result<()> {
    match inv {
        AiInvocation::Login => run_login().await,
        AiInvocation::Status => run_status().await,
        AiInvocation::Logout => run_logout().await,
        AiInvocation::Chat { provider, model } => run_chat(provider, model).await,
    }
}

// ── Login ─────────────────────────────────────────────────────────────────────

async fn run_login() -> Result<()> {
    let opts = OAuthOptions::default();
    codex_oauth::run_browser_login(&opts)
        .await
        .context("Codex OAuth login failed")
}

// ── Status ────────────────────────────────────────────────────────────────────

async fn run_status() -> Result<()> {
    let opts = OAuthOptions::default();
    let auth = resolve_auth(&opts).await.context("failed to resolve auth")?;

    match &auth {
        ResolvedAuth::OAuthToken(_) => {
            match TokenStore::load(&opts.fission_home).await {
                Ok(store) => {
                    if let Some(token) = store.stored_token() {
                        println!("✓ Authenticated via Codex OAuth");
                        println!("  Provider : {}", token.provider);
                        if let Some(exp) = token.expires_at {
                            let now = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs();
                            let remaining = exp.saturating_sub(now);
                            println!("  Expires  : in {}m {}s", remaining / 60, remaining % 60);
                        }
                    }
                }
                Err(e) => println!("⚠ OAuth token present but store unreadable: {e}"),
            }
        }
        ResolvedAuth::ApiKey(_) => {
            let key_var = if std::env::var("FISSION_AI_API_KEY").is_ok() {
                "FISSION_AI_API_KEY"
            } else {
                "OPENAI_API_KEY"
            };
            println!("✓ Authenticated via API key ({key_var})");
        }
        ResolvedAuth::None => {
            println!("✗ Not authenticated");
            println!("  Run `fission_cli ai login` for Codex OAuth (no API key needed).");
            println!("  Or set OPENAI_API_KEY / FISSION_AI_API_KEY in the environment.");
        }
    }
    Ok(())
}

// ── Logout ────────────────────────────────────────────────────────────────────

async fn run_logout() -> Result<()> {
    let opts = OAuthOptions::default();
    TokenStore::clear(&opts.fission_home)
        .await
        .context("failed to remove auth token")?;
    println!("✓ Logged out — token removed from ~/.fission/auth.json");
    Ok(())
}

// ── Chat (TUI) ────────────────────────────────────────────────────────────────

async fn run_chat(provider_override: Option<String>, model_override: Option<String>) -> Result<()> {
    // Resolve provider kind — parse the override string directly (no env mutation).
    let kind: ProviderKind = match provider_override.as_deref() {
        Some(s) => s.parse().map_err(|e: String| anyhow::anyhow!("{e}"))?,
        None => provider_kind_from_env(),
    };

    // Resolve auth.
    let opts = OAuthOptions::default();
    let auth = resolve_auth(&opts).await.context("failed to resolve auth")?;

    if !auth.is_authenticated() && kind != ProviderKind::Ollama {
        eprintln!("⚠ Not authenticated. Run `fission_cli ai login` first,");
        eprintln!("  or set OPENAI_API_KEY / FISSION_AI_API_KEY,");
        eprintln!("  or use --provider ollama for local models.");
        return Ok(());
    }

    let pipeline = fission_ai::AiPipeline::build(
        kind,
        auth,
        Some(
            "You are Fission AI, an expert assistant for binary analysis, \
             reverse engineering, and decompilation. Be concise and technically precise."
                .to_string(),
        ),
        model_override,
    )
    .context("failed to initialise AI pipeline")?;

    // Launch TUI on the current thread (ratatui takes over the terminal).
    fission_tui::run_tui(pipeline).context("TUI exited with error")
}
