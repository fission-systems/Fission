//! GitHub Copilot provider.
//!
//! Uses the GitHub Copilot API (`api.githubcopilot.com`) with an OAuth token
//! obtained via [`crate::auth::copilot_oauth::run_copilot_login`].
//!
//! The Copilot API is OpenAI-compatible (`/v1/chat/completions`), so this is
//! a thin wrapper over [`OpenAiProvider`] with the Copilot base URL and the
//! extra headers that GitHub requires.
//!
//! Reference: vendor/opencode-1.15.13/packages/opencode/src/plugin/github-copilot/copilot.ts

use async_trait::async_trait;

use crate::auth::copilot_oauth::COPILOT_API_BASE;
use crate::session::Message;
use super::{AiProvider, ChunkStream, ProviderResult};
use super::openai::OpenAiProvider;

/// GitHub Copilot provider — uses a GitHub OAuth token.
///
/// Default model: `claude-sonnet-4.5` (strong, mid-tier).
/// Other available models (as of 2026-06):
/// - `gpt-5.4`, `gpt-5.5`, `gpt-5.4-mini`, `gpt-5-mini`
/// - `claude-opus-4.5`, `claude-opus-4.7`, `claude-opus-4.8`
/// - `claude-haiku-4.5`
/// - `gemini-2.5-pro`, `gemini-3-flash-preview`, `gemini-3.5-flash`
/// - `gpt-5.2-codex`, `gpt-5.3-codex` (coding-focused)
///
/// Override at runtime: `fission_cli ai chat --provider copilot --model gpt-5.4`
#[derive(Debug)]
pub struct CopilotProvider {
    inner: OpenAiProvider,
    model_name: String,
}

impl CopilotProvider {
    /// Create a new Copilot provider.
    ///
    /// `bearer_token` is the GitHub OAuth token stored by `TokenStore`.
    /// `model` defaults to `"claude-sonnet-4.5"` if `None`.
    pub fn new(bearer_token: Option<String>, model: String) -> Self {
        let base = COPILOT_API_BASE.to_string();
        Self {
            model_name: model.clone(),
            inner: OpenAiProvider::new_with_extra_headers(
                bearer_token,
                Some(base),
                model,
                vec![
                    // Required by the Copilot API
                    ("Openai-Intent".to_string(), "conversation-edits".to_string()),
                    ("x-initiator".to_string(), "user".to_string()),
                ],
            ),
        }
    }
}

#[async_trait]
impl AiProvider for CopilotProvider {
    fn name(&self) -> &str {
        "github-copilot"
    }

    fn model(&self) -> &str {
        &self.model_name
    }

    fn requires_auth(&self) -> bool {
        true
    }

    async fn chat_stream(&self, messages: &[Message], tools: Option<&[crate::tools::ToolDefinition]>) -> ProviderResult<ChunkStream> {
        self.inner.chat_stream(messages, tools).await
    }
}
