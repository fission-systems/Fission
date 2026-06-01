//! Authentication layer for fission-ai.
//!
//! Priority chain (highest first):
//! 1. Stored OAuth token (`~/.fission/auth.json`)
//! 2. `FISSION_AI_API_KEY` env var
//! 3. `OPENAI_API_KEY` env var
//! 4. Ollama (no auth required, via `FISSION_AI_OLLAMA_URL` or localhost default)

pub mod api_key;
pub mod codex_oauth;
pub mod copilot_oauth;
pub mod token_store;

use std::path::PathBuf;
use thiserror::Error;

/// Base URL for Codex/ChatGPT OAuth endpoints.
pub const CODEX_AUTH_BASE_URL: &str = "https://auth.openai.com";
/// OpenAI client ID used by OpenCode — supports localhost redirect URI for Browser OAuth.
pub const CODEX_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";

/// GitHub Copilot OAuth client ID (from OpenCode reference).
pub use copilot_oauth::COPILOT_CLIENT_ID;

/// Environment variable names recognised by fission-ai.
pub const ENV_FISSION_AI_API_KEY: &str = "FISSION_AI_API_KEY";
pub const ENV_OPENAI_API_KEY: &str = "OPENAI_API_KEY";
pub const ENV_FISSION_AI_OLLAMA_URL: &str = "FISSION_AI_OLLAMA_URL";
pub const ENV_FISSION_AI_PROVIDER: &str = "FISSION_AI_PROVIDER";

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("not authenticated — run `fission_cli ai login`")]
    NotAuthenticated,
    #[error("OAuth device code request failed: {0}")]
    DeviceCodeRequest(String),
    #[error("OAuth token poll failed: {0}")]
    TokenPoll(String),
    #[error("OAuth token exchange failed: {0}")]
    TokenExchange(String),
    #[error("token store error: {0}")]
    TokenStore(String),
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Other(String),
}

pub type AuthResult<T> = Result<T, AuthError>;

/// Options for the OAuth / authentication flow.
#[derive(Debug, Clone)]
pub struct OAuthOptions {
    /// Issuer base URL (default: CODEX_AUTH_BASE_URL)
    pub auth_base_url: String,
    /// OAuth client ID
    pub client_id: String,
    /// Directory for storing `auth.json` (default: `~/.fission`)
    pub fission_home: PathBuf,
}

impl Default for OAuthOptions {
    fn default() -> Self {
        Self {
            auth_base_url: CODEX_AUTH_BASE_URL.to_string(),
            client_id: CODEX_CLIENT_ID.to_string(),
            fission_home: default_fission_home(),
        }
    }
}

/// Returns the resolved credential for the current session, following the
/// priority chain described in the module docs.
pub async fn resolve_auth(opts: &OAuthOptions) -> AuthResult<ResolvedAuth> {
    // 1. Stored OAuth token
    if let Ok(store) = token_store::TokenStore::load(&opts.fission_home).await {
        if let Some(token) = store.valid_access_token() {
            return Ok(ResolvedAuth::OAuthToken(token.to_string()));
        }
    }

    // 2. FISSION_AI_API_KEY
    if let Some(key) = api_key::read_fission_api_key() {
        return Ok(ResolvedAuth::ApiKey(key));
    }

    // 3. OPENAI_API_KEY
    if let Some(key) = api_key::read_openai_api_key() {
        return Ok(ResolvedAuth::ApiKey(key));
    }

    // 4. Unauthenticated (Ollama / local)
    Ok(ResolvedAuth::None)
}

/// The resolved credential to use for a provider request.
#[derive(Debug, Clone)]
pub enum ResolvedAuth {
    /// Bearer token from Codex/ChatGPT OAuth
    OAuthToken(String),
    /// API key (FISSION_AI_API_KEY or OPENAI_API_KEY)
    ApiKey(String),
    /// No authentication (e.g. Ollama)
    None,
}

impl ResolvedAuth {
    /// Returns the bearer token string if this is an OAuth token or API key.
    pub fn bearer_token(&self) -> Option<&str> {
        match self {
            Self::OAuthToken(t) | Self::ApiKey(t) => Some(t.as_str()),
            Self::None => None,
        }
    }

    pub fn is_authenticated(&self) -> bool {
        !matches!(self, Self::None)
    }
}

fn default_fission_home() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".fission")
}
