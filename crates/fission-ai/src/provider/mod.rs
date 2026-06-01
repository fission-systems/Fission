//! Multi-provider AI abstraction for fission-ai.
//!
//! Providers implement [`AiProvider`] and are selected at runtime via
//! [`ProviderConfig`] / environment variables.

pub mod codex;
pub mod copilot;
pub mod ollama;
pub mod openai;

use std::fmt;
use std::pin::Pin;
use std::sync::Arc;
use async_trait::async_trait;
use futures::Stream;
use thiserror::Error;

use crate::session::Message;
use crate::tools::ToolDefinition;

// ── Streaming response chunk ──────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ProviderToolCallFunctionDelta {
    pub name: Option<String>,
    pub arguments: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ProviderToolCallDelta {
    pub index: usize,
    pub id: Option<String>,
    pub kind: Option<String>,
    pub function: Option<ProviderToolCallFunctionDelta>,
}

/// A single token/delta from a streaming AI response.
#[derive(Debug, Clone)]
pub struct ResponseChunk {
    /// The delta text from this chunk.
    pub delta: String,
    /// Tool call fragments.
    pub tool_calls: Option<Vec<ProviderToolCallDelta>>,
    /// Whether this is the final chunk in the stream.
    pub done: bool,
}

// ── Provider error ────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("not authenticated — run `fission_cli ai login`")]
    NotAuthenticated,
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("provider error: {0}")]
    Other(String),
}

pub type ProviderResult<T> = Result<T, ProviderError>;

// ── Provider trait ────────────────────────────────────────────────────────────

/// Shared handle to a concrete provider implementation.
pub type SharedAiProvider = Arc<dyn AiProvider>;

/// Opaque stream of [`ResponseChunk`] items from a chat request.
pub type ChunkStream = Pin<Box<dyn Stream<Item = ProviderResult<ResponseChunk>> + Send>>;

/// Trait that every model provider must implement.
#[async_trait]
pub trait AiProvider: fmt::Debug + Send + Sync {
    /// Human-readable provider name shown in the TUI status bar.
    fn name(&self) -> &str;

    /// The model string used for this provider (e.g. `"gpt-4o"`, `"llama3"`).
    fn model(&self) -> &str;

    /// Whether this provider requires authentication credentials.
    fn requires_auth(&self) -> bool {
        true
    }

    /// Send a chat completion request and return a streaming response.
    async fn chat_stream(&self, messages: &[Message], tools: Option<&[ToolDefinition]>) -> ProviderResult<ChunkStream>;

    /// Return a one-shot (non-streaming) response.  Default implementation
    /// collects the stream into a single string.
    async fn chat(&self, messages: &[Message], tools: Option<&[ToolDefinition]>) -> ProviderResult<String> {
        use futures::StreamExt;
        let mut stream = self.chat_stream(messages, tools).await?;
        let mut out = String::new();
        while let Some(chunk) = stream.next().await {
            out.push_str(&chunk?.delta);
        }
        Ok(out)
    }
}

// ── Provider selection ────────────────────────────────────────────────────────

/// Which provider backend to use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderKind {
    /// Codex/ChatGPT via OAuth — no API key required.
    Codex,
    /// GitHub Copilot via OAuth — requires Copilot Individual/Business subscription.
    Copilot,
    /// OpenAI-compatible endpoint (OpenAI, Azure, local vLLM, etc.).
    OpenAi,
    /// Local Ollama server.
    Ollama,
}

impl fmt::Display for ProviderKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Codex => write!(f, "codex"),
            Self::Copilot => write!(f, "copilot"),
            Self::OpenAi => write!(f, "openai"),
            Self::Ollama => write!(f, "ollama"),
        }
    }
}

impl std::str::FromStr for ProviderKind {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "codex" | "chatgpt" => Ok(Self::Codex),
            "copilot" | "github-copilot" | "github_copilot" => Ok(Self::Copilot),
            "openai" => Ok(Self::OpenAi),
            "ollama" => Ok(Self::Ollama),
            other => Err(format!("unknown provider: {other}. Use codex|copilot|openai|ollama")),
        }
    }
}

/// Configuration used to build a concrete [`SharedAiProvider`].
#[derive(Debug, Clone)]
pub struct ProviderConfig {
    pub kind: ProviderKind,
    /// Bearer token / API key (pre-resolved by auth layer).
    pub bearer_token: Option<String>,
    /// Override base URL (useful for OpenAI-compatible servers).
    pub base_url: Option<String>,
    /// Model string override.
    pub model: Option<String>,
}

/// Resolve the provider from environment variables, falling back to defaults.
pub fn provider_kind_from_env() -> ProviderKind {
    use crate::auth::ENV_FISSION_AI_PROVIDER;
    if let Ok(val) = std::env::var(ENV_FISSION_AI_PROVIDER) {
        if let Ok(kind) = val.parse::<ProviderKind>() {
            return kind;
        }
    }
    // Auto-detect: prefer Codex if token exists, then Ollama if reachable.
    ProviderKind::Codex
}

/// Build a concrete [`SharedAiProvider`] from a [`ProviderConfig`].
pub fn build_provider(cfg: ProviderConfig) -> SharedAiProvider {
    match cfg.kind {
        ProviderKind::Codex => Arc::new(codex::CodexProvider::new(
            cfg.bearer_token,
            cfg.model.unwrap_or_else(|| "gpt-4o".into()),
        )),
        ProviderKind::Copilot => Arc::new(copilot::CopilotProvider::new(
            cfg.bearer_token,
            cfg.model.unwrap_or_else(|| "claude-sonnet-4.5".into()),
        )),
        ProviderKind::OpenAi => Arc::new(openai::OpenAiProvider::new(
            cfg.bearer_token,
            cfg.base_url,
            cfg.model.unwrap_or_else(|| "gpt-4o".into()),
        )),
        ProviderKind::Ollama => Arc::new(ollama::OllamaProvider::new(
            cfg.base_url,
            cfg.model.unwrap_or_else(|| "llama3".into()),
        )),
    }
}
