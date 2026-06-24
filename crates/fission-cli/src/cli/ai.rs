//! `fission_cli ai` subcommand handler.
//!
//! Routes `ai login / copilot-login / status / logout / chat` to the
//! appropriate fission-ai and fission-tui entry points.

use anyhow::{Context, Result};
use fission_ai::auth::{
    OAuthOptions, ResolvedAuth, codex_oauth, copilot_oauth, resolve_auth, token_store::TokenStore,
};
use fission_ai::provider::{ProviderKind, provider_kind_from_env};

use fission_loader::loader::LoadedBinary;
use std::fs;

use crate::cli::AiInvocation;

/// Async entry point for all `ai` subcommands.
pub async fn run_ai(inv: AiInvocation) -> Result<()> {
    match inv {
        AiInvocation::Login => run_login().await,
        AiInvocation::CopilotLogin => run_copilot_login().await,
        AiInvocation::Status => run_status().await,
        AiInvocation::Logout => run_logout().await,
        AiInvocation::Chat(args) => run_chat(args).await,
        AiInvocation::Analyze(args) => run_analyze(args).await,
    }
}

// ── Codex / ChatGPT Browser OAuth login ──────────────────────────────────────

async fn run_login() -> Result<()> {
    let opts = OAuthOptions::default();
    codex_oauth::run_browser_login(&opts)
        .await
        .context("Codex OAuth login failed")
}

// ── GitHub Copilot Device Code login ─────────────────────────────────────────

async fn run_copilot_login() -> Result<()> {
    let opts = OAuthOptions::default();
    copilot_oauth::run_copilot_login(&opts.fission_home)
        .await
        .context("GitHub Copilot login failed")
}

// ── Status ────────────────────────────────────────────────────────────────────

async fn run_status() -> Result<()> {
    let opts = OAuthOptions::default();
    let auth = resolve_auth(&opts)
        .await
        .context("failed to resolve auth")?;

    match &auth {
        ResolvedAuth::OAuthToken(_) => match TokenStore::load(&opts.fission_home).await {
            Ok(store) => {
                if let Some(token) = store.stored_token() {
                    let provider_display = match token.provider.as_str() {
                        "copilot" => "GitHub Copilot OAuth",
                        "codex" => "Codex (ChatGPT) OAuth",
                        other => other,
                    };
                    println!("✓ Authenticated via {provider_display}");
                    println!("  Provider : {}", token.provider);
                    if let Some(exp) = token.expires_at {
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs();
                        let remaining = exp.saturating_sub(now);
                        println!("  Expires  : in {}m {}s", remaining / 60, remaining % 60);
                    } else {
                        println!("  Expires  : long-lived token");
                    }
                }
            }
            Err(e) => println!("⚠ OAuth token present but store unreadable: {e}"),
        },
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
            println!(
                "  Run `fission_cli ai login`         for Codex/ChatGPT (ChatGPT Plus required)"
            );
            println!("  Run `fission_cli ai copilot-login` for GitHub Copilot ($10/mo)");
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

async fn run_chat(args: crate::cli::args::AiChatArgs) -> Result<()> {
    // Resolve provider kind.
    let kind: ProviderKind = match args.provider.as_deref() {
        Some(s) => s.parse().map_err(|e: String| anyhow::anyhow!("{e}"))?,
        None => provider_kind_from_env(),
    };

    // Resolve auth — for Copilot, stored token is used as bearer.
    let opts = OAuthOptions::default();
    let auth = resolve_auth(&opts)
        .await
        .context("failed to resolve auth")?;

    if !auth.is_authenticated() && kind != ProviderKind::Ollama {
        eprintln!("⚠ Not authenticated.");
        eprintln!("  Run `fission_cli ai copilot-login` for GitHub Copilot (recommended)");
        eprintln!("  Run `fission_cli ai login`         for Codex/ChatGPT");
        eprintln!("  Or use --provider ollama for local models.");
        return Ok(());
    }

    let mut system_prompt = "You are Fission AI, an expert assistant for binary analysis, \
             reverse engineering, and decompilation. Be concise and technically precise."
        .to_string();

    if let Some(ref bin_path) = args.binary {
        let binary_data = fs::read(&bin_path)
            .with_context(|| format!("failed to read binary `{}`", bin_path.display()))?;
        let binary = LoadedBinary::from_bytes(binary_data, bin_path.to_string_lossy().to_string())
            .with_context(|| format!("failed to parse binary `{}`", bin_path.display()))?;

        let arch_display = binary
            .architecture
            .as_ref()
            .map(|arch| format!("{} {}-bit ({})", arch.processor, arch.bitness, arch.variant))
            .unwrap_or_else(|| "unknown".to_string());

        let imports = fission_loader::loader::function_view::canonical_imports_sorted(&binary);
        let exports = fission_loader::loader::function_view::canonical_exports_sorted(&binary);
        let counts = fission_loader::loader::function_view::canonical_view_counts(&binary);

        let mut context = format!(
            "\n\n[Loaded Binary Context]\n\
             Path: {}\n\
             Format: {}\n\
             Arch: {}\n\
             Image Base: 0x{:x}\n\
             Entry Point: 0x{:x}\n\
             Sections: {}\n\
             Total Functions: {}\n\
             Import Count: {}\n\
             Export Count: {}\n",
            bin_path.display(),
            binary.format,
            arch_display,
            binary.image_base,
            binary.entry_point,
            binary.sections.len(),
            counts.functions,
            counts.imports,
            counts.exports
        );

        if !imports.is_empty() {
            context.push_str("\nKey Imports (up to 30):\n");
            for imp in imports.iter().take(30) {
                context.push_str(&format!("- {}\n", imp.name));
            }
        }

        if !exports.is_empty() {
            context.push_str("\nKey Exports (up to 30):\n");
            for exp in exports.iter().take(30) {
                context.push_str(&format!("- {}\n", exp.name));
            }
        }
        system_prompt.push_str(&context);
    }

    let pipeline = fission_ai::AiPipeline::build(
        kind,
        auth,
        Some(system_prompt),
        args.model,
        args.binary.clone(),
    )
    .context("failed to initialise AI pipeline")?;

    // Launch TUI on the current thread. We must escape the current tokio
    // context because the TUI runner creates its own dedicated runtime.
    tokio::task::block_in_place(|| fission_tui::run_tui(pipeline)).context("TUI exited with error")
}

// ── Analyze Pseudocode ────────────────────────────────────────────────────────

const DEFAULT_PSEUDOCODE: &str = r#"
// Decompiled function: check_license
int check_license(char *param_1) {
    int local_1 = 0;
    int local_2 = 0;
    while (param_1[local_1] != '\0') {
        local_2 = local_2 + (int)param_1[local_1];
        local_1 = local_1 + 1;
    }
    if (local_2 == 0x539) {
        return 1;
    }
    return 0;
}
"#;

async fn run_analyze(args: crate::cli::args::AiAnalyzeArgs) -> Result<()> {
    use fission_ai::provider::PseudocodeAnalyzer;
    use fission_ai::provider::mock::MockProvider;

    let code = args.code.as_deref().unwrap_or(DEFAULT_PSEUDOCODE);

    println!("=== Input Pseudocode ===");
    println!("{}", code.trim());
    println!();

    let provider = MockProvider::new("mock-model".to_string());
    println!("=== Performing Analysis ===");
    let analysis = provider.analyze_pseudocode(code).await?;

    println!("=== Resulting Analysis ===");
    println!("{}", analysis);

    Ok(())
}
